use super::pin::{NetlistPin, PinRef};
use egui::Pos2;
use std::collections::HashMap;

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
    pub(crate) floating_wires: Vec<u64>,
    pub(crate) isolated_wires: Vec<u64>,
    pub(crate) explicit_junctions: Vec<Pos2>,
    pub(crate) no_connects: Vec<NoConnectMarker>,
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
}
