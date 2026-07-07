use crate::model::ComponentKind;

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
