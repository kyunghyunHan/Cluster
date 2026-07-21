use crate::app::{EditorDocumentState, Selection, Tool};
use crate::model::cad::{CadNet, NetClass, Point2, SymbolInstance};
use crate::model::{
    AttachmentIndex, Component, ComponentKind, IdAllocator, ProjectDocument, SchematicEntityIndex,
    Wire, WireEndpoint, component_pins, distance_to_segment,
};
use crate::pcb::board::{BoardOutline, RemovedFootprintPolicy};
use crate::pcb::track::TrackSegment;
use crate::pcb::via::Via;
use crate::ui::canvas::spatial_index::SchematicSpatialIndex;
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
    attachment_index: &'a mut AttachmentIndex,
    spatial_index: &'a mut SchematicSpatialIndex,
}

impl<'a> CommandContext<'a> {
    pub(crate) fn new(
        document: &'a mut ProjectDocument,
        editor: &'a mut EditorDocumentState,
        allocator: &'a mut IdAllocator,
        entity_index: &'a mut SchematicEntityIndex,
        attachment_index: &'a mut AttachmentIndex,
        spatial_index: &'a mut SchematicSpatialIndex,
    ) -> Self {
        Self {
            document,
            editor,
            allocator,
            entity_index,
            attachment_index,
            spatial_index,
        }
    }

    fn rebuild_entity_index(&mut self) {
        self.entity_index.rebuild(
            &self.document.components,
            &self.document.wires,
            &self.document.annotations,
        );
        self.attachment_index
            .rebuild(&self.document.components, &self.document.wires);
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
        if let Some(component) = self.document.components.last() {
            self.spatial_index.update_component(component);
            self.entity_index
                .insert_component(component.id, self.document.components.len() - 1);
        }
        id
    }

    pub(crate) fn insert_component(&mut self, component: Component) {
        self.document.components.push(component);
        if let Some(component) = self.document.components.last() {
            self.spatial_index.update_component(component);
            self.entity_index
                .insert_component(component.id, self.document.components.len() - 1);
        }
    }

    pub(crate) fn insert_wire(&mut self, wire: Wire) {
        self.document.wires.push(wire);
        if let Some(wire) = self.document.wires.last() {
            self.spatial_index.update_wire(wire);
            self.entity_index
                .insert_wire(wire.id, self.document.wires.len() - 1);
            self.attachment_index
                .add_wire(wire, &self.document.components);
        }
    }

    pub(crate) fn remove_components(&mut self, ids: &HashSet<u64>) -> usize {
        ids.iter()
            .copied()
            .filter(|id| self.remove_component(*id))
            .count()
    }

    pub(crate) fn remove_component(&mut self, id: u64) -> bool {
        let Some(index) = self.entity_index.component(id) else {
            return false;
        };
        self.document.components.swap_remove(index);
        let moved = self
            .document
            .components
            .get(index)
            .map(|component| (component.id, index));
        self.spatial_index.remove_component(id);
        self.entity_index.remove_component(id, moved);
        self.attachment_index.remove_component(id);
        true
    }

    pub(crate) fn remove_wire(&mut self, id: u64) -> bool {
        let Some(index) = self.entity_index.wire(id) else {
            return false;
        };
        self.document.wires.swap_remove(index);
        let moved = self.document.wires.get(index).map(|wire| (wire.id, index));
        self.spatial_index.remove_wire(id);
        self.entity_index.remove_wire(id, moved);
        self.attachment_index.remove_wire(id);
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
        self.spatial_index
            .update_component(&self.document.components[index]);
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
            .flat_map(|id| self.attachment_index.attached_wires(*id))
            .copied()
            .collect::<HashSet<_>>();
        for &index in &indices {
            self.document.components[index].pos += delta;
            self.spatial_index
                .update_component(&self.document.components[index]);
        }
        let new_pins = indices
            .iter()
            .flat_map(|&index| component_pins(&self.document.components[index]))
            .collect::<Vec<_>>();
        self.move_attached_wires(&attached_wire_ids, &old_pins, &new_pins);
        true
    }

    pub(crate) fn rotate_component(&mut self, id: u64) -> bool {
        let Some(index) = self.entity_index.component(id) else {
            return false;
        };
        let old_pins = component_pins(&self.document.components[index]);
        let attached_wire_ids = self
            .attachment_index
            .attached_wires(id)
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        self.document.components[index].rotation =
            (self.document.components[index].rotation + 90) % 360;
        self.spatial_index
            .update_component(&self.document.components[index]);
        let new_pins = component_pins(&self.document.components[index]);
        self.move_attached_wires(&attached_wire_ids, &old_pins, &new_pins);
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
                self.spatial_index.update_wire(wire);
                continue;
            }
            crate::tidy_wire_points(wire);
            self.spatial_index.update_wire(wire);
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
        if let Some(pin) = self.spatial_index.nearest_pin(point, 1.0) {
            return WireEndpoint::Pin(pin);
        }
        WireEndpoint::FreePoint(point)
    }

    pub(crate) fn split_wire_at_point(&mut self, point: Pos2) {
        let radius = Vec2::splat(2.5);
        let mut candidates = self
            .spatial_index
            .query_wires(point - radius, point + radius)
            .into_iter()
            .filter_map(|wire_id| self.entity_index.wire(wire_id))
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        let split_target = candidates.into_iter().find_map(|wire_index| {
            let wire = &self.document.wires[wire_index];
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
        self.spatial_index
            .update_wire(&self.document.wires[wire_index]);
        if let Some(wire) = self.document.wires.last() {
            self.spatial_index.update_wire(wire);
            self.entity_index
                .insert_wire(wire.id, self.document.wires.len() - 1);
        }
        self.attachment_index
            .add_wire(&self.document.wires[wire_index], &self.document.components);
        if let Some(wire) = self.document.wires.last() {
            self.attachment_index
                .add_wire(wire, &self.document.components);
        }
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
        let wire = &mut self.document.wires[wire_index];
        wire.points[point_index] = position;
        let is_endpoint = point_index == 0 || point_index + 1 == wire.points.len();
        if !is_endpoint {
            crate::ui::app::straighten_neighbor_segments(wire, point_index);
        }
        self.spatial_index
            .update_wire(&self.document.wires[wire_index]);
        self.attachment_index
            .add_wire(&self.document.wires[wire_index], &self.document.components);
        true
    }

    pub(crate) fn insert_wire_control_point(&mut self, position: Pos2) -> Option<(u64, usize)> {
        let radius = Vec2::splat(10.0);
        let wire_ids = self
            .spatial_index
            .query_wires(position - radius, position + radius);
        for wire_id in wire_ids {
            let Some(index) = self.entity_index.wire(wire_id) else {
                continue;
            };
            let segment_index = self.document.wires[index]
                .points
                .windows(2)
                .position(|segment| distance_to_segment(position, segment[0], segment[1]) <= 10.0);
            let Some(segment_index) = segment_index else {
                continue;
            };
            let segment = &self.document.wires[index].points[segment_index..=segment_index + 1];
            let inserted = if (segment[0].y - segment[1].y).abs() <= 0.5 {
                Pos2::new(
                    position.x.clamp(
                        segment[0].x.min(segment[1].x),
                        segment[0].x.max(segment[1].x),
                    ),
                    segment[0].y,
                )
            } else if (segment[0].x - segment[1].x).abs() <= 0.5 {
                Pos2::new(
                    segment[0].x,
                    position.y.clamp(
                        segment[0].y.min(segment[1].y),
                        segment[0].y.max(segment[1].y),
                    ),
                )
            } else {
                crate::ui::app::closest_point_on_segment(position, segment[0], segment[1])
            };
            let point_index = segment_index + 1;
            self.document.wires[index]
                .points
                .insert(point_index, inserted);
            self.spatial_index.update_wire(&self.document.wires[index]);
            self.attachment_index
                .add_wire(&self.document.wires[index], &self.document.components);
            return Some((wire_id, point_index));
        }
        None
    }

    pub(crate) fn tidy_wires(&mut self, wire_id: Option<u64>) -> usize {
        let mut count = 0;
        for wire in &mut self.document.wires {
            if wire_id.is_none_or(|id| id == wire.id) {
                crate::tidy_wire_points(wire);
                self.spatial_index.update_wire(wire);
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
        self.rebuild_entity_index();
        self.spatial_index.sync(&[], &[]);
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
        self.document.board.set_outline(outline);
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
        changed |= self.document.board.set_outline(outline);
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
