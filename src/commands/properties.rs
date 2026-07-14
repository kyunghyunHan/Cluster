use crate::commands::ChangeSet;

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
    pub(crate) fn apply(self, app: &mut crate::CircuitApp) -> ChangeSet {
        match self {
            Self::SetComponentValue {
                component_id,
                value,
            } => {
                let Some(component) = app
                    .components
                    .iter_mut()
                    .find(|component| component.id == component_id)
                else {
                    return ChangeSet::none();
                };
                component.value = value;
            }
            Self::ToggleSwitch { component_id } => {
                let Some(component) = app
                    .components
                    .iter_mut()
                    .find(|component| component.id == component_id)
                else {
                    return ChangeSet::none();
                };
                let was_open = component.value.to_ascii_lowercase().contains("open");
                component.value = if was_open { "closed" } else { "open" }.to_string();
                let state = if was_open { "▶ CLOSED" } else { "■ OPEN" };
                app.status = format!("{} {state}", component.label);
            }
            Self::SetComponentProperties {
                component_id,
                label,
                value,
            } => {
                let Some(component) = app
                    .components
                    .iter_mut()
                    .find(|component| component.id == component_id)
                else {
                    return ChangeSet::none();
                };
                component.label = label;
                component.value = value;
            }
        }
        ChangeSet::schematic()
    }
}
