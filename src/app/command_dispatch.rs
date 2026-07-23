use crate::commands::context::{CommandContext, CommandOutcome, CommandPostAction};
use crate::commands::{ChangeSet, EditorCommand};
use crate::model::IdAllocator;

impl crate::CircuitApp {
    pub(crate) fn execute_editor_command(&mut self, command: EditorCommand) -> ChangeSet {
        let compound_transaction = self.editor.history.pending.is_some();
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
        let pcb_local_impact = (!compound_transaction)
            .then(|| command.pcb_local_analysis_impact(&self.document.board))
            .flatten();
        let board_capture = if compound_transaction {
            None
        } else {
            command.pcb_delta_scope(&self.document.board).map(|scope| {
                crate::editor::delta::DocumentDelta::capture_board(&self.document.board, scope)
            })
        };
        let schematic_capture = if compound_transaction || board_capture.is_some() {
            None
        } else {
            self.schematic_delta_capture(&command)
        };
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
        if changes.persistence_changed && !compound_transaction {
            let delta = if let Some(capture) = board_capture {
                crate::editor::delta::DocumentDelta::from_board_capture(
                    capture,
                    &self.document.board,
                )
            } else if let Some(capture) = schematic_capture {
                crate::editor::delta::DocumentDelta::from_schematic_capture(capture, &self.document)
            } else {
                debug_assert!(false, "persistent command did not capture a delta basis");
                crate::editor::delta::DocumentDelta::empty()
            };
            self.push_history_delta(delta, description, merge_key);
        }
        if !compound_transaction {
            self.dispatch_changes(changes);
        }
        if changes.schematic_geometry_changed {
            self.mark_schematic_indices_current();
        }
        if !compound_transaction
            && changes.persistence_changed
            && let Some(impact) = pcb_local_impact
        {
            self.refresh_local_pcb_analysis(&impact.track_ids, &impact.net_ids);
        }
        self.apply_command_outcome(outcome);
        #[cfg(debug_assertions)]
        self.assert_editor_invariants();
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

        use crate::editor::delta::SchematicDeltaScope;

        let mut scope = SchematicDeltaScope::default();
        let attached_wires = |component_ids: &HashSet<u64>| {
            component_ids
                .iter()
                .flat_map(|id| self.analysis.attachment_index.attached_wires(*id))
                .copied()
                .collect::<HashSet<_>>()
        };
        match command {
            EditorCommand::Component(
                ComponentCommand::Place { .. } | ComponentCommand::PlaceCustom { .. },
            ) => {
                scope.capture_new_components = true;
                scope.capture_metadata = true;
            }
            EditorCommand::Component(ComponentCommand::Paste { .. }) => {
                scope.capture_new_components = true;
                scope.capture_new_wires = true;
                scope.capture_metadata = true;
            }
            EditorCommand::Component(ComponentCommand::Move { component_ids, .. }) => {
                scope.component_ids.clone_from(component_ids);
                scope.wire_ids.extend(attached_wires(component_ids));
            }
            EditorCommand::Properties(
                PropertiesCommand::SetComponentValue { component_id, .. }
                | PropertiesCommand::SetComponentProperties { component_id, .. }
                | PropertiesCommand::ToggleSwitch { component_id },
            ) => {
                scope.component_ids.insert(*component_id);
            }
            EditorCommand::Selection(SelectionCommand::Rotate) => {
                let crate::app::Selection::Component(component_id) = self.editor.selected? else {
                    return None;
                };
                scope.component_ids.insert(component_id);
                scope
                    .wire_ids
                    .extend(attached_wires(&HashSet::from([component_id])));
            }
            EditorCommand::Wiring(WiringCommand::MoveControlPoint { wire_id, .. }) => {
                scope.wire_ids.insert(*wire_id);
            }
            EditorCommand::Wiring(WiringCommand::Add { points }) => {
                scope.capture_new_wires = true;
                scope.capture_metadata = true;
                for point in points.first().into_iter().chain(points.last()) {
                    scope.wire_ids.extend(
                        self.analysis
                            .schematic_spatial_index
                            .wire_segments_near(*point, 0.5)
                            .into_iter()
                            .map(|segment| segment.wire_id),
                    );
                }
            }
            EditorCommand::Wiring(WiringCommand::InsertControlPoint { position }) => {
                scope.wire_ids.extend(
                    self.analysis
                        .schematic_spatial_index
                        .wire_segments_near(*position, 20.0)
                        .into_iter()
                        .map(|segment| segment.wire_id),
                );
            }
            EditorCommand::Wiring(WiringCommand::Tidy { wire_id }) => match wire_id {
                Some(id) => {
                    scope.wire_ids.insert(*id);
                }
                None => scope
                    .wire_ids
                    .extend(self.document.wires.iter().map(|wire| wire.id)),
            },
            EditorCommand::Selection(SelectionCommand::Delete) => {
                if !self.editor.multi_selected.is_empty() {
                    scope.component_ids.clone_from(&self.editor.multi_selected);
                    scope
                        .wire_ids
                        .extend(attached_wires(&self.editor.multi_selected));
                } else if let Some(selection) = self.editor.selected {
                    match selection {
                        crate::app::Selection::Component(id) => {
                            scope.component_ids.insert(id);
                            scope.wire_ids.extend(attached_wires(&HashSet::from([id])));
                        }
                        crate::app::Selection::Wire(id) => {
                            scope.wire_ids.insert(id);
                        }
                    }
                }
            }
            EditorCommand::Selection(SelectionCommand::Duplicate) => {
                scope.capture_new_components = true;
                scope.capture_metadata = true;
            }
            EditorCommand::Selection(
                SelectionCommand::Align(_) | SelectionCommand::Distribute { .. },
            ) => {
                if self.editor.multi_selected.is_empty() {
                    if let Some(crate::app::Selection::Component(id)) = self.editor.selected {
                        scope.component_ids.insert(id);
                    }
                } else {
                    scope.component_ids.clone_from(&self.editor.multi_selected);
                }
                let ids = scope.component_ids.clone();
                scope.wire_ids.extend(attached_wires(&ids));
            }
            EditorCommand::Document(crate::commands::document::DocumentCommand::Reset) => {
                scope.component_ids.extend(
                    self.document
                        .components
                        .iter()
                        .map(|component| component.id),
                );
                scope
                    .wire_ids
                    .extend(self.document.wires.iter().map(|wire| wire.id));
                scope.capture_annotations = true;
                scope.capture_metadata = true;
            }
            EditorCommand::Pcb(_) => return None,
        }
        Some(crate::editor::delta::DocumentDelta::capture_schematic(
            &self.document,
            scope,
        ))
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
        #[cfg(debug_assertions)]
        self.assert_editor_invariants();
        changes
    }

    #[cfg(debug_assertions)]
    pub(crate) fn assert_editor_invariants(&self) {
        let document_invariants = crate::model::validate_document_invariants(
            &self.document,
            &self.analysis.schematic_entity_index,
        );
        debug_assert!(
            document_invariants.is_ok(),
            "document invariant violation: {document_invariants:?}"
        );
        let board_invariants = self.document.board.validate_invariants();
        debug_assert!(
            board_invariants.is_ok(),
            "board invariant violation: {board_invariants:?}"
        );
        debug_assert!(self.editor.selected.is_none_or(|selection| {
            match selection {
                crate::app::Selection::Component(id) => self
                    .document
                    .components
                    .iter()
                    .any(|component| component.id == id),
                crate::app::Selection::Wire(id) => {
                    self.document.wires.iter().any(|wire| wire.id == id)
                }
            }
        }));
        debug_assert!(self.editor.multi_selected.iter().all(|id| {
            self.document
                .components
                .iter()
                .any(|component| component.id == *id)
        }));
        debug_assert!(self.editor.drag.as_ref().is_none_or(|drag| {
            match drag {
                crate::model::DragState::Component { id, .. } => self
                    .document
                    .components
                    .iter()
                    .any(|component| component.id == *id),
                crate::model::DragState::WirePoint {
                    wire_id,
                    point_index,
                } => self
                    .document
                    .wires
                    .iter()
                    .find(|wire| wire.id == *wire_id)
                    .is_some_and(|wire| *point_index < wire.points.len()),
            }
        }));
        debug_assert_eq!(
            self.editor.history.undo_memory_bytes,
            self.editor
                .history
                .undo
                .iter()
                .map(|entry| entry.memory_cost)
                .sum::<usize>()
        );
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
        app.begin_schematic_history_transaction(
            "Move component",
            [id].into_iter().collect(),
            std::collections::HashSet::new(),
        );
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
    fn deterministic_random_command_sequence_matches_linear_document_state() {
        use crate::commands::selection::SelectionCommand;
        use crate::commands::wiring::WiringCommand;
        use crate::model::SavedCircuit;

        fn next(seed: &mut u64) -> u64 {
            *seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            *seed
        }

        fn assert_consistent(app: &crate::CircuitApp) {
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
            assert!(
                app.analysis
                    .schematic_spatial_index
                    .is_consistent(&app.document.components, &app.document.wires)
            );
            assert!(
                crate::model::validate_document_invariants(
                    &app.document,
                    &app.analysis.schematic_entity_index,
                )
                .is_ok()
            );
            for (expected, component) in app.document.components.iter().enumerate() {
                assert_eq!(
                    app.analysis.schematic_entity_index.component(component.id),
                    Some(expected)
                );
            }
            for (expected, wire) in app.document.wires.iter().enumerate() {
                assert_eq!(
                    app.analysis.schematic_entity_index.wire(wire.id),
                    Some(expected)
                );
            }
        }

        fn assert_same_schematic(
            actual: &crate::model::CircuitSnapshot,
            expected: &crate::model::CircuitSnapshot,
        ) {
            let mut actual_components = actual.components.clone();
            let mut expected_components = expected.components.clone();
            actual_components.sort_by_key(|component| component.id);
            expected_components.sort_by_key(|component| component.id);
            assert_eq!(actual_components, expected_components);

            let mut actual_wires = actual.wires.clone();
            let mut expected_wires = expected.wires.clone();
            actual_wires.sort_by_key(|wire| wire.id);
            expected_wires.sort_by_key(|wire| wire.id);
            assert_eq!(actual_wires, expected_wires);
            assert_eq!(actual.annotations, expected.annotations);
            let normalize_pages = |pages: &[crate::model::ProjectPage]| {
                pages
                    .iter()
                    .cloned()
                    .map(|mut page| {
                        page.components.sort_by_key(|component| component.id);
                        page.wires.sort_by_key(|wire| wire.id);
                        page
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                normalize_pages(&actual.pages),
                normalize_pages(&expected.pages)
            );
            assert_eq!(actual.current_page, expected.current_page);
        }

        let mut app = crate::CircuitApp::new();
        let initial = app.snapshot();
        let mut seed = 0x5eed_cafe_f00d_u64;
        for step in 0..240 {
            match next(&mut seed) % 7 {
                0 | 1 => {
                    let x = (next(&mut seed) % 40) as f32 * 20.0;
                    let y = (next(&mut seed) % 30) as f32 * 20.0;
                    app.execute_editor_command(EditorCommand::Component(ComponentCommand::Place {
                        kind: ComponentKind::Resistor,
                        position: Pos2::new(x, y),
                        value: "1k".to_string(),
                    }));
                }
                2 if !app.components.is_empty() => {
                    let index = next(&mut seed) as usize % app.components.len();
                    let id = app.components[index].id;
                    app.execute_editor_command(EditorCommand::Component(ComponentCommand::Move {
                        component_ids: [id].into_iter().collect(),
                        delta: egui::vec2(20.0, 0.0),
                    }));
                }
                3 if !app.components.is_empty() => {
                    let index = next(&mut seed) as usize % app.components.len();
                    app.editor.selected =
                        Some(crate::app::Selection::Component(app.components[index].id));
                    app.execute_editor_command(EditorCommand::Selection(SelectionCommand::Rotate));
                }
                4 => {
                    let base = (step % 30) as f32 * 20.0;
                    app.execute_editor_command(EditorCommand::Wiring(WiringCommand::Add {
                        points: vec![Pos2::new(base, 700.0), Pos2::new(base + 20.0, 700.0)],
                    }));
                }
                5 if !app.components.is_empty() => {
                    let index = next(&mut seed) as usize % app.components.len();
                    app.editor.selected =
                        Some(crate::app::Selection::Component(app.components[index].id));
                    app.execute_editor_command(EditorCommand::Selection(SelectionCommand::Delete));
                }
                _ if next(&mut seed).is_multiple_of(2) => app.undo(),
                _ => app.redo(),
            }
            assert_consistent(&app);
        }

        let final_snapshot = app.snapshot();
        while !app.editor.history.undo.is_empty() {
            app.undo();
            assert_consistent(&app);
        }
        assert_same_schematic(&app.snapshot(), &initial);

        while !app.editor.history.redo.is_empty() {
            app.redo();
            assert_consistent(&app);
        }
        assert_same_schematic(&app.snapshot(), &final_snapshot);

        let before = crate::engine::netlist::build_canonical_connectivity_with_annotations(
            &app.components,
            &app.wires,
            &app.annotations.netlist_annotations(),
        );
        let json = serde_json::to_string(&SavedCircuit::from_app(&app)).unwrap();
        let (round_trip, notes) = serde_json::from_str::<SavedCircuit>(&json)
            .unwrap()
            .into_snapshot()
            .unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        let after = crate::engine::netlist::build_canonical_connectivity_with_annotations(
            &round_trip.components,
            &round_trip.wires,
            &round_trip.annotations.netlist_annotations(),
        );
        assert_eq!(before.pin_nets, after.pin_nets);
        assert_eq!(before.junction_id_nets, after.junction_id_nets);
        assert_eq!(before.junction_nets, after.junction_nets);
        assert_eq!(before.wire_segment_nets, after.wire_segment_nets);
        assert_eq!(before.diagnostics, after.diagnostics);
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
    fn net_label_value_change_invalidates_connectivity() {
        use crate::commands::properties::PropertiesCommand;

        let mut app = crate::CircuitApp::new();
        app.execute_editor_command(EditorCommand::Component(ComponentCommand::Place {
            kind: ComponentKind::NetLabel,
            position: Pos2::new(100.0, 100.0),
            value: "BUS_A".to_string(),
        }));
        let label_id = app.components[0].id;
        let cached = app.current_connectivity();
        let revision = app.analysis.revisions.schematic_connectivity;

        let changes = app.execute_editor_command(EditorCommand::Properties(
            PropertiesCommand::SetComponentValue {
                component_id: label_id,
                value: "BUS_B".to_string(),
            },
        ));

        assert!(changes.schematic_connectivity_changed);
        assert!(changes.simulation_topology_changed);
        assert!(app.analysis.revisions.schematic_connectivity > revision);
        assert!(app.analysis.cached_connectivity.is_none());
        let rebuilt = app.current_connectivity();
        assert!(!std::sync::Arc::ptr_eq(&cached, &rebuilt));
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
