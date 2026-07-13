use crate::commands::CommandDirtyState;

#[allow(dead_code)]
pub(crate) enum PcbCommand {
    Noop,
}

impl PcbCommand {
    pub(crate) fn apply(self, _app: &mut crate::CircuitApp) -> CommandDirtyState {
        CommandDirtyState::none()
    }
}
