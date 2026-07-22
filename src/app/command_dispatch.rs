use crate::commands::context::{CommandContext, CommandOutcome, CommandPostAction};
use crate::commands::{ChangeSet, EditorCommand};
use crate::model::IdAllocator;

impl crate::CircuitApp {
    pub(crate) fn execute_editor_command(&mut self, command: EditorCommand) -> ChangeSet {
        self.editor.document.grid = self.grid;
        if self.analysis.schematic_entity_revision != self.analysis.revisions.schematic_geometry {
            self.analysis.schematic_entity_index.rebuild(
                &self.document.components,
                &self.document.wires,
                &self.document.annotations,
            );
            self.analysis.schematic_entity_revision = self.analysis.revisions.schematic_geometry;
        }
        if self.analysis.schematic_spatial_revision != self.analysis.revisions.schematic_geometry {
            self.analysis
                .schematic_spatial_index
                .sync(&self.document.components, &self.document.wires);
            self.analysis.schematic_spatial_revision = self.analysis.revisions.schematic_geometry;
        }
        if self.analysis.attachment_revision != self.analysis.revisions.schematic_geometry {
            self.analysis
                .attachment_index
                .rebuild(&self.document.components, &self.document.wires);
            self.analysis.attachment_revision = self.analysis.revisions.schematic_geometry;
        }
        let description = command.description();
        let merge_key = command.merge_key();
        let pcb_local_impact = command.pcb_local_analysis_impact(&self.document.board);
        let board_capture = command.pcb_delta_scope(&self.document.board).map(|scope| {
            crate::editor::delta::DocumentDelta::capture_board(&self.document.board, scope)
        });
        let schematic_capture = if board_capture.is_none() {
            self.schematic_delta_capture(&command)
        } else {
            None
        };
        let snapshot =
            (board_capture.is_none() && schematic_capture.is_none()).then(|| self.snapshot());
        let mut allocator = IdAllocator::new(self.document.next_id, self.document.counters.clone());
        let outcome = command.apply(&mut CommandContext::new(
            &mut self.document,
            &mut self.editor.document,
            &mut allocator,
            &mut self.analysis.schematic_entity_index,
            &mut self.analysis.attachment_index,
            &mut self.analysis.schematic_spatial_index,
        ));
        allocator.commit(&mut self.document.next_id, &mut self.document.counters);
        let changes = outcome.changes;
        if changes.persistence_changed {
            let delta = if let Some(capture) = board_capture {
                crate::editor::delta::DocumentDelta::from_board_capture(
                    capture,
                    &self.document.board,
                )
            } else if let Some(capture) = schematic_capture {
                crate::editor::delta::DocumentDelta::from_schematic_capture(capture, &self.document)
            } else if let Some(snapshot) = snapshot.as_ref() {
                crate::editor::delta::DocumentDelta::between(snapshot, &self.snapshot())
            } else {
                debug_assert!(false, "persistent command did not capture a delta basis");
                crate::editor::delta::DocumentDelta::empty()
            };
            self.push_history_delta(delta, description, merge_key);
        }
        self.dispatch_changes(changes);
        if changes.schematic_geometry_changed {
            self.mark_schematic_indices_current();
        }
        if changes.persistence_changed
            && let Some(impact) = pcb_local_impact
        {
            self.refresh_local_pcb_analysis(&impact.track_ids, &impact.net_ids);
        }
        self.apply_command_outcome(outcome);
        changes
    }

    fn schematic_delta_capture(
        &self,
        command: &EditorCommand,
    ) -> Option<crate::editor::delta::SchematicDeltaCapture> {
        use crate::commands::component::ComponentCommand;
        use crate::commands::properties::PropertiesCommand;
        use crate::commands::selection::SelectionCommand;
        use crate::commands::wiring::WiringCommand;
        use std::collections::HashSet;

        let (component_ids, wire_ids) = match command {
            EditorCommand::Component(ComponentCommand::Move { component_ids, .. }) => {
                let wire_ids = component_ids
                    .iter()
                    .flat_map(|id| self.analysis.attachment_index.attached_wires(*id))
                    .copied()
                    .collect();
                (component_ids.clone(), wire_ids)
            }
            EditorCommand::Properties(
                PropertiesCommand::SetComponentValue { component_id, .. }
                | PropertiesCommand::SetComponentProperties { component_id, .. }
                | PropertiesCommand::ToggleSwitch { component_id },
            ) => (HashSet::from([*component_id]), HashSet::new()),
            EditorCommand::Selection(SelectionCommand::Rotate) => {
                let crate::app::Selection::Component(component_id) = self.editor.selected? else {
                    return None;
                };
                let wire_ids = self
                    .analysis
                    .attachment_index
                    .attached_wires(component_id)
                    .iter()
                    .copied()
                    .collect();
                (HashSet::from([component_id]), wire_ids)
            }
            EditorCommand::Wiring(WiringCommand::MoveControlPoint { wire_id, .. }) => {
                (HashSet::new(), HashSet::from([*wire_id]))
            }
            _ => return None,
        };
        Some(
            crate::editor::delta::DocumentDelta::capture_schematic_entities(
                &self.document,
                component_ids,
                wire_ids,
            ),
        )
    }

    pub(crate) fn execute_continuous_editor_command(
        &mut self,
        command: EditorCommand,
    ) -> ChangeSet {
        self.editor.document.grid = self.grid;
        if self.analysis.schematic_entity_revision != self.analysis.revisions.schematic_geometry {
            self.analysis.schematic_entity_index.rebuild(
                &self.document.components,
                &self.document.wires,
                &self.document.annotations,
            );
            self.analysis.schematic_entity_revision = self.analysis.revisions.schematic_geometry;
        }
        if self.analysis.schematic_spatial_revision != self.analysis.revisions.schematic_geometry {
            self.analysis
                .schematic_spatial_index
                .sync(&self.document.components, &self.document.wires);
            self.analysis.schematic_spatial_revision = self.analysis.revisions.schematic_geometry;
        }
        if self.analysis.attachment_revision != self.analysis.revisions.schematic_geometry {
            self.analysis
                .attachment_index
                .rebuild(&self.document.components, &self.document.wires);
            self.analysis.attachment_revision = self.analysis.revisions.schematic_geometry;
        }
        let mut allocator = IdAllocator::new(self.document.next_id, self.document.counters.clone());
        let outcome = command.apply(&mut CommandContext::new(
            &mut self.document,
            &mut self.editor.document,
            &mut allocator,
            &mut self.analysis.schematic_entity_index,
            &mut self.analysis.attachment_index,
            &mut self.analysis.schematic_spatial_index,
        ));
        allocator.commit(&mut self.document.next_id, &mut self.document.counters);
        let changes = outcome.changes;
        // A drag is a rollback-capable history transaction. Geometry changes
        // immediately for visual feedback, but persistent revisions and heavy
        // connectivity/ERC/simulation invalidation happen once on release.
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

    pub(crate) fn mark_schematic_indices_current(&mut self) {
        self.analysis.schematic_entity_revision = self.analysis.revisions.schematic_geometry;
        self.analysis.attachment_revision = self.analysis.revisions.schematic_geometry;
        self.analysis.schematic_spatial_revision = self.analysis.revisions.schematic_geometry;
        #[cfg(debug_assertions)]
        {
            debug_assert!(self.analysis.schematic_entity_index.is_consistent(
                &self.document.components,
                &self.document.wires,
                &self.document.annotations,
            ));
            debug_assert!(
                self.analysis
                    .attachment_index
                    .is_consistent(&self.document.components, &self.document.wires)
            );
            debug_assert!(
                self.analysis
                    .schematic_spatial_index
                    .is_consistent(&self.document.components, &self.document.wires)
            );
        }
    }

    pub(crate) fn dispatch_changes(&mut self, changes: ChangeSet) {
        let revisions = &mut self.analysis.revisions;
        if changes.persistence_changed {
            revisions.persistence = revisions.persistence.saturating_add(1);
            if changes.schematic_geometry_changed
                || changes.schematic_connectivity_changed
                || changes.electrical_values_changed
                || changes.simulation_topology_changed
                || changes.simulation_parameters_changed
            {
                self.analysis.circuit_revision = revisions.persistence;
            }
            self.editor.history.dirty = true;
        }
        if changes.schematic_geometry_changed {
            revisions.schematic_geometry = revisions.schematic_geometry.saturating_add(1);
        }
        if changes.schematic_connectivity_changed {
            revisions.schematic_connectivity = revisions.schematic_connectivity.saturating_add(1);
        }
        if changes.electrical_values_changed {
            revisions.electrical_parameters = revisions.electrical_parameters.saturating_add(1);
        }
        if changes.simulation_topology_changed {
            revisions.simulation_topology = revisions.simulation_topology.saturating_add(1);
        }
        if changes.simulation_parameters_changed {
            revisions.simulation_parameters = revisions.simulation_parameters.saturating_add(1);
        }
        if changes.pcb_sync_changed {
            revisions.board_topology = revisions.board_topology.saturating_add(1);
        }
        if changes.pcb_geometry_changed {
            revisions.board_geometry = revisions.board_geometry.saturating_add(1);
        }
        if changes.pcb_rules_changed {
            revisions.board_rules = revisions.board_rules.saturating_add(1);
        }
        if changes.visual_only {
            revisions.visual = revisions.visual.saturating_add(1);
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
    use crate::commands::component::ComponentCommand;
    use crate::commands::pcb::PcbCommand;
    use crate::model::cad::Point2;
    use crate::pcb::layer::BoardLayer;
    use crate::pcb::track::TrackSegment;
    use crate::pcb::via::Via;
    use egui::Pos2;

    #[test]
    fn editor_command_owns_history_and_cache_invalidation() {
        let mut app = crate::CircuitApp::new();
        let revision = app.analysis.circuit_revision;
        let changes =
            app.execute_editor_command(EditorCommand::Component(ComponentCommand::Place {
                kind: ComponentKind::Resistor,
                position: Pos2::new(100.0, 100.0),
                value: "10k".to_string(),
            }));
        assert_eq!(app.components.len(), 1);
        assert_eq!(app.editor.history.undo.len(), 1);
        assert!(app.analysis.circuit_revision > revision);
        assert_eq!(changes, ChangeSet::schematic());
    }

    #[test]
    fn drag_preview_defers_analysis_revision_until_release() {
        let mut app = crate::CircuitApp::new();
        app.execute_editor_command(EditorCommand::Component(ComponentCommand::Place {
            kind: ComponentKind::Resistor,
            position: Pos2::new(100.0, 100.0),
            value: "10k".to_string(),
        }));
        let id = app.components[0].id;
        let revision = app.analysis.revisions;
        app.begin_history_transaction("Move component", None);
        let changes = app.execute_continuous_editor_command(EditorCommand::Component(
            ComponentCommand::Move {
                component_ids: [id].into_iter().collect(),
                delta: egui::vec2(20.0, 0.0),
            },
        ));
        assert!(changes.persistence_changed);
        assert_eq!(app.analysis.revisions, revision);
        assert_eq!(app.components[0].pos, Pos2::new(120.0, 100.0));

        app.finish_history_transaction();
        app.dispatch_changes(changes);
        assert!(app.analysis.revisions.persistence > revision.persistence);
        assert_eq!(app.editor.history.undo.len(), 2);
    }

    #[test]
    fn schematic_indices_remain_consistent_across_undo_redo_and_reset() {
        let mut app = crate::CircuitApp::new();
        for x in [40.0, 120.0] {
            app.execute_editor_command(EditorCommand::Component(ComponentCommand::Place {
                kind: ComponentKind::Resistor,
                position: Pos2::new(x, 80.0),
                value: "1k".to_string(),
            }));
        }
        assert!(app.analysis.schematic_entity_index.is_consistent(
            &app.document.components,
            &app.document.wires,
            &app.document.annotations,
        ));
        assert!(
            app.analysis
                .attachment_index
                .is_consistent(&app.document.components, &app.document.wires)
        );

        app.undo();
        assert!(app.analysis.schematic_entity_index.is_consistent(
            &app.document.components,
            &app.document.wires,
            &app.document.annotations,
        ));
        app.redo();
        assert!(app.analysis.schematic_entity_index.is_consistent(
            &app.document.components,
            &app.document.wires,
            &app.document.annotations,
        ));
        app.execute_editor_command(EditorCommand::Document(
            crate::commands::document::DocumentCommand::Reset,
        ));
        assert!(app.analysis.schematic_entity_index.is_consistent(
            &app.document.components,
            &app.document.wires,
            &app.document.annotations,
        ));
    }

    #[test]
    fn deleting_connected_component_detaches_typed_endpoints_without_stale_indices() {
        let mut app = crate::CircuitApp::new();
        app.load_led_demo();
        let resistor_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::Resistor)
            .map(|component| component.id)
            .unwrap();
        app.editor.selected = Some(crate::app::Selection::Component(resistor_id));
        app.execute_editor_command(EditorCommand::Selection(
            crate::commands::selection::SelectionCommand::Delete,
        ));

        assert!(app.wires.iter().all(|wire| {
            !matches!(&wire.start, crate::model::WireEndpoint::Pin(pin) if pin.component_id == resistor_id)
                && !matches!(&wire.end, crate::model::WireEndpoint::Pin(pin) if pin.component_id == resistor_id)
        }));
        assert!(app.analysis.schematic_entity_index.is_consistent(
            &app.document.components,
            &app.document.wires,
            &app.document.annotations,
        ));
        assert!(
            app.analysis
                .attachment_index
                .is_consistent(&app.document.components, &app.document.wires)
        );
    }

    #[test]
    fn property_change_reuses_connectivity_projections() {
        let mut app = crate::CircuitApp::new();
        let connectivity = app.current_connectivity();
        let connected_pins = app.current_connected_pins();
        let _simulation = app.current_simulation();
        app.dispatch_changes(ChangeSet::properties());
        assert!(std::sync::Arc::ptr_eq(
            &connectivity,
            &app.analysis.cached_connectivity.as_ref().unwrap().1
        ));
        assert!(std::sync::Arc::ptr_eq(
            &connected_pins,
            &app.analysis.cached_connected_pins.as_ref().unwrap().1
        ));
        assert!(app.analysis.cached_simulation.is_none());
    }

    #[test]
    fn pcb_primitive_is_undoable_and_keeps_indices_consistent() {
        let mut app = crate::CircuitApp::new();
        let schematic_revision = app.analysis.circuit_revision;
        app.execute_editor_command(EditorCommand::Pcb(PcbCommand::AddTrack(TrackSegment {
            id: 1,
            net_id: 0,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(1.0, 1.0),
            end: Point2::new(10.0, 1.0),
            width_mm: 0.25,
        })));
        assert_eq!(app.analysis.circuit_revision, schematic_revision);
        assert!(app.document.board.entity_index_is_consistent());
        app.undo();
        assert!(app.document.board.tracks.is_empty());
        assert!(app.document.board.entity_index_is_consistent());
        app.redo();
        assert_eq!(app.document.board.tracks.len(), 1);
        assert!(app.document.board.entity_index_is_consistent());
    }

    #[test]
    fn complete_route_with_via_is_one_undo_item() {
        let mut app = crate::CircuitApp::new();
        app.execute_editor_command(EditorCommand::Pcb(PcbCommand::AddRoute {
            tracks: vec![TrackSegment {
                id: 1,
                net_id: 4,
                layer: BoardLayer::FrontCopper,
                start: Point2::new(5.0, 5.0),
                end: Point2::new(10.0, 5.0),
                width_mm: 0.25,
            }],
            vias: vec![Via {
                id: 3,
                net_id: 4,
                position: Point2::new(10.0, 5.0),
                diameter_mm: 0.6,
                drill_mm: 0.3,
            }],
        }));
        assert_eq!(app.editor.history.undo.len(), 1);
        app.undo();
        assert!(app.document.board.tracks.is_empty());
        assert!(app.document.board.vias.is_empty());
        app.redo();
        assert_eq!(app.document.board.tracks.len(), 1);
        assert_eq!(app.document.board.vias.len(), 1);
    }
}
