use crate::commands::ChangeSet;
use crate::commands::context::{CommandContext, CommandOutcome, CommandPostAction};

pub(crate) enum DocumentCommand {
    Reset,
}

impl DocumentCommand {
    pub(crate) fn apply(self, context: &mut CommandContext<'_>) -> CommandOutcome {
        match self {
            Self::Reset => {
                context.reset_document_and_editor();
            }
        }
        CommandOutcome::new(ChangeSet::schematic())
            .with_post_action(CommandPostAction::ResetWorkspaceView)
    }
}
