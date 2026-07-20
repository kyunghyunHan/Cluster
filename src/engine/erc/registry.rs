use super::{ErcCheck, ErcContext, ErcDependency};
use crate::engine::validation::{ErcSeverity, ErcViolation};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(crate) struct ErcSettings {
    pub(crate) disabled_rules: HashSet<&'static str>,
    pub(crate) severity_overrides: HashMap<&'static str, ErcSeverity>,
}

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

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn run_with_settings(
        &self,
        context: &ErcContext<'_>,
        settings: &ErcSettings,
    ) -> Vec<ErcViolation> {
        self.run_dependencies(
            context,
            settings,
            &[ErcDependency::Topology, ErcDependency::Values],
        )
    }

    pub(crate) fn run_dependencies(
        &self,
        context: &ErcContext<'_>,
        settings: &ErcSettings,
        dependencies: &[ErcDependency],
    ) -> Vec<ErcViolation> {
        let dependencies = dependencies.iter().copied().collect::<HashSet<_>>();
        let mut violations = Vec::new();
        let mut ids = HashSet::new();
        for rule in &self.rules {
            if !dependencies.contains(&rule.dependency())
                || !ids.insert(rule.id())
                || settings.disabled_rules.contains(rule.id())
            {
                continue;
            }
            let start = violations.len();
            rule.check(context, &mut violations);
            if let Some(&severity) = settings.severity_overrides.get(rule.id()) {
                for violation in &mut violations[start..] {
                    violation.severity = severity;
                }
            }
        }
        violations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::erc::FunctionRule;
    use crate::engine::validation::ErcRule;
    use crate::model::CircuitNetlist;

    fn emit_issue(_netlist: &CircuitNetlist, violations: &mut Vec<ErcViolation>) {
        violations.push(ErcViolation {
            rule: ErcRule::General,
            severity: ErcSeverity::Warning,
            component_id: None,
            wire_id: None,
            message: "test".to_string(),
        });
    }

    #[test]
    fn settings_disable_rules_and_override_severity() {
        let mut registry = ErcRegistry::default();
        registry.register(FunctionRule::new("test.rule", emit_issue));
        let netlist = CircuitNetlist::default();
        let context = ErcContext { netlist: &netlist };
        let overridden = registry.run_with_settings(
            &context,
            &ErcSettings {
                severity_overrides: [("test.rule", ErcSeverity::Info)].into_iter().collect(),
                ..Default::default()
            },
        );
        assert_eq!(overridden.len(), 1);
        assert_eq!(overridden[0].severity, ErcSeverity::Info);

        let disabled = registry.run_with_settings(
            &context,
            &ErcSettings {
                disabled_rules: ["test.rule"].into_iter().collect(),
                ..Default::default()
            },
        );
        assert!(disabled.is_empty());
    }
}
