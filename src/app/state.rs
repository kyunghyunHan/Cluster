use crate::model::ComponentKind;

/// Active editing tool on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tool {
    Select,
    Place(ComponentKind),
    Wire,
}

/// Alignment direction for multi-select alignment commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlignDir {
    Left,
    Right,
    Top,
    Bottom,
    CenterH,
    CenterV,
}

/// What is currently selected on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Selection {
    Component(u64),
    Wire(u64),
}
