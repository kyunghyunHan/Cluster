use std::collections::HashMap;
use super::pin::{NetlistPin, PinRef};

#[derive(Debug, Clone)]
pub(crate) struct Net {
    pub(crate) id: usize,
    pub(crate) name: String,
    pub(crate) connected_pins: Vec<PinRef>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CircuitNetlist {
    pub(crate) nets: Vec<Net>,
    pub(crate) pins: Vec<NetlistPin>,
    pub(crate) wire_nets: HashMap<u64, usize>,
    pub(crate) floating_wires: Vec<u64>,
}
