//! Small transient-analysis MVP for beginner RC and blink lessons.
//!
//! This is intentionally narrow: it recognizes one capacitor, a resistive
//! charge/discharge path, and a DC or PWM-like square source. Unsupported
//! circuits return `None` instead of pretending to simulate arbitrary dynamics.

use crate::{
    Component, ComponentKind, Wire, component_pin_defs, engine::mna,
    engine::netlist::build_circuit_netlist, parse_metric_value,
};

#[derive(Clone, Debug)]
pub(crate) struct TransientResult {
    pub(crate) kind: TransientKind,
    pub(crate) samples: Vec<TransientSample>,
    pub(crate) summary: String,
    pub(crate) limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TransientKind {
    RcStep,
    PwmRc,
}

#[derive(Clone, Debug)]
pub(crate) struct TransientSample {
    pub(crate) t_s: f64,
    pub(crate) v_cap: f64,
    pub(crate) source_v: f64,
}

pub(crate) fn solve_transient(components: &[Component], wires: &[Wire]) -> Option<TransientResult> {
    let capacitor = components
        .iter()
        .find(|component| component.kind == ComponentKind::Capacitor)?;
    let cap_pins = component_pin_defs(capacitor);
    let cap_a = cap_pins.first()?;
    let cap_b = cap_pins.get(1)?;
    let capacitance = parse_metric_value(&capacitor.value, "f")
        .or_else(|| parse_metric_value(&capacitor.value, "F"))
        .unwrap_or(100e-9) as f64;
    if capacitance <= 0.0 {
        return None;
    }

    let netlist = build_circuit_netlist(components, wires);
    let cap_a_net = netlist
        .pins
        .iter()
        .find(|pin| pin.component_id == capacitor.id && pin.pin_name == cap_a.label)?
        .net_id;
    let cap_b_net = netlist
        .pins
        .iter()
        .find(|pin| pin.component_id == capacitor.id && pin.pin_name == cap_b.label)?
        .net_id;
    if cap_a_net == cap_b_net {
        return None;
    }

    let source = components.iter().find(|component| {
        matches!(
            component.kind,
            ComponentKind::VSource | ComponentKind::Battery
        )
    })?;
    let source_voltage = source_voltage(source);
    let source_is_pwm = source.value.to_ascii_lowercase().contains("pwm")
        || source.value.to_ascii_lowercase().contains("square");

    let mut resistance = None;
    for resistor in components
        .iter()
        .filter(|component| component.kind == ComponentKind::Resistor)
    {
        let pins = component_pin_defs(resistor);
        let Some(a) = pins.first() else {
            continue;
        };
        let Some(b) = pins.get(1) else {
            continue;
        };
        let Some(ra) = netlist
            .pins
            .iter()
            .find(|pin| pin.component_id == resistor.id && pin.pin_name == a.label)
        else {
            continue;
        };
        let Some(rb) = netlist
            .pins
            .iter()
            .find(|pin| pin.component_id == resistor.id && pin.pin_name == b.label)
        else {
            continue;
        };
        if ra.net_id == cap_a_net
            || ra.net_id == cap_b_net
            || rb.net_id == cap_a_net
            || rb.net_id == cap_b_net
        {
            resistance = Some(parse_metric_value(&resistor.value, "ohm").unwrap_or(1_000.0) as f64);
            break;
        }
    }
    let resistance = resistance?;
    if resistance <= 0.0 {
        return None;
    }

    let tau = resistance * capacitance;
    let duration = (tau * 5.0).max(0.010);
    let steps = 80usize;
    let mut samples = Vec::with_capacity(steps + 1);

    if source_is_pwm {
        let freq = parse_frequency_hz(&source.value).unwrap_or(500.0).max(1.0);
        let duty = parse_duty(&source.value).unwrap_or(0.5).clamp(0.0, 1.0);
        let duration = duration.max(3.0 / freq);
        let dt = duration / steps as f64;
        let mut v_cap = 0.0;
        for step in 0..=steps {
            let t = step as f64 * dt;
            let phase = (t * freq).fract();
            let target = if phase < duty { source_voltage } else { 0.0 };
            samples.push(TransientSample {
                t_s: t,
                v_cap,
                source_v: target,
            });
            v_cap += (target - v_cap) * (1.0 - (-dt / tau).exp());
        }
        Some(TransientResult {
            kind: TransientKind::PwmRc,
            samples,
            summary: format!(
                "PWM RC transient: tau={}s, source={}V",
                mna::format_si(tau, ""),
                mna::format_voltage(source_voltage)
            ),
            limitations: vec![
                "MVP model: one capacitor and one effective resistor are simulated.".to_string(),
                "Square-wave source is parsed from a VSource/Battery value containing PWM or square.".to_string(),
            ],
        })
    } else {
        for step in 0..=steps {
            let t = step as f64 * duration / steps as f64;
            samples.push(TransientSample {
                t_s: t,
                v_cap: source_voltage * (1.0 - (-t / tau).exp()),
                source_v: source_voltage,
            });
        }
        Some(TransientResult {
            kind: TransientKind::RcStep,
            samples,
            summary: format!(
                "RC step transient: tau={}s, final {}",
                mna::format_si(tau, ""),
                mna::format_voltage(source_voltage)
            ),
            limitations: vec![
                "MVP model: one capacitor and one effective resistor are simulated.".to_string(),
                "Initial capacitor voltage is assumed to be 0 V.".to_string(),
            ],
        })
    }
}

fn source_voltage(source: &Component) -> f64 {
    parse_metric_value(&source.value, "v").unwrap_or(if source.kind == ComponentKind::Battery {
        9.0
    } else {
        5.0
    }) as f64
}

fn parse_frequency_hz(value: &str) -> Option<f64> {
    value
        .split(|c: char| c.is_ascii_whitespace() || c == ',' || c == ';')
        .find_map(|token| parse_metric_value(token, "hz").map(|v| v as f64))
}

fn parse_duty(value: &str) -> Option<f64> {
    let lower = value.to_ascii_lowercase();
    for token in lower.split(|c: char| c.is_ascii_whitespace() || c == ',' || c == ';') {
        if let Some(percent) = token.strip_suffix('%') {
            if let Ok(v) = percent.parse::<f64>() {
                return Some(v / 100.0);
            }
        }
        if let Some(duty) = token.strip_prefix("duty=") {
            if let Ok(v) = duty.trim_end_matches('%').parse::<f64>() {
                return Some(if duty.ends_with('%') { v / 100.0 } else { v });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PinRole;
    use egui::Pos2;

    fn comp(id: u64, kind: ComponentKind, pos: Pos2, label: &str, value: &str) -> Component {
        Component {
            id,
            kind,
            pos,
            rotation: 0,
            label: label.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn rc_step_reaches_expected_final_voltage() {
        let src = comp(1, ComponentKind::VSource, Pos2::new(0.0, 0.0), "V1", "5V");
        let r = comp(
            2,
            ComponentKind::Resistor,
            Pos2::new(140.0, 0.0),
            "R1",
            "1k",
        );
        let c = comp(
            3,
            ComponentKind::Capacitor,
            Pos2::new(260.0, 0.0),
            "C1",
            "1uF",
        );
        let gnd = comp(
            4,
            ComponentKind::Ground,
            Pos2::new(260.0, 100.0),
            "GND",
            "0V",
        );
        let sp = component_pin_defs(&src);
        let rp = component_pin_defs(&r);
        let cp = component_pin_defs(&c);
        let gp = component_pin_defs(&gnd)[0].pos;
        let wires = vec![
            Wire::new(
                1,
                vec![
                    sp.iter().find(|p| p.role == PinRole::Positive).unwrap().pos,
                    rp[0].pos,
                ],
            ),
            Wire::new(2, vec![rp[1].pos, cp[0].pos]),
            Wire::new(3, vec![cp[1].pos, gp]),
            Wire::new(
                4,
                vec![
                    sp.iter().find(|p| p.role == PinRole::Ground).unwrap().pos,
                    gp,
                ],
            ),
        ];
        let transient = solve_transient(&[src, r, c, gnd], &wires).expect("RC transient");
        assert_eq!(transient.kind, TransientKind::RcStep);
        let final_v = transient.samples.last().unwrap().v_cap;
        assert!((final_v - 5.0).abs() < 0.05, "final_v={final_v}");
    }

    #[test]
    fn pwm_source_generates_square_targets() {
        let src = comp(
            1,
            ComponentKind::VSource,
            Pos2::new(0.0, 0.0),
            "V1",
            "5V PWM 100Hz 25%",
        );
        let r = comp(
            2,
            ComponentKind::Resistor,
            Pos2::new(140.0, 0.0),
            "R1",
            "10k",
        );
        let c = comp(
            3,
            ComponentKind::Capacitor,
            Pos2::new(260.0, 0.0),
            "C1",
            "1uF",
        );
        let gnd = comp(
            4,
            ComponentKind::Ground,
            Pos2::new(260.0, 100.0),
            "GND",
            "0V",
        );
        let sp = component_pin_defs(&src);
        let rp = component_pin_defs(&r);
        let cp = component_pin_defs(&c);
        let gp = component_pin_defs(&gnd)[0].pos;
        let wires = vec![
            Wire::new(
                1,
                vec![
                    sp.iter().find(|p| p.role == PinRole::Positive).unwrap().pos,
                    rp[0].pos,
                ],
            ),
            Wire::new(2, vec![rp[1].pos, cp[0].pos]),
            Wire::new(3, vec![cp[1].pos, gp]),
            Wire::new(
                4,
                vec![
                    sp.iter().find(|p| p.role == PinRole::Ground).unwrap().pos,
                    gp,
                ],
            ),
        ];
        let transient = solve_transient(&[src, r, c, gnd], &wires).expect("PWM RC transient");
        assert_eq!(transient.kind, TransientKind::PwmRc);
        assert!(transient.samples.iter().any(|sample| sample.source_v > 4.9));
        assert!(transient.samples.iter().any(|sample| sample.source_v < 0.1));
    }
}
