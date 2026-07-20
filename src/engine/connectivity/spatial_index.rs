use crate::model::Wire;
use egui::Pos2;
use std::collections::{HashMap, HashSet};

const SEGMENT_BUCKET_SIZE: f32 = 64.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::engine) struct SegmentRef {
    pub(in crate::engine) wire_index: usize,
    pub(in crate::engine) segment_index: usize,
}

/// Uniform-grid candidate index for wire segment contact queries.
pub(in crate::engine) struct SegmentSpatialIndex {
    buckets: HashMap<(i32, i32), Vec<SegmentRef>>,
}

impl SegmentSpatialIndex {
    pub(in crate::engine) fn new(wires: &[Wire]) -> Self {
        let mut index = Self {
            buckets: HashMap::new(),
        };
        for (wire_index, wire) in wires.iter().enumerate() {
            index.add_wire(wire_index, wire);
        }
        index
    }

    pub(in crate::engine) fn add_wire(&mut self, wire_index: usize, wire: &Wire) {
        for (segment_index, segment) in wire.points.windows(2).enumerate() {
            let min_x = bucket(segment[0].x.min(segment[1].x));
            let max_x = bucket(segment[0].x.max(segment[1].x));
            let min_y = bucket(segment[0].y.min(segment[1].y));
            let max_y = bucket(segment[0].y.max(segment[1].y));
            let reference = SegmentRef {
                wire_index,
                segment_index,
            };
            for x in min_x..=max_x {
                for y in min_y..=max_y {
                    self.buckets.entry((x, y)).or_default().push(reference);
                }
            }
        }
    }

    #[allow(dead_code)] // Incremental API is exercised by regression tests and the staged cache.
    pub(in crate::engine) fn update_wire(&mut self, wire_index: usize, wire: &Wire) {
        self.remove_wire_segments(wire_index);
        self.add_wire(wire_index, wire);
    }

    #[allow(dead_code)] // Incremental API is exercised by regression tests and the staged cache.
    pub(in crate::engine) fn remove_wire(&mut self, wire_index: usize) {
        self.remove_wire_segments(wire_index);
        for candidates in self.buckets.values_mut() {
            for candidate in candidates {
                if candidate.wire_index > wire_index {
                    candidate.wire_index -= 1;
                }
            }
        }
    }

    #[allow(dead_code)]
    fn remove_wire_segments(&mut self, wire_index: usize) {
        self.buckets.retain(|_, candidates| {
            candidates.retain(|candidate| candidate.wire_index != wire_index);
            !candidates.is_empty()
        });
    }

    pub(in crate::engine) fn candidates(&self, point: Pos2) -> Vec<SegmentRef> {
        let origin = (bucket(point.x), bucket(point.y));
        let mut seen = HashSet::new();
        let mut candidates = Vec::new();
        for x in (origin.0 - 1)..=(origin.0 + 1) {
            for y in (origin.1 - 1)..=(origin.1 + 1) {
                for &candidate in self.buckets.get(&(x, y)).into_iter().flatten() {
                    if seen.insert(candidate) {
                        candidates.push(candidate);
                    }
                }
            }
        }
        candidates.sort_by_key(|candidate| (candidate.wire_index, candidate.segment_index));
        candidates
    }

    pub(in crate::engine) fn candidate_pairs(&self) -> Vec<(SegmentRef, SegmentRef)> {
        let mut pairs = HashSet::new();
        for candidates in self.buckets.values() {
            for (index, &left) in candidates.iter().enumerate() {
                for &right in &candidates[index + 1..] {
                    if left != right {
                        pairs.insert(if segment_key(left) < segment_key(right) {
                            (left, right)
                        } else {
                            (right, left)
                        });
                    }
                }
            }
        }
        let mut pairs = pairs.into_iter().collect::<Vec<_>>();
        pairs.sort_by_key(|(left, right)| (segment_key(*left), segment_key(*right)));
        pairs
    }
}

fn bucket(value: f32) -> i32 {
    (value / SEGMENT_BUCKET_SIZE).floor() as i32
}

fn segment_key(reference: SegmentRef) -> (usize, usize) {
    (reference.wire_index, reference.segment_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_index_returns_nearby_candidates_only() {
        let wires = vec![
            Wire::new(1, vec![Pos2::new(0.0, 0.0), Pos2::new(20.0, 0.0)]),
            Wire::new(2, vec![Pos2::new(10_000.0, 0.0), Pos2::new(10_020.0, 0.0)]),
        ];
        let index = SegmentSpatialIndex::new(&wires);
        assert_eq!(
            index.candidates(Pos2::new(10.0, 0.0)),
            vec![SegmentRef {
                wire_index: 0,
                segment_index: 0,
            }]
        );
    }

    #[test]
    fn segment_index_supports_incremental_add_update_remove() {
        let mut wires = vec![
            Wire::new(1, vec![Pos2::new(0.0, 0.0), Pos2::new(20.0, 0.0)]),
            Wire::new(2, vec![Pos2::new(200.0, 0.0), Pos2::new(220.0, 0.0)]),
        ];
        let mut index = SegmentSpatialIndex::new(&wires);

        wires[0].points = vec![Pos2::new(500.0, 0.0), Pos2::new(520.0, 0.0)];
        index.update_wire(0, &wires[0]);
        assert!(index.candidates(Pos2::new(10.0, 0.0)).is_empty());
        assert_eq!(index.candidates(Pos2::new(510.0, 0.0))[0].wire_index, 0);

        wires.push(Wire::new(
            3,
            vec![Pos2::new(300.0, 0.0), Pos2::new(320.0, 0.0)],
        ));
        index.add_wire(2, &wires[2]);
        assert!(
            index
                .candidates(Pos2::new(310.0, 0.0))
                .iter()
                .any(|candidate| candidate.wire_index == 2)
        );

        wires.remove(0);
        index.remove_wire(0);
        assert!(
            index
                .candidates(Pos2::new(210.0, 0.0))
                .iter()
                .any(|candidate| candidate.wire_index == 0)
        );
        assert!(
            index
                .candidates(Pos2::new(310.0, 0.0))
                .iter()
                .any(|candidate| candidate.wire_index == 1)
        );
    }
}
