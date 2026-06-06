use serde::{Deserialize, Serialize};
use egui::Vec2;
use super::component::{Component, ComponentKind};
use super::wire::Wire;

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
