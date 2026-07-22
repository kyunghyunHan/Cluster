use crate::model::{Component, PinRef, Wire, component_pin_defs, component_size};
use egui::{Pos2, Rect, Vec2};
use std::collections::{HashMap, HashSet};

const CELL: f32 = 64.0;

#[derive(Debug, Clone, PartialEq)]
struct IndexedPin {
    reference: PinRef,
    position: Pos2,
}

/// Shared editor geometry index for viewport culling, hit-testing, snapping,
/// wiring commands and selection. It is derived from the document revision;
/// canonical connectivity keeps its independent reference builder so the two
/// implementations can be compared in differential tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct WireSegmentRef {
    pub(crate) wire_id: u64,
    pub(crate) segment_index: usize,
}

#[derive(Default)]
pub(crate) struct SchematicSpatialIndex {
    wire_segment_buckets: HashMap<(i32, i32), Vec<WireSegmentRef>>,
    cells_by_wire: HashMap<u64, Vec<(i32, i32)>>,
    geometry_by_wire: HashMap<u64, Vec<Pos2>>,
    component_buckets: HashMap<(i32, i32), Vec<u64>>,
    cells_by_component: HashMap<u64, Vec<(i32, i32)>>,
    bounds_by_component: HashMap<u64, Rect>,
    pin_buckets: HashMap<(i32, i32), Vec<IndexedPin>>,
    pins_by_component: HashMap<u64, Vec<IndexedPin>>,
}

impl SchematicSpatialIndex {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn sync(&mut self, components: &[Component], wires: &[Wire]) {
        self.sync_wires(wires);
        self.sync_components(components);
    }

    fn sync_wires(&mut self, wires: &[Wire]) {
        let live = wires.iter().map(|wire| wire.id).collect::<HashSet<_>>();
        let removed = self
            .geometry_by_wire
            .keys()
            .filter(|id| !live.contains(id))
            .copied()
            .collect::<Vec<_>>();
        for id in removed {
            self.remove_wire(id);
        }
        for wire in wires {
            if self.geometry_by_wire.get(&wire.id) != Some(&wire.points) {
                self.update_wire(wire);
            }
        }
    }

    fn sync_components(&mut self, components: &[Component]) {
        let live = components
            .iter()
            .map(|component| component.id)
            .collect::<HashSet<_>>();
        let removed = self
            .bounds_by_component
            .keys()
            .filter(|id| !live.contains(id))
            .copied()
            .collect::<Vec<_>>();
        for id in removed {
            self.remove_component(id);
        }
        for component in components {
            let bounds = component_bounds(component);
            let pins = component_pin_defs(component)
                .into_iter()
                .map(|pin| IndexedPin {
                    reference: PinRef {
                        component_id: component.id,
                        pin_name: pin.label.to_string(),
                    },
                    position: pin.pos,
                })
                .collect::<Vec<_>>();
            if self.bounds_by_component.get(&component.id) != Some(&bounds)
                || self.pins_by_component.get(&component.id) != Some(&pins)
            {
                self.replace_component(component.id, bounds, pins);
            }
        }
    }

    pub(crate) fn update_wire(&mut self, wire: &Wire) {
        self.remove_wire(wire.id);
        let mut cells = HashSet::new();
        for (segment_index, pair) in wire.points.windows(2).enumerate() {
            let segment = WireSegmentRef {
                wire_id: wire.id,
                segment_index,
            };
            for bucket in cells_for_rect(Rect::from_two_pos(pair[0], pair[1])) {
                cells.insert(bucket);
                self.wire_segment_buckets
                    .entry(bucket)
                    .or_default()
                    .push(segment);
            }
        }
        let cells = cells.into_iter().collect::<Vec<_>>();
        self.cells_by_wire.insert(wire.id, cells);
        self.geometry_by_wire.insert(wire.id, wire.points.clone());
    }

    pub(crate) fn update_component(&mut self, component: &Component) {
        let pins = component_pin_defs(component)
            .into_iter()
            .map(|pin| IndexedPin {
                reference: PinRef {
                    component_id: component.id,
                    pin_name: pin.label.to_string(),
                },
                position: pin.pos,
            })
            .collect();
        self.replace_component(component.id, component_bounds(component), pins);
    }

    pub(crate) fn remove_wire(&mut self, id: u64) {
        if let Some(cells) = self.cells_by_wire.remove(&id) {
            for cell in cells {
                if let Some(values) = self.wire_segment_buckets.get_mut(&cell) {
                    values.retain(|candidate| candidate.wire_id != id);
                    if values.is_empty() {
                        self.wire_segment_buckets.remove(&cell);
                    }
                }
            }
        }
        self.geometry_by_wire.remove(&id);
    }

    fn replace_component(&mut self, id: u64, bounds: Rect, pins: Vec<IndexedPin>) {
        self.remove_component(id);
        let cells = cells_for_rect(bounds).collect::<Vec<_>>();
        for &bucket in &cells {
            self.component_buckets.entry(bucket).or_default().push(id);
        }
        for pin in &pins {
            self.pin_buckets
                .entry(cell(pin.position))
                .or_default()
                .push(pin.clone());
        }
        self.cells_by_component.insert(id, cells);
        self.bounds_by_component.insert(id, bounds);
        self.pins_by_component.insert(id, pins);
    }

    pub(crate) fn remove_component(&mut self, id: u64) {
        if let Some(cells) = self.cells_by_component.remove(&id) {
            remove_from_buckets(&mut self.component_buckets, &cells, id);
        }
        if let Some(pins) = self.pins_by_component.remove(&id) {
            for pin in pins {
                let bucket = cell(pin.position);
                if let Some(values) = self.pin_buckets.get_mut(&bucket) {
                    values.retain(|candidate| candidate.reference.component_id != id);
                    if values.is_empty() {
                        self.pin_buckets.remove(&bucket);
                    }
                }
            }
        }
        self.bounds_by_component.remove(&id);
    }

    pub(crate) fn query_wires(&self, min: Pos2, max: Pos2) -> HashSet<u64> {
        let mut result = HashSet::new();
        for bucket in cells_for_rect(Rect::from_min_max(min, max)) {
            result.extend(
                self.wire_segment_buckets
                    .get(&bucket)
                    .into_iter()
                    .flatten()
                    .map(|segment| segment.wire_id),
            );
        }
        result
    }

    pub(crate) fn query_components(&self, min: Pos2, max: Pos2) -> HashSet<u64> {
        query_buckets(&self.component_buckets, min, max)
            .into_iter()
            .filter(|id| {
                self.bounds_by_component
                    .get(id)
                    .is_some_and(|bounds| bounds.intersects(Rect::from_min_max(min, max)))
            })
            .collect()
    }

    pub(crate) fn pins_near(&self, position: Pos2, tolerance: f32) -> Vec<(PinRef, Pos2)> {
        let radius = Vec2::splat(tolerance);
        let mut candidates = Vec::new();
        for x in cell(position - radius).0..=cell(position + radius).0 {
            for y in cell(position - radius).1..=cell(position + radius).1 {
                for pin in self.pin_buckets.get(&(x, y)).into_iter().flatten() {
                    let distance = pin.position.distance(position);
                    if distance <= tolerance {
                        candidates.push((pin.reference.clone(), pin.position));
                    }
                }
            }
        }
        candidates.sort_by(|(left_ref, left_pos), (right_ref, right_pos)| {
            left_pos
                .distance_sq(position)
                .total_cmp(&right_pos.distance_sq(position))
                .then_with(|| left_ref.component_id.cmp(&right_ref.component_id))
                .then_with(|| left_ref.pin_name.cmp(&right_ref.pin_name))
                .then_with(|| left_pos.x.total_cmp(&right_pos.x))
                .then_with(|| left_pos.y.total_cmp(&right_pos.y))
        });
        candidates.dedup();
        candidates
    }

    pub(crate) fn nearest_pin(&self, position: Pos2, tolerance: f32) -> Option<PinRef> {
        let radius = Vec2::splat(tolerance);
        let mut best: Option<(&IndexedPin, f32)> = None;
        for x in cell(position - radius).0..=cell(position + radius).0 {
            for y in cell(position - radius).1..=cell(position + radius).1 {
                for pin in self.pin_buckets.get(&(x, y)).into_iter().flatten() {
                    let distance = pin.position.distance_sq(position);
                    if distance > tolerance * tolerance {
                        continue;
                    }
                    let replace = best.is_none_or(|(current, current_distance)| {
                        distance
                            .total_cmp(&current_distance)
                            .then_with(|| {
                                pin.reference
                                    .component_id
                                    .cmp(&current.reference.component_id)
                            })
                            .then_with(|| pin.reference.pin_name.cmp(&current.reference.pin_name))
                            .then_with(|| pin.position.x.total_cmp(&current.position.x))
                            .then_with(|| pin.position.y.total_cmp(&current.position.y))
                            .is_lt()
                    });
                    if replace {
                        best = Some((pin, distance));
                    }
                }
            }
        }
        best.map(|(pin, _)| pin.reference.clone())
    }

    pub(crate) fn wire_segments_near(&self, position: Pos2, tolerance: f32) -> Vec<WireSegmentRef> {
        let radius = Vec2::splat(tolerance);
        let mut result =
            self.segment_refs_in_rect(Rect::from_min_max(position - radius, position + radius));
        result.retain(|segment| {
            self.segment_points(*segment).is_some_and(|(start, end)| {
                crate::model::distance_to_segment(position, start, end) <= tolerance
            })
        });
        result.sort_by(|left, right| {
            let left_distance = self
                .segment_points(*left)
                .map_or(f32::INFINITY, |(start, end)| {
                    crate::model::distance_to_segment(position, start, end)
                });
            let right_distance = self
                .segment_points(*right)
                .map_or(f32::INFINITY, |(start, end)| {
                    crate::model::distance_to_segment(position, start, end)
                });
            left_distance
                .total_cmp(&right_distance)
                .then_with(|| left.cmp(right))
        });
        result
    }

    pub(crate) fn components_in_viewport(&self, rect: Rect) -> HashSet<u64> {
        self.query_components(rect.min, rect.max)
    }

    pub(crate) fn wire_segments_in_viewport(&self, rect: Rect) -> Vec<WireSegmentRef> {
        self.segment_refs_in_rect(rect)
    }

    fn segment_refs_in_rect(&self, rect: Rect) -> Vec<WireSegmentRef> {
        let mut result = HashSet::new();
        for bucket in cells_for_rect(rect) {
            result.extend(
                self.wire_segment_buckets
                    .get(&bucket)
                    .into_iter()
                    .flatten()
                    .copied(),
            );
        }
        let mut result = result.into_iter().collect::<Vec<_>>();
        result.sort_unstable();
        result
    }

    fn segment_points(&self, segment: WireSegmentRef) -> Option<(Pos2, Pos2)> {
        let points = self.geometry_by_wire.get(&segment.wire_id)?;
        Some((
            *points.get(segment.segment_index)?,
            *points.get(segment.segment_index + 1)?,
        ))
    }

    pub(crate) fn is_consistent(&self, components: &[Component], wires: &[Wire]) -> bool {
        let expected_wire_geometry = wires
            .iter()
            .map(|wire| (wire.id, wire.points.clone()))
            .collect::<HashMap<_, _>>();
        let expected_component_ids = components
            .iter()
            .map(|component| component.id)
            .collect::<HashSet<_>>();
        if self.geometry_by_wire != expected_wire_geometry
            || self
                .bounds_by_component
                .keys()
                .copied()
                .collect::<HashSet<_>>()
                != expected_component_ids
        {
            return false;
        }
        let expected_segments = wires
            .iter()
            .flat_map(|wire| {
                (0..wire.points.len().saturating_sub(1)).map(move |segment_index| WireSegmentRef {
                    wire_id: wire.id,
                    segment_index,
                })
            })
            .collect::<HashSet<_>>();
        let indexed_segments = self
            .wire_segment_buckets
            .values()
            .flatten()
            .copied()
            .collect::<HashSet<_>>();
        indexed_segments == expected_segments
            && self.wire_segment_buckets.values().flatten().all(|segment| {
                self.segment_points(*segment).is_some()
                    && expected_wire_geometry.contains_key(&segment.wire_id)
            })
            && components.iter().all(|component| {
                component_pin_defs(component).into_iter().all(|pin| {
                    self.pins_near(pin.pos, 0.01)
                        .iter()
                        .any(|(reference, pos)| {
                            reference.component_id == component.id
                                && reference.pin_name == pin.label
                                && *pos == pin.pos
                        })
                })
            })
    }
}

fn component_bounds(component: &Component) -> Rect {
    let size = component_size(component);
    let rotation = component.rotation.rem_euclid(360);
    let effective = if rotation == 90 || rotation == 270 {
        Vec2::new(size.y, size.x)
    } else {
        size
    };
    Rect::from_center_size(component.pos, effective)
}

fn cells_for_rect(rect: Rect) -> impl Iterator<Item = (i32, i32)> {
    let min = cell(rect.min);
    let max = cell(rect.max);
    (min.0..=max.0).flat_map(move |x| (min.1..=max.1).map(move |y| (x, y)))
}

fn query_buckets(buckets: &HashMap<(i32, i32), Vec<u64>>, min: Pos2, max: Pos2) -> HashSet<u64> {
    let mut result = HashSet::new();
    for bucket in cells_for_rect(Rect::from_min_max(min, max)) {
        result.extend(buckets.get(&bucket).into_iter().flatten());
    }
    result
}

fn remove_from_buckets(buckets: &mut HashMap<(i32, i32), Vec<u64>>, cells: &[(i32, i32)], id: u64) {
    for cell in cells {
        if let Some(values) = buckets.get_mut(cell) {
            values.retain(|candidate| *candidate != id);
            if values.is_empty() {
                buckets.remove(cell);
            }
        }
    }
}

fn cell(position: Pos2) -> (i32, i32) {
    (
        (position.x / CELL).floor() as i32,
        (position.y / CELL).floor() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ComponentKind;

    fn component(id: u64, position: Pos2) -> Component {
        Component {
            id,
            kind: ComponentKind::Resistor,
            pos: position,
            rotation: 0,
            label: format!("R{id}"),
            value: "1k".to_string(),
            part_id: None,
        }
    }

    #[test]
    fn shared_index_updates_wires_components_and_pins_incrementally() {
        let mut index = SchematicSpatialIndex::default();
        let mut wire = Wire::new(7, vec![Pos2::new(0.0, 0.0), Pos2::new(10.0, 0.0)]);
        let mut component = component(3, Pos2::new(40.0, 0.0));
        index.sync(
            std::slice::from_ref(&component),
            std::slice::from_ref(&wire),
        );
        assert!(
            index
                .query_wires(Pos2::new(-1.0, -1.0), Pos2::new(20.0, 1.0))
                .contains(&7)
        );
        assert!(
            index
                .query_components(Pos2::new(0.0, -40.0), Pos2::new(80.0, 40.0))
                .contains(&3)
        );
        let pin = component_pin_defs(&component)[0].pos;
        assert_eq!(index.nearest_pin(pin, 1.0).unwrap().component_id, 3);

        wire.points = vec![Pos2::new(500.0, 0.0), Pos2::new(510.0, 0.0)];
        component.pos = Pos2::new(500.0, 100.0);
        index.sync(
            std::slice::from_ref(&component),
            std::slice::from_ref(&wire),
        );
        assert!(
            index
                .query_wires(Pos2::new(-1.0, -1.0), Pos2::new(20.0, 1.0))
                .is_empty()
        );
        assert!(
            index
                .query_components(Pos2::new(0.0, -40.0), Pos2::new(80.0, 40.0))
                .is_empty()
        );
        index.sync(&[], &[]);
        assert!(
            index
                .query_wires(Pos2::new(400.0, -1.0), Pos2::new(520.0, 1.0))
                .is_empty()
        );
    }
}
