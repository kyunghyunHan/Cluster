//! Document command boundary.
//!
//! UI code submits an [`EditorCommand`]. Command handlers are the only
//! production code allowed to mutate schematic collections directly.

pub(crate) mod component;
pub(crate) mod document;
pub(crate) mod lessons;
pub(crate) mod pcb;
pub(crate) mod properties;
pub(crate) mod selection;
pub(crate) mod wiring;

use component::ComponentCommand;
use document::DocumentCommand;
use lessons::LessonCommand;
use pcb::PcbCommand;
use properties::PropertiesCommand;
use selection::SelectionCommand;
use wiring::WiringCommand;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ChangeSet {
    pub(crate) document_changed: bool,
    pub(crate) connectivity_changed: bool,
    pub(crate) electrical_changed: bool,
    pub(crate) board_changed: bool,
    pub(crate) visual_only: bool,
    pub(crate) autosave_eligible: bool,
}

impl ChangeSet {
    pub(crate) const fn schematic() -> Self {
        Self {
            document_changed: true,
            connectivity_changed: true,
            electrical_changed: true,
            board_changed: false,
            visual_only: false,
            autosave_eligible: true,
        }
    }

    pub(crate) const fn none() -> Self {
        Self {
            document_changed: false,
            connectivity_changed: false,
            electrical_changed: false,
            board_changed: false,
            visual_only: false,
            autosave_eligible: false,
        }
    }

    pub(crate) const fn needs_repaint(self) -> bool {
        self.document_changed
            || self.connectivity_changed
            || self.electrical_changed
            || self.board_changed
            || self.visual_only
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

    pub(crate) fn apply(self, app: &mut crate::CircuitApp) -> ChangeSet {
        match self {
            Self::Component(command) => command.apply(app),
            Self::Wiring(command) => command.apply(app),
            Self::Selection(command) => command.apply(app),
            Self::Properties(command) => command.apply(app),
            Self::Document(command) => command.apply(app),
            Self::Pcb(command) => command.apply(app),
            Self::Lesson(command) => command.apply(app),
        }
    }
}

impl crate::CircuitApp {
    pub(crate) fn execute_editor_command(&mut self, command: EditorCommand) -> ChangeSet {
        let snapshot = self.snapshot();
        let description = command.description();
        let merge_key = command.merge_key();
        let changes = command.apply(self);
        if changes.document_changed || changes.board_changed {
            let merges_with_previous = merge_key.is_some()
                && self.editor.history.undo.last().is_some_and(|entry| {
                    entry.merge_key == merge_key
                        && entry.created_at.elapsed() <= std::time::Duration::from_millis(750)
                });
            if !merges_with_previous {
                self.editor.history.undo.push(crate::ui::app::HistoryEntry {
                    snapshot,
                    description,
                    merge_key,
                    created_at: std::time::Instant::now(),
                });
            }
            if self.editor.history.undo.len() > 80 {
                self.editor.history.undo.remove(0);
            }
            self.editor.history.redo.clear();
        }
        self.dispatch_changes(changes);
        changes
    }

    pub(crate) fn execute_continuous_editor_command(
        &mut self,
        command: EditorCommand,
    ) -> ChangeSet {
        let changes = command.apply(self);
        self.dispatch_changes(changes);
        changes
    }

    pub(crate) fn dispatch_changes(&mut self, changes: ChangeSet) {
        if changes.autosave_eligible {
            self.editor.history.dirty = true;
        }
        self.analysis.dirty_flags.geometry_dirty |= changes.document_changed;
        self.analysis.dirty_flags.connectivity_dirty |= changes.connectivity_changed;
        self.analysis.dirty_flags.validation_dirty |=
            changes.connectivity_changed || changes.electrical_changed;
        self.analysis.dirty_flags.simulation_dirty |=
            changes.connectivity_changed || changes.electrical_changed;
        self.analysis.dirty_flags.pcb_sync_dirty |= changes.connectivity_changed;
        self.analysis.dirty_flags.pcb_drc_dirty |= changes.board_changed;
        if changes.board_changed {
            self.analysis.pcb_drc.clear();
        }
        if changes.connectivity_changed {
            self.invalidate_connectivity_cache();
        } else if changes.electrical_changed {
            self.invalidate_simulation_cache();
        }
        if changes.connectivity_changed || changes.electrical_changed {
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
        assert!(dirty.connectivity_changed);
        assert!(!dirty.board_changed);
        assert!(!dirty.visual_only);
        assert!(dirty.electrical_changed);
        assert!(dirty.autosave_eligible);
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
        assert!(changes.board_changed);
        assert!(!changes.connectivity_changed);
        assert_eq!(app.analysis.circuit_revision, revision);
        assert!(app.analysis.dirty_flags.pcb_drc_dirty);

        app.undo();
        assert!(app.document.board.tracks.is_empty());
        app.redo();
        assert_eq!(app.document.board.tracks.len(), 1);
    }
}
