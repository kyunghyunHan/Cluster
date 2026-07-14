use crate::model::CircuitNetlist;

/// Immutable input shared by all static ERC rules.
pub(crate) struct ErcContext<'a> {
    pub(crate) netlist: &'a CircuitNetlist,
}
