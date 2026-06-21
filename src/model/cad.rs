#![allow(dead_code)]

use super::{CircuitNetlist, Component, ComponentKind, PinRef, PinRole};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub(crate) const CAD_SCHEMA_VERSION: u32 = 1;

pub(crate) type SymbolId = String;
pub(crate) type FootprintId = String;
pub(crate) type NetClassId = String;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct Point2 {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

impl Point2 {
    pub(crate) fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct Size2 {
    pub(crate) w: f32,
    pub(crate) h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SymbolPinType {
    Input,
    Output,
    Bidirectional,
    Passive,
    PowerIn,
    PowerOut,
    OpenCollector,
    NoConnect,
}

impl From<PinRole> for SymbolPinType {
    fn from(role: PinRole) -> Self {
        match role {
            PinRole::Passive => SymbolPinType::Passive,
            PinRole::Positive => SymbolPinType::PowerIn,
            PinRole::Ground => SymbolPinType::PowerIn,
            PinRole::Digital | PinRole::I2c => SymbolPinType::Bidirectional,
            PinRole::Control => SymbolPinType::Input,
            PinRole::Output => SymbolPinType::Output,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SymbolPin {
    pub(crate) number: String,
    pub(crate) name: String,
    pub(crate) pin_type: SymbolPinType,
    pub(crate) position: Point2,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SymbolFields {
    pub(crate) manufacturer: Option<String>,
    pub(crate) mpn: Option<String>,
    pub(crate) datasheet: Option<String>,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Symbol {
    pub(crate) symbol_id: SymbolId,
    pub(crate) display_name: String,
    pub(crate) default_reference_prefix: String,
    pub(crate) pins: Vec<SymbolPin>,
    pub(crate) fields: SymbolFields,
    pub(crate) footprint_link: Option<FootprintId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SymbolInstance {
    pub(crate) instance_id: u64,
    pub(crate) symbol_id: SymbolId,
    pub(crate) reference: String,
    pub(crate) value: String,
    pub(crate) position: Point2,
    pub(crate) rotation_deg: i32,
    pub(crate) fields: SymbolFields,
    pub(crate) footprint_link: Option<FootprintId>,
}

impl SymbolInstance {
    pub(crate) fn from_component(component: &Component) -> Self {
        Self {
            instance_id: component.id,
            symbol_id: symbol_id_for_kind(component.kind).to_string(),
            reference: component.label.clone(),
            value: component.value.clone(),
            position: Point2::new(component.pos.x, component.pos.y),
            rotation_deg: component.rotation.rem_euclid(360),
            fields: SymbolFields::default(),
            footprint_link: default_footprint_for_kind(component.kind).map(str::to_string),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CadNet {
    pub(crate) net_id: usize,
    pub(crate) name: String,
    pub(crate) connected_pins: Vec<PinRef>,
    pub(crate) class_id: NetClassId,
}

impl CadNet {
    pub(crate) fn from_circuit_netlist(netlist: &CircuitNetlist, default_class: &str) -> Vec<Self> {
        netlist
            .nets
            .iter()
            .map(|net| Self {
                net_id: net.id,
                name: net.name.clone(),
                connected_pins: net.connected_pins.clone(),
                class_id: default_class.to_string(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NetClass {
    pub(crate) class_id: NetClassId,
    pub(crate) clearance_mm: f32,
    pub(crate) track_width_mm: f32,
    pub(crate) via_diameter_mm: f32,
    pub(crate) via_drill_mm: f32,
    pub(crate) diff_pair: Option<String>,
}

impl Default for NetClass {
    fn default() -> Self {
        Self {
            class_id: "Default".to_string(),
            clearance_mm: 0.2,
            track_width_mm: 0.25,
            via_diameter_mm: 0.8,
            via_drill_mm: 0.4,
            diff_pair: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CadProjectData {
    pub(crate) schema_version: u32,
    pub(crate) symbols: Vec<SymbolInstance>,
    pub(crate) nets: Vec<CadNet>,
    pub(crate) net_classes: Vec<NetClass>,
    pub(crate) board: Option<crate::pcb::board::Board>,
    pub(crate) properties: HashMap<String, String>,
}

impl CadProjectData {
    pub(crate) fn from_schematic(components: &[Component], netlist: &CircuitNetlist) -> Self {
        let default_class = NetClass::default();
        Self {
            schema_version: CAD_SCHEMA_VERSION,
            symbols: components
                .iter()
                .map(SymbolInstance::from_component)
                .collect(),
            nets: CadNet::from_circuit_netlist(netlist, &default_class.class_id),
            net_classes: vec![default_class],
            board: None,
            properties: HashMap::new(),
        }
    }
}

pub(crate) fn symbol_id_for_kind(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Resistor => "Device:R",
        ComponentKind::Capacitor => "Device:C",
        ComponentKind::Inductor => "Device:L",
        ComponentKind::Led => "Device:LED",
        ComponentKind::Diode => "Device:D",
        ComponentKind::Battery => "Device:Battery",
        ComponentKind::Ground => "Power:GND",
        ComponentKind::Esp32 => "MCU:ESP32_DevKit_V1",
        ComponentKind::ArduinoUno => "MCU:Arduino_Uno_R3",
        ComponentKind::RaspberryPiPico => "MCU:RaspberryPi_Pico",
        ComponentKind::Oled => "Display:SSD1306_OLED_I2C",
        ComponentKind::Relay => "Module:Relay",
        ComponentKind::MotorDriver => "Module:L298N",
        ComponentKind::Servo => "Motor:SG90_Servo",
        _ => "Cluster:GenericSymbol",
    }
}

pub(crate) fn default_footprint_for_kind(kind: ComponentKind) -> Option<&'static str> {
    match kind {
        ComponentKind::Resistor => Some("R_THT_Axial"),
        ComponentKind::Capacitor => Some("C_THT_Radial"),
        ComponentKind::Led => Some("LED_THT_5mm"),
        ComponentKind::Diode => Some("D_DO-35"),
        ComponentKind::Esp32 => Some("Module_ESP32_DevKit_V1"),
        ComponentKind::ArduinoUno => Some("Module_Arduino_Uno_R3"),
        ComponentKind::RaspberryPiPico => Some("Module_RaspberryPi_Pico"),
        ComponentKind::Oled => Some("Module_SSD1306_0.96_I2C"),
        ComponentKind::Relay => Some("Module_Relay_1CH"),
        ComponentKind::MotorDriver => Some("Module_L298N"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;

    #[test]
    fn component_converts_to_symbol_instance_with_footprint_link() {
        let component = Component {
            id: 42,
            kind: ComponentKind::Resistor,
            pos: Pos2::new(10.0, 20.0),
            rotation: 450,
            label: "R1".to_string(),
            value: "10k".to_string(),
        };

        let symbol = SymbolInstance::from_component(&component);

        assert_eq!(symbol.symbol_id, "Device:R");
        assert_eq!(symbol.reference, "R1");
        assert_eq!(symbol.rotation_deg, 90);
        assert_eq!(symbol.footprint_link.as_deref(), Some("R_THT_Axial"));
    }

    #[test]
    fn default_net_class_has_manufacturable_beginner_rules() {
        let class = NetClass::default();
        assert!(class.clearance_mm >= 0.15);
        assert!(class.track_width_mm >= 0.2);
        assert!(class.via_drill_mm >= 0.3);
    }
}
