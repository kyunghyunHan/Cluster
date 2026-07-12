pub(crate) mod cad;
pub(crate) mod circuit;
pub(crate) mod component;
pub(crate) mod custom_part;
pub(crate) mod graph;
pub(crate) mod library;
pub(crate) mod net;
pub(crate) mod pin;
pub(crate) mod pin_defs;
pub(crate) mod wire;

pub(crate) use circuit::{
    CircuitSnapshot, Counters, DragState, SavedCircuit, SavedComponent, SavedJunctionDot,
    SavedNoConnectMarker, SavedPage, SavedPoint, SavedWire,
};
pub(crate) use component::{Component, ComponentKind, SimulationSupport, electrical_metadata};
pub(crate) use custom_part::{
    CUSTOM_PARTS_DIR, custom_part, custom_part_list, load_custom_parts_dir, sample_part_json,
};
pub(crate) use graph::build_schematic_graph;
pub(crate) use net::{
    CircuitNetlist, Net, NetLabelScope, NetlistAnnotations, NoConnectMarker, WireNetSegment,
};
pub(crate) use pin::{CircuitPin, ElectricalType, NetlistPin, PinRef, PinRole};
// Only the items needed by code outside the model module are re-exported.
// module_pin, module_pin_defs, breadboard_pin_defs, module_pin_y are internal
// helpers used only within pin_defs.rs.
pub(crate) use pin_defs::{
    component_pin_defs, component_pins, component_size, module_pin_y, rotate_point,
};
pub(crate) use wire::{
    Wire, WireEndpoint, WireSegmentId, distance_to_segment, point_touches_wire_segment,
};
