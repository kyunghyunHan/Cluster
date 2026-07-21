use crate::app::{EditorDocumentState, Selection, Tool};
use crate::model::cad::{CadNet, NetClass, Point2, SymbolInstance};
use crate::model::{
    Component, ComponentKind, IdAllocator, PinRef, ProjectDocument, SchematicEntityIndex, Wire,
    WireEndpoint, component_pin_defs, component_pins, distance_to_segment,
};
use crate::pcb::board::{BoardOutline, RemovedFootprintPolicy};
use crate::pcb::track::TrackSegment;
use crate::pcb::via::Via;
use egui::{Pos2, Vec2};
use std::collections::HashSet;

/// Narrow mutation boundary used by every document command.
///
/// It intentionally contains only persistent document data, command-visible
/// editor state and the command-local allocator. No application, UI workspace,
/// cache, persistence or analysis capability can be reached from here.
pub(crate) struct CommandContext<'a> {
    document: &'a mut ProjectDocument,
    editor: &'a mut EditorDocumentState,
    allocator: &'a mut IdAllocator,
    entity_index: &'a mut SchematicEntityIndex,
}

impl<'a> CommandContext<'a> {
    pub(crate) fn new(
        document: &'a mut ProjectDocument,
        editor: &'a mut EditorDocumentState,
        allocator: &'a mut IdAllocator,
        entity_index: &'a mut SchematicEntityIndex,
    ) -> Self {
        Self {
            document,
            editor,
            allocator,
            entity_index,
        }
    }

    fn rebuild_entity_index(&mut self) {
        self.entity_index.rebuild(
            &self.document.components,
            &self.document.wires,
            &self.document.annotations,
        );
    }

    pub(crate) fn components(&self) -> &[Component] {
        &self.document.components
    }

    pub(crate) fn next_id(&mut self) -> u64 {
        self.allocator.allocate_id()
    }

    pub(crate) fn next_label(&mut self, kind: ComponentKind) -> String {
        self.allocator
            .allocate_label(kind, &self.document.components)
    }

    pub(crate) fn next_custom_label(&self, part_id: Option<&str>) -> String {
        self.allocator
            .allocate_custom_label(part_id, &self.document.components)
    }

    pub(crate) fn place_component(
        &mut self,
        kind: ComponentKind,
        position: Pos2,
        value: String,
        part_id: Option<String>,
    ) -> u64 {
        let label = if kind == ComponentKind::Custom {
            self.next_custom_label(part_id.as_deref())
        } else {
            self.next_label(kind)
        };
        let id = self.next_id();
        self.document.components.push(Component {
            id,
            kind,
            pos: position,
            rotation: 0,
            label,
            value,
            part_id,
        });
        self.rebuild_entity_index();
        id
    }

    pub(crate) fn insert_component(&mut self, component: Component) {
        self.document.components.push(component);
        self.rebuild_entity_index();
    }

    pub(crate) fn insert_wire(&mut self, wire: Wire) {
        self.document.wires.push(wire);
        self.rebuild_entity_index();
    }

    pub(crate) fn remove_components(&mut self, ids: &HashSet<u64>) -> usize {
        let before = self.document.components.len();
        self.document
            .components
            .retain(|component| !ids.contains(&component.id));
        self.rebuild_entity_index();
        before - self.document.components.len()
    }

    pub(crate) fn remove_component(&mut self, id: u64) -> bool {
        let Some(index) = self.entity_index.component(id) else {
            return false;
        };
        self.document.components.remove(index);
        self.rebuild_entity_index();
        true
    }

    pub(crate) fn remove_wire(&mut self, id: u64) -> bool {
        let Some(index) = self.entity_index.wire(id) else {
            return false;
        };
        self.document.wires.remove(index);
        self.rebuild_entity_index();
        true
    }

    pub(crate) fn update_component(
        &mut self,
        id: u64,
        update: impl FnOnce(&mut Component),
    ) -> bool {
        let Some(index) = self.entity_index.component(id) else {
            return false;
        };
        update(&mut self.document.components[index]);
        true
    }

    pub(crate) fn move_components(&mut self, ids: &HashSet<u64>, delta: Vec2) -> bool {
        let mut indices = ids
            .iter()
            .filter_map(|id| self.entity_index.component(*id))
            .collect::<Vec<_>>();
        indices.sort_unstable();
        if indices.is_empty() {
            return false;
        }
        let old_pins = indices
            .iter()
            .flat_map(|&index| component_pins(&self.document.components[index]))
            .collect::<Vec<_>>();
        let attached_wire_ids = ids
            .iter()
            .flat_map(|id| self.entity_index.attached_wires(*id))
            .copied()
            .collect::<HashSet<_>>();
        for &index in &indices {
            self.document.components[index].pos += delta;
        }
        let new_pins = indices
            .iter()
            .flat_map(|&index| component_pins(&self.document.components[index]))
            .collect::<Vec<_>>();
        self.move_attached_wires(&attached_wire_ids, &old_pins, &new_pins);
        self.rebuild_entity_index();
        true
    }

    pub(crate) fn rotate_component(&mut self, id: u64) -> bool {
        let Some(index) = self.entity_index.component(id) else {
            return false;
        };
        let old_pins = component_pins(&self.document.components[index]);
        let attached_wire_ids = self
            .entity_index
            .attached_wires(id)
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        self.document.components[index].rotation =
            (self.document.components[index].rotation + 90) % 360;
        let new_pins = component_pins(&self.document.components[index]);
        self.move_attached_wires(&attached_wire_ids, &old_pins, &new_pins);
        self.rebuild_entity_index();
        true
    }

    fn move_attached_wires(
        &mut self,
        attached_wire_ids: &HashSet<u64>,
        old_pins: &[Pos2],
        new_pins: &[Pos2],
    ) {
        let wire_indices = attached_wire_ids
            .iter()
            .filter_map(|id| self.entity_index.wire(*id))
            .collect::<Vec<_>>();
        for index in wire_indices {
            let wire = &mut self.document.wires[index];
            crate::move_attached_wire_endpoints(std::slice::from_mut(wire), old_pins, new_pins);
            if wire.points.len() <= 2 {
                continue;
            }
            crate::tidy_wire_points(wire);
        }
    }

    pub(crate) fn set_component_axis_position(
        &mut self,
        id: u64,
        vertical: bool,
        value: f32,
    ) -> bool {
        self.update_component(id, |component| {
            if vertical {
                component.pos.y = value;
            } else {
                component.pos.x = value;
            }
        })
    }

    pub(crate) fn infer_wire_endpoint(&self, point: Pos2) -> WireEndpoint {
        for component in &self.document.components {
            for pin in component_pin_defs(component) {
                if point.distance(pin.pos) <= 1.0 {
                    return WireEndpoint::Pin(PinRef {
                        component_id: component.id,
                        pin_name: pin.label.to_string(),
                    });
                }
            }
        }
        WireEndpoint::FreePoint(point)
    }

    pub(crate) fn split_wire_at_point(&mut self, point: Pos2) {
        let split_target = self
            .document
            .wires
            .iter()
            .enumerate()
            .find_map(|(wire_index, wire)| {
                wire.points
                    .windows(2)
                    .enumerate()
                    .find(|(_, pair)| {
                        distance_to_segment(point, pair[0], pair[1]) < 2.5
                            && point.distance(pair[0]) > 5.0
                            && point.distance(pair[1]) > 5.0
                    })
                    .map(|(segment_index, _)| (wire_index, segment_index))
            });
        let Some((wire_index, segment_index)) = split_target else {
            return;
        };
        let old_start = self.document.wires[wire_index].start.clone();
        let old_end = self.document.wires[wire_index].end.clone();
        let mut first = self.document.wires[wire_index].points[..=segment_index].to_vec();
        first.push(point);
        let mut second = vec![point];
        second.extend_from_slice(&self.document.wires[wire_index].points[segment_index + 1..]);
        self.document.wires[wire_index].points = crate::simplify_wire(first);
        self.document.wires[wire_index].start = old_start;
        self.document.wires[wire_index].end = WireEndpoint::FreePoint(point);
        let id = self.next_id();
        self.document.wires.push(Wire {
            id,
            points: crate::simplify_wire(second),
            start: WireEndpoint::FreePoint(point),
            end: old_end,
        });
        self.rebuild_entity_index();
    }

    pub(crate) fn move_wire_control_point(
        &mut self,
        wire_id: u64,
        point_index: usize,
        position: Pos2,
    ) -> bool {
        let Some(wire_index) = self.entity_index.wire(wire_id) else {
            return false;
        };
        if point_index >= self.document.wires[wire_index].points.len() {
            return false;
        }
        crate::ui::app::move_wire_control_point(
            &mut self.document.wires,
            wire_id,
            point_index,
            position,
        );
        true
    }

    pub(crate) fn insert_wire_control_point(&mut self, position: Pos2) -> Option<(u64, usize)> {
        crate::ui::app::insert_wire_control_point(position, &mut self.document.wires)
    }

    pub(crate) fn tidy_wires(&mut self, wire_id: Option<u64>) -> usize {
        let mut count = 0;
        for wire in &mut self.document.wires {
            if wire_id.is_none_or(|id| id == wire.id) {
                crate::tidy_wire_points(wire);
                count += 1;
            }
        }
        count
    }

    pub(crate) fn selected(&self) -> Option<Selection> {
        self.editor.selected
    }

    pub(crate) fn take_selected(&mut self) -> Option<Selection> {
        self.editor.selected.take()
    }

    pub(crate) fn set_selected(&mut self, selected: Option<Selection>) {
        self.editor.selected = selected;
    }

    pub(crate) fn multi_selected(&self) -> &HashSet<u64> {
        &self.editor.multi_selected
    }

    pub(crate) fn set_multi_selected(&mut self, selected: HashSet<u64>) {
        self.editor.multi_selected = selected;
    }

    pub(crate) fn clear_multi_selected(&mut self) {
        self.editor.multi_selected.clear();
    }

    pub(crate) fn set_drag(&mut self, drag: Option<crate::model::DragState>) {
        self.editor.drag = drag;
    }

    pub(crate) fn grid(&self) -> f32 {
        self.editor.grid
    }

    pub(crate) fn reset_document_and_editor(&mut self) {
        self.document.components.clear();
        self.document.wires.clear();
        self.allocator.reset();
        self.editor.selected = None;
        self.editor.multi_selected.clear();
        self.editor.drag = None;
        self.editor.draft_wire.clear();
        self.editor.wire_from_select = false;
        self.editor.snap_target = None;
        self.editor.tool = Tool::Select;
    }

    pub(crate) fn move_footprint(&mut self, footprint_id: u64, position: Point2) -> bool {
        self.document.board.move_footprint(footprint_id, position)
    }

    pub(crate) fn rotate_footprint(&mut self, footprint_id: u64, delta_deg: f32) -> bool {
        self.document
            .board
            .rotate_footprint(footprint_id, delta_deg)
    }

    pub(crate) fn flip_footprint(&mut self, footprint_id: u64) -> bool {
        self.document.board.flip_footprint(footprint_id)
    }

    pub(crate) fn add_track(&mut self, track: TrackSegment) {
        self.document.board.add_track(track);
    }

    pub(crate) fn remove_track(&mut self, track_id: u64) -> bool {
        self.document.board.remove_track(track_id).is_some()
    }

    pub(crate) fn add_via(&mut self, via: Via) {
        self.document.board.add_via(via);
    }

    pub(crate) fn remove_via(&mut self, via_id: u64) -> bool {
        self.document.board.remove_via(via_id).is_some()
    }

    pub(crate) fn set_outline(&mut self, outline: BoardOutline) {
        self.document.board.outline = outline;
        self.document.board.rebuild_entity_index();
    }

    pub(crate) fn set_board_geometry(
        &mut self,
        footprint_positions: Vec<(u64, Point2)>,
        tracks: Vec<TrackSegment>,
        vias: Vec<Via>,
        outline: BoardOutline,
    ) -> bool {
        let mut changed = false;
        for (id, position) in footprint_positions {
            changed |= self.document.board.move_footprint(id, position);
        }
        for updated in tracks {
            changed |= self.document.board.edit_track(updated);
        }
        for updated in vias {
            changed |= self.document.board.edit_via(updated);
        }
        changed |= self.document.board.outline != outline;
        self.document.board.outline = outline;
        if changed {
            self.document.board.rebuild_entity_index();
        }
        changed
    }

    pub(crate) fn edit_track(&mut self, updated: TrackSegment) -> bool {
        self.document.board.edit_track(updated)
    }

    pub(crate) fn set_net_class(&mut self, updated: NetClass) -> bool {
        if let Some(class) = self
            .document
            .board
            .net_classes
            .iter_mut()
            .find(|class| class.class_id == updated.class_id)
        {
            *class = updated;
        } else {
            self.document.board.net_classes.push(updated);
        }
        true
    }

    pub(crate) fn apply_eco(
        &mut self,
        symbols: &[SymbolInstance],
        nets: &[CadNet],
        policy: RemovedFootprintPolicy,
    ) -> bool {
        !self
            .document
            .board
            .apply_eco_patch(symbols, nets, policy)
            .is_empty()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum CommandPostAction {
    #[default]
    None,
    ResetWorkspaceView,
}

pub(crate) struct CommandOutcome {
    pub(crate) changes: super::ChangeSet,
    pub(crate) status: Option<String>,
    pub(crate) post_action: CommandPostAction,
}

impl CommandOutcome {
    pub(crate) fn new(changes: super::ChangeSet) -> Self {
        Self {
            changes,
            status: None,
            post_action: CommandPostAction::None,
        }
    }

    pub(crate) fn unchanged() -> Self {
        Self::new(super::ChangeSet::none())
    }

    pub(crate) fn with_status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }

    pub(crate) fn with_post_action(mut self, post_action: CommandPostAction) -> Self {
        self.post_action = post_action;
        self
    }
}
