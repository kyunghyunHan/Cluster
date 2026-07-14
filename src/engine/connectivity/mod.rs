//! Internal stages for building the canonical connectivity graph.
//!
//! These modules operate only on schematic input and builder-local state.
//! Downstream features consume `CanonicalConnectivity`, never these stages.

pub(in crate::engine) mod diagnostics;
pub(in crate::engine) mod geometry;
pub(in crate::engine) mod labels;
pub(in crate::engine) mod union_find;
