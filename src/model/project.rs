//! Runtime project document.
//!
//! This type owns user-created schematic data. It is deliberately free of
//! selections, tools, dialogs, caches, and simulation results. Persistence
//! continues through the schema-versioned `SavedCircuit` compatibility DTO.

use super::{Component, Counters, PinRef, SchematicAnnotations, Wire, component_pin_defs};
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
    }

    pub(crate) fn component(&self, id: u64) -> Option<usize> {
        self.component_by_id.get(&id).copied()
    }

    pub(crate) fn insert_component(&mut self, id: u64, index: usize) {
        self.component_by_id.insert(id, index);
    }

    pub(crate) fn remove_component(&mut self, id: u64, moved: Option<(u64, usize)>) {
        self.component_by_id.remove(&id);
        if let Some((moved_id, index)) = moved {
            self.component_by_id.insert(moved_id, index);
        }
    }

    pub(crate) fn wire(&self, id: u64) -> Option<usize> {
        self.wire_by_id.get(&id).copied()
    }

    pub(crate) fn insert_wire(&mut self, id: u64, index: usize) {
        self.wire_by_id.insert(id, index);
    }

    pub(crate) fn remove_wire(&mut self, id: u64, moved: Option<(u64, usize)>) {
        self.wire_by_id.remove(&id);
        if let Some((moved_id, index)) = moved {
            self.wire_by_id.insert(moved_id, index);
        }
    }

    #[allow(dead_code)] // Used by the junction edit path as that UI is completed.
    pub(crate) fn junction(&self, id: JunctionId) -> Option<usize> {
        self.junction_by_id.get(&id).copied()
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
    }
}

/// Runtime attachment graph used by move/rotate and endpoint editing. It is
/// deliberately separate from electrical connectivity: it answers which
/// saved wire endpoints follow which physical symbol pins.
#[derive(Debug, Clone, Default)]
pub(crate) struct AttachmentIndex {
    wires_by_component: HashMap<u64, Vec<u64>>,
    wires_by_pin: HashMap<PinRef, Vec<u64>>,
    endpoint_pins_by_wire: HashMap<u64, Vec<PinRef>>,
}

impl AttachmentIndex {
    pub(crate) fn rebuild(&mut self, components: &[Component], wires: &[Wire]) {
        self.wires_by_component.clear();
        self.wires_by_pin.clear();
        self.endpoint_pins_by_wire.clear();
        let pin_grid = pin_grid(components);
        for wire in wires {
            self.add_wire_with_grid(wire, &pin_grid);
        }
    }

    fn add_wire_with_grid(
        &mut self,
        wire: &Wire,
        pin_grid: &HashMap<(i32, i32), Vec<(PinRef, egui::Pos2)>>,
    ) {
        let endpoints = [
            (&wire.start, wire.points.first().copied()),
            (&wire.end, wire.points.last().copied()),
        ];
        let mut pins = Vec::new();
        for (endpoint, position) in endpoints {
            let pin = match endpoint {
                WireEndpoint::Pin(pin) => Some(pin.clone()),
                _ => position.and_then(|position| nearest_pin(position, pin_grid, 20.0)),
            };
            let Some(pin) = pin else {
                continue;
            };
            self.wires_by_component
                .entry(pin.component_id)
                .or_default()
                .push(wire.id);
            self.wires_by_pin
                .entry(pin.clone())
                .or_default()
                .push(wire.id);
            pins.push(pin);
        }
        pins.sort_by(|left, right| {
            (left.component_id, left.pin_name.as_str())
                .cmp(&(right.component_id, right.pin_name.as_str()))
        });
        pins.dedup();
        if !pins.is_empty() {
            self.endpoint_pins_by_wire.insert(wire.id, pins);
        }
        dedup_map_values(&mut self.wires_by_component);
        dedup_map_values(&mut self.wires_by_pin);
    }

    pub(crate) fn add_wire(&mut self, wire: &Wire, components: &[Component]) {
        self.remove_wire(wire.id);
        self.add_wire_with_grid(wire, &pin_grid(components));
    }

    pub(crate) fn remove_wire(&mut self, wire_id: u64) {
        self.endpoint_pins_by_wire.remove(&wire_id);
        self.wires_by_component.retain(|_, wires| {
            wires.retain(|id| *id != wire_id);
            !wires.is_empty()
        });
        self.wires_by_pin.retain(|_, wires| {
            wires.retain(|id| *id != wire_id);
            !wires.is_empty()
        });
    }

    pub(crate) fn remove_component(&mut self, component_id: u64) {
        let wire_ids = self
            .wires_by_component
            .remove(&component_id)
            .unwrap_or_default();
        self.wires_by_pin
            .retain(|pin, _| pin.component_id != component_id);
        for wire_id in wire_ids {
            let Some(pins) = self.endpoint_pins_by_wire.get_mut(&wire_id) else {
                continue;
            };
            pins.retain(|pin| pin.component_id != component_id);
            if pins.is_empty() {
                self.endpoint_pins_by_wire.remove(&wire_id);
            }
        }
    }

    pub(crate) fn attached_wires(&self, component_id: u64) -> &[u64] {
        self.wires_by_component
            .get(&component_id)
            .map_or(&[], Vec::as_slice)
    }

    #[allow(dead_code)] // Exposed for pin-level inspection and endpoint diagnostics.
    pub(crate) fn attached_wires_for_pin(&self, pin: &PinRef) -> &[u64] {
        self.wires_by_pin.get(pin).map_or(&[], Vec::as_slice)
    }

    #[allow(dead_code)] // Exposed for pin-level inspection and endpoint diagnostics.
    pub(crate) fn endpoint_pins(&self, wire_id: u64) -> &[PinRef] {
        self.endpoint_pins_by_wire
            .get(&wire_id)
            .map_or(&[], Vec::as_slice)
    }

    #[cfg(test)]
    pub(crate) fn is_consistent(&self, components: &[Component], wires: &[Wire]) -> bool {
        let mut expected = Self::default();
        expected.rebuild(components, wires);
        self.wires_by_component == expected.wires_by_component
            && self.wires_by_pin == expected.wires_by_pin
            && self.endpoint_pins_by_wire == expected.endpoint_pins_by_wire
    }
}

fn pin_grid(components: &[Component]) -> HashMap<(i32, i32), Vec<(PinRef, egui::Pos2)>> {
    let mut grid = HashMap::new();
    for component in components {
        for pin in component_pin_defs(component) {
            grid.entry(pin_cell(pin.pos))
                .or_insert_with(Vec::new)
                .push((
                    PinRef {
                        component_id: component.id,
                        pin_name: pin.label.to_string(),
                    },
                    pin.pos,
                ));
        }
    }
    grid
}

fn nearest_pin(
    position: egui::Pos2,
    grid: &HashMap<(i32, i32), Vec<(PinRef, egui::Pos2)>>,
    tolerance: f32,
) -> Option<PinRef> {
    let origin = pin_cell(position);
    let mut best: Option<(&PinRef, f32)> = None;
    for x in (origin.0 - 1)..=(origin.0 + 1) {
        for y in (origin.1 - 1)..=(origin.1 + 1) {
            for (pin, pin_position) in grid.get(&(x, y)).into_iter().flatten() {
                let distance = position.distance(*pin_position);
                if distance <= tolerance
                    && best.is_none_or(|(_, best_distance)| distance < best_distance)
                {
                    best = Some((pin, distance));
                }
            }
        }
    }
    best.map(|(pin, _)| pin.clone())
}

fn dedup_map_values<K: std::hash::Hash + Eq>(map: &mut HashMap<K, Vec<u64>>) {
    for values in map.values_mut() {
        values.sort_unstable();
        values.dedup();
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
        let mut attachments = AttachmentIndex::default();
        attachments.rebuild(&components, &wires);
        assert_eq!(attachments.attached_wires(1), &[10]);
        assert_eq!(attachments.endpoint_pins(10).len(), 2);
        assert_eq!(
            attachments.attached_wires_for_pin(&PinRef {
                component_id: 1,
                pin_name: "A".to_string(),
            }),
            &[10]
        );

        components.swap(0, 2);
        components.remove(1);
        components.push(component(4));
        wires.insert(0, Wire::new(11, vec![Pos2::ZERO, Pos2::new(1.0, 0.0)]));
        annotations.junction_dots.push(JunctionDot {
            id: JunctionId(31),
            position: Pos2::new(1.0, 0.0),
        });
        index.rebuild(&components, &wires, &annotations);
        attachments.rebuild(&components, &wires);
        assert!(index.is_consistent(&components, &wires, &annotations));
        assert!(attachments.is_consistent(&components, &wires));
        assert_eq!(index.component(3), Some(0));
        assert_eq!(index.component(4), Some(2));
        assert_eq!(index.wire(10), Some(1));
        assert_eq!(index.junction(JunctionId(31)), Some(1));
    }
}
