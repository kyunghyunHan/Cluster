use crate::commands::ChangeSet;
use crate::commands::context::{CommandContext, CommandOutcome};

#[allow(dead_code)]
pub(crate) enum PropertiesCommand {
    SetComponentValue {
        component_id: u64,
        value: String,
    },
    SetComponentProperties {
        component_id: u64,
        label: String,
        value: String,
    },
    ToggleSwitch {
        component_id: u64,
    },
}

impl PropertiesCommand {
    pub(crate) fn apply(self, context: &mut CommandContext<'_>) -> CommandOutcome {
        match self {
            Self::SetComponentValue {
                component_id,
                value,
            } => {
                if !context.update_component(component_id, |component| component.value = value) {
                    return CommandOutcome::unchanged();
                }
            }
            Self::ToggleSwitch { component_id } => {
                let mut status = None;
                if !context.update_component(component_id, |component| {
                    let was_open = component.value.to_ascii_lowercase().contains("open");
                    component.value = if was_open { "closed" } else { "open" }.to_string();
                    let state = if was_open { "▶ CLOSED" } else { "■ OPEN" };
                    status = Some(format!("{} {state}", component.label));
                }) {
                    return CommandOutcome::unchanged();
                }
                return CommandOutcome::new(ChangeSet::properties())
                    .with_status(status.unwrap_or_default());
            }
            Self::SetComponentProperties {
                component_id,
                label,
                value,
            } => {
                if !context.update_component(component_id, |component| {
                    component.label = label;
                    component.value = value;
                }) {
                    return CommandOutcome::unchanged();
                }
            }
        }
        CommandOutcome::new(ChangeSet::properties())
    }
}
