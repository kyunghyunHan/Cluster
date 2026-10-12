//! Per-instance pad identities and layer-aware copper connectivity.
use super::geometry::{pad_segment_distance, point_segment_distance, segment_distance};
use crate::model::cad::{CadNet, Point2};
use crate::pcb::{
    board::Board,
    footprint::{FootprintTransform, Pad},
    layer::BoardLayer,
};

#[derive(Clone)]
pub(crate) struct ConnectedPad {
    pub(crate) footprint_id: u64,
    pub(crate) number: String,
    pub(crate) schematic_pin: String,
    pub(crate) net_id: usize,
    pub(crate) position: Point2,
    pub(crate) pad: Pad,
    pub(crate) transform: FootprintTransform,
    pub(crate) layers: Vec<BoardLayer>,
}

/// Only declared mappings are used. Unknown packages never use their origin
/// as an electrical terminal. Library pad.net_id is not an instance assignment.
pub(crate) fn resolve_pads(board: &Board, nets: &[CadNet]) -> Vec<ConnectedPad> {
    let mut result = Vec::new();
    for footprint in &board.footprints {
        let Some(symbol) = footprint.symbol_instance_id else {
            continue;
        };
        let Some(definition) = board
            .footprint_library
            .iter()
            .find(|d| d.footprint_id == footprint.footprint_id)
        else {
            continue;
        };
        for net in nets {
            for pin in net
                .connected_pins
                .iter()
                .filter(|p| p.component_id == symbol)
            {
                let number = match (footprint.footprint_id.as_str(), pin.pin_name.as_str()) {
                    ("R_THT_Axial", "A") => "1",
                    ("R_THT_Axial", "B") => "2",
                    // LED pad 1 is the square cathode; schematic A is anode.
                    ("LED_THT_5mm", "A") => "2",
                    ("LED_THT_5mm", "B") => "1",
                    (_, name) => name,
                };
                if let Some(pad) = definition.pads.iter().find(|p| p.number == number) {
                    result.push(ConnectedPad {
                        footprint_id: footprint.id,
                        number: number.into(),
                        schematic_pin: pin.pin_name.clone(),
                        net_id: net.net_id,
                        position: footprint.transform().local_to_board(pad.position),
                        pad: pad.clone(),
                        transform: footprint.transform(),
                        layers: footprint.transform().transform_pad(pad).layers,
                    });
                }
            }
        }
    }
    result.sort_by(|a, b| {
        (a.net_id, a.footprint_id, &a.number).cmp(&(b.net_id, b.footprint_id, &b.number))
    });
    result.dedup_by(|a, b| {
        a.net_id == b.net_id && a.footprint_id == b.footprint_id && a.number == b.number
    });
    result
}

/// Returns connected pad islands. Unplaced pads cannot complete a route.
pub(crate) fn pad_islands(board: &Board, pads: &[ConnectedPad], net_id: usize) -> Vec<Vec<usize>> {
    let tracks = board
        .tracks
        .iter()
        .filter(|t| t.net_id == net_id)
        .collect::<Vec<_>>();
    let vias = board
        .vias
        .iter()
        .filter(|v| v.net_id == net_id)
        .collect::<Vec<_>>();
    let mut parent = (0..pads.len() + tracks.len() + vias.len()).collect::<Vec<_>>();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut join = |a, b| {
        let a = root(&mut parent, a);
        let b = root(&mut parent, b);
        parent[b] = a;
    };
    let placed = |p: &ConnectedPad| {
        board
            .footprints
            .iter()
            .any(|f| f.id == p.footprint_id && f.placed)
    };
    for (i, pad) in pads.iter().enumerate().filter(|(_, p)| placed(p)) {
        for (j, other) in pads
            .iter()
            .enumerate()
            .skip(i + 1)
            .filter(|(_, p)| placed(p))
        {
            if pad.layers.iter().any(|l| other.layers.contains(l))
                && super::geometry::pad_distance(
                    &pad.pad,
                    pad.transform,
                    &other.pad,
                    other.transform,
                ) <= 1e-6
            {
                join(i, j);
            }
        }
        for (j, track) in tracks.iter().enumerate() {
            if pad.layers.contains(&track.layer)
                && pad_segment_distance(&pad.pad, pad.transform, track.start, track.end)
                    <= track.width_mm * 0.5 + 1e-6
            {
                join(i, pads.len() + j);
            }
        }
        for (j, via) in vias.iter().enumerate() {
            if pad_segment_distance(&pad.pad, pad.transform, via.position, via.position)
                <= via.diameter_mm * 0.5 + 1e-6
            {
                join(i, pads.len() + tracks.len() + j);
            }
        }
    }
    for (i, a) in tracks.iter().enumerate() {
        for (j, b) in tracks.iter().enumerate().skip(i + 1) {
            if a.layer == b.layer
                && segment_distance(a.start, a.end, b.start, b.end)
                    <= (a.width_mm + b.width_mm) * 0.5 + 1e-6
            {
                join(pads.len() + i, pads.len() + j);
            }
        }
        for (j, via) in vias.iter().enumerate() {
            if point_segment_distance(via.position, a.start, a.end)
                <= (via.diameter_mm + a.width_mm) * 0.5 + 1e-6
            {
                join(pads.len() + i, pads.len() + tracks.len() + j);
            }
        }
    }
    for (i, a) in vias.iter().enumerate() {
        for (j, b) in vias.iter().enumerate().skip(i + 1) {
            if (a.position.x - b.position.x).hypot(a.position.y - b.position.y)
                <= (a.diameter_mm + b.diameter_mm) * 0.5 + 1e-6
            {
                join(pads.len() + tracks.len() + i, pads.len() + tracks.len() + j);
            }
        }
    }
    let mut groups = std::collections::BTreeMap::<usize, Vec<usize>>::new();
    for i in 0..pads.len() {
        let r = root(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    groups.into_values().collect()
}
