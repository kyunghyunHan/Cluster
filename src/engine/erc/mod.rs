//! Registry-driven electrical rule checking.

mod context;
mod registry;
mod rule;
pub(crate) mod rules;

pub(crate) use context::ErcContext;
pub(crate) use registry::{ErcRegistry, ErcSettings};
pub(crate) use rule::{ErcCheck, ErcDependency, FunctionRule};
