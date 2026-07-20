use crate::model::Wire;
use egui::Pos2;
use std::collections::{HashMap, HashSet};

const CELL: f32 = 64.0;

/// Incremental schematic wire index used by viewport culling. Geometry edits
/// update only the buckets touched by the changed wire; additions/removals do
/// not rebuild the whole document index.
#[derive(Default)]
pub(crate) struct SchematicSpatialIndex {
    buckets: HashMap<(i32, i32), Vec<u64>>,
    cells_by_wire: HashMap<u64, Vec<(i32, i32)>>,
    geometry_by_wire: HashMap<u64, Vec<Pos2>>,
}

impl SchematicSpatialIndex {
    pub(crate) fn sync(&mut self, wires: &[Wire]) {
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

    pub(crate) fn update_wire(&mut self, wire: &Wire) {
        self.remove_wire(wire.id);
        let mut cells = HashSet::new();
        for pair in wire.points.windows(2) {
            for x in cell(pair[0].x.min(pair[1].x))..=cell(pair[0].x.max(pair[1].x)) {
                for y in cell(pair[0].y.min(pair[1].y))..=cell(pair[0].y.max(pair[1].y)) {
                    cells.insert((x, y));
                }
            }
        }
        let cells = cells.into_iter().collect::<Vec<_>>();
        for &bucket in &cells {
            self.buckets.entry(bucket).or_default().push(wire.id);
        }
        self.cells_by_wire.insert(wire.id, cells);
        self.geometry_by_wire.insert(wire.id, wire.points.clone());
    }

    pub(crate) fn remove_wire(&mut self, id: u64) {
        if let Some(cells) = self.cells_by_wire.remove(&id) {
            for cell in cells {
                if let Some(values) = self.buckets.get_mut(&cell) {
                    values.retain(|candidate| *candidate != id);
                    if values.is_empty() {
                        self.buckets.remove(&cell);
                    }
                }
            }
        }
        self.geometry_by_wire.remove(&id);
    }

    pub(crate) fn query_rect(&self, min: Pos2, max: Pos2) -> HashSet<u64> {
        let mut result = HashSet::new();
        for x in cell(min.x)..=cell(max.x) {
            for y in cell(min.y)..=cell(max.y) {
                result.extend(self.buckets.get(&(x, y)).into_iter().flatten());
            }
        }
        result
    }
}

fn cell(value: f32) -> i32 {
    (value / CELL).floor() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_index_updates_and_removes_incrementally() {
        let mut index = SchematicSpatialIndex::default();
        let mut wire = Wire::new(7, vec![Pos2::new(0.0, 0.0), Pos2::new(10.0, 0.0)]);
        index.sync(std::slice::from_ref(&wire));
        assert!(
            index
                .query_rect(Pos2::new(-1.0, -1.0), Pos2::new(20.0, 1.0))
                .contains(&7)
        );
        wire.points = vec![Pos2::new(500.0, 0.0), Pos2::new(510.0, 0.0)];
        index.sync(std::slice::from_ref(&wire));
        assert!(
            !index
                .query_rect(Pos2::new(-1.0, -1.0), Pos2::new(20.0, 1.0))
                .contains(&7)
        );
        index.sync(&[]);
        assert!(
            index
                .query_rect(Pos2::new(400.0, -1.0), Pos2::new(520.0, 1.0))
                .is_empty()
        );
    }
}
