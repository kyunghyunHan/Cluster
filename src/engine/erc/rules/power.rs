use crate::engine::validation::{ErcRule, ErcSeverity, ErcViolation};
use crate::model::{CircuitNetlist, ComponentKind, ElectricalType};

pub(crate) fn check_missing_ground(netlist: &CircuitNetlist, violations: &mut Vec<ErcViolation>) {
    if netlist.pins.is_empty() {
        return;
    }
    let has_ground = netlist.nets.iter().any(|net| net.name == "GND")
        || netlist.pins.iter().any(|pin| {
            pin.electrical_type == ElectricalType::Ground
                || pin.component_kind == ComponentKind::Ground
        });
    if !has_ground {
        violations.push(ErcViolation {
            rule: ErcRule::MissingGround,
            severity: ErcSeverity::Error,
            component_id: None,
            wire_id: None,
            message: "No GND reference found. Add a Ground symbol to your circuit.".to_string(),
        });
    }
}
