use super::spatial_index::SegmentSpatialIndex;
use super::union_find::{ConnectivityNodes, ConnectivityUnionFind};
use crate::model::{Wire, point_touches_wire_segment};
use egui::Pos2;

pub(in crate::engine) fn resolve_contacts(
    wires: &[Wire],
    contacts: impl IntoIterator<Item = Pos2>,
    nodes: &mut ConnectivityNodes,
    nets: &mut ConnectivityUnionFind,
) {
    let segment_index = SegmentSpatialIndex::new(wires);
    for contact in contacts {
        let contact_node = nodes.node_for(contact);
        nets.ensure(contact_node);
        for candidate in segment_index.candidates(contact) {
            let segment = &wires[candidate.wire_index].points
                [candidate.segment_index..=candidate.segment_index + 1];
            if point_touches_wire_segment(contact, segment[0], segment[1]) {
                let a = nodes.node_for(segment[0]);
                let b = nodes.node_for(segment[1]);
                nets.ensure(a);
                nets.ensure(b);
                nets.union(contact_node, a);
                nets.union(contact_node, b);
            }
        }
    }
}
