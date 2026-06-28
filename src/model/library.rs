#![allow(dead_code)]

use super::cad::{FootprintId, SymbolId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub(crate) const LIBRARY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PinMapping {
    pub(crate) symbol_pin: String,
    pub(crate) footprint_pad: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SupplierFields {
    pub(crate) manufacturer: Option<String>,
    pub(crate) manufacturer_part_number: Option<String>,
    pub(crate) lcsc_part_number: Option<String>,
    pub(crate) jlcpcb_part_number: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LibraryPart {
    pub(crate) part_id: String,
    pub(crate) display_name: String,
    pub(crate) symbol_id: SymbolId,
    pub(crate) footprint_id: FootprintId,
    pub(crate) pin_map: Vec<PinMapping>,
    pub(crate) model_3d_path: Option<String>,
    pub(crate) default_value: Option<String>,
    pub(crate) simulation_model_path: Option<String>,
    pub(crate) package_type: Option<String>,
    pub(crate) supplier: SupplierFields,
    pub(crate) properties: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LibraryCatalog {
    pub(crate) schema_version: u32,
    pub(crate) name: String,
    pub(crate) parts: Vec<LibraryPart>,
    pub(crate) user_library_roots: Vec<PathBuf>,
}

impl LibraryCatalog {
    pub(crate) fn empty_user_catalog(name: impl Into<String>) -> Self {
        Self {
            schema_version: LIBRARY_SCHEMA_VERSION,
            name: name.into(),
            parts: Vec::new(),
            user_library_roots: Vec::new(),
        }
    }

    pub(crate) fn find_part(&self, query: &str) -> Vec<&LibraryPart> {
        let query = query.to_ascii_lowercase();
        self.parts
            .iter()
            .filter(|part| {
                part.part_id.to_ascii_lowercase().contains(&query)
                    || part.display_name.to_ascii_lowercase().contains(&query)
                    || part
                        .supplier
                        .manufacturer_part_number
                        .as_deref()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .contains(&query)
                    || part
                        .supplier
                        .lcsc_part_number
                        .as_deref()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .contains(&query)
            })
            .collect()
    }
}

pub(crate) fn esp32_devkit_part() -> LibraryPart {
    LibraryPart {
        part_id: "cluster:esp32-devkit-v1".to_string(),
        display_name: "ESP32 DevKit V1".to_string(),
        symbol_id: "MCU:ESP32_DevKit_V1".to_string(),
        footprint_id: "Module_ESP32_DevKit_V1".to_string(),
        pin_map: vec![
            PinMapping {
                symbol_pin: "3V3".to_string(),
                footprint_pad: "3V3".to_string(),
            },
            PinMapping {
                symbol_pin: "GND".to_string(),
                footprint_pad: "GND".to_string(),
            },
            PinMapping {
                symbol_pin: "GPIO21_SDA".to_string(),
                footprint_pad: "IO21".to_string(),
            },
            PinMapping {
                symbol_pin: "GPIO22_SCL".to_string(),
                footprint_pad: "IO22".to_string(),
            },
        ],
        model_3d_path: Some("models/esp32-devkit-v1.step".to_string()),
        default_value: Some("ESP32 DevKit V1".to_string()),
        simulation_model_path: None,
        package_type: Some("Module".to_string()),
        supplier: SupplierFields {
            manufacturer: Some("Espressif".to_string()),
            manufacturer_part_number: Some("ESP32-DEVKIT-V1".to_string()),
            lcsc_part_number: None,
            jlcpcb_part_number: None,
        },
        properties: HashMap::new(),
    }
}

pub(crate) fn stm32_blue_pill_part() -> LibraryPart {
    LibraryPart {
        part_id: "cluster:stm32-blue-pill-f103c8".to_string(),
        display_name: "STM32 Blue Pill F103C8".to_string(),
        symbol_id: "MCU:STM32_BluePill_F103C8".to_string(),
        footprint_id: "Module_STM32_BluePill".to_string(),
        pin_map: vec![
            PinMapping {
                symbol_pin: "3V3".to_string(),
                footprint_pad: "3V3".to_string(),
            },
            PinMapping {
                symbol_pin: "GND".to_string(),
                footprint_pad: "GND".to_string(),
            },
            PinMapping {
                symbol_pin: "PB7_SDA".to_string(),
                footprint_pad: "PB7".to_string(),
            },
            PinMapping {
                symbol_pin: "PB6_SCL".to_string(),
                footprint_pad: "PB6".to_string(),
            },
            PinMapping {
                symbol_pin: "PA13_SWDIO".to_string(),
                footprint_pad: "PA13".to_string(),
            },
            PinMapping {
                symbol_pin: "PA14_SWCLK".to_string(),
                footprint_pad: "PA14".to_string(),
            },
        ],
        model_3d_path: Some("models/stm32-blue-pill.step".to_string()),
        default_value: Some("STM32F103C8T6 Blue Pill".to_string()),
        simulation_model_path: None,
        package_type: Some("Module".to_string()),
        supplier: SupplierFields {
            manufacturer: Some("STMicroelectronics".to_string()),
            manufacturer_part_number: Some("STM32F103C8T6".to_string()),
            lcsc_part_number: None,
            jlcpcb_part_number: None,
        },
        properties: HashMap::from([
            ("core".to_string(), "Cortex-M3".to_string()),
            ("logic_voltage".to_string(), "3.3V".to_string()),
            ("debug".to_string(), "SWD".to_string()),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_part_carries_mapping_supplier_and_model_metadata() {
        let part = esp32_devkit_part();

        assert_eq!(part.symbol_id, "MCU:ESP32_DevKit_V1");
        assert_eq!(part.footprint_id, "Module_ESP32_DevKit_V1");
        assert!(
            part.pin_map
                .iter()
                .any(|pin| pin.symbol_pin.contains("SDA"))
        );
        assert_eq!(part.supplier.manufacturer.as_deref(), Some("Espressif"));
        assert!(part.model_3d_path.as_deref().unwrap().ends_with(".step"));
    }

    #[test]
    fn stm32_part_carries_swd_and_i2c_mapping() {
        let part = stm32_blue_pill_part();

        assert_eq!(part.symbol_id, "MCU:STM32_BluePill_F103C8");
        assert_eq!(
            part.supplier.manufacturer.as_deref(),
            Some("STMicroelectronics")
        );
        assert!(
            part.pin_map
                .iter()
                .any(|pin| pin.symbol_pin.contains("SDA"))
        );
        assert!(
            part.pin_map
                .iter()
                .any(|pin| pin.symbol_pin.contains("SWDIO"))
        );
    }

    #[test]
    fn user_catalog_searches_mpn_and_lcsc_fields() {
        let mut catalog = LibraryCatalog::empty_user_catalog("User");
        let mut part = esp32_devkit_part();
        part.supplier.lcsc_part_number = Some("C123456".to_string());
        catalog.parts.push(part);

        assert_eq!(catalog.find_part("devkit").len(), 1);
        assert_eq!(catalog.find_part("C123456").len(), 1);
    }
}
