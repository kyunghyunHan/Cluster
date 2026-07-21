use super::spatial_index::{SegmentRef, SegmentSpatialIndex};
use super::union_find::{ConnectivityNodes, ConnectivityUnionFind};
use crate::model::{Wire, point_touches_wire_segment};
use egui::Pos2;
use std::collections::HashMap;

pub(in crate::engine) type ContactSplits = HashMap<usize, Vec<(f32, Pos2)>>;

#[derive(Default)]
pub(in crate::engine) struct ContactResolution {
    pub(in crate::engine) splits: ContactSplits,
    pub(in crate::engine) candidate_lookup_ms: f64,
    pub(in crate::engine) exact_checks_ms: f64,
    pub(in crate::engine) union_ms: f64,
}

pub(in crate::engine) fn resolve_contacts(
    wires: &[Wire],
    segment_index: &SegmentSpatialIndex,
    contacts: impl IntoIterator<Item = Pos2>,
    nodes: &mut ConnectivityNodes,
    nets: &mut ConnectivityUnionFind,
) -> ContactResolution {
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
    let mut candidate_lookup_ms = 0.0;
    let mut exact_checks_ms = 0.0;
    let mut union_ms = 0.0;
    let mut splits = HashMap::new();
    for contact in contacts {
        let started = std::time::Instant::now();
        let candidates = segment_index.candidates(contact);
        candidate_lookup_ms += started.elapsed().as_secs_f64() * 1_000.0;
        let started = std::time::Instant::now();
        let hits = candidates
            .into_iter()
            .filter_map(|candidate| {
                contact_hit(wires, &cumulative_by_wire, contact, candidate)
                    .map(|parameter| (candidate, parameter))
            })
            .collect::<Vec<_>>();
        exact_checks_ms += started.elapsed().as_secs_f64() * 1_000.0;
        let started = std::time::Instant::now();
        for (candidate, parameter) in hits {
            let contact_node = nodes.node_for(contact);
            nets.ensure(contact_node);
            let segment = &wires[candidate.wire_index].points
                [candidate.segment_index..=candidate.segment_index + 1];
            let a = nodes.node_for(segment[0]);
            let b = nodes.node_for(segment[1]);
            nets.ensure(a);
            nets.ensure(b);
            nets.union(contact_node, a);
            nets.union(contact_node, b);
            splits
                .entry(candidate.wire_index)
                .or_insert_with(Vec::new)
                .push((parameter, contact));
        }
        union_ms += started.elapsed().as_secs_f64() * 1_000.0;
    }
    ContactResolution {
        splits,
        candidate_lookup_ms,
        exact_checks_ms,
        union_ms,
    }
}

fn contact_hit(
    wires: &[Wire],
    cumulative_by_wire: &[Vec<f32>],
    contact: Pos2,
    candidate: SegmentRef,
) -> Option<f32> {
    let segment =
        &wires[candidate.wire_index].points[candidate.segment_index..=candidate.segment_index + 1];
    if !point_touches_wire_segment(contact, segment[0], segment[1]) {
        return None;
    }
    let delta = segment[1] - segment[0];
    let length_squared = delta.length_sq();
    if length_squared <= f32::EPSILON {
        return None;
    }
    let factor = (delta.dot(contact - segment[0]) / length_squared).clamp(0.0, 1.0);
    Some(
        cumulative_by_wire[candidate.wire_index][candidate.segment_index]
            + factor * length_squared.sqrt(),
    )
}
