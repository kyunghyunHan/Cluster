//! Runtime project document.
//!
//! This type owns user-created schematic data. It is deliberately free of
//! selections, tools, dialogs, caches, and simulation results. Persistence
//! continues through the schema-versioned `SavedCircuit` compatibility DTO.

use super::{Component, Counters, SchematicAnnotations, Wire, component_pin_defs};
use super::{JunctionId, WireEndpoint};
use crate::pcb::board::Board;
use std::collections::HashMap;

/// Runtime-only direct lookup tables for schematic entities.
///
/// The vectors remain the source of truth for stable rendering and save order;
/// this index is rebuilt at document boundaries and updated by commands.
#[derive(Debug, Clone, Default)]
pub(crate) struct SchematicEntityIndex {
    component_by_id: HashMap<u64, usize>,
    wire_by_id: HashMap<u64, usize>,
    junction_by_id: HashMap<JunctionId, usize>,
    attached_wires_by_component: HashMap<u64, Vec<u64>>,
}

impl SchematicEntityIndex {
    pub(crate) fn rebuild(
        &mut self,
        components: &[Component],
        wires: &[Wire],
        annotations: &SchematicAnnotations,
    ) {
        self.component_by_id = components
            .iter()
            .enumerate()
            .map(|(index, component)| (component.id, index))
            .collect();
        self.wire_by_id = wires
            .iter()
            .enumerate()
            .map(|(index, wire)| (wire.id, index))
            .collect();
        self.junction_by_id = annotations
            .junction_dots
            .iter()
            .enumerate()
            .map(|(index, junction)| (junction.id, index))
            .collect();
        self.attached_wires_by_component.clear();
        let mut pin_components = HashMap::<(i32, i32), Vec<(u64, egui::Pos2)>>::new();
        for component in components {
            for pin in component_pin_defs(component) {
                pin_components
                    .entry(pin_cell(pin.pos))
                    .or_default()
                    .push((component.id, pin.pos));
            }
        }
        for wire in wires {
            for endpoint in [&wire.start, &wire.end] {
                if let WireEndpoint::Pin(pin) = endpoint {
                    self.attached_wires_by_component
                        .entry(pin.component_id)
                        .or_default()
                        .push(wire.id);
                }
            }
            for position in wire.points.first().into_iter().chain(wire.points.last()) {
                let origin = pin_cell(*position);
                for x in (origin.0 - 1)..=(origin.0 + 1) {
                    for y in (origin.1 - 1)..=(origin.1 + 1) {
                        for &(component_id, pin_position) in
                            pin_components.get(&(x, y)).into_iter().flatten()
                        {
                            if position.distance(pin_position) <= 20.0 {
                                self.attached_wires_by_component
                                    .entry(component_id)
                                    .or_default()
                                    .push(wire.id);
                            }
                        }
                    }
                }
            }
        }
        for wire_ids in self.attached_wires_by_component.values_mut() {
            wire_ids.sort_unstable();
            wire_ids.dedup();
        }
    }

    pub(crate) fn component(&self, id: u64) -> Option<usize> {
        self.component_by_id.get(&id).copied()
    }

    pub(crate) fn wire(&self, id: u64) -> Option<usize> {
        self.wire_by_id.get(&id).copied()
    }

    #[allow(dead_code)] // Used by the junction edit path as that UI is completed.
    pub(crate) fn junction(&self, id: JunctionId) -> Option<usize> {
        self.junction_by_id.get(&id).copied()
    }

    pub(crate) fn attached_wires(&self, component_id: u64) -> &[u64] {
        self.attached_wires_by_component
            .get(&component_id)
            .map_or(&[], Vec::as_slice)
    }

    #[cfg(test)]
    pub(crate) fn is_consistent(
        &self,
        components: &[Component],
        wires: &[Wire],
        annotations: &SchematicAnnotations,
    ) -> bool {
        let mut expected = Self::default();
        expected.rebuild(components, wires, annotations);
        self.component_by_id == expected.component_by_id
            && self.wire_by_id == expected.wire_by_id
            && self.junction_by_id == expected.junction_by_id
            && self.attached_wires_by_component == expected.attached_wires_by_component
    }
}

fn pin_cell(position: egui::Pos2) -> (i32, i32) {
    (
        (position.x / 32.0).floor() as i32,
        (position.y / 32.0).floor() as i32,
    )
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DocumentRevisions {
    pub persistence: u64,
    pub schematic_geometry: u64,
    pub schematic_connectivity: u64,
    pub electrical_parameters: u64,
    pub simulation_topology: u64,
    pub simulation_parameters: u64,
    pub board_topology: u64,
    pub board_geometry: u64,
    pub board_rules: u64,
    pub visual: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProjectPage {
    pub(crate) name: String,
    pub(crate) components: Vec<Component>,
    pub(crate) wires: Vec<Wire>,
    pub(crate) next_id: u64,
    pub(crate) counters: Counters,
    pub(crate) annotations: SchematicAnnotations,
}

impl ProjectPage {
    pub(crate) fn empty(name: String) -> Self {
        Self {
            name,
            components: Vec::new(),
            wires: Vec::new(),
            next_id: 1,
            counters: Counters::default(),
            annotations: SchematicAnnotations::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectDocument {
    pub(crate) components: Vec<Component>,
    pub(crate) wires: Vec<Wire>,
    pub(crate) next_id: u64,
    pub(crate) counters: Counters,
    pub(crate) annotations: SchematicAnnotations,
    pub(crate) pages: Vec<ProjectPage>,
    pub(crate) current_page: usize,
    pub(crate) board: Board,
}

impl Default for ProjectDocument {
    fn default() -> Self {
        Self {
            components: Vec::new(),
            wires: Vec::new(),
            next_id: 1,
            counters: Counters::default(),
            annotations: SchematicAnnotations::default(),
            pages: vec![ProjectPage::empty("Page 1".to_string())],
            current_page: 0,
            board: Board::new_two_layer(80.0, 50.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ComponentKind, JunctionDot, PinRef};
    use egui::Pos2;

    fn component(id: u64) -> Component {
        Component {
            id,
            kind: ComponentKind::Resistor,
            pos: Pos2::new(id as f32 * 20.0, 0.0),
            rotation: 0,
            label: format!("R{id}"),
            value: "1k".to_string(),
            part_id: None,
        }
    }

    #[test]
    fn schematic_entity_index_survives_insert_delete_reorder_and_rebuild() {
        let mut components = vec![component(1), component(2), component(3)];
        let mut wires = vec![Wire::new(10, vec![Pos2::ZERO, Pos2::new(20.0, 0.0)])];
        wires[0].start = WireEndpoint::Pin(PinRef {
            component_id: 1,
            pin_name: "A".to_string(),
        });
        let mut annotations = SchematicAnnotations {
            junction_dots: vec![JunctionDot {
                id: JunctionId(30),
                position: Pos2::new(10.0, 0.0),
            }],
            ..Default::default()
        };
        let mut index = SchematicEntityIndex::default();
        index.rebuild(&components, &wires, &annotations);
        assert_eq!(index.component(2), Some(1));
        assert_eq!(index.wire(10), Some(0));
        assert_eq!(index.junction(JunctionId(30)), Some(0));
        assert_eq!(index.attached_wires(1), &[10]);

        components.swap(0, 2);
        components.remove(1);
        components.push(component(4));
        wires.insert(0, Wire::new(11, vec![Pos2::ZERO, Pos2::X]));
        annotations.junction_dots.push(JunctionDot {
            id: JunctionId(31),
            position: Pos2::X,
        });
        index.rebuild(&components, &wires, &annotations);
        assert!(index.is_consistent(&components, &wires, &annotations));
        assert_eq!(index.component(3), Some(0));
        assert_eq!(index.component(4), Some(2));
        assert_eq!(index.wire(10), Some(1));
        assert_eq!(index.junction(JunctionId(31)), Some(1));
    }
}
