//! Registry-driven electrical rule checking.

mod context;
mod registry;
mod rule;

pub(crate) use context::ErcContext;
pub(crate) use registry::ErcRegistry;
pub(crate) use rule::{ErcCheck, FunctionRule};
