use crate::model::*;
use crate::parse_metric_value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ErcSeverity {
    Error,
    Warning,
    #[allow(dead_code)]
    Info,
}

#[derive(Debug, Clone)]
pub(crate) struct ErcViolation {
    pub(crate) severity: ErcSeverity,
    pub(crate) component_id: Option<u64>,
    pub(crate) wire_id: Option<u64>,
    pub(crate) message: String,
}

pub(crate) fn validate_beginner_rules(netlist: &CircuitNetlist) -> Vec<ErcViolation> {
    let mut v = Vec::new();

    check_missing_gnd(netlist, &mut v);
    check_power_gnd_short(netlist, &mut v);
    check_led_without_resistor(netlist, &mut v);
    check_reversed_led(netlist, &mut v);
    check_reversed_diode(netlist, &mut v);
    check_5v_on_3v3(netlist, &mut v);
    check_gpio_drives_motor(netlist, &mut v);
    check_oled_sda_scl_swap(netlist, &mut v);
    check_missing_values(netlist, &mut v);
    check_floating_pins(netlist, &mut v);

    v
}

// ─── Individual rule implementations ────────────────────────────────────────

fn check_missing_gnd(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    if netlist.pins.is_empty() {
        return;
    }
    let has_gnd = netlist.nets.iter().any(|net| net.name == "GND")
        || netlist.pins.iter().any(|pin| {
            pin.electrical_type == ElectricalType::Ground
                || pin.component_kind == ComponentKind::Ground
        });
    if !has_gnd {
        v.push(ErcViolation {
            severity: ErcSeverity::Error,
            component_id: None,
            wire_id: None,
            message: "No GND reference found. Add a Ground symbol to your circuit.".to_string(),
        });
    }
}

fn check_power_gnd_short(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        let pins: Vec<&NetlistPin> = netlist
            .pins
            .iter()
            .filter(|p| p.net_id == net.id)
            .collect();

        let power_pin = pins.iter().find(|p| pin_is_power_positive(p));
        let gnd_pin = pins.iter().find(|p| {
            p.electrical_type == ElectricalType::Ground
                || p.component_kind == ComponentKind::Ground
        });

        if let (Some(pwr), Some(gnd)) = (power_pin, gnd_pin) {
            v.push(ErcViolation {
                severity: ErcSeverity::Error,
                component_id: Some(pwr.component_id),
                wire_id: None,
                message: format!(
                    "Power short: {} {} is directly connected to GND ({}).",
                    pwr.component_label, pwr.pin_name, gnd.component_label
                ),
            });
        }
    }
}

fn check_led_without_resistor(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for led in netlist
        .pins
        .iter()
        .filter(|p| p.component_kind == ComponentKind::Led && p.pin_name == "A")
    {
        let led_nets: std::collections::HashSet<usize> = netlist
            .pins
            .iter()
            .filter(|p| p.component_id == led.component_id)
            .map(|p| p.net_id)
            .collect();

        let has_resistor = netlist.pins.iter().any(|p| {
            p.component_kind == ComponentKind::Resistor && led_nets.contains(&p.net_id)
        });
        if !has_resistor {
            v.push(ErcViolation {
                severity: ErcSeverity::Warning,
                component_id: Some(led.component_id),
                wire_id: None,
                message: format!(
                    "LED {} has no current limiting resistor on either terminal.",
                    led.component_label
                ),
            });
        }
    }
}

fn check_reversed_led(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        if net.name != "GND" {
            continue;
        }
        let pins: Vec<&NetlistPin> = netlist
            .pins
            .iter()
            .filter(|p| p.net_id == net.id)
            .collect();
        for pin in &pins {
            if pin.component_kind == ComponentKind::Led && pin.pin_name == "A" {
                v.push(ErcViolation {
                    severity: ErcSeverity::Error,
                    component_id: Some(pin.component_id),
                    wire_id: None,
                    message: format!(
                        "LED {} anode (A) is connected to GND — LED is reversed.",
                        pin.component_label
                    ),
                });
            }
        }
    }
}

fn check_reversed_diode(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    let diode_kinds = [
        ComponentKind::Diode,
        ComponentKind::SchottkyDiode,
        ComponentKind::ZenerDiode,
    ];
    for net in &netlist.nets {
        if net.name != "GND" {
            continue;
        }
        let pins: Vec<&NetlistPin> = netlist
            .pins
            .iter()
            .filter(|p| p.net_id == net.id)
            .collect();
        for pin in &pins {
            if diode_kinds.contains(&pin.component_kind) && pin.pin_name == "A" {
                // Anode on GND net — likely reversed if cathode is on a higher potential net.
                let cathode_net = netlist
                    .pins
                    .iter()
                    .find(|p| {
                        p.component_id == pin.component_id
                            && (p.pin_name == "K" || p.pin_name == "B")
                    })
                    .map(|p| p.net_id);
                let cathode_is_power = cathode_net.is_some_and(|nid| {
                    netlist.pins.iter().any(|p| {
                        p.net_id == nid && pin_is_power_positive(p)
                    })
                });
                if cathode_is_power {
                    v.push(ErcViolation {
                        severity: ErcSeverity::Error,
                        component_id: Some(pin.component_id),
                        wire_id: None,
                        message: format!(
                            "Diode {} appears reversed (anode at GND, cathode at power).",
                            pin.component_label
                        ),
                    });
                }
            }
        }
    }
}

fn check_5v_on_3v3(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        let pins: Vec<&NetlistPin> = netlist
            .pins
            .iter()
            .filter(|p| p.net_id == net.id)
            .collect();
        let has_5v = pins.iter().any(|p| pin_is_5v_source(p));
        let target_3v3 = pins.iter().find(|p| pin_name_is_3v3(&p.pin_name));
        if has_5v {
            if let Some(target) = target_3v3 {
                v.push(ErcViolation {
                    severity: ErcSeverity::Error,
                    component_id: Some(target.component_id),
                    wire_id: None,
                    message: format!(
                        "{} connects 5V to a 3.3V rail/pin.",
                        net.name
                    ),
                });
            }
        }

        // ESP32 recommended I2C pins
        for gpio in pins
            .iter()
            .filter(|p| pin_is_microcontroller_gpio(p) && !pin_is_i2c_named(&p.pin_name))
        {
            if pins
                .iter()
                .any(|p| p.component_kind == ComponentKind::DcMotor)
            {
                v.push(ErcViolation {
                    severity: ErcSeverity::Error,
                    component_id: Some(gpio.component_id),
                    wire_id: None,
                    message: format!(
                        "{} {} is connected directly to a motor. Use a transistor, relay, or driver.",
                        gpio.component_label, gpio.pin_name
                    ),
                });
            }
        }
    }
}

fn check_gpio_drives_motor(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        let pins: Vec<&NetlistPin> = netlist
            .pins
            .iter()
            .filter(|p| p.net_id == net.id)
            .collect();
        let has_motor = pins.iter().any(|p| p.component_kind == ComponentKind::DcMotor);
        if !has_motor {
            continue;
        }
        for gpio in pins.iter().filter(|p| pin_is_microcontroller_gpio(p)) {
            // Already reported in check_5v_on_3v3 path, but also catch non-5V cases
            if !pins.iter().any(|p| pin_is_5v_source(p)) {
                v.push(ErcViolation {
                    severity: ErcSeverity::Warning,
                    component_id: Some(gpio.component_id),
                    wire_id: None,
                    message: format!(
                        "{} GPIO {} drives a motor directly. Use a driver IC or transistor.",
                        gpio.component_label, gpio.pin_name
                    ),
                });
            }
        }
    }
}

fn check_oled_sda_scl_swap(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        let pins: Vec<&NetlistPin> = netlist
            .pins
            .iter()
            .filter(|p| p.net_id == net.id)
            .collect();

        let oled_sda = pins.iter().any(|p| {
            p.component_kind == ComponentKind::Oled && p.pin_name.eq_ignore_ascii_case("SDA")
        });
        let oled_scl = pins.iter().any(|p| {
            p.component_kind == ComponentKind::Oled && p.pin_name.eq_ignore_ascii_case("SCL")
        });

        if oled_sda
            && pins
                .iter()
                .any(|p| pin_is_controller_scl(p) && !pin_is_controller_sda(p))
        {
            v.push(ErcViolation {
                severity: ErcSeverity::Error,
                component_id: pins
                    .iter()
                    .find(|p| p.component_kind == ComponentKind::Oled)
                    .map(|p| p.component_id),
                wire_id: None,
                message: "OLED SDA is connected to a controller SCL pin.".to_string(),
            });
        }
        if oled_scl
            && pins
                .iter()
                .any(|p| pin_is_controller_sda(p) && !pin_is_controller_scl(p))
        {
            v.push(ErcViolation {
                severity: ErcSeverity::Error,
                component_id: pins
                    .iter()
                    .find(|p| p.component_kind == ComponentKind::Oled)
                    .map(|p| p.component_id),
                wire_id: None,
                message: "OLED SCL is connected to a controller SDA pin.".to_string(),
            });
        }
    }
}

fn check_missing_values(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    let reported: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut seen = reported;

    for pin in &netlist.pins {
        if seen.contains(&pin.component_id) {
            continue;
        }
        match pin.component_kind {
            ComponentKind::Resistor => {
                if pin.component_value.trim().is_empty()
                    || parse_metric_value(&pin.component_value, "ohm").is_none()
                {
                    seen.insert(pin.component_id);
                    v.push(ErcViolation {
                        severity: ErcSeverity::Warning,
                        component_id: Some(pin.component_id),
                        wire_id: None,
                        message: format!(
                            "Resistor {} has no valid resistance value.",
                            pin.component_label
                        ),
                    });
                }
            }
            ComponentKind::Battery | ComponentKind::VSource => {
                if pin.component_value.trim().is_empty()
                    || parse_metric_value(&pin.component_value, "v").is_none()
                {
                    seen.insert(pin.component_id);
                    v.push(ErcViolation {
                        severity: ErcSeverity::Warning,
                        component_id: Some(pin.component_id),
                        wire_id: None,
                        message: format!(
                            "{} {} has no valid voltage value.",
                            if pin.component_kind == ComponentKind::Battery {
                                "Battery"
                            } else {
                                "Voltage source"
                            },
                            pin.component_label
                        ),
                    });
                }
            }
            _ => {}
        }
    }
}

fn check_floating_pins(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    let connected_component_ids: std::collections::HashSet<u64> = netlist
        .pins
        .iter()
        .filter(|p| p.connected_by_wire)
        .map(|p| p.component_id)
        .collect();

    // Warn about components with no connected pins at all (fully unconnected).
    let mut component_net_counts: std::collections::HashMap<u64, std::collections::HashSet<usize>> =
        std::collections::HashMap::new();
    for pin in &netlist.pins {
        component_net_counts
            .entry(pin.component_id)
            .or_default()
            .insert(pin.net_id);
    }

    // Warn about floating wires (no component connection on either end).
    for wire_id in &netlist.floating_wires {
        v.push(ErcViolation {
            severity: ErcSeverity::Warning,
            component_id: None,
            wire_id: Some(*wire_id),
            message: "Wire is floating (not connected to any component pin).".to_string(),
        });
    }

    for wire_id in &netlist.isolated_wires {
        v.push(ErcViolation {
            severity: ErcSeverity::Warning,
            component_id: None,
            wire_id: Some(*wire_id),
            message: "Wire segment is isolated: it connects to only one component pin."
                .to_string(),
        });
    }

    let mut reported_components = std::collections::HashSet::new();
    for pin in &netlist.pins {
        if !connected_component_ids.contains(&pin.component_id)
            && !matches!(pin.component_kind, ComponentKind::Ground | ComponentKind::NetLabel)
            && reported_components.insert(pin.component_id)
        {
            v.push(ErcViolation {
                severity: ErcSeverity::Warning,
                component_id: Some(pin.component_id),
                wire_id: None,
                message: format!(
                    "{} {} has no connected wires.",
                    component_kind_short(pin.component_kind),
                    pin.component_label
                ),
            });
        } else if !pin.connected_by_wire
            && matches!(
                pin.electrical_type,
                ElectricalType::Digital
                    | ElectricalType::I2c
                    | ElectricalType::Control
            )
        {
            v.push(ErcViolation {
                severity: ErcSeverity::Warning,
                component_id: Some(pin.component_id),
                wire_id: None,
                message: format!(
                    "{} {} input/pin {} is floating.",
                    component_kind_short(pin.component_kind),
                    pin.component_label,
                    pin.pin_name
                ),
            });
        }
    }
}

// ─── Helper predicates ───────────────────────────────────────────────────────

fn pin_is_power_positive(pin: &NetlistPin) -> bool {
    matches!(
        pin.component_kind,
        ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource
    ) && (pin.electrical_type == ElectricalType::PowerIn
        || pin.pin_name == "+"
        || pin.pin_name.eq_ignore_ascii_case("POS"))
}

fn pin_is_5v_source(pin: &NetlistPin) -> bool {
    pin.pin_name.eq_ignore_ascii_case("5V")
        || (matches!(
            pin.component_kind,
            ComponentKind::VSource | ComponentKind::Battery
        ) && parse_metric_value(&pin.component_value, "v").is_some_and(|v| v > 3.6))
}

fn pin_name_is_3v3(name: &str) -> bool {
    let compact = name.to_ascii_uppercase().replace(['.', ' '], "");
    compact.contains("3V3") || compact.contains("3.3V")
}

pub(crate) fn pin_is_microcontroller_gpio(pin: &NetlistPin) -> bool {
    matches!(
        pin.component_kind,
        ComponentKind::Esp32
            | ComponentKind::Esp32S3
            | ComponentKind::Esp32C3
            | ComponentKind::ArduinoUno
            | ComponentKind::RaspberryPiPico
    ) && (pin.pin_name.to_ascii_uppercase().contains("GPIO")
        || pin.pin_name.to_ascii_uppercase().starts_with("GP")
        || pin.pin_name.to_ascii_uppercase().starts_with('D'))
}

pub(crate) fn pin_is_i2c_named(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("sda") || lower.contains("scl")
}

pub(crate) fn pin_is_controller_sda(pin: &NetlistPin) -> bool {
    pin_is_microcontroller(pin) && pin.pin_name.to_ascii_lowercase().contains("sda")
}

pub(crate) fn pin_is_controller_scl(pin: &NetlistPin) -> bool {
    pin_is_microcontroller(pin) && pin.pin_name.to_ascii_lowercase().contains("scl")
}

fn pin_is_microcontroller(pin: &NetlistPin) -> bool {
    matches!(
        pin.component_kind,
        ComponentKind::Esp32
            | ComponentKind::Esp32S3
            | ComponentKind::Esp32C3
            | ComponentKind::ArduinoUno
            | ComponentKind::RaspberryPiPico
    )
}

fn component_kind_short(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Resistor => "Resistor",
        ComponentKind::Capacitor => "Capacitor",
        ComponentKind::Led => "LED",
        ComponentKind::Diode => "Diode",
        ComponentKind::Battery => "Battery",
        ComponentKind::VSource => "Voltage source",
        ComponentKind::ISource => "Current source",
        ComponentKind::Esp32 | ComponentKind::Esp32S3 | ComponentKind::Esp32C3 => "ESP32",
        ComponentKind::ArduinoUno => "Arduino",
        ComponentKind::RaspberryPiPico => "Pico",
        ComponentKind::Oled => "OLED",
        _ => "Component",
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pin(
        component_id: u64,
        label: &str,
        kind: ComponentKind,
        value: &str,
        pin_name: &str,
        net_id: usize,
        connected_by_wire: bool,
    ) -> NetlistPin {
        NetlistPin {
            component_id,
            component_label: label.to_string(),
            component_kind: kind,
            component_value: value.to_string(),
            pin_name: pin_name.to_string(),
            electrical_type: ElectricalType::Passive,
            position: egui::Pos2::ZERO,
            net_id,
            connected_by_wire,
        }
    }

    fn make_net(id: usize, name: &str) -> Net {
        Net {
            id,
            name: name.to_string(),
            connected_pins: Vec::new(),
        }
    }

    fn netlist(nets: Vec<Net>, pins: Vec<NetlistPin>) -> CircuitNetlist {
        CircuitNetlist {
            nets,
            pins,
            wire_nets: Default::default(),
            floating_wires: Vec::new(),
            isolated_wires: Vec::new(),
        }
    }

    // ── LED without resistor ──────────────────────────────────────────────

    #[test]
    fn reports_led_without_resistor() {
        let nl = netlist(
            vec![make_net(0, "NET_001"), make_net(1, "GND")],
            vec![
                make_pin(1, "LED1", ComponentKind::Led, "red", "A", 0, true),
                make_pin(1, "LED1", ComponentKind::Led, "red", "K", 1, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(v.iter().any(|e| e.message.contains("current limiting resistor")));
    }

    // ── LED with resistor → no warning ───────────────────────────────────

    #[test]
    fn no_warning_when_led_has_resistor() {
        let nl = netlist(
            vec![make_net(0, "NET_001"), make_net(1, "GND")],
            vec![
                make_pin(1, "LED1", ComponentKind::Led, "red", "A", 0, true),
                make_pin(1, "LED1", ComponentKind::Led, "red", "K", 1, true),
                make_pin(2, "R1", ComponentKind::Resistor, "330", "A", 0, true),
                make_pin(2, "R1", ComponentKind::Resistor, "330", "B", 2, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(!v.iter().any(|e| e.message.contains("current limiting resistor")));
    }

    // ── Missing GND ───────────────────────────────────────────────────────

    #[test]
    fn reports_missing_gnd() {
        let nl = netlist(
            vec![make_net(0, "NET_001")],
            vec![
                make_pin(1, "R1", ComponentKind::Resistor, "1k", "A", 0, true),
                make_pin(1, "R1", ComponentKind::Resistor, "1k", "B", 0, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(v.iter().any(|e| e.message.contains("GND")));
    }

    // ── Power-GND short ───────────────────────────────────────────────────

    #[test]
    fn reports_power_gnd_short() {
        let nl = netlist(
            vec![make_net(0, "GND")],
            vec![
                make_pin(1, "BAT1", ComponentKind::Battery, "9V", "+", 0, true),
                make_pin(2, "GND1", ComponentKind::Ground, "0V", "GND", 0, true),
            ],
        );
        // Mark battery + as PowerIn
        let mut nl2 = nl;
        nl2.pins[0].electrical_type = ElectricalType::PowerIn;
        nl2.pins[1].electrical_type = ElectricalType::Ground;
        let v = validate_beginner_rules(&nl2);
        assert!(v.iter().any(|e| e.severity == ErcSeverity::Error && e.message.contains("short")));
    }

    // ── 5V on 3.3V pin ───────────────────────────────────────────────────

    #[test]
    fn reports_5v_on_3v3() {
        let nl = netlist(
            vec![make_net(0, "NET_001")],
            vec![
                make_pin(1, "ARD1", ComponentKind::ArduinoUno, "", "5V", 0, true),
                make_pin(2, "ESP1", ComponentKind::Esp32, "", "3V3", 0, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(v.iter().any(|e| e.severity == ErcSeverity::Error));
    }

    // ── Reversed LED ─────────────────────────────────────────────────────

    #[test]
    fn reports_reversed_led() {
        let nl = netlist(
            vec![make_net(0, "GND"), make_net(1, "VCC")],
            vec![
                // Anode (A) on GND net
                make_pin(1, "LED1", ComponentKind::Led, "red", "A", 0, true),
                make_pin(1, "LED1", ComponentKind::Led, "red", "K", 1, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(v.iter().any(|e| e.message.contains("reversed")));
    }

    // ── Resistor without value ────────────────────────────────────────────

    #[test]
    fn reports_resistor_without_value() {
        let nl = netlist(
            vec![make_net(0, "GND"), make_net(1, "NET_001")],
            vec![
                make_pin(1, "R1", ComponentKind::Resistor, "", "A", 0, true),
                make_pin(1, "R1", ComponentKind::Resistor, "", "B", 1, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(v.iter().any(|e| e.message.contains("resistance value")));
    }

    // ── Battery without voltage ───────────────────────────────────────────

    #[test]
    fn reports_battery_without_voltage() {
        let nl = netlist(
            vec![make_net(0, "GND"), make_net(1, "VCC")],
            vec![
                make_pin(1, "BAT1", ComponentKind::Battery, "", "+", 1, true),
                make_pin(1, "BAT1", ComponentKind::Battery, "", "-", 0, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(v.iter().any(|e| e.message.contains("voltage value")));
    }

    // ── Floating wire ─────────────────────────────────────────────────────

    #[test]
    fn reports_floating_wire() {
        let mut nl = netlist(
            vec![make_net(0, "NET_001")],
            vec![],
        );
        nl.floating_wires.push(42);
        let v = validate_beginner_rules(&nl);
        assert!(v.iter().any(|e| e.wire_id == Some(42)));
    }

    // ── OLED SDA/SCL swap ────────────────────────────────────────────────

    #[test]
    fn reports_oled_sda_scl_swap() {
        let nl = netlist(
            vec![make_net(0, "NET_001")],
            vec![
                make_pin(1, "OLED1", ComponentKind::Oled, "0.96 I2C", "SDA", 0, true),
                make_pin(2, "ESP1", ComponentKind::Esp32, "", "GPIO22_SCL", 0, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(v.iter().any(|e| e.message.contains("SDA") && e.message.contains("SCL")));
    }
}
