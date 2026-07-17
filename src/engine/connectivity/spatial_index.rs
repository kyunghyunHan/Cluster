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
        let mut buckets: HashMap<(i32, i32), Vec<SegmentRef>> = HashMap::new();
        for (wire_index, wire) in wires.iter().enumerate() {
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
                        buckets.entry((x, y)).or_default().push(reference);
                    }
                }
            }
        }
        Self { buckets }
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
}
