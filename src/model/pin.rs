use super::component::ComponentKind;
use egui::Pos2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ElectricalType {
    Passive,
    PowerIn,
    Ground,
    Digital,
    I2c,
    Control,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PinRole {
    Passive,
    Positive,
    Ground,
    Digital,
    I2c,
    Control,
    Output,
}

#[derive(Debug, Clone)]
pub(crate) struct CircuitPin {
    pub(crate) label: &'static str,
    pub(crate) role: PinRole,
    pub(crate) pos: Pos2,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct PinRef {
    pub(crate) component_id: u64,
    pub(crate) pin_name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct NetlistPin {
    pub(crate) component_id: u64,
    pub(crate) component_label: String,
    pub(crate) component_kind: ComponentKind,
    pub(crate) component_value: String,
    pub(crate) pin_name: String,
    pub(crate) electrical_type: ElectricalType,
    pub(crate) position: Pos2,
    pub(crate) net_id: usize,
    pub(crate) connected_by_wire: bool,
    pub(crate) no_connect: bool,
}
