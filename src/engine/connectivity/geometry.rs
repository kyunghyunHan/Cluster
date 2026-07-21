use super::spatial_index::SegmentSpatialIndex;
use crate::model::Wire;
use egui::Pos2;
use std::collections::HashMap;

pub(in crate::engine) struct NormalizedWireSegment {
    pub(in crate::engine) id: u64,
    pub(in crate::engine) wire_id: u64,
    pub(in crate::engine) points: Vec<Pos2>,
}

pub(in crate::engine) fn wire_endpoint_contact_points(wires: &[Wire]) -> Vec<Pos2> {
    let mut points = Vec::new();
    for wire in wires {
        points.extend(wire.points.first().copied());
        points.extend(wire.points.last().copied());
    }
    points
}

pub(in crate::engine) fn normalized_wire_segments(
    wires: &[Wire],
    explicit_junctions: &[Pos2],
) -> Vec<NormalizedWireSegment> {
    let contacts = wire_endpoint_contact_points(wires)
        .into_iter()
        .chain(explicit_junctions.iter().copied())
        .collect::<Vec<_>>();
    let cumulative_by_wire = wires
        .iter()
        .map(|wire| {
            let mut cumulative = vec![0.0f32; wire.points.len()];
            for index in 1..wire.points.len() {
                cumulative[index] =
                    cumulative[index - 1] + wire.points[index - 1].distance(wire.points[index]);
            }
            cumulative
        })
        .collect::<Vec<_>>();
    let index = SegmentSpatialIndex::new(wires);
    let mut splits_by_wire: HashMap<usize, Vec<(f32, Pos2)>> = HashMap::new();
    for contact in contacts {
        for candidate in index.candidates(contact) {
            let wire = &wires[candidate.wire_index];
            let segment = &wire.points[candidate.segment_index..=candidate.segment_index + 1];
            let delta = segment[1] - segment[0];
            let length_squared = delta.length_sq();
            if length_squared <= f32::EPSILON {
                continue;
            }
            let factor = (delta.dot(contact - segment[0]) / length_squared).clamp(0.0, 1.0);
            let projected = segment[0] + delta * factor;
            if projected.distance(contact) > 1.0 {
                continue;
            }
            let parameter = cumulative_by_wire[candidate.wire_index][candidate.segment_index]
                + factor * length_squared.sqrt();
            splits_by_wire
                .entry(candidate.wire_index)
                .or_default()
                .push((parameter, contact));
        }
    }

    let mut output = Vec::new();
    let mut next_id = 1u64;
    for (wire_index, wire) in wires.iter().enumerate() {
        if wire.points.len() < 2 {
            continue;
        }
        let cumulative = &cumulative_by_wire[wire_index];
        let mut splits = splits_by_wire.remove(&wire_index).unwrap_or_default();
        splits.sort_by(|left, right| left.0.total_cmp(&right.0));
        splits.dedup_by(|left, right| (left.0 - right.0).abs() <= 0.001);

        for pair in splits.windows(2) {
            let (from_parameter, from) = pair[0];
            let (to_parameter, to) = pair[1];
            if from.distance(to) <= f32::EPSILON {
                continue;
            }
            let mut points = vec![from];
            for (index, &point) in wire.points.iter().enumerate() {
                let parameter = cumulative[index];
                if parameter > from_parameter + 0.001 && parameter < to_parameter - 0.001 {
                    points.push(point);
                }
            }
            points.push(to);
            output.push(NormalizedWireSegment {
                id: next_id,
                wire_id: wire.id,
                points,
            });
            next_id += 1;
        }
    }
    output
}
