use crate::commands::CommandDirtyState;

#[allow(dead_code)]
pub(crate) enum LessonCommand {
    Noop,
}

impl LessonCommand {
    pub(crate) fn apply(self, _app: &mut crate::CircuitApp) -> CommandDirtyState {
        CommandDirtyState::none()
    }
}
