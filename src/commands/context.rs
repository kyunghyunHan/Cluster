use crate::app::{Selection, Tool};
use crate::model::cad::Point2;
use crate::model::{Component, ComponentKind, Counters, DragState, Wire, WireEndpoint};
use crate::pcb::board::{Board, BoardOutline};
use crate::pcb::track::TrackSegment;
use crate::pcb::via::Via;
use egui::Pos2;
use std::collections::HashSet;

/// The only application capability exposed to document commands.
///
/// The wrapped application reference is deliberately private to this module.
/// Command modules can use the narrow operations below, but cannot invalidate
/// caches, open dialogs, alter view state, or reach persistence directly.
pub(crate) struct CommandContext<'a> {
    app: &'a mut crate::CircuitApp,
}

impl<'a> CommandContext<'a> {
    pub(super) fn new(app: &'a mut crate::CircuitApp) -> Self {
        Self { app }
    }

    pub(crate) fn components(&self) -> &[Component] {
        &self.app.document.components
    }

    pub(crate) fn components_mut(&mut self) -> &mut Vec<Component> {
        &mut self.app.document.components
    }

    pub(crate) fn wires_mut(&mut self) -> &mut Vec<Wire> {
        &mut self.app.document.wires
    }

    pub(crate) fn board_mut(&mut self) -> &mut Board {
        &mut self.app.document.board
    }

    pub(crate) fn next_id(&mut self) -> u64 {
        self.app.next_id()
    }

    pub(crate) fn next_label(&mut self, kind: ComponentKind) -> String {
        self.app.next_label(kind)
    }

    pub(crate) fn next_custom_label(&self, part_id: Option<&str>) -> String {
        self.app.next_custom_label(part_id)
    }

    pub(crate) fn place_component(&mut self, kind: ComponentKind, position: Pos2) -> u64 {
        self.app.place_component(kind, position)
    }

    pub(crate) fn place_custom_component(&mut self, part_id: &str, position: Pos2) -> u64 {
        self.app.place_custom_component(part_id, position)
    }

    pub(crate) fn infer_wire_endpoint(&self, point: Pos2) -> WireEndpoint {
        self.app.infer_wire_endpoint(point)
    }

    pub(crate) fn split_wire_at_point(&mut self, point: Pos2) {
        self.app.split_wire_at_point(point);
    }

    pub(crate) fn selected(&self) -> Option<Selection> {
        self.app.editor.selected
    }

    pub(crate) fn take_selected(&mut self) -> Option<Selection> {
        self.app.editor.selected.take()
    }

    pub(crate) fn set_selected(&mut self, selected: Option<Selection>) {
        self.app.editor.selected = selected;
    }

    pub(crate) fn multi_selected(&self) -> &HashSet<u64> {
        &self.app.editor.multi_selected
    }

    pub(crate) fn set_multi_selected(&mut self, selected: HashSet<u64>) {
        self.app.editor.multi_selected = selected;
    }

    pub(crate) fn clear_multi_selected(&mut self) {
        self.app.editor.multi_selected.clear();
    }

    pub(crate) fn set_drag(&mut self, drag: Option<DragState>) {
        self.app.editor.drag = drag;
    }

    pub(crate) fn grid(&self) -> f32 {
        self.app.grid
    }

    pub(crate) fn reset_document_and_editor(&mut self) {
        self.app.document.components.clear();
        self.app.document.wires.clear();
        self.app.document.counters = Counters::default();
        self.app.document.next_id = 1;
        self.app.editor.selected = None;
        self.app.editor.multi_selected.clear();
        self.app.editor.drag = None;
        self.app.editor.draft_wire.clear();
        self.app.editor.wire_from_select = false;
        self.app.editor.snap_target = None;
        self.app.editor.tool = Tool::Select;
    }

    pub(crate) fn move_footprint(&mut self, footprint_id: u64, position: Point2) -> bool {
        self.board_mut()
            .footprints
            .iter_mut()
            .find(|footprint| footprint.id == footprint_id)
            .is_some_and(|footprint| {
                footprint.position = position;
                footprint.placed = true;
                true
            })
    }

    pub(crate) fn rotate_footprint(&mut self, footprint_id: u64, delta_deg: f32) -> bool {
        self.board_mut()
            .footprints
            .iter_mut()
            .find(|footprint| footprint.id == footprint_id)
            .is_some_and(|footprint| {
                footprint.rotation_deg = (footprint.rotation_deg + delta_deg).rem_euclid(360.0);
                true
            })
    }

    pub(crate) fn add_track(&mut self, track: TrackSegment) {
        self.board_mut().tracks.push(track);
    }

    pub(crate) fn remove_track(&mut self, track_id: u64) -> bool {
        let board = self.board_mut();
        let before = board.tracks.len();
        board.tracks.retain(|track| track.id != track_id);
        before != board.tracks.len()
    }

    pub(crate) fn add_via(&mut self, via: Via) {
        self.board_mut().vias.push(via);
    }

    pub(crate) fn remove_via(&mut self, via_id: u64) -> bool {
        let board = self.board_mut();
        let before = board.vias.len();
        board.vias.retain(|via| via.id != via_id);
        before != board.vias.len()
    }

    pub(crate) fn set_outline(&mut self, outline: BoardOutline) {
        self.board_mut().outline = outline;
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
