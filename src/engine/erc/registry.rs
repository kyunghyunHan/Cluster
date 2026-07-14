use super::{ErcCheck, ErcContext};
use crate::engine::validation::ErcViolation;
use std::collections::HashSet;

#[derive(Default)]
pub(crate) struct ErcRegistry {
    rules: Vec<Box<dyn ErcCheck>>,
}

impl ErcRegistry {
    pub(crate) fn register(&mut self, rule: impl ErcCheck + 'static) {
        debug_assert!(
            self.rules
                .iter()
                .all(|registered| registered.id() != rule.id()),
            "duplicate ERC registry id: {}",
            rule.id()
        );
        self.rules.push(Box::new(rule));
    }

    pub(crate) fn run(&self, context: &ErcContext<'_>) -> Vec<ErcViolation> {
        let mut violations = Vec::new();
        let mut ids = HashSet::new();
        for rule in &self.rules {
            if ids.insert(rule.id()) {
                rule.check(context, &mut violations);
            }
        }
        violations
    }
}
