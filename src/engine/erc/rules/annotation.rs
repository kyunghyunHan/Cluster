use crate::engine::validation::{ErcRule, ErcSeverity, ErcViolation};
use crate::model::{CircuitNetlist, ComponentKind};
use std::collections::{HashMap, HashSet};

pub(crate) fn check_duplicate_references(
    netlist: &CircuitNetlist,
    violations: &mut Vec<ErcViolation>,
) {
    let mut references: HashMap<&str, HashSet<u64>> = HashMap::new();
    for pin in &netlist.pins {
        if matches!(
            pin.component_kind,
            ComponentKind::NetLabel | ComponentKind::TextNote
        ) {
            continue;
        }
        references
            .entry(pin.component_label.as_str())
            .or_default()
            .insert(pin.component_id);
    }
    for (reference, ids) in references {
        if reference.trim().is_empty() || ids.len() <= 1 {
            continue;
        }
        violations.push(ErcViolation {
            rule: ErcRule::DuplicateReference,
            severity: ErcSeverity::Error,
            component_id: ids.iter().copied().min(),
            wire_id: None,
            message: format!(
                "Duplicate reference {reference}: {} components share the same designator. Rename or re-annotate the schematic.",
                ids.len()
            ),
        });
    }
}

pub(crate) fn check_duplicate_named_nets(
    netlist: &CircuitNetlist,
    violations: &mut Vec<ErcViolation>,
) {
    let mut names: HashMap<String, Vec<usize>> = HashMap::new();
    for net in &netlist.nets {
        if net.name.starts_with("NET_") {
            continue;
        }
        names
            .entry(net.name.trim().to_ascii_uppercase())
            .or_default()
            .push(net.id);
    }
    for (name, ids) in names {
        if name.is_empty() || ids.len() <= 1 {
            continue;
        }
        violations.push(ErcViolation {
            rule: ErcRule::DuplicateNamedNet,
            severity: ErcSeverity::Error,
            component_id: None,
            wire_id: None,
            message: format!(
                "Duplicate net name {name}: the same named net exists on {} disconnected islands. Add a wire/junction or rename one label.",
                ids.len()
            ),
        });
    }
}

pub(crate) fn check_no_connect_pins(netlist: &CircuitNetlist, violations: &mut Vec<ErcViolation>) {
    for pin in netlist
        .pins
        .iter()
        .filter(|pin| pin.no_connect && pin.connected_by_wire)
    {
        violations.push(ErcViolation {
            rule: ErcRule::NoConnectWired,
            severity: ErcSeverity::Error,
            component_id: Some(pin.component_id),
            wire_id: None,
            message: format!(
                "{} pin {} is marked no-connect but is wired to {}. Remove the wire or remove the no-connect marker.",
                pin.component_label,
                pin.pin_name,
                netlist
                    .nets
                    .iter()
                    .find(|net| net.id == pin.net_id)
                    .map(|net| net.name.as_str())
                    .unwrap_or("the net")
            ),
        });
    }
}
