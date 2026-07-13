use crate::app::Tool;
use crate::commands::CommandDirtyState;
use crate::model::Counters;
use egui::Vec2;

pub(crate) enum DocumentCommand {
    Reset,
}

impl DocumentCommand {
    pub(crate) fn apply(self, app: &mut crate::CircuitApp) -> CommandDirtyState {
        match self {
            Self::Reset => {
                app.components.clear();
                app.wires.clear();
                app.selected = None;
                app.multi_selected.clear();
                app.drag = None;
                app.draft_wire.clear();
                app.wire_from_select = false;
                app.hovered_net_wire = None;
                app.highlighted_net_wires.clear();
                app.snap_target = None;
                app.inline_edit = None;
                app.context_menu = None;
                app.counters = Counters::default();
                app.next_id = 1;
                app.tool = Tool::Select;
                app.zoom = 1.0;
                app.pan = Vec2::ZERO;
            }
        }
        CommandDirtyState::document()
    }
}
