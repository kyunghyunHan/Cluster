use super::pin::{NetlistPin, PinRef};
use egui::Pos2;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum NetLabelScope {
    /// Same-name labels connect only when they are already geometrically wired.
    Local,
    /// Same-name labels connect within one schematic page only.
    Page,
    /// Same-name labels connect across all pages.
    #[default]
    Global,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct JunctionDot {
    pub(crate) id: u64,
    pub(crate) position: Pos2,
}

#[derive(Debug, Clone)]
pub(crate) struct Net {
    pub(crate) id: usize,
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
    pub(crate) net_id: usize,
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
    pub(crate) no_connects: Vec<Pos2>,
    pub(crate) net_label_scopes: HashMap<u64, NetLabelScope>,
}
