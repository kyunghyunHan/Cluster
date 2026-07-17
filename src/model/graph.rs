//! Canonical schematic connectivity graph.
//!
//! The graph is derived from persisted circuit geometry and typed endpoints.
//! It is never serialized; all downstream electrical consumers share one
//! revision-cached instance.

use super::{CircuitNetlist, JunctionId, PinRef, WireSegmentId};
use egui::Pos2;
use std::collections::HashMap;

pub(crate) type NetId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ConnectivityPoint {
    pub(crate) x_milli: i64,
    pub(crate) y_milli: i64,
}

impl From<Pos2> for ConnectivityPoint {
    fn from(value: Pos2) -> Self {
        Self {
            x_milli: (value.x as f64 * 1_000.0).round() as i64,
            y_milli: (value.y as f64 * 1_000.0).round() as i64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConnectivityDiagnostic {
    DegenerateWire {
        wire_id: u64,
    },
    UnresolvedPinEndpoint {
        wire_id: u64,
        pin: PinRef,
    },
    UnresolvedJunctionEndpoint {
        wire_id: u64,
        junction_id: JunctionId,
    },
    FloatingWire {
        wire_id: u64,
    },
    DuplicateLabel {
        normalized_name: String,
    },
    AmbiguousCrossing {
        first_wire_id: u64,
        first_segment: usize,
        second_wire_id: u64,
        second_segment: usize,
        position: ConnectivityPoint,
    },
    OrphanJunction {
        position: ConnectivityPoint,
    },
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Default)]
pub(crate) struct CanonicalConnectivity {
    pub(crate) netlist: CircuitNetlist,
    pub(crate) pin_nets: HashMap<PinRef, NetId>,
    pub(crate) junction_id_nets: HashMap<JunctionId, NetId>,
    pub(crate) junction_nets: HashMap<ConnectivityPoint, NetId>,
    pub(crate) wire_segment_nets: HashMap<WireSegmentId, NetId>,
    pub(crate) diagnostics: Vec<ConnectivityDiagnostic>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl CanonicalConnectivity {
    pub(crate) fn net_for_pin(&self, pin: &PinRef) -> Option<NetId> {
        self.pin_nets.get(pin).copied()
    }

    pub(crate) fn net_for_junction(&self, position: Pos2) -> Option<NetId> {
        self.junction_nets
            .get(&ConnectivityPoint::from(position))
            .copied()
    }

    pub(crate) fn net_for_junction_id(&self, junction_id: JunctionId) -> Option<NetId> {
        self.junction_id_nets.get(&junction_id).copied()
    }

    pub(crate) fn net_for_segment(&self, segment: WireSegmentId) -> Option<NetId> {
        self.wire_segment_nets.get(&segment).copied()
    }
}
