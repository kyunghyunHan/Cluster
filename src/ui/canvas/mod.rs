//! Schematic canvas rendering and interaction boundaries.
//!
//! Extraction is intentionally incremental: coordinate transforms and the
//! background are owned here first, while the app keeps coordinating input.

pub(crate) mod background;
pub(crate) mod components;
pub(crate) mod current_flow;
pub(crate) mod hit_test;
pub(crate) mod interaction;
pub(crate) mod overlays;
pub(crate) mod selection;
pub(crate) mod view;
pub(crate) mod wires;

pub(crate) use background::draw_grid;
pub(crate) use hit_test::{
    hit_test, hit_test_component, hit_test_wire, hit_test_wire_control_point,
};
pub(crate) use overlays::draw_probe_card;
pub(crate) use selection::selection_summary;
pub(crate) use view::CanvasView;
