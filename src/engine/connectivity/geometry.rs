use crate::model::Wire;
use egui::Pos2;

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
    let mut output = Vec::new();
    let mut next_id = 1u64;

    for wire in wires {
        if wire.points.len() < 2 {
            continue;
        }
        let mut cumulative = vec![0.0f32; wire.points.len()];
        for index in 1..wire.points.len() {
            cumulative[index] =
                cumulative[index - 1] + wire.points[index - 1].distance(wire.points[index]);
        }
        let mut splits = contacts
            .iter()
            .filter_map(|&contact| {
                polyline_parameter(&wire.points, contact, 1.0).map(|parameter| (parameter, contact))
            })
            .collect::<Vec<_>>();
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

fn polyline_parameter(points: &[Pos2], position: Pos2, tolerance: f32) -> Option<f32> {
    let mut cumulative = 0.0;
    for segment in points.windows(2) {
        let delta = segment[1] - segment[0];
        let length_squared = delta.length_sq();
        if length_squared <= f32::EPSILON {
            continue;
        }
        let factor = (delta.dot(position - segment[0]) / length_squared).clamp(0.0, 1.0);
        let length = length_squared.sqrt();
        if (segment[0] + delta * factor).distance(position) <= tolerance {
            return Some(cumulative + factor * length);
        }
        cumulative += length;
    }
    None
}
