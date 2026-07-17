//! Document command boundary.
//!
//! UI code submits an [`EditorCommand`]. Command handlers are the only
//! production code allowed to mutate schematic collections directly.

pub(crate) mod component;
mod context;
pub(crate) mod document;
pub(crate) mod lessons;
pub(crate) mod pcb;
pub(crate) mod properties;
pub(crate) mod selection;
pub(crate) mod wiring;

use component::ComponentCommand;
use context::{CommandContext, CommandOutcome, CommandPostAction};
use document::DocumentCommand;
use lessons::LessonCommand;
use pcb::PcbCommand;
use properties::PropertiesCommand;
use selection::SelectionCommand;
use wiring::WiringCommand;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ChangeSet {
    pub(crate) document_changed: bool,
    pub(crate) connectivity_dirty: bool,
    pub(crate) validation_dirty: bool,
    pub(crate) simulation_dirty: bool,
    pub(crate) pcb_dirty: bool,
    pub(crate) autosave_dirty: bool,
}

impl ChangeSet {
    pub(crate) const fn schematic() -> Self {
        Self {
            document_changed: true,
            connectivity_dirty: true,
            validation_dirty: true,
            simulation_dirty: true,
            pcb_dirty: true,
            autosave_dirty: true,
        }
    }

    pub(crate) const fn properties() -> Self {
        Self {
            document_changed: true,
            connectivity_dirty: false,
            validation_dirty: true,
            simulation_dirty: true,
            pcb_dirty: true,
            autosave_dirty: true,
        }
    }

    pub(crate) const fn board() -> Self {
        Self {
            document_changed: true,
            connectivity_dirty: false,
            validation_dirty: false,
            simulation_dirty: false,
            pcb_dirty: true,
            autosave_dirty: true,
        }
    }

    pub(crate) const fn none() -> Self {
        Self {
            document_changed: false,
            connectivity_dirty: false,
            validation_dirty: false,
            simulation_dirty: false,
            pcb_dirty: false,
            autosave_dirty: false,
        }
    }

    pub(crate) const fn needs_repaint(self) -> bool {
        self.document_changed
            || self.connectivity_dirty
            || self.validation_dirty
            || self.simulation_dirty
            || self.pcb_dirty
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandMergeKey {
    ComponentMove,
    WireControlPoint(u64),
    ComponentProperties(u64),
    BoardFootprint(u64),
}

#[allow(dead_code)] // Variants are populated as actions are extracted module by module.
pub(crate) enum EditorCommand {
    Component(ComponentCommand),
    Wiring(WiringCommand),
    Selection(SelectionCommand),
    Properties(PropertiesCommand),
    Document(DocumentCommand),
    Pcb(PcbCommand),
    Lesson(LessonCommand),
}

impl EditorCommand {
    pub(crate) const fn description(&self) -> &'static str {
        match self {
            Self::Component(ComponentCommand::Place { .. }) => "Place component",
            Self::Component(ComponentCommand::PlaceCustom { .. }) => "Place custom component",
            Self::Component(ComponentCommand::Paste { .. }) => "Paste selection",
            Self::Component(ComponentCommand::Move { .. }) => "Move component",
            Self::Wiring(WiringCommand::Add { .. }) => "Place wire",
            Self::Wiring(WiringCommand::MoveControlPoint { .. }) => "Move wire point",
            Self::Wiring(WiringCommand::InsertControlPoint { .. }) => "Insert wire point",
            Self::Wiring(WiringCommand::Tidy { .. }) => "Tidy wires",
            Self::Selection(SelectionCommand::Delete) => "Delete selection",
            Self::Selection(SelectionCommand::Rotate) => "Rotate component",
            Self::Selection(SelectionCommand::Duplicate) => "Duplicate selection",
            Self::Selection(SelectionCommand::Align(_)) => "Align components",
            Self::Selection(SelectionCommand::Distribute { .. }) => "Distribute components",
            Self::Properties(PropertiesCommand::SetComponentValue { .. }) => "Edit value",
            Self::Properties(PropertiesCommand::SetComponentProperties { .. }) => {
                "Edit component properties"
            }
            Self::Properties(PropertiesCommand::ToggleSwitch { .. }) => "Toggle switch",
            Self::Document(DocumentCommand::Reset) => "New document",
            Self::Pcb(PcbCommand::MoveFootprint { .. }) => "Move PCB footprint",
            Self::Pcb(PcbCommand::RotateFootprint { .. }) => "Rotate PCB footprint",
            Self::Pcb(PcbCommand::AddTrack(_)) => "Route PCB track",
            Self::Pcb(PcbCommand::RemoveTrack { .. }) => "Remove PCB track",
            Self::Pcb(PcbCommand::AddVia(_)) => "Place PCB via",
            Self::Pcb(PcbCommand::RemoveVia { .. }) => "Remove PCB via",
            Self::Pcb(PcbCommand::SetOutline(_)) => "Edit board outline",
            Self::Lesson(LessonCommand::Noop) => "Apply lesson action",
        }
    }

    pub(crate) const fn merge_key(&self) -> Option<CommandMergeKey> {
        match self {
            Self::Component(ComponentCommand::Move { .. }) => Some(CommandMergeKey::ComponentMove),
            Self::Wiring(WiringCommand::MoveControlPoint { wire_id, .. }) => {
                Some(CommandMergeKey::WireControlPoint(*wire_id))
            }
            Self::Properties(PropertiesCommand::SetComponentValue { component_id, .. })
            | Self::Properties(PropertiesCommand::SetComponentProperties {
                component_id, ..
            }) => Some(CommandMergeKey::ComponentProperties(*component_id)),
            Self::Pcb(PcbCommand::MoveFootprint { footprint_id, .. }) => {
                Some(CommandMergeKey::BoardFootprint(*footprint_id))
            }
            _ => None,
        }
    }

    fn apply(self, context: &mut CommandContext<'_>) -> CommandOutcome {
        match self {
            Self::Component(command) => command.apply(context),
            Self::Wiring(command) => command.apply(context),
            Self::Selection(command) => command.apply(context),
            Self::Properties(command) => command.apply(context),
            Self::Document(command) => command.apply(context),
            Self::Pcb(command) => command.apply(context),
            Self::Lesson(command) => command.apply(context),
        }
    }
}

impl crate::CircuitApp {
    pub(crate) fn execute_editor_command(&mut self, command: EditorCommand) -> ChangeSet {
        let snapshot = self.snapshot();
        let description = command.description();
        let merge_key = command.merge_key();
        let outcome = command.apply(&mut CommandContext::new(self));
        let changes = outcome.changes;
        if changes.document_changed {
            let delta = crate::editor::delta::DocumentDelta::between(&snapshot, &self.snapshot());
            self.push_history_delta(delta, description, merge_key);
        }
        self.dispatch_changes(changes);
        self.apply_command_outcome(outcome);
        changes
    }

    pub(crate) fn execute_continuous_editor_command(
        &mut self,
        command: EditorCommand,
    ) -> ChangeSet {
        let outcome = command.apply(&mut CommandContext::new(self));
        let changes = outcome.changes;
        self.dispatch_changes(changes);
        self.apply_command_outcome(outcome);
        changes
    }

    fn apply_command_outcome(&mut self, outcome: CommandOutcome) {
        if let Some(status) = outcome.status {
            self.status = status;
        }
        if outcome.post_action == CommandPostAction::ResetWorkspaceView {
            self.hovered_net_wire = None;
            self.highlighted_net_wires.clear();
            self.inline_edit = None;
            self.context_menu = None;
            self.zoom = 1.0;
            self.pan = egui::Vec2::ZERO;
        }
    }

    pub(crate) fn dispatch_changes(&mut self, changes: ChangeSet) {
        if changes.autosave_dirty {
            self.editor.history.dirty = true;
        }
        self.analysis.dirty_flags.geometry_dirty |= changes.document_changed;
        self.analysis.dirty_flags.connectivity_dirty |= changes.connectivity_dirty;
        self.analysis.dirty_flags.validation_dirty |= changes.validation_dirty;
        self.analysis.dirty_flags.simulation_dirty |= changes.simulation_dirty;
        self.analysis.dirty_flags.pcb_sync_dirty |= changes.connectivity_dirty;
        self.analysis.dirty_flags.pcb_drc_dirty |= changes.pcb_dirty;
        if changes.pcb_dirty {
            self.analysis.pcb_drc.clear();
        }
        if changes.connectivity_dirty {
            self.invalidate_connectivity_cache();
        } else if changes.simulation_dirty {
            self.invalidate_simulation_cache();
        }
        if changes.simulation_dirty {
            self.simulation_run_state = if self.simulate {
                crate::ui::app::SimulationRunState::Dirty
            } else {
                crate::ui::app::SimulationRunState::Stopped
            };
        }
        self.workspace_state.repaint_requested |= changes.needs_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComponentKind;
    use egui::Pos2;

    #[test]
    fn schematic_change_has_precise_invalidation_semantics() {
        let dirty = ChangeSet::schematic();
        assert!(dirty.document_changed);
        assert!(dirty.connectivity_dirty);
        assert!(dirty.validation_dirty);
        assert!(dirty.simulation_dirty);
        assert!(dirty.pcb_dirty);
        assert!(dirty.autosave_dirty);
    }

    #[test]
    fn editor_command_owns_history_and_cache_invalidation() {
        let mut app = crate::CircuitApp::new();
        let revision = app.analysis.circuit_revision;
        let dirty = app.execute_editor_command(EditorCommand::Component(
            crate::commands::component::ComponentCommand::Place {
                kind: ComponentKind::Resistor,
                position: Pos2::new(100.0, 100.0),
            },
        ));

        assert_eq!(app.components.len(), 1);
        assert_eq!(app.editor.history.undo.len(), 1);
        assert!(app.analysis.circuit_revision > revision);
        assert!(app.analysis.cached_connectivity.is_none());
        assert_eq!(dirty, ChangeSet::schematic());
    }

    #[test]
    fn pcb_primitive_command_is_undoable_and_invalidates_drc_only() {
        use crate::model::cad::Point2;
        use crate::pcb::layer::BoardLayer;
        use crate::pcb::track::TrackSegment;

        let mut app = crate::CircuitApp::new();
        let revision = app.analysis.circuit_revision;
        let changes =
            app.execute_editor_command(EditorCommand::Pcb(PcbCommand::AddTrack(TrackSegment {
                id: 1,
                net_id: 0,
                layer: BoardLayer::FrontCopper,
                start: Point2::new(1.0, 1.0),
                end: Point2::new(10.0, 1.0),
                width_mm: 0.25,
            })));

        assert_eq!(app.document.board.tracks.len(), 1);
        assert!(changes.document_changed);
        assert!(changes.pcb_dirty);
        assert!(!changes.connectivity_dirty);
        assert_eq!(app.analysis.circuit_revision, revision);
        assert!(app.analysis.dirty_flags.pcb_drc_dirty);

        app.undo();
        assert!(app.document.board.tracks.is_empty());
        app.redo();
        assert_eq!(app.document.board.tracks.len(), 1);
    }
}
