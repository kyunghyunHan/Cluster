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
pub(crate) struct CommandDirtyState {
    pub(crate) document_changed: bool,
    pub(crate) connectivity_dirty: bool,
    pub(crate) validation_dirty: bool,
    pub(crate) simulation_dirty: bool,
    pub(crate) pcb_dirty: bool,
    pub(crate) autosave_dirty: bool,
}

impl CommandDirtyState {
    pub(crate) const fn document() -> Self {
        Self {
            document_changed: true,
            connectivity_dirty: true,
            validation_dirty: true,
            simulation_dirty: true,
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
    pub(crate) fn apply(self, app: &mut crate::CircuitApp) -> CommandDirtyState {
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
    pub(crate) fn execute_editor_command(&mut self, command: EditorCommand) -> CommandDirtyState {
        let snapshot = self.snapshot();
        let dirty = command.apply(self);
        if dirty.document_changed {
            self.history_state.undo.push(snapshot);
            if self.history_state.undo.len() > 80 {
                self.history_state.undo.remove(0);
            }
            self.history_state.redo.clear();
        }
        self.apply_command_dirty(dirty);
        dirty
    }

    pub(crate) fn execute_continuous_editor_command(
        &mut self,
        command: EditorCommand,
    ) -> CommandDirtyState {
        let dirty = command.apply(self);
        self.apply_command_dirty(dirty);
        dirty
    }

    fn apply_command_dirty(&mut self, dirty: CommandDirtyState) {
        if dirty.document_changed || dirty.autosave_dirty {
            self.history_state.dirty = true;
        }
        self.dirty_flags.geometry_dirty |= dirty.document_changed;
        self.dirty_flags.connectivity_dirty |= dirty.connectivity_dirty;
        self.dirty_flags.validation_dirty |= dirty.validation_dirty;
        self.dirty_flags.simulation_dirty |= dirty.simulation_dirty;
        self.dirty_flags.pcb_sync_dirty |= dirty.pcb_dirty;
        if dirty.connectivity_dirty {
            self.invalidate_analysis_cache();
        }
        if dirty.simulation_dirty {
            self.simulation_run_state = if self.simulate {
                crate::ui::app::SimulationRunState::Dirty
            } else {
                crate::ui::app::SimulationRunState::Stopped
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComponentKind;
    use egui::Pos2;

    #[test]
    fn document_command_returns_all_required_dirty_states() {
        let dirty = CommandDirtyState::document();
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
        let revision = app.circuit_revision;
        let dirty = app.execute_editor_command(EditorCommand::Component(
            crate::commands::component::ComponentCommand::Place {
                kind: ComponentKind::Resistor,
                position: Pos2::new(100.0, 100.0),
            },
        ));

        assert_eq!(app.components.len(), 1);
        assert_eq!(app.history_state.undo.len(), 1);
        assert!(app.circuit_revision > revision);
        assert!(app.cached_connectivity.is_none());
        assert_eq!(dirty, CommandDirtyState::document());
    }
}
