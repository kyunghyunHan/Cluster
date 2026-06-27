pub(crate) mod cad;
pub(crate) mod circuit;
pub(crate) mod component;
pub(crate) mod graph;
pub(crate) mod library;
pub(crate) mod net;
pub(crate) mod pin;
pub(crate) mod pin_defs;
pub(crate) mod wire;

pub(crate) use circuit::{
    CircuitSnapshot, Counters, DragState, SavedCircuit, SavedComponent, SavedPage, SavedPoint,
    SavedWire,
};
pub(crate) use component::{Component, ComponentKind, SimulationSupport, electrical_metadata};
pub(crate) use graph::{
    Branch, BranchKind, Junction, NodeId, PinConnection as GraphPinConnection, SchematicGraph,
    SchematicNet as GraphNet, SchematicNode, WireSegment, build_schematic_graph,
};
pub(crate) use net::{CircuitNetlist, Net, NetlistAnnotations, NoConnectMarker};
pub(crate) use pin::{CircuitPin, ElectricalType, NetlistPin, PinRef, PinRole};
// Only the items needed by code outside the model module are re-exported.
// module_pin, module_pin_defs, breadboard_pin_defs, module_pin_y are internal
// helpers used only within pin_defs.rs.
pub(crate) use pin_defs::{
    component_pin_defs, component_pins, component_size, module_pin_y, rotate_point,
};
pub(crate) use wire::{Wire, distance_to_segment, point_touches_wire_segment};
