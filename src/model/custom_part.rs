//! User-defined custom parts loaded from JSON files.
//!
//! Each `*.json` file in the `cluster_parts/` folder next to the executable's
//! working directory describes one schematic part: a name, a body size, and a
//! list of pins on the left/right edges. Loaded parts are placed like built-in
//! module components (`ComponentKind::Custom` + `Component::part_id`), take
//! part in wiring, netlisting, and ERC, and are marked "Symbol only" for
//! simulation.

use super::pin::PinRole;
use egui::Vec2;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{LazyLock, Mutex, RwLock};

pub(crate) const CUSTOM_PARTS_DIR: &str = "cluster_parts";
pub(crate) const CUSTOM_PART_SCHEMA_VERSION: u32 = 2;

/// Pin spacing used by module-style symbols (see `module_pin_y`).
const PIN_SPACING: f32 = 20.0;

// ── JSON file schema ──────────────────────────────────────────────────────────

/// Raw JSON form of a custom part file. Field defaults keep hand-written
/// files short: only `id`, `name`, and `pins` are required.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CustomPartFile {
    #[serde(default = "default_schema_version")]
    pub(crate) schema_version: u32,
    /// Stable identifier stored in circuit files, e.g. "user:bme280".
    pub(crate) id: String,
    /// Display name shown in the palette and on the canvas body.
    pub(crate) name: String,
    /// Short text drawn inside the body (defaults to a trimmed `name`).
    #[serde(default)]
    pub(crate) chip_label: String,
    #[serde(default)]
    pub(crate) description: String,
    /// Reference-label prefix for placed instances, e.g. "U" -> U1, U2.
    #[serde(default)]
    pub(crate) label_prefix: String,
    /// Initial `value` field of placed instances.
    #[serde(default)]
    pub(crate) default_value: String,
    /// Body size in canvas pixels; 0 = auto from pin count.
    #[serde(default)]
    pub(crate) width: f32,
    #[serde(default)]
    pub(crate) height: f32,
    pub(crate) pins: Vec<CustomPinFile>,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    #[serde(default)]
    pub(crate) operating_voltage: Option<CustomVoltageRange>,
    #[serde(default)]
    pub(crate) interfaces: Vec<CustomInterfaceFile>,
    #[serde(default)]
    pub(crate) footprint: Option<CustomFootprintFile>,
    #[serde(default)]
    pub(crate) simulation: Option<CustomSimulationFile>,
    #[serde(default)]
    pub(crate) documentation: Option<String>,
}

fn default_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CustomPinFile {
    pub(crate) name: String,
    /// One of: passive, positive, power_out, ground, digital, i2c, control,
    /// output, input, bidirectional, open_collector, no_connect.
    #[serde(default)]
    pub(crate) role: String,
    /// "left" (default) or "right".
    #[serde(default)]
    pub(crate) side: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CustomVoltageRange {
    pub(crate) min_v: f32,
    pub(crate) max_v: f32,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CustomInterfaceFile {
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) pins: HashMap<String, String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CustomFootprintFile {
    pub(crate) id: String,
    pub(crate) width_mm: f32,
    pub(crate) height_mm: f32,
    pub(crate) pads: Vec<CustomPadFile>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct CustomPadFile {
    pub(crate) number: String,
    pub(crate) pin: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct CustomSimulationFile {
    pub(crate) support: String,
    #[serde(default)]
    pub(crate) model: Option<String>,
}

// ── Resolved in-memory definition ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CustomPartDef {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) chip_label: String,
    pub(crate) description: String,
    pub(crate) label_prefix: String,
    pub(crate) default_value: String,
    pub(crate) size: Vec2,
    pub(crate) left_pins: Vec<(&'static str, PinRole)>,
    pub(crate) right_pins: Vec<(&'static str, PinRole)>,
    pub(crate) tags: Vec<String>,
    pub(crate) operating_voltage: Option<CustomVoltageRange>,
    pub(crate) interfaces: Vec<CustomInterfaceFile>,
    pub(crate) footprint: Option<CustomFootprintFile>,
    pub(crate) simulation: Option<CustomSimulationFile>,
    pub(crate) documentation: Option<String>,
}

pub(crate) fn parse_pin_role(role: &str) -> Option<PinRole> {
    Some(match role.trim().to_ascii_lowercase().as_str() {
        "" | "passive" => PinRole::Passive,
        "positive" | "power" | "power_in" | "vcc" => PinRole::Positive,
        "power_out" | "power_output" => PinRole::PowerOutput,
        "ground" | "gnd" => PinRole::Ground,
        "digital" | "gpio" => PinRole::Digital,
        "i2c" => PinRole::I2c,
        "control" => PinRole::Control,
        "output" | "out" => PinRole::Output,
        "input" | "in" => PinRole::Input,
        "bidirectional" | "bidir" | "io" => PinRole::Bidirectional,
        "open_collector" | "open_drain" => PinRole::OpenCollector,
        "no_connect" | "nc" => PinRole::NoConnect,
        _ => return None,
    })
}

/// Validate a raw part file and resolve it into a `CustomPartDef`.
pub(crate) fn resolve_custom_part(file: CustomPartFile) -> Result<CustomPartDef, String> {
    if file.schema_version > CUSTOM_PART_SCHEMA_VERSION {
        return Err(format!(
            "schema_version {} is newer than supported version {}",
            file.schema_version, CUSTOM_PART_SCHEMA_VERSION
        ));
    }
    let id = file.id.trim().to_string();
    if id.is_empty() {
        return Err("part `id` must not be empty".to_string());
    }
    let name = file.name.trim().to_string();
    if name.is_empty() {
        return Err("part `name` must not be empty".to_string());
    }
    if file.pins.is_empty() {
        return Err("part must declare at least one pin".to_string());
    }
    if !file.width.is_finite() || !file.height.is_finite() || file.width < 0.0 || file.height < 0.0
    {
        return Err("symbol width/height must be finite and non-negative".to_string());
    }
    if let Some(range) = &file.operating_voltage
        && (!range.min_v.is_finite()
            || !range.max_v.is_finite()
            || range.min_v < 0.0
            || range.max_v < range.min_v)
    {
        return Err("operating_voltage must contain a finite min_v <= max_v".to_string());
    }

    let mut left_pins = Vec::new();
    let mut right_pins = Vec::new();
    let mut pin_names = HashSet::new();
    for pin in &file.pins {
        let pin_name = pin.name.trim();
        if pin_name.is_empty() {
            return Err("every pin needs a non-empty `name`".to_string());
        }
        if !pin_names.insert(pin_name.to_ascii_lowercase()) {
            return Err(format!("duplicate pin name `{pin_name}`"));
        }
        let role = parse_pin_role(&pin.role)
            .ok_or_else(|| format!("pin {pin_name}: unknown role `{}`", pin.role))?;
        let entry = (intern_pin_name(pin_name), role);
        match pin.side.trim().to_ascii_lowercase().as_str() {
            "" | "left" => left_pins.push(entry),
            "right" => right_pins.push(entry),
            other => return Err(format!("pin {pin_name}: unknown side `{other}`")),
        }
    }

    if let Some(footprint) = &file.footprint {
        if footprint.id.trim().is_empty()
            || !footprint.width_mm.is_finite()
            || !footprint.height_mm.is_finite()
            || footprint.width_mm <= 0.0
            || footprint.height_mm <= 0.0
        {
            return Err("footprint needs an id and positive finite dimensions".to_string());
        }
        if footprint.pads.is_empty() {
            return Err("footprint must declare at least one pad".to_string());
        }
        let mut pad_numbers = HashSet::new();
        let mut mapped_pins = HashSet::new();
        for pad in &footprint.pads {
            if pad.number.trim().is_empty() || !pad_numbers.insert(pad.number.trim()) {
                return Err(format!("duplicate or empty footprint pad `{}`", pad.number));
            }
            let pin = pad.pin.trim().to_ascii_lowercase();
            if !pin_names.contains(&pin) {
                return Err(format!(
                    "footprint pad {} maps unknown pin `{}`",
                    pad.number, pad.pin
                ));
            }
            mapped_pins.insert(pin);
        }
        if let Some(missing) = pin_names.difference(&mapped_pins).next() {
            return Err(format!("footprint has no pad for pin `{missing}`"));
        }
    }

    let rows = left_pins.len().max(right_pins.len()) as f32;
    let width = if file.width > 0.0 { file.width } else { 120.0 };
    let height = if file.height > 0.0 {
        file.height
    } else {
        (rows * PIN_SPACING + 40.0).max(60.0)
    };

    let chip_label = if file.chip_label.trim().is_empty() {
        name.chars().take(6).collect::<String>().to_uppercase()
    } else {
        file.chip_label.trim().to_string()
    };
    let label_prefix = if file.label_prefix.trim().is_empty() {
        "U".to_string()
    } else {
        file.label_prefix.trim().to_string()
    };

    Ok(CustomPartDef {
        id,
        name,
        chip_label,
        description: file.description.trim().to_string(),
        label_prefix,
        default_value: file.default_value.trim().to_string(),
        size: Vec2::new(width, height),
        left_pins,
        right_pins,
        tags: file.tags,
        operating_voltage: file.operating_voltage,
        interfaces: file.interfaces,
        footprint: file.footprint,
        simulation: file.simulation,
        documentation: file.documentation,
    })
}

pub(crate) fn parse_custom_part_json(text: &str) -> Result<CustomPartDef, String> {
    let file: CustomPartFile =
        serde_json::from_str(text).map_err(|error| format!("invalid JSON: {error}"))?;
    resolve_custom_part(file)
}

// ── Pin-name interner ─────────────────────────────────────────────────────────
//
// `CircuitPin::label` is `&'static str` throughout the editor. Custom parts
// have dynamic pin names, so we leak each unique name once and hand out the
// same `&'static str` on every later request. The leak is bounded by the set
// of distinct pin names the user ever loads in one app run.

static PIN_NAME_INTERNER: LazyLock<Mutex<HashSet<&'static str>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub(crate) fn intern_pin_name(name: &str) -> &'static str {
    let mut interner = PIN_NAME_INTERNER.lock().expect("pin name interner");
    if let Some(existing) = interner.get(name) {
        return existing;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    interner.insert(leaked);
    leaked
}

// ── Registry ──────────────────────────────────────────────────────────────────

static CUSTOM_PARTS: LazyLock<RwLock<HashMap<String, CustomPartDef>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub(crate) fn register_custom_part(def: CustomPartDef) {
    CUSTOM_PARTS
        .write()
        .expect("custom part registry")
        .insert(def.id.clone(), def);
}

pub(crate) fn custom_part(id: &str) -> Option<CustomPartDef> {
    CUSTOM_PARTS
        .read()
        .expect("custom part registry")
        .get(id)
        .cloned()
}

/// (id, display name) of every registered part, sorted by display name.
pub(crate) fn custom_part_list() -> Vec<(String, String)> {
    let mut list: Vec<(String, String)> = CUSTOM_PARTS
        .read()
        .expect("custom part registry")
        .values()
        .map(|def| (def.id.clone(), def.name.clone()))
        .collect();
    list.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    list
}

/// Load every `*.json` part file in `dir`, replacing previously loaded parts
/// that share the same id. Returns the number of parts loaded plus one
/// human-readable note per skipped file. A missing directory loads zero parts
/// without an error so first launch stays quiet.
pub(crate) fn load_custom_parts_dir(dir: &Path) -> (usize, Vec<String>) {
    let mut notes = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            if dir.exists() {
                notes.push(format!("Cannot read {}: {error}", dir.display()));
            }
            return (0, notes);
        }
    };

    let mut files: Vec<_> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    files.sort();

    let mut loaded = 0;
    for path in files {
        let display = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("part file")
            .to_string();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                notes.push(format!("{display}: {error}"));
                continue;
            }
        };
        match parse_custom_part_json(&text) {
            Ok(def) => {
                register_custom_part(def);
                loaded += 1;
            }
            Err(error) => notes.push(format!("{display}: {error}")),
        }
    }
    (loaded, notes)
}

/// Example part file written by the palette's "Create sample part" action.
pub(crate) fn sample_part_json() -> String {
    let sample = CustomPartFile {
        schema_version: CUSTOM_PART_SCHEMA_VERSION,
        id: "user:bme280".to_string(),
        name: "BME280 Sensor".to_string(),
        chip_label: "BME280".to_string(),
        description: "Temperature/humidity/pressure sensor breakout (I2C)".to_string(),
        label_prefix: "U".to_string(),
        default_value: "BME280".to_string(),
        width: 0.0,
        height: 0.0,
        pins: vec![
            CustomPinFile {
                name: "VCC".to_string(),
                role: "positive".to_string(),
                side: "left".to_string(),
            },
            CustomPinFile {
                name: "GND".to_string(),
                role: "ground".to_string(),
                side: "left".to_string(),
            },
            CustomPinFile {
                name: "SDA".to_string(),
                role: "i2c".to_string(),
                side: "right".to_string(),
            },
            CustomPinFile {
                name: "SCL".to_string(),
                role: "i2c".to_string(),
                side: "right".to_string(),
            },
        ],
        tags: vec!["sensor".to_string(), "i2c".to_string()],
        operating_voltage: Some(CustomVoltageRange {
            min_v: 1.8,
            max_v: 3.6,
        }),
        interfaces: vec![CustomInterfaceFile {
            kind: "i2c".to_string(),
            pins: HashMap::from([
                ("sda".to_string(), "SDA".to_string()),
                ("scl".to_string(), "SCL".to_string()),
            ]),
        }],
        footprint: None,
        simulation: Some(CustomSimulationFile {
            support: "symbolic".to_string(),
            model: None,
        }),
        documentation: None,
    };
    serde_json::to_string_pretty(&sample).expect("sample part serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_part_json_parses_with_defaults() {
        let def = parse_custom_part_json(
            r#"{
                "id": "user:test-min",
                "name": "Minimal Part",
                "pins": [{"name": "A"}, {"name": "B", "side": "right"}]
            }"#,
        )
        .expect("minimal part parses");

        assert_eq!(def.id, "user:test-min");
        assert_eq!(def.label_prefix, "U");
        assert_eq!(
            def.left_pins,
            vec![(intern_pin_name("A"), PinRole::Passive)]
        );
        assert_eq!(
            def.right_pins,
            vec![(intern_pin_name("B"), PinRole::Passive)]
        );
        assert!(def.size.x > 0.0 && def.size.y >= 60.0);
    }

    #[test]
    fn invalid_role_and_missing_pins_are_rejected() {
        let bad_role = parse_custom_part_json(
            r#"{"id": "user:x", "name": "X", "pins": [{"name": "A", "role": "warp"}]}"#,
        );
        assert!(bad_role.unwrap_err().contains("unknown role"));

        let no_pins = parse_custom_part_json(r#"{"id": "user:x", "name": "X", "pins": []}"#);
        assert!(no_pins.unwrap_err().contains("at least one pin"));
    }

    #[test]
    fn schema_v2_validates_duplicate_pins_and_footprint_mapping() {
        let duplicate = parse_custom_part_json(
            r#"{
                "schema_version": 2,
                "id": "user:duplicate",
                "name": "Duplicate",
                "pins": [{"name": "VCC"}, {"name": "vcc"}]
            }"#,
        );
        assert!(duplicate.unwrap_err().contains("duplicate pin name"));

        let missing_pad = parse_custom_part_json(
            r#"{
                "schema_version": 2,
                "id": "user:footprint",
                "name": "Footprint",
                "pins": [{"name": "A"}, {"name": "B"}],
                "footprint": {
                    "id": "User:TwoPad",
                    "width_mm": 4.0,
                    "height_mm": 2.0,
                    "pads": [{"number": "1", "pin": "A"}]
                }
            }"#,
        );
        assert!(missing_pad.unwrap_err().contains("no pad for pin"));
    }

    #[test]
    fn schema_v2_structured_metadata_parses_while_v1_defaults_remain_supported() {
        let def = parse_custom_part_json(
            r#"{
                "schema_version": 2,
                "id": "user:i2c-module",
                "name": "I2C Module",
                "pins": [{"name": "SDA", "role": "i2c"}],
                "tags": ["sensor", "i2c"],
                "operating_voltage": {"min_v": 1.8, "max_v": 3.6},
                "interfaces": [{"kind": "i2c", "pins": {"sda": "SDA"}}],
                "simulation": {"support": "symbolic"},
                "documentation": "docs/i2c-module.md"
            }"#,
        )
        .expect("schema v2 part parses");
        assert_eq!(def.tags, ["sensor", "i2c"]);
        assert_eq!(def.operating_voltage.unwrap().max_v, 3.6);
        assert_eq!(def.interfaces[0].kind, "i2c");
        assert_eq!(def.documentation.as_deref(), Some("docs/i2c-module.md"));
    }

    #[test]
    fn sample_part_json_round_trips_through_parser() {
        let def = parse_custom_part_json(&sample_part_json()).expect("sample parses");
        assert_eq!(def.id, "user:bme280");
        assert_eq!(def.left_pins.len(), 2);
        assert_eq!(def.right_pins.len(), 2);
        assert!(def.right_pins.iter().any(|(_, role)| *role == PinRole::I2c));
    }

    #[test]
    fn interner_returns_identical_static_reference() {
        let a = intern_pin_name("INTERN_TEST_PIN");
        let b = intern_pin_name("INTERN_TEST_PIN");
        assert!(std::ptr::eq(a, b));
    }

    #[test]
    fn registry_registers_and_lists_parts() {
        let def = parse_custom_part_json(
            r#"{"id": "user:test-registry", "name": "Registry Part",
                "pins": [{"name": "P1"}]}"#,
        )
        .unwrap();
        register_custom_part(def.clone());

        assert_eq!(custom_part("user:test-registry"), Some(def));
        assert!(
            custom_part_list()
                .iter()
                .any(|(id, name)| id == "user:test-registry" && name == "Registry Part")
        );
    }

    #[test]
    fn load_custom_parts_dir_reads_json_and_reports_bad_files() {
        let dir = std::env::temp_dir().join(format!(
            "cluster_parts_test_{}",
            std::process::id() as u64 + 7_431
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("good.json"), sample_part_json()).unwrap();
        std::fs::write(dir.join("bad.json"), "{not json").unwrap();
        std::fs::write(dir.join("ignored.txt"), "not a part").unwrap();

        let (loaded, notes) = load_custom_parts_dir(&dir);

        assert_eq!(loaded, 1);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].starts_with("bad.json"));
        assert!(custom_part("user:bme280").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_directory_loads_nothing_quietly() {
        let dir = std::env::temp_dir().join("cluster_parts_test_does_not_exist_xyz");
        let (loaded, notes) = load_custom_parts_dir(&dir);
        assert_eq!(loaded, 0);
        assert!(notes.is_empty());
    }
}
