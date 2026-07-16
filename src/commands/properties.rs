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
                let Some(component) = context
                    .components_mut()
                    .iter_mut()
                    .find(|component| component.id == component_id)
                else {
                    return CommandOutcome::unchanged();
                };
                component.value = value;
            }
            Self::ToggleSwitch { component_id } => {
                let Some(component) = context
                    .components_mut()
                    .iter_mut()
                    .find(|component| component.id == component_id)
                else {
                    return CommandOutcome::unchanged();
                };
                let was_open = component.value.to_ascii_lowercase().contains("open");
                component.value = if was_open { "closed" } else { "open" }.to_string();
                let state = if was_open { "▶ CLOSED" } else { "■ OPEN" };
                let status = format!("{} {state}", component.label);
                return CommandOutcome::new(ChangeSet::properties()).with_status(status);
            }
            Self::SetComponentProperties {
                component_id,
                label,
                value,
            } => {
                let Some(component) = context
                    .components_mut()
                    .iter_mut()
                    .find(|component| component.id == component_id)
                else {
                    return CommandOutcome::unchanged();
                };
                component.label = label;
                component.value = value;
            }
        }
        CommandOutcome::new(ChangeSet::properties())
    }
}
