use super::component::ComponentKind;
use egui::Pos2;
use serde::{Deserialize, Serialize};

/// Electrical type of a pin, used by ERC to detect rule violations.
///
/// Variants map to IEC 60617 / KiCad pin-type semantics so that
/// rules like output-output conflict can be checked precisely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ElectricalType {
    /// Two-terminal passive device (resistor, capacitor, etc.)
    Passive,
    /// Requires power to be supplied (VCC, VIN on a module)
    PowerIn,
    /// Provides power to the net (voltage regulator output, battery +)
    PowerOutput,
    /// Dedicated ground return
    Ground,
    /// Generic digital signal (GPIO without direction specified)
    Digital,
    /// I²C bus line (SDA / SCL)
    I2c,
    /// Driven unidirectional output
    Output,
    /// Receives signal (must be driven by an output)
    Input,
    /// Can act as both input and output (e.g. I/O bus pins)
    Bidirectional,
    /// Open-collector / open-drain output (can only pull low)
    OpenCollector,
    /// Intentionally unconnected; suppresses "unconnected pin" ERC warnings
    NoConnect,
    /// Legacy control pin that doesn't cleanly fit other types
    Control,
}

impl ElectricalType {
    /// True if this type can drive a net (source current or assert a logic level).
    pub(crate) fn is_driver(self) -> bool {
        matches!(
            self,
            ElectricalType::Output
                | ElectricalType::PowerOutput
                | ElectricalType::Bidirectional
                | ElectricalType::OpenCollector
        )
    }

    /// Short label for display in inspector / ERC panels.
    pub(crate) fn short_label(self) -> &'static str {
        match self {
            ElectricalType::Passive => "Passive",
            ElectricalType::PowerIn => "Power In",
            ElectricalType::PowerOutput => "Power Out",
            ElectricalType::Ground => "Ground",
            ElectricalType::Digital => "Digital",
            ElectricalType::I2c => "I²C",
            ElectricalType::Output => "Output",
            ElectricalType::Input => "Input",
            ElectricalType::Bidirectional => "Bidir",
            ElectricalType::OpenCollector => "Open-Collector",
            ElectricalType::NoConnect => "No-Connect",
            ElectricalType::Control => "Control",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PinRole {
    Passive,
    Positive,
    /// Provides regulated power to downstream components
    PowerOutput,
    Ground,
    Digital,
    I2c,
    Control,
    Output,
    /// Receives a signal; must be driven by an Output or similar
    Input,
    /// Can act as input or output
    Bidirectional,
    /// Open-collector/open-drain; can only pull low
    OpenCollector,
    /// Intentionally left unconnected; suppresses ERC warnings
    NoConnect,
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
