use crate::commands::ChangeSet;

#[allow(dead_code)]
pub(crate) enum LessonCommand {
    Noop,
}

impl LessonCommand {
    pub(crate) fn apply(self, _app: &mut crate::CircuitApp) -> ChangeSet {
        ChangeSet::none()
    }
}
