use crate::commands::context::{CommandContext, CommandOutcome};

#[allow(dead_code)]
pub(crate) enum LessonCommand {
    Noop,
}

impl LessonCommand {
    pub(crate) fn apply(self, _context: &mut CommandContext<'_>) -> CommandOutcome {
        CommandOutcome::unchanged()
    }
}
