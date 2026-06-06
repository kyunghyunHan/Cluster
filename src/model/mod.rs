use egui::{Pos2, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum ComponentKind {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    Led,
    ZenerDiode,
    Switch,
    PushButton,
    SlideSwitch,
    Ground,
    VSource,
    ISource,
    Battery,
    OpAmp,
    Lamp,
    Potentiometer,
    NpnTransistor,
    PnpTransistor,
    Nmosfet,
    Pmosfet,
    VoltageReg,
    Fuse,
    LogicNot,
    LogicAnd,
    LogicOr,
    LogicNand,
    LogicNor,
    LogicXor,
    Esp32,
    Esp32S3,
    Esp32C3,
    ArduinoUno,
    RaspberryPiPico,
    Breadboard,
    Relay,
    DcMotor,
    Servo,
    Oled,
    Sensor,
    NetLabel,
    Timer555,
    Crystal,
    Transformer,
    Display7Seg,
    Thermistor,
    Varistor,
    VoltageRef,
    MotorDriver,
    SchottkyDiode,
    TvsDiode,
    Phototransistor,
    Optocoupler,
    GenericIc,
}

#[derive(Debug, Clone)]
pub(crate) struct Component {
    pub(crate) id: u64,
    pub(crate) kind: ComponentKind,
    pub(crate) pos: Pos2,
    pub(crate) rotation: i32,
    pub(crate) label: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Wire {
    pub(crate) id: u64,
    pub(crate) points: Vec<Pos2>,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
}

#[derive(Debug, Clone)]
pub(crate) struct Net {
    pub(crate) id: usize,
    pub(crate) name: String,
    pub(crate) connected_pins: Vec<PinRef>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CircuitNetlist {
    pub(crate) nets: Vec<Net>,
    pub(crate) pins: Vec<NetlistPin>,
    pub(crate) wire_nets: HashMap<u64, usize>,
    pub(crate) floating_wires: Vec<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Counters {
    pub(crate) resistor: usize,
    pub(crate) capacitor: usize,
    pub(crate) inductor: usize,
    pub(crate) diode: usize,
    pub(crate) led: usize,
    pub(crate) zener: usize,
    pub(crate) switch: usize,
    pub(crate) ground: usize,
    pub(crate) vsource: usize,
    pub(crate) isource: usize,
    pub(crate) battery: usize,
    pub(crate) opamp: usize,
    pub(crate) lamp: usize,
    pub(crate) pot: usize,
    pub(crate) npn: usize,
    pub(crate) pnp: usize,
    pub(crate) mosfet: usize,
    pub(crate) vreg: usize,
    pub(crate) fuse: usize,
    pub(crate) logic_gate: usize,
    pub(crate) esp32: usize,
    pub(crate) arduino: usize,
    pub(crate) pico: usize,
    pub(crate) breadboard: usize,
    pub(crate) relay: usize,
    pub(crate) motor: usize,
    pub(crate) servo: usize,
    pub(crate) oled: usize,
    pub(crate) sensor: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct CircuitSnapshot {
    pub(crate) components: Vec<Component>,
    pub(crate) wires: Vec<Wire>,
    pub(crate) next_id: u64,
    pub(crate) counters: Counters,
    pub(crate) pages: Vec<(String, Vec<Component>, Vec<Wire>, u64, Counters)>,
    pub(crate) current_page: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SavedCircuit {
    pub(crate) schema_version: u32,
    pub(crate) next_id: u64,
    pub(crate) counters: Counters,
    pub(crate) components: Vec<SavedComponent>,
    pub(crate) wires: Vec<SavedWire>,
    #[serde(default)]
    pub(crate) pages: Vec<SavedPage>,
    #[serde(default)]
    pub(crate) current_page: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SavedPage {
    pub(crate) name: String,
    pub(crate) next_id: u64,
    pub(crate) counters: Counters,
    pub(crate) components: Vec<SavedComponent>,
    pub(crate) wires: Vec<SavedWire>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SavedComponent {
    pub(crate) id: u64,
    pub(crate) kind: ComponentKind,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) rotation: i32,
    pub(crate) label: String,
    pub(crate) value: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SavedWire {
    pub(crate) id: u64,
    pub(crate) points: Vec<SavedPoint>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SavedPoint {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

#[derive(Debug, Clone)]
pub(crate) enum DragState {
    Component { id: u64, offset: Vec2 },
    WirePoint { wire_id: u64, point_index: usize },
}
