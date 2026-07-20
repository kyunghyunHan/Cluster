use crate::model::ComponentKind;
use crate::model::DragState;
use egui::Pos2;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tool {
    Select,
    Wire,
    Place(ComponentKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Selection {
    Component(u64),
    Wire(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlignDir {
    Left,
    Right,
    Top,
    Bottom,
    CenterH,
    CenterV,
}

/// Transient editor state that document commands are allowed to change.
///
/// History, clipboard, dialogs and workspace state deliberately live outside
/// this capability.
pub(crate) struct EditorDocumentState {
    pub(crate) tool: Tool,
    pub(crate) selected: Option<Selection>,
    pub(crate) drag: Option<DragState>,
    pub(crate) draft_wire: Vec<Pos2>,
    pub(crate) wire_from_select: bool,
    pub(crate) multi_selected: HashSet<u64>,
    pub(crate) snap_target: Option<Pos2>,
    pub(crate) grid: f32,
}

impl Default for EditorDocumentState {
    fn default() -> Self {
        Self {
            tool: Tool::Select,
            selected: None,
            drag: None,
            draft_wire: Vec::new(),
            wire_from_select: false,
            multi_selected: HashSet::new(),
            snap_target: None,
            grid: 20.0,
        }
    }
}
