use super::ErcContext;
use crate::engine::validation::ErcViolation;
use crate::model::CircuitNetlist;

pub(crate) trait ErcCheck {
    fn id(&self) -> &'static str;
    fn check(&self, context: &ErcContext<'_>, violations: &mut Vec<ErcViolation>);
}

pub(crate) struct FunctionRule {
    id: &'static str,
    check: fn(&CircuitNetlist, &mut Vec<ErcViolation>),
}

impl FunctionRule {
    pub(crate) const fn new(
        id: &'static str,
        check: fn(&CircuitNetlist, &mut Vec<ErcViolation>),
    ) -> Self {
        Self { id, check }
    }
}

impl ErcCheck for FunctionRule {
    fn id(&self) -> &'static str {
        self.id
    }

    fn check(&self, context: &ErcContext<'_>, violations: &mut Vec<ErcViolation>) {
        (self.check)(context.netlist, violations);
    }
}
