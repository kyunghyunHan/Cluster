//! Current-flow rendering lives in `ui::current_flow` during the compatibility
//! window. Re-exporting it here establishes the final canvas-owned boundary.
pub(crate) use crate::ui::current_flow::*;
