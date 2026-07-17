use super::intersections::{SegmentIntersection, classify};
use super::spatial_index::SegmentSpatialIndex;
use crate::model::{ConnectivityDiagnostic, ConnectivityPoint, Wire};
use egui::Pos2;

pub(in crate::engine) fn geometry_diagnostics(wires: &[Wire]) -> Vec<ConnectivityDiagnostic> {
    wires
        .iter()
        .filter(|wire| {
            wire.points.len() < 2
                || wire
                    .points
                    .windows(2)
                    .all(|segment| segment[0].distance(segment[1]) <= f32::EPSILON)
        })
        .map(|wire| ConnectivityDiagnostic::DegenerateWire { wire_id: wire.id })
        .collect()
}

pub(in crate::engine) fn crossing_diagnostics(
    wires: &[Wire],
    explicit_junctions: &[Pos2],
) -> Vec<ConnectivityDiagnostic> {
    let index = SegmentSpatialIndex::new(wires);
    index
        .candidate_pairs()
        .into_iter()
        .filter(|(left, right)| left.wire_index != right.wire_index)
        .filter_map(|(left, right)| {
            let a = &wires[left.wire_index];
            let b = &wires[right.wire_index];
            let a_segment = &a.points[left.segment_index..=left.segment_index + 1];
            let b_segment = &b.points[right.segment_index..=right.segment_index + 1];
            let SegmentIntersection::Crossing(position) =
                classify(a_segment[0], a_segment[1], b_segment[0], b_segment[1])
            else {
                return None;
            };
            if explicit_junctions
                .iter()
                .any(|junction| junction.distance(position) <= 1.0)
            {
                return None;
            }
            Some(ConnectivityDiagnostic::AmbiguousCrossing {
                first_wire_id: a.id,
                first_segment: left.segment_index,
                second_wire_id: b.id,
                second_segment: right.segment_index,
                position: ConnectivityPoint::from(position),
            })
        })
        .collect()
}

pub(in crate::engine) fn orphan_junction_diagnostics(
    wires: &[Wire],
    explicit_junctions: &[Pos2],
) -> Vec<ConnectivityDiagnostic> {
    let index = SegmentSpatialIndex::new(wires);
    explicit_junctions
        .iter()
        .copied()
        .filter(|&position| {
            !index.candidates(position).into_iter().any(|candidate| {
                let segment = &wires[candidate.wire_index].points
                    [candidate.segment_index..=candidate.segment_index + 1];
                crate::model::point_touches_wire_segment(position, segment[0], segment[1])
            })
        })
        .map(|position| ConnectivityDiagnostic::OrphanJunction {
            position: ConnectivityPoint::from(position),
        })
        .collect()
}
