//! Data-driven guided lesson definitions and checkpoint evaluation.

#![allow(dead_code)] // The existing hard-coded lesson UI migrates onto this boundary incrementally.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LessonDefinition {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) stages: Vec<LessonStage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LessonStage {
    pub(crate) id: String,
    pub(crate) instruction: String,
    pub(crate) checkpoints: Vec<LessonCheckpoint>,
    #[serde(default)]
    pub(crate) hints: Vec<LessonHint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LessonHint {
    pub(crate) title: String,
    pub(crate) body: String,
    #[serde(default)]
    pub(crate) target_component: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum LessonCheckpoint {
    Connected {
        from_component: u64,
        from_pin: String,
        to_component: u64,
        to_pin: String,
    },
    ComponentValueRange {
        component_id: u64,
        minimum: f64,
        maximum: f64,
    },
    ErcIssueAbsent {
        rule_id: String,
    },
    SimulatedVoltageRange {
        component_id: u64,
        minimum_volts: f64,
        maximum_volts: f64,
    },
    SimulatedCurrentRange {
        component_id: u64,
        minimum_amps: f64,
        maximum_amps: f64,
    },
    PcbPlaced {
        component_id: u64,
    },
    DrcClean,
    ExportCompleted {
        format: String,
    },
}

pub(crate) trait LessonEvaluationContext {
    fn pins_connected(&self, from: (u64, &str), to: (u64, &str)) -> bool;
    fn component_value(&self, component_id: u64) -> Option<f64>;
    fn has_erc_issue(&self, rule_id: &str) -> bool;
    fn component_voltage(&self, component_id: u64) -> Option<f64>;
    fn component_current(&self, component_id: u64) -> Option<f64>;
    fn pcb_is_placed(&self, component_id: u64) -> bool;
    fn drc_error_count(&self) -> usize;
    fn export_completed(&self, format: &str) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LessonCheckpointResult {
    pub(crate) passed: bool,
    pub(crate) message: String,
}

impl LessonCheckpoint {
    pub(crate) fn evaluate(
        &self,
        context: &impl LessonEvaluationContext,
    ) -> LessonCheckpointResult {
        let (passed, message) = match self {
            Self::Connected {
                from_component,
                from_pin,
                to_component,
                to_pin,
            } => (
                context.pins_connected(
                    (*from_component, from_pin.as_str()),
                    (*to_component, to_pin.as_str()),
                ),
                "Required pins are connected.",
            ),
            Self::ComponentValueRange {
                component_id,
                minimum,
                maximum,
            } => (
                context
                    .component_value(*component_id)
                    .is_some_and(|value| value >= *minimum && value <= *maximum),
                "Component value is in range.",
            ),
            Self::ErcIssueAbsent { rule_id } => (
                !context.has_erc_issue(rule_id),
                "Required ERC issue is absent.",
            ),
            Self::SimulatedVoltageRange {
                component_id,
                minimum_volts,
                maximum_volts,
            } => (
                context
                    .component_voltage(*component_id)
                    .is_some_and(|value| value >= *minimum_volts && value <= *maximum_volts),
                "Simulated voltage is in range.",
            ),
            Self::SimulatedCurrentRange {
                component_id,
                minimum_amps,
                maximum_amps,
            } => (
                context
                    .component_current(*component_id)
                    .is_some_and(|value| value >= *minimum_amps && value <= *maximum_amps),
                "Simulated current is in range.",
            ),
            Self::PcbPlaced { component_id } => (
                context.pcb_is_placed(*component_id),
                "Component is placed on the PCB.",
            ),
            Self::DrcClean => (context.drc_error_count() == 0, "PCB DRC is clean."),
            Self::ExportCompleted { format } => (
                context.export_completed(format),
                "Required export is complete.",
            ),
        };
        LessonCheckpointResult {
            passed,
            message: if passed {
                message.to_string()
            } else {
                format!("Not complete: {}", message.to_ascii_lowercase())
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ReadyLesson;

    impl LessonEvaluationContext for ReadyLesson {
        fn pins_connected(&self, _: (u64, &str), _: (u64, &str)) -> bool {
            true
        }
        fn component_value(&self, _: u64) -> Option<f64> {
            Some(1_000.0)
        }
        fn has_erc_issue(&self, _: &str) -> bool {
            false
        }
        fn component_voltage(&self, _: u64) -> Option<f64> {
            Some(3.3)
        }
        fn component_current(&self, _: u64) -> Option<f64> {
            Some(0.0033)
        }
        fn pcb_is_placed(&self, _: u64) -> bool {
            true
        }
        fn drc_error_count(&self) -> usize {
            0
        }
        fn export_completed(&self, _: &str) -> bool {
            true
        }
    }

    #[test]
    fn every_checkpoint_kind_can_be_evaluated_from_shared_analysis_state() {
        let checkpoints = [
            LessonCheckpoint::Connected {
                from_component: 1,
                from_pin: "A".into(),
                to_component: 2,
                to_pin: "B".into(),
            },
            LessonCheckpoint::ComponentValueRange {
                component_id: 1,
                minimum: 900.0,
                maximum: 1_100.0,
            },
            LessonCheckpoint::ErcIssueAbsent {
                rule_id: "short".into(),
            },
            LessonCheckpoint::SimulatedVoltageRange {
                component_id: 1,
                minimum_volts: 3.0,
                maximum_volts: 3.6,
            },
            LessonCheckpoint::SimulatedCurrentRange {
                component_id: 1,
                minimum_amps: 0.003,
                maximum_amps: 0.004,
            },
            LessonCheckpoint::PcbPlaced { component_id: 1 },
            LessonCheckpoint::DrcClean,
            LessonCheckpoint::ExportCompleted {
                format: "svg".into(),
            },
        ];
        assert!(
            checkpoints
                .iter()
                .all(|checkpoint| checkpoint.evaluate(&ReadyLesson).passed)
        );
    }
}
