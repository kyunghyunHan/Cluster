use super::graph::NetId;
use super::ids::JunctionId;
use super::pin::{NetlistPin, PinRef};
use egui::Pos2;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)] // Page scope is serialized for forward-compatible labels.
pub(crate) enum NetLabelScope {
    /// Same-name labels connect only when they are already geometrically wired.
    Local,
    /// Same-name labels connect within one schematic page only.
    Page,
    /// Same-name labels connect across all pages.
    #[default]
    Global,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct JunctionDot {
    pub(crate) id: JunctionId,
    pub(crate) position: Pos2,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NoConnectDot {
    pub(crate) id: u64,
    pub(crate) position: Pos2,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct SchematicAnnotations {
    pub(crate) junction_dots: Vec<JunctionDot>,
    pub(crate) no_connect_markers: Vec<NoConnectDot>,
}

impl SchematicAnnotations {
    pub(crate) fn netlist_annotations(&self) -> NetlistAnnotations {
        NetlistAnnotations {
            junction_endpoints: self
                .junction_dots
                .iter()
                .map(|dot| (dot.id, dot.position))
                .collect(),
            no_connects: self
                .no_connect_markers
                .iter()
                .map(|marker| marker.position)
                .collect(),
            ..NetlistAnnotations::default()
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Net {
    pub(crate) id: NetId,
    pub(crate) name: String,
    pub(crate) connected_pins: Vec<PinRef>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub(crate) struct CircuitNetlist {
    pub(crate) nets: Vec<Net>,
    pub(crate) pins: Vec<NetlistPin>,
    pub(crate) wire_nets: HashMap<u64, usize>,
    pub(crate) wire_segments: Vec<WireNetSegment>,
    pub(crate) floating_wires: Vec<u64>,
    pub(crate) isolated_wires: Vec<u64>,
    pub(crate) explicit_junctions: Vec<Pos2>,
    pub(crate) no_connects: Vec<NoConnectMarker>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct WireNetSegment {
    pub(crate) id: u64,
    pub(crate) source_wire_id: u64,
    pub(crate) net_id: NetId,
    pub(crate) points: Vec<Pos2>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct NoConnectMarker {
    pub(crate) component_id: u64,
    pub(crate) pin_name: String,
    pub(crate) position: Pos2,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct NetlistAnnotations {
    pub(crate) junctions: Vec<Pos2>,
    /// Stable junction identity for typed wire endpoints. The legacy
    /// position-only list remains supported.
    pub(crate) junction_endpoints: HashMap<super::JunctionId, Pos2>,
    pub(crate) no_connects: Vec<Pos2>,
    pub(crate) net_label_scopes: HashMap<u64, NetLabelScope>,
}
