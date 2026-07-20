use super::ErcContext;
use crate::engine::validation::ErcViolation;
use crate::model::CircuitNetlist;

pub(crate) trait ErcCheck {
    fn id(&self) -> &'static str;
    fn dependency(&self) -> ErcDependency;
    fn check(&self, context: &ErcContext<'_>, violations: &mut Vec<ErcViolation>);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ErcDependency {
    Topology,
    Values,
}

pub(crate) struct FunctionRule {
    id: &'static str,
    dependency: ErcDependency,
    check: fn(&CircuitNetlist, &mut Vec<ErcViolation>),
}

impl FunctionRule {
    pub(crate) const fn new(
        id: &'static str,
        check: fn(&CircuitNetlist, &mut Vec<ErcViolation>),
    ) -> Self {
        Self {
            id,
            dependency: ErcDependency::Topology,
            check,
        }
    }

    pub(crate) const fn with_dependency(mut self, dependency: ErcDependency) -> Self {
        self.dependency = dependency;
        self
    }
}

impl ErcCheck for FunctionRule {
    fn id(&self) -> &'static str {
        self.id
    }

    fn dependency(&self) -> ErcDependency {
        self.dependency
    }

    fn check(&self, context: &ErcContext<'_>, violations: &mut Vec<ErcViolation>) {
        (self.check)(context.netlist, violations);
    }
}
