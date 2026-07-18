//! Document command boundary.
//!
//! UI code submits an [`EditorCommand`]. Command handlers are the only
//! production code allowed to mutate schematic collections directly.

pub(crate) mod component;
mod context;
pub(crate) mod document;
pub(crate) mod pcb;
pub(crate) mod properties;
pub(crate) mod selection;
pub(crate) mod wiring;

use component::ComponentCommand;
use context::{CommandContext, CommandOutcome, CommandPostAction};
use document::DocumentCommand;
use pcb::PcbCommand;
use properties::PropertiesCommand;
use selection::SelectionCommand;
use wiring::WiringCommand;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ChangeSet {
    pub(crate) persistence_changed: bool,
    pub(crate) schematic_geometry_changed: bool,
    pub(crate) schematic_connectivity_changed: bool,
    pub(crate) electrical_values_changed: bool,
    pub(crate) simulation_topology_changed: bool,
    pub(crate) simulation_parameters_changed: bool,
    pub(crate) pcb_sync_changed: bool,
    pub(crate) pcb_geometry_changed: bool,
    pub(crate) pcb_rules_changed: bool,
    pub(crate) visual_only: bool,
}

impl ChangeSet {
    pub(crate) const fn schematic() -> Self {
        Self {
            persistence_changed: true,
            schematic_geometry_changed: true,
            schematic_connectivity_changed: true,
            simulation_topology_changed: true,
            pcb_sync_changed: true,
            ..Self::none()
        }
    }

    pub(crate) const fn properties() -> Self {
        Self {
            persistence_changed: true,
            electrical_values_changed: true,
            simulation_parameters_changed: true,
            pcb_sync_changed: true,
            ..Self::none()
        }
    }

    pub(crate) const fn board() -> Self {
        Self {
            persistence_changed: true,
            pcb_geometry_changed: true,
            ..Self::none()
        }
    }

    pub(crate) const fn none() -> Self {
        Self {
            persistence_changed: false,
            schematic_geometry_changed: false,
            schematic_connectivity_changed: false,
            electrical_values_changed: false,
            simulation_topology_changed: false,
            simulation_parameters_changed: false,
            pcb_sync_changed: false,
            pcb_geometry_changed: false,
            pcb_rules_changed: false,
            visual_only: false,
        }
    }

    pub(crate) const fn needs_repaint(self) -> bool {
        self.persistence_changed || self.visual_only
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
            Self::Pcb(PcbCommand::MoveFootprints(_)) => "Move PCB footprints",
            Self::Pcb(PcbCommand::RotateFootprint { .. }) => "Rotate PCB footprint",
            Self::Pcb(PcbCommand::RotateFootprints { .. }) => "Rotate PCB footprints",
            Self::Pcb(PcbCommand::FlipFootprints { .. }) => "Flip PCB footprints",
            Self::Pcb(PcbCommand::AddTrack(_)) => "Route PCB track",
            Self::Pcb(PcbCommand::AddRoute { .. }) => "Route PCB connection",
            Self::Pcb(PcbCommand::RemoveTrack { .. }) => "Remove PCB track",
            Self::Pcb(PcbCommand::DeleteTracks { .. }) => "Delete PCB tracks",
            Self::Pcb(PcbCommand::EditTrack(_)) => "Edit PCB track",
            Self::Pcb(PcbCommand::AddVia(_)) => "Place PCB via",
            Self::Pcb(PcbCommand::RemoveVia { .. }) => "Remove PCB via",
            Self::Pcb(PcbCommand::DeleteVias { .. }) => "Delete PCB vias",
            Self::Pcb(PcbCommand::SetOutline(_)) => "Edit board outline",
            Self::Pcb(PcbCommand::ChangeNetClass(_)) => "Change PCB net class",
            Self::Pcb(PcbCommand::ApplyEco { .. }) => "Apply schematic PCB changes",
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
        if changes.persistence_changed {
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
        if changes.persistence_changed {
            self.editor.history.dirty = true;
        }
        self.analysis.dirty_flags.geometry_dirty |= changes.schematic_geometry_changed;
        self.analysis.dirty_flags.connectivity_dirty |= changes.schematic_connectivity_changed;
        self.analysis.dirty_flags.validation_dirty |=
            changes.schematic_connectivity_changed || changes.electrical_values_changed;
        self.analysis.dirty_flags.simulation_dirty |=
            changes.simulation_topology_changed || changes.simulation_parameters_changed;
        self.analysis.dirty_flags.pcb_sync_dirty |= changes.pcb_sync_changed;
        self.analysis.dirty_flags.pcb_drc_dirty |=
            changes.pcb_geometry_changed || changes.pcb_rules_changed;
        if changes.pcb_geometry_changed || changes.pcb_rules_changed {
            self.analysis.pcb_drc.clear();
        }
        if changes.schematic_connectivity_changed {
            self.invalidate_connectivity_cache();
        } else if changes.simulation_topology_changed || changes.simulation_parameters_changed {
            self.invalidate_simulation_cache();
        }
        if changes.simulation_topology_changed || changes.simulation_parameters_changed {
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
        assert!(dirty.persistence_changed);
        assert!(dirty.schematic_geometry_changed);
        assert!(dirty.schematic_connectivity_changed);
        assert!(dirty.simulation_topology_changed);
        assert!(dirty.pcb_sync_changed);
        assert!(!dirty.pcb_geometry_changed);
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
        assert!(changes.persistence_changed);
        assert!(changes.pcb_geometry_changed);
        assert!(!changes.schematic_connectivity_changed);
        assert_eq!(app.analysis.circuit_revision, revision);
        assert!(app.analysis.dirty_flags.pcb_drc_dirty);

        app.undo();
        assert!(app.document.board.tracks.is_empty());
        app.redo();
        assert_eq!(app.document.board.tracks.len(), 1);
    }

    #[test]
    fn complete_route_with_via_is_one_undo_item() {
        use crate::model::cad::Point2;
        use crate::pcb::layer::BoardLayer;
        use crate::pcb::track::TrackSegment;
        use crate::pcb::via::Via;

        let mut app = crate::CircuitApp::new();
        app.execute_editor_command(EditorCommand::Pcb(PcbCommand::AddRoute {
            tracks: vec![
                TrackSegment {
                    id: 1,
                    net_id: 4,
                    layer: BoardLayer::FrontCopper,
                    start: Point2::new(5.0, 5.0),
                    end: Point2::new(10.0, 5.0),
                    width_mm: 0.25,
                },
                TrackSegment {
                    id: 2,
                    net_id: 4,
                    layer: BoardLayer::BackCopper,
                    start: Point2::new(10.0, 5.0),
                    end: Point2::new(15.0, 10.0),
                    width_mm: 0.25,
                },
            ],
            vias: vec![Via {
                id: 3,
                net_id: 4,
                position: Point2::new(10.0, 5.0),
                diameter_mm: 0.6,
                drill_mm: 0.3,
            }],
        }));

        assert_eq!(app.editor.history.undo.len(), 1);
        assert_eq!(app.document.board.tracks.len(), 2);
        assert_eq!(app.document.board.vias.len(), 1);
        app.undo();
        assert!(app.document.board.tracks.is_empty());
        assert!(app.document.board.vias.is_empty());
        app.redo();
        assert_eq!(app.document.board.tracks.len(), 2);
        assert_eq!(app.document.board.vias.len(), 1);
    }
}
