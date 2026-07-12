use super::component::{Component, ComponentKind};
use super::wire::{SavedWireEndpoint, Wire};
use egui::Vec2;
use serde::{Deserialize, Serialize};

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
    pub(crate) meter: usize,
    pub(crate) dht: usize,
    pub(crate) hcsr04: usize,
    pub(crate) buzzer: usize,
    pub(crate) neopixel: usize,
    pub(crate) pir: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct CircuitSnapshot {
    pub(crate) components: Vec<Component>,
    pub(crate) wires: Vec<Wire>,
    pub(crate) next_id: u64,
    pub(crate) counters: Counters,
    #[allow(clippy::type_complexity)] // Migrated to SchematicPage in the next storage schema.
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
    pub(crate) junction_dots: Vec<SavedJunctionDot>,
    #[serde(default)]
    pub(crate) no_connect_markers: Vec<SavedNoConnectMarker>,
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
    #[serde(default)]
    pub(crate) junction_dots: Vec<SavedJunctionDot>,
    #[serde(default)]
    pub(crate) no_connect_markers: Vec<SavedNoConnectMarker>,
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
    /// Custom part definition id; absent for built-in kinds (schema <= v4
    /// files never contain it, so old circuits load unchanged).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) part_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SavedWire {
    pub(crate) id: u64,
    pub(crate) points: Vec<SavedPoint>,
    #[serde(default)]
    pub(crate) start: Option<SavedWireEndpoint>,
    #[serde(default)]
    pub(crate) end: Option<SavedWireEndpoint>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SavedJunctionDot {
    pub(crate) id: u64,
    pub(crate) x: f32,
    pub(crate) y: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SavedNoConnectMarker {
    pub(crate) id: u64,
    pub(crate) x: f32,
    pub(crate) y: f32,
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
