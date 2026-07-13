#![allow(dead_code)]

use crate::engine::mna::parse_si_value;
use crate::engine::netlist::build_circuit_netlist;
use crate::model::component_pin_defs;
use crate::model::{Component, ComponentKind, Wire};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NgspiceResult {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) operating_point: HashMap<String, f64>,
}

pub(crate) fn export_spice_netlist(components: &[Component], wires: &[Wire]) -> String {
    let netlist = build_circuit_netlist(components, wires);
    export_spice_netlist_with_netlist(components, &netlist)
}

pub(crate) fn export_spice_netlist_with_netlist(
    components: &[Component],
    netlist: &crate::model::CircuitNetlist,
) -> String {
    let net_names = netlist
        .nets
        .iter()
        .map(|net| {
            let name = if net.name.eq_ignore_ascii_case("GND") {
                "0".to_string()
            } else {
                sanitize_node_name(&net.name)
            };
            (net.id, name)
        })
        .collect::<HashMap<_, _>>();

    let mut out = String::from("* Cluster educational SPICE export\n");
    out.push_str(
        "* Built-in Cluster simulation is DC-only and simplified; use ngspice for real analysis.\n",
    );
    let mut primitive_count = 0usize;

    for component in components {
        let pins = component_pin_defs(component);
        let pin_net = |label: &str| {
            netlist
                .pins
                .iter()
                .find(|pin| pin.component_id == component.id && pin.pin_name == label)
                .and_then(|pin| net_names.get(&pin.net_id))
                .cloned()
                .unwrap_or_else(|| "NC".to_string())
        };
        match component.kind {
            ComponentKind::Resistor => {
                primitive_count += 1;
                out.push_str(&format!(
                    "R{} {} {} {}\n",
                    spice_ref(&component.label, "R"),
                    pin_net("A"),
                    pin_net("B"),
                    spice_value(&component.value, "1k")
                ));
            }
            ComponentKind::Capacitor => {
                primitive_count += 1;
                out.push_str(&format!(
                    "C{} {} {} {}\n",
                    spice_ref(&component.label, "C"),
                    pin_net("A"),
                    pin_net("B"),
                    spice_value(&component.value, "100n")
                ));
            }
            ComponentKind::Inductor => {
                primitive_count += 1;
                out.push_str(&format!(
                    "L{} {} {} {}\n",
                    spice_ref(&component.label, "L"),
                    pin_net("A"),
                    pin_net("B"),
                    spice_value(&component.value, "10u")
                ));
            }
            ComponentKind::Battery | ComponentKind::VSource => {
                primitive_count += 1;
                out.push_str(&format!(
                    "V{} {} {} DC {}\n",
                    spice_ref(&component.label, "V"),
                    pin_net("+"),
                    pin_net("-"),
                    parse_si_value(&component.value).unwrap_or(5.0)
                ));
            }
            ComponentKind::ISource => {
                primitive_count += 1;
                out.push_str(&format!(
                    "I{} {} {} DC {}\n",
                    spice_ref(&component.label, "I"),
                    pin_net("+"),
                    pin_net("-"),
                    parse_si_value(&component.value).unwrap_or(0.001)
                ));
            }
            ComponentKind::Diode | ComponentKind::Led | ComponentKind::SchottkyDiode => {
                primitive_count += 1;
                let model = match component.kind {
                    ComponentKind::Led => "DLED",
                    ComponentKind::SchottkyDiode => "DSCH",
                    _ => "DGEN",
                };
                out.push_str(&format!(
                    "D{} {} {} {}\n",
                    spice_ref(&component.label, "D"),
                    pin_net("A"),
                    pin_net("K"),
                    model
                ));
            }
            _ if !pins.is_empty() => {
                out.push_str(&format!(
                    "* {} omitted: no SPICE model assigned for {:?}\n",
                    component.label, component.kind
                ));
            }
            _ => {}
        }
    }

    if primitive_count == 0 {
        out.push_str("* No supported SPICE primitives in schematic.\n");
    }
    out.push_str(".model DGEN D(Is=1e-14 Rs=1 N=1.8)\n");
    out.push_str(".model DLED D(Is=1e-20 Rs=8 N=2.2 Vfwd=1.8)\n");
    out.push_str(".model DSCH D(Is=1e-8 Rs=0.2 N=1.05)\n");
    out.push_str(".op\n.end\n");
    out
}

pub(crate) fn run_ngspice_batch(netlist_path: &Path) -> Result<NgspiceResult, String> {
    let output = Command::new("ngspice")
        .arg("-b")
        .arg(netlist_path)
        .output()
        .map_err(|error| format!("ngspice is not installed or not on PATH: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(format!("ngspice failed: {}", stderr.trim()));
    }
    let operating_point = parse_operating_point(&stdout);
    Ok(NgspiceResult {
        stdout,
        stderr,
        operating_point,
    })
}

pub(crate) fn parse_operating_point(output: &str) -> HashMap<String, f64> {
    let mut values = HashMap::new();
    for line in output.lines() {
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else { continue };
        let Some(value) = fields.next() else { continue };
        if let Ok(parsed) = value.parse::<f64>() {
            values.insert(name.trim().to_string(), parsed);
        }
    }
    values
}

fn sanitize_node_name(name: &str) -> String {
    let mut out = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if out.is_empty() || out.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        out.insert(0, 'N');
    }
    out
}

fn spice_ref(label: &str, prefix: &str) -> String {
    let stripped = label
        .strip_prefix(prefix)
        .unwrap_or(label)
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    if stripped.is_empty() {
        "1".to_string()
    } else {
        stripped
    }
}

fn spice_value(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.replace('Ω', "ohm")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;

    fn comp(id: u64, kind: ComponentKind, pos: Pos2, label: &str, value: &str) -> Component {
        Component {
            id,
            kind,
            pos,
            rotation: 0,
            label: label.to_string(),
            value: value.to_string(),
            part_id: None,
        }
    }

    #[test]
    fn spice_export_contains_supported_primitives_and_op() {
        let r = comp(
            1,
            ComponentKind::Resistor,
            Pos2::new(100.0, 100.0),
            "R1",
            "1k",
        );
        let v = comp(
            2,
            ComponentKind::Battery,
            Pos2::new(0.0, 100.0),
            "BAT1",
            "5V",
        );
        let wires = Vec::new();

        let netlist = export_spice_netlist(&[r, v], &wires);

        assert!(netlist.contains("R1"));
        assert!(netlist.contains("VBAT1"));
        assert!(netlist.contains(".op"));
        assert!(netlist.contains("educational"));
    }

    #[test]
    fn parses_ngspice_operating_point_table_lines() {
        let parsed = parse_operating_point("v(out) 3.300000e+00\n@r1[i] 1.2e-03\n");

        assert_eq!(parsed["v(out)"], 3.3);
        assert_eq!(parsed["@r1[i]"], 0.0012);
    }
}
