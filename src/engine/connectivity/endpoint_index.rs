use crate::model::{Component, PinRef, component_pin_defs};
use egui::Pos2;
use std::collections::HashMap;

/// Resolved pin endpoints keyed by stable schematic identity.
pub(in crate::engine) struct EndpointIndex {
    pin_positions: HashMap<PinRef, Pos2>,
}

impl EndpointIndex {
    pub(in crate::engine) fn new(components: &[Component]) -> Self {
        let mut pin_positions = HashMap::new();
        for component in components {
            for pin in component_pin_defs(component) {
                pin_positions.insert(
                    PinRef {
                        component_id: component.id,
                        pin_name: pin.label.to_string(),
                    },
                    pin.pos,
                );
            }
        }
        Self { pin_positions }
    }

    pub(in crate::engine) fn pin_position(&self, pin: &PinRef) -> Option<Pos2> {
        self.pin_positions.get(pin).copied()
    }
}
