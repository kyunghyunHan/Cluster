use crate::app::{AlignDir, Selection};
use crate::commands::EditorCommand;
use crate::commands::component::ComponentCommand;
use crate::commands::document::DocumentCommand;
use crate::commands::pcb::PcbCommand;
use crate::commands::selection::SelectionCommand;
use crate::commands::wiring::WiringCommand;
use crate::engine::validation::ErcAutoFix;
use crate::engine::{
    mna, netlist::build_canonical_connectivity_with_annotations, simulation as simulation_engine,
};
use crate::model::cad::{CadProjectData, Point2};
use crate::model::*;
use crate::pcb::board::{Board, BoardOutline, RemovedFootprintPolicy};
use crate::pcb::drc::{DrcSeverity, run_drc_with_nets};
use crate::pcb::layer::BoardLayer;
use crate::pcb::track::TrackSegment;
use crate::storage::save::{ProjectFolderLayout, write_with_backup};
use crate::ui::bottom_dock::{
    PcbDockSummary, PcbDrcRow, PcbDrcSeverity, PcbPreviewData, PcbPreviewDiagnostic,
    PcbPreviewFootprint, PcbPreviewRatsnest, PcbPreviewTrack,
};
use crate::ui::breadboard::BreadboardRoute;
use egui::{Pos2, Vec2};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

impl crate::CircuitApp {
    // ── ID / label helpers ───────────────────────────────────────────────────

    pub(crate) fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub(crate) fn next_label(&mut self, kind: ComponentKind) -> String {
        match kind {
            ComponentKind::Resistor => {
                self.counters.resistor += 1;
                format!("R{}", self.counters.resistor)
            }
            ComponentKind::Capacitor => {
                self.counters.capacitor += 1;
                format!("C{}", self.counters.capacitor)
            }
            ComponentKind::Inductor => {
                self.counters.inductor += 1;
                format!("L{}", self.counters.inductor)
            }
            ComponentKind::Diode => {
                self.counters.diode += 1;
                format!("D{}", self.counters.diode)
            }
            ComponentKind::Led => {
                self.counters.led += 1;
                format!("LED{}", self.counters.led)
            }
            ComponentKind::ZenerDiode => {
                self.counters.zener += 1;
                format!("ZD{}", self.counters.zener)
            }
            ComponentKind::NpnTransistor => {
                self.counters.npn += 1;
                format!("Q{}", self.counters.npn)
            }
            ComponentKind::PnpTransistor => {
                self.counters.pnp += 1;
                format!("Q{}", self.counters.pnp + 100)
            }
            ComponentKind::Nmosfet => {
                self.counters.mosfet += 1;
                format!("M{}", self.counters.mosfet)
            }
            ComponentKind::Pmosfet => {
                self.counters.mosfet += 1;
                format!("M{}", self.counters.mosfet + 100)
            }
            ComponentKind::Potentiometer => {
                self.counters.pot += 1;
                format!("RV{}", self.counters.pot)
            }
            ComponentKind::VoltageReg => {
                self.counters.vreg += 1;
                format!("U{}", self.counters.vreg + 50)
            }
            ComponentKind::Fuse => {
                self.counters.fuse += 1;
                format!("F{}", self.counters.fuse)
            }
            ComponentKind::LogicNot
            | ComponentKind::LogicAnd
            | ComponentKind::LogicOr
            | ComponentKind::LogicNand
            | ComponentKind::LogicNor
            | ComponentKind::LogicXor => {
                self.counters.logic_gate += 1;
                let prefix = match kind {
                    ComponentKind::LogicNot => "INV",
                    ComponentKind::LogicAnd => "AND",
                    ComponentKind::LogicOr => "OR",
                    ComponentKind::LogicNand => "NAND",
                    ComponentKind::LogicNor => "NOR",
                    ComponentKind::LogicXor => "XOR",
                    _ => "G",
                };
                format!("{}{}", prefix, self.counters.logic_gate)
            }
            ComponentKind::Switch | ComponentKind::PushButton | ComponentKind::SlideSwitch => {
                self.counters.switch += 1;
                format!("SW{}", self.counters.switch)
            }
            ComponentKind::Ground => {
                self.counters.ground += 1;
                if self.counters.ground == 1 {
                    "GND".to_string()
                } else {
                    format!("GND{}", self.counters.ground)
                }
            }
            ComponentKind::VSource => {
                self.counters.vsource += 1;
                format!("V{}", self.counters.vsource)
            }
            ComponentKind::ISource => {
                self.counters.isource += 1;
                format!("I{}", self.counters.isource)
            }
            ComponentKind::Battery => {
                self.counters.battery += 1;
                format!("BAT{}", self.counters.battery)
            }
            ComponentKind::OpAmp => {
                self.counters.opamp += 1;
                format!("U{}", self.counters.opamp)
            }
            ComponentKind::Lamp => {
                self.counters.lamp += 1;
                format!("LA{}", self.counters.lamp)
            }
            ComponentKind::Esp32 | ComponentKind::Esp32S3 | ComponentKind::Esp32C3 => {
                self.counters.esp32 += 1;
                format!("ESP{}", self.counters.esp32)
            }
            ComponentKind::ArduinoUno => {
                self.counters.arduino += 1;
                format!("ARD{}", self.counters.arduino)
            }
            ComponentKind::RaspberryPiPico => {
                self.counters.pico += 1;
                format!("PICO{}", self.counters.pico)
            }
            ComponentKind::Stm32BluePill | ComponentKind::Stm32Nucleo64 => {
                self.counters.logic_gate += 1;
                format!("STM{}", self.counters.logic_gate)
            }
            ComponentKind::Breadboard => {
                self.counters.breadboard += 1;
                format!("BB{}", self.counters.breadboard)
            }
            ComponentKind::Relay => {
                self.counters.relay += 1;
                format!("K{}", self.counters.relay)
            }
            ComponentKind::DcMotor => {
                self.counters.motor += 1;
                format!("M{}", self.counters.motor)
            }
            ComponentKind::Servo => {
                self.counters.servo += 1;
                format!("SV{}", self.counters.servo)
            }
            ComponentKind::Oled => {
                self.counters.oled += 1;
                format!("OLED{}", self.counters.oled)
            }
            ComponentKind::Sensor => {
                self.counters.sensor += 1;
                format!("SEN{}", self.counters.sensor)
            }
            ComponentKind::NetLabel => "NET1".to_string(),
            ComponentKind::Timer555 => {
                self.counters.logic_gate += 1;
                format!("U{}", self.counters.logic_gate + 200)
            }
            ComponentKind::Crystal => {
                self.counters.logic_gate += 1;
                format!("X{}", self.counters.logic_gate)
            }
            ComponentKind::Transformer => {
                self.counters.logic_gate += 1;
                format!("T{}", self.counters.logic_gate)
            }
            ComponentKind::Display7Seg => {
                self.counters.oled += 1;
                format!("DS{}", self.counters.oled)
            }
            ComponentKind::Thermistor => {
                self.counters.resistor += 1;
                format!("RT{}", self.counters.resistor)
            }
            ComponentKind::Varistor => {
                self.counters.resistor += 1;
                format!("RV{}", self.counters.resistor)
            }
            ComponentKind::VoltageRef => {
                self.counters.vreg += 1;
                format!("VR{}", self.counters.vreg)
            }
            ComponentKind::MotorDriver => {
                self.counters.motor += 1;
                format!("MD{}", self.counters.motor)
            }
            ComponentKind::SchottkyDiode => {
                self.counters.diode += 1;
                format!("DS{}", self.counters.diode)
            }
            ComponentKind::TvsDiode => {
                self.counters.diode += 1;
                format!("DT{}", self.counters.diode)
            }
            ComponentKind::Phototransistor => {
                self.counters.npn += 1;
                format!("QP{}", self.counters.npn)
            }
            ComponentKind::Optocoupler => {
                self.counters.logic_gate += 1;
                format!("OK{}", self.counters.logic_gate)
            }
            ComponentKind::GenericIc => {
                self.counters.logic_gate += 1;
                format!("IC{}", self.counters.logic_gate)
            }
            ComponentKind::Voltmeter => {
                self.counters.meter += 1;
                format!("VM{}", self.counters.meter)
            }
            ComponentKind::Ammeter => {
                self.counters.meter += 1;
                format!("AM{}", self.counters.meter)
            }
            ComponentKind::TextNote => "NOTE".to_string(),
            ComponentKind::Dht11 | ComponentKind::Dht22 => {
                self.counters.dht += 1;
                format!("DHT{}", self.counters.dht)
            }
            ComponentKind::HcSr04 => {
                self.counters.hcsr04 += 1;
                format!("US{}", self.counters.hcsr04)
            }
            ComponentKind::Buzzer => {
                self.counters.buzzer += 1;
                format!("BZ{}", self.counters.buzzer)
            }
            ComponentKind::NeoPixel => {
                self.counters.neopixel += 1;
                format!("NP{}", self.counters.neopixel)
            }
            ComponentKind::PirSensor => {
                self.counters.pir += 1;
                format!("PIR{}", self.counters.pir)
            }
            ComponentKind::Custom => self.next_custom_label(None),
        }
    }

    /// Next reference label for a custom part: the definition's `label_prefix`
    /// (default "U") plus one past the highest numeric suffix currently used
    /// with that prefix. Scanning existing labels (instead of a counter)
    /// self-heals after deletes and works for any user-chosen prefix.
    pub(crate) fn next_custom_label(&self, part_id: Option<&str>) -> String {
        let prefix = part_id
            .and_then(custom_part)
            .map(|def| def.label_prefix)
            .unwrap_or_else(|| "U".to_string());
        let highest = self
            .components
            .iter()
            .filter_map(|component| component.label.strip_prefix(&prefix))
            .filter_map(|suffix| suffix.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        format!("{prefix}{}", highest + 1)
    }

    pub(crate) fn default_value(kind: ComponentKind) -> String {
        match kind {
            ComponentKind::Resistor => "10k".to_string(),
            ComponentKind::Capacitor => "100nF".to_string(),
            ComponentKind::Inductor => "10uH".to_string(),
            ComponentKind::Diode => "1N4148".to_string(),
            ComponentKind::Led => "red".to_string(),
            ComponentKind::ZenerDiode => "5.1V".to_string(),
            ComponentKind::NpnTransistor => "2N2222".to_string(),
            ComponentKind::PnpTransistor => "2N2907".to_string(),
            ComponentKind::Nmosfet => "2N7000".to_string(),
            ComponentKind::Pmosfet => "IRF9540".to_string(),
            ComponentKind::Potentiometer => "10k".to_string(),
            ComponentKind::VoltageReg => "LM7805".to_string(),
            ComponentKind::Fuse => "500mA".to_string(),
            ComponentKind::LogicNot => "74HC04".to_string(),
            ComponentKind::LogicAnd => "74HC08".to_string(),
            ComponentKind::LogicOr => "74HC32".to_string(),
            ComponentKind::LogicNand => "74HC00".to_string(),
            ComponentKind::LogicNor => "74HC02".to_string(),
            ComponentKind::LogicXor => "74HC86".to_string(),
            ComponentKind::Switch => "closed".to_string(),
            ComponentKind::PushButton => "open".to_string(),
            ComponentKind::SlideSwitch => "closed".to_string(),
            ComponentKind::Ground => "0V".to_string(),
            ComponentKind::VSource => "5V".to_string(),
            ComponentKind::ISource => "10mA".to_string(),
            ComponentKind::Battery => "9V".to_string(),
            ComponentKind::OpAmp => "LM358".to_string(),
            ComponentKind::Lamp => "12V".to_string(),
            ComponentKind::Esp32 => "ESP32-WROOM".to_string(),
            ComponentKind::Esp32S3 => "ESP32-S3 DevKit".to_string(),
            ComponentKind::Esp32C3 => "ESP32-C3 Mini".to_string(),
            ComponentKind::ArduinoUno => "ATmega328P".to_string(),
            ComponentKind::RaspberryPiPico => "RP2040".to_string(),
            ComponentKind::Stm32BluePill => "STM32F103C8T6 Blue Pill".to_string(),
            ComponentKind::Stm32Nucleo64 => "STM32 Nucleo-64".to_string(),
            ComponentKind::Breadboard => "400 tie".to_string(),
            ComponentKind::Relay => "5V coil".to_string(),
            ComponentKind::DcMotor => "6V DC".to_string(),
            ComponentKind::Servo => "PWM servo".to_string(),
            ComponentKind::Oled => "0.96 I2C".to_string(),
            ComponentKind::Sensor => "I2C sensor".to_string(),
            ComponentKind::NetLabel => "VCC".to_string(),
            ComponentKind::Timer555 => "NE555".to_string(),
            ComponentKind::Crystal => "16MHz".to_string(),
            ComponentKind::Transformer => "1:1".to_string(),
            ComponentKind::Display7Seg => "common-cathode".to_string(),
            ComponentKind::Thermistor => "10k NTC".to_string(),
            ComponentKind::Varistor => "14D471".to_string(),
            ComponentKind::VoltageRef => "LM336-2.5".to_string(),
            ComponentKind::MotorDriver => "L298N".to_string(),
            ComponentKind::SchottkyDiode => "1N5819".to_string(),
            ComponentKind::TvsDiode => "P6KE12A".to_string(),
            ComponentKind::Phototransistor => "NPN opto".to_string(),
            ComponentKind::Optocoupler => "PC817".to_string(),
            ComponentKind::GenericIc => "IC".to_string(),
            ComponentKind::Voltmeter => "DC".to_string(),
            ComponentKind::Ammeter => "DC".to_string(),
            ComponentKind::TextNote => "Add your note here".to_string(),
            ComponentKind::Dht11 => "DHT11".to_string(),
            ComponentKind::Dht22 => "DHT22".to_string(),
            ComponentKind::HcSr04 => "HC-SR04".to_string(),
            ComponentKind::Buzzer => "5V active".to_string(),
            ComponentKind::NeoPixel => "WS2812B".to_string(),
            ComponentKind::PirSensor => "HC-SR501".to_string(),
            // Placement of custom parts goes through `place_custom_component`,
            // which fills the value from the part definition.
            ComponentKind::Custom => String::new(),
        }
    }

    // ── Circuit mutation ─────────────────────────────────────────────────────

    pub(crate) fn add_component(&mut self, kind: ComponentKind, pos: Pos2) {
        self.execute_editor_command(EditorCommand::Component(ComponentCommand::Place {
            kind,
            position: pos,
            value: Self::default_value(kind),
        }));
    }

    pub(crate) fn place_component(&mut self, kind: ComponentKind, pos: Pos2) -> u64 {
        let label = self.next_label(kind);
        let id = self.next_id();
        self.components.push(Component {
            id,
            kind,
            pos,
            rotation: 0,
            label,
            value: Self::default_value(kind),
            part_id: None,
        });
        if let Some(component) = self.document.components.last() {
            self.analysis
                .schematic_entity_index
                .add_component(component.id, self.document.components.len() - 1);
            self.analysis
                .schematic_spatial_index
                .update_component(component);
        }
        id
    }

    pub(crate) fn add_custom_component(&mut self, part_id: &str, pos: Pos2) {
        self.execute_editor_command(EditorCommand::Component(ComponentCommand::PlaceCustom {
            part_id: part_id.to_string(),
            position: pos,
            value: custom_part(part_id)
                .map(|definition| definition.default_value)
                .unwrap_or_default(),
        }));
    }

    /// Rescan `cluster_parts/` and report the outcome in the status bar.
    /// Reloading only adds or replaces definitions; parts already placed on
    /// the canvas pick up new pin layouts immediately because pin lookups go
    /// through the registry on every frame.
    pub(crate) fn reload_custom_parts(&mut self) {
        let dir = Path::new(CUSTOM_PARTS_DIR);
        let (loaded, notes) = load_custom_parts_dir(dir);
        self.status = if notes.is_empty() {
            format!("Loaded {loaded} custom part(s) from {CUSTOM_PARTS_DIR}/.")
        } else {
            format!(
                "Loaded {loaded} custom part(s); skipped: {}",
                notes.join(" | ")
            )
        };
        // Pin layouts may have changed; the document itself did not.
        self.invalidate_analysis_cache();
    }

    /// Write an example part file the user can copy and edit, then reload.
    /// Never overwrites an existing file.
    pub(crate) fn create_sample_custom_part(&mut self) {
        let dir = Path::new(CUSTOM_PARTS_DIR);
        if let Err(error) = fs::create_dir_all(dir) {
            self.status = format!("Cannot create {CUSTOM_PARTS_DIR}/: {error}");
            return;
        }
        let path = dir.join("sample-bme280.json");
        if path.exists() {
            self.status = format!("{} already exists. Edit it or copy it.", path.display());
            return;
        }
        if let Err(error) = fs::write(&path, sample_part_json()) {
            self.status = format!("Cannot write {}: {error}", path.display());
            return;
        }
        let (loaded, _) = load_custom_parts_dir(dir);
        self.status = format!(
            "Created {} and loaded {loaded} part(s). Edit the JSON to make your own.",
            path.display()
        );
        self.invalidate_analysis_cache();
    }

    #[allow(dead_code)] // Direct fixture construction; UI placement uses the command path.
    pub(crate) fn place_custom_component(&mut self, part_id: &str, pos: Pos2) -> u64 {
        let def = custom_part(part_id);
        let label = self.next_custom_label(Some(part_id));
        let id = self.next_id();
        self.components.push(Component {
            id,
            kind: ComponentKind::Custom,
            pos,
            rotation: 0,
            label,
            value: def.map(|def| def.default_value).unwrap_or_default(),
            part_id: Some(part_id.to_string()),
        });
        if let Some(component) = self.document.components.last() {
            self.analysis
                .schematic_entity_index
                .add_component(component.id, self.document.components.len() - 1);
            self.analysis
                .schematic_spatial_index
                .update_component(component);
        }
        id
    }

    pub(crate) fn place_note(&mut self, pos: Pos2, text: &str) -> u64 {
        let note = self.place_component(ComponentKind::TextNote, pos);
        if let Some(component) = self
            .components
            .iter_mut()
            .find(|component| component.id == note)
        {
            component.value = text.to_string();
        }
        note
    }

    pub(crate) fn apply_erc_auto_fix(&mut self, fix: ErcAutoFix) {
        let target_id = match fix {
            ErcAutoFix::AddLedSeriesResistor { component_id }
            | ErcAutoFix::AddI2cPullups { component_id }
            | ErcAutoFix::AddRelayFlybackDiode { component_id }
            | ErcAutoFix::AddGpioDriverNote { component_id }
            | ErcAutoFix::AddLevelShifterNote { component_id } => component_id,
        };
        let Some(target) = self
            .components
            .iter()
            .find(|component| component.id == target_id)
        else {
            self.status = "Auto fix target no longer exists.".to_string();
            return;
        };
        let base_pos = target.pos;
        self.begin_new_entities_history_transaction("Apply ERC auto fix");
        match fix {
            ErcAutoFix::AddLedSeriesResistor { .. } => {
                let id = self
                    .place_component(ComponentKind::Resistor, base_pos + Vec2::new(-120.0, -70.0));
                if let Some(component) = self
                    .components
                    .iter_mut()
                    .find(|component| component.id == id)
                {
                    component.value = "330 ohm".to_string();
                }
                self.editor.selected = Some(Selection::Component(id));
                self.status = "Auto fix placed a 330 ohm resistor. Wire it in series with the LED."
                    .to_string();
            }
            ErcAutoFix::AddI2cPullups { .. } => {
                let sda = self
                    .place_component(ComponentKind::Resistor, base_pos + Vec2::new(220.0, -90.0));
                let scl = self
                    .place_component(ComponentKind::Resistor, base_pos + Vec2::new(220.0, -40.0));
                for id in [sda, scl] {
                    if let Some(component) = self
                        .components
                        .iter_mut()
                        .find(|component| component.id == id)
                    {
                        component.value = "4.7k".to_string();
                    }
                }
                let wired = self.wire_i2c_pullup_fix(target_id, sda, scl);
                self.editor.selected = Some(Selection::Component(sda));
                self.status = if wired {
                    "Auto fix added and wired two 4.7k I2C pull-ups.".to_string()
                } else {
                    "Auto fix placed two 4.7k pull-ups. Wire them from SDA/SCL to logic VCC."
                        .to_string()
                };
            }
            ErcAutoFix::AddGpioDriverNote { .. } => {
                let id = self.place_note(
                    base_pos + Vec2::new(120.0, -100.0),
                    "Use a MOSFET/transistor driver and separate supply for this load.",
                );
                self.editor.selected = Some(Selection::Component(id));
                self.status = "Auto fix added a driver suggestion note.".to_string();
            }
            ErcAutoFix::AddRelayFlybackDiode { .. } => {
                let diode =
                    self.place_component(ComponentKind::Diode, base_pos + Vec2::new(-120.0, 0.0));
                self.add_wire_between(diode, "A", target_id, "COIL-");
                self.add_wire_between(diode, "B", target_id, "COIL+");
                self.editor.selected = Some(Selection::Component(diode));
                self.status = "Auto fix added a flyback diode across the relay coil.".to_string();
            }
            ErcAutoFix::AddLevelShifterNote { .. } => {
                let id = self.place_note(
                    base_pos + Vec2::new(120.0, -100.0),
                    "Add level shifter or resistor divider before this 3.3V GPIO.",
                );
                self.editor.selected = Some(Selection::Component(id));
                self.status = "Auto fix added a level-shifter suggestion note.".to_string();
            }
        }
        self.mark_dirty();
    }

    fn wire_i2c_pullup_fix(
        &mut self,
        target_id: u64,
        sda_resistor: u64,
        scl_resistor: u64,
    ) -> bool {
        let Some((bus_id, sda_pin, scl_pin, power_pin)) = self.i2c_pullup_anchor(target_id) else {
            return false;
        };
        self.add_wire_between(sda_resistor, "A", bus_id, sda_pin);
        self.add_wire_between(sda_resistor, "B", bus_id, power_pin);
        self.add_wire_between(scl_resistor, "A", bus_id, scl_pin);
        self.add_wire_between(scl_resistor, "B", bus_id, power_pin);
        true
    }

    fn i2c_pullup_anchor(
        &self,
        target_id: u64,
    ) -> Option<(u64, &'static str, &'static str, &'static str)> {
        let target = self
            .components
            .iter()
            .find(|component| component.id == target_id)?;
        if let Some(mapping) = i2c_mapping_for_kind(target.kind) {
            return Some((target.id, mapping.0, mapping.1, mapping.2));
        }
        self.components.iter().find_map(|component| {
            let mapping = i2c_mapping_for_kind(component.kind)?;
            Some((component.id, mapping.0, mapping.1, mapping.2))
        })
    }

    pub(crate) fn add_wire(&mut self, points: Vec<Pos2>) {
        self.execute_editor_command(EditorCommand::Wiring(WiringCommand::Add { points }));
    }

    pub(crate) fn same_net_wires(&mut self, wire_id: u64) -> HashSet<u64> {
        let connectivity = self.current_connectivity();
        let Some(net_id) = connectivity.netlist.wire_nets.get(&wire_id).copied() else {
            return HashSet::new();
        };
        connectivity
            .netlist
            .wire_nets
            .iter()
            .filter_map(|(&candidate, &candidate_net)| {
                (candidate_net == net_id).then_some(candidate)
            })
            .collect()
    }

    pub(crate) fn reset_canvas(&mut self) {
        self.backup_dirty_work("reset");
        self.execute_editor_command(EditorCommand::Document(DocumentCommand::Reset));
    }

    pub(crate) fn add_wire_between(&mut self, a_id: u64, a_pin: &str, b_id: u64, b_pin: &str) {
        let Some(a) = self.pin_pos(a_id, a_pin) else {
            return;
        };
        let Some(b) = self.pin_pos(b_id, b_pin) else {
            return;
        };
        let corner_h = Pos2::new(b.x, a.y);
        let corner_v = Pos2::new(a.x, b.y);
        let path_h = vec![a, corner_h, b];
        let path_v = vec![a, corner_v, b];
        let pin_obstacles = self
            .components
            .iter()
            .flat_map(component_pin_defs)
            .map(|pin| pin.pos)
            .filter(|pin| pin.distance(a) > 4.0 && pin.distance(b) > 4.0)
            .collect::<Vec<_>>();
        let corner = if crate::wire_path_pin_crossings(&path_h, &pin_obstacles)
            <= crate::wire_path_pin_crossings(&path_v, &pin_obstacles)
        {
            corner_h
        } else {
            corner_v
        };
        self.add_wire(vec![a, corner, b]);
    }

    pub(crate) fn select_breadboard_route(
        &mut self,
        netlist: &CircuitNetlist,
        route: BreadboardRoute,
    ) {
        self.hovered_net_wire = None;
        self.highlighted_net_wires = netlist
            .wire_nets
            .iter()
            .filter_map(|(wire_id, net_id)| (*net_id == route.net_id).then_some(*wire_id))
            .collect();
        self.editor.selected = Some(Selection::Component(route.from_component_id));
        self.status = format!(
            "Breadboard route: {} {} -> {} {}.",
            route.from_label, route.from_pin, route.to_label, route.to_pin
        );
    }

    pub(crate) fn connect_breadboard_route(&mut self, route: BreadboardRoute) {
        if route.connected {
            let netlist = self.current_netlist();
            self.select_breadboard_route(&netlist, route);
            return;
        }
        let from_pin = route.from_pin.clone();
        let to_pin = route.to_pin.clone();
        self.add_wire_between(
            route.from_component_id,
            &from_pin,
            route.to_component_id,
            &to_pin,
        );
        self.editor.selected = Some(Selection::Component(route.from_component_id));
        self.status = format!(
            "Added jumper: {} {} -> {} {}.",
            route.from_label, from_pin, route.to_label, to_pin
        );
    }

    pub(crate) fn update_pcb_from_schematic(&mut self) {
        let netlist = self.current_netlist();
        let cad = CadProjectData::from_schematic(&self.components, &netlist);
        let report = self.document.board.eco_report(&cad.symbols, &cad.nets);
        self.execute_editor_command(EditorCommand::Pcb(PcbCommand::ApplyEco {
            symbols: cad.symbols.clone(),
            nets: cad.nets.clone(),
            removed_policy: RemovedFootprintPolicy::KeepAsOrphan,
        }));
        self.refresh_pcb_analysis(&cad);
        self.analysis.pcb_cad = Some(cad);
        self.pcb_ui.last_sync_revision = self.analysis.circuit_revision;
        self.pcb_ui.selected_drc_index = None;
        self.analysis.dirty_flags.pcb_sync_dirty = false;

        let summary = self.pcb_dock_summary();
        self.status = format!(
            "PCB updated (ECO): +{} / {} orphaned, {} footprint(s), {} unrouted, {} DRC error(s).",
            report.added_symbols.len(),
            report.removed_footprints.len(),
            summary.footprint_count,
            summary.ratsnest_count,
            summary.drc_errors
        );
    }

    pub(crate) fn auto_place_pcb_footprints(&mut self) {
        self.ensure_pcb_synced();
        let board_width = self
            .document
            .board
            .outline
            .points
            .iter()
            .map(|point| point.x)
            .fold(80.0_f32, f32::max);
        let start_x = 8.0;
        let start_y = 8.0;
        let step_x = 14.0;
        let step_y = 10.0;
        let usable_width = (board_width - start_x * 2.0).max(step_x);
        let per_row = (usable_width / step_x).floor().max(1.0) as usize;
        let moves = self
            .document
            .board
            .footprints
            .iter()
            .enumerate()
            .map(|(index, footprint)| {
                let col = index % per_row;
                let row = index / per_row;
                (
                    footprint.id,
                    crate::model::cad::Point2::new(
                        start_x + col as f32 * step_x,
                        start_y + row as f32 * step_y,
                    ),
                )
            })
            .collect();
        self.execute_editor_command(EditorCommand::Pcb(PcbCommand::MoveFootprints(moves)));
        self.refresh_pcb_analysis_from_current();
        self.status = format!(
            "Auto-placed {} PCB footprint(s).",
            self.document.board.footprints.len()
        );
    }

    pub(crate) fn fit_pcb_board_to_contents(&mut self) {
        self.ensure_pcb_synced();
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        for footprint in &self.document.board.footprints {
            min_x = min_x.min(footprint.position.x);
            min_y = min_y.min(footprint.position.y);
            max_x = max_x.max(footprint.position.x);
            max_y = max_y.max(footprint.position.y);
        }
        for track in &self.document.board.tracks {
            for point in [track.start, track.end] {
                min_x = min_x.min(point.x);
                min_y = min_y.min(point.y);
                max_x = max_x.max(point.x);
                max_y = max_y.max(point.y);
            }
        }
        for via in &self.document.board.vias {
            min_x = min_x.min(via.position.x);
            min_y = min_y.min(via.position.y);
            max_x = max_x.max(via.position.x);
            max_y = max_y.max(via.position.y);
        }

        if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
            self.status = "No PCB geometry to fit.".to_string();
            return;
        }

        let margin = 6.0_f32;
        let shift_x = margin - min_x;
        let shift_y = margin - min_y;
        let width = (max_x - min_x + margin * 2.0).max(25.0);
        let height = (max_y - min_y + margin * 2.0).max(20.0);
        let footprint_positions = self
            .document
            .board
            .footprints
            .iter()
            .map(|footprint| {
                (
                    footprint.id,
                    Point2::new(
                        footprint.position.x + shift_x,
                        footprint.position.y + shift_y,
                    ),
                )
            })
            .collect();
        let tracks = self
            .document
            .board
            .tracks
            .iter()
            .cloned()
            .map(|mut track| {
                track.start.x += shift_x;
                track.start.y += shift_y;
                track.end.x += shift_x;
                track.end.y += shift_y;
                track
            })
            .collect();
        let vias = self
            .document
            .board
            .vias
            .iter()
            .cloned()
            .map(|mut via| {
                via.position.x += shift_x;
                via.position.y += shift_y;
                via
            })
            .collect();
        self.execute_editor_command(EditorCommand::Pcb(PcbCommand::SetGeometry {
            footprint_positions,
            tracks,
            vias,
            outline: BoardOutline::rectangular(width, height),
        }));
        self.refresh_pcb_analysis_from_current();
        self.status = format!("Fit PCB board to {:.1} x {:.1} mm.", width, height);
    }

    pub(crate) fn route_pcb_ratsnest(&mut self) {
        self.ensure_pcb_synced();
        let Some(cad) = self.analysis.pcb_cad.clone() else {
            self.status = "Update PCB before routing ratsnest.".to_string();
            return;
        };
        let footprint_by_id = self
            .document
            .board
            .footprints
            .iter()
            .map(|footprint| (footprint.id, footprint.position))
            .collect::<HashMap<_, _>>();
        let mut next_id = self
            .document
            .board
            .tracks
            .iter()
            .map(|track| track.id)
            .max()
            .unwrap_or(0)
            + 1;
        let mut tracks = Vec::new();
        for ratsnest in self.document.board.ratsnest_edges(&cad.nets) {
            if self
                .document
                .board
                .tracks
                .iter()
                .any(|track| track.net_id == ratsnest.net_id)
            {
                continue;
            }
            let (Some(start), Some(end)) = (
                footprint_by_id.get(&ratsnest.from_footprint_id).copied(),
                footprint_by_id.get(&ratsnest.to_footprint_id).copied(),
            ) else {
                continue;
            };
            tracks.push(TrackSegment {
                id: next_id,
                net_id: ratsnest.net_id,
                layer: BoardLayer::FrontCopper,
                start,
                end,
                width_mm: self
                    .document
                    .board
                    .design_rules
                    .min_track_width_mm
                    .max(0.25),
            });
            next_id += 1;
        }
        let added = tracks.len();
        if !tracks.is_empty() {
            self.execute_editor_command(EditorCommand::Pcb(PcbCommand::AddRoute {
                tracks,
                vias: Vec::new(),
            }));
        }
        self.refresh_pcb_analysis(&cad);
        self.analysis.pcb_cad = Some(cad);
        self.status = if added == 0 {
            "No unrouted ratsnest edges needed a new track.".to_string()
        } else {
            format!("Added {added} straight PCB track(s) from ratsnest.")
        };
    }

    pub(crate) fn export_pcb_fabrication_files(&mut self) {
        self.ensure_pcb_synced();
        let drc_errors = self
            .analysis
            .pcb_drc
            .iter()
            .filter(|violation| violation.severity == DrcSeverity::Error)
            .count();
        if drc_errors > 0 {
            self.status = format!("PCB export blocked: fix {drc_errors} DRC error(s) first.");
            return;
        }
        let Some(mut cad) = self.analysis.pcb_cad.clone() else {
            self.status = "Update PCB before fabrication export.".to_string();
            return;
        };
        cad.board = Some(self.document.board.clone());
        let writes = [
            (
                "cluster_pcb_F_Cu.gbr",
                crate::export::gerber::gerber_for_layer(
                    &self.document.board,
                    BoardLayer::FrontCopper,
                ),
            ),
            (
                "cluster_pcb_B_Cu.gbr",
                crate::export::gerber::gerber_for_layer(
                    &self.document.board,
                    BoardLayer::BackCopper,
                ),
            ),
            (
                "cluster_pcb_Edge_Cuts.gbr",
                crate::export::gerber::gerber_for_layer(&self.document.board, BoardLayer::EdgeCuts),
            ),
            (
                "cluster_pcb.drl",
                crate::export::gerber::excellon_drill(&self.document.board),
            ),
            ("cluster_pcb_bom.csv", crate::export::gerber::bom_csv(&cad)),
            ("cluster_pcb_cpl.csv", crate::export::gerber::cpl_csv(&cad)),
        ];
        for (path, contents) in writes {
            if let Err(error) = fs::write(path, contents) {
                self.status = format!("PCB export failed at {path}: {error}");
                return;
            }
        }
        self.status =
            "Exported PCB Gerber, drill, BOM, and CPL files in the project folder.".to_string();
    }

    pub(crate) fn save_project_folder(&mut self) {
        match self.save_project_folder_to("project.cluster") {
            Ok(()) => {
                self.editor.history.dirty = false;
                self.last_autorecover_revision = self.analysis.revisions.persistence;
                self.status =
                    "Saved project.cluster with schematic, PCB, and CAD data.".to_string();
            }
            Err(error) => {
                self.status = format!("Project save failed: {error}");
            }
        }
    }

    pub(crate) fn load_project_folder(&mut self) {
        match self.load_project_folder_from("project.cluster") {
            Ok(()) => {
                self.status = "Loaded project.cluster schematic and PCB data.".to_string();
            }
            Err(error) => {
                self.status = format!("Project load failed: {error}");
            }
        }
    }

    pub(crate) fn save_project_folder_to(&mut self, root: impl AsRef<Path>) -> Result<(), String> {
        self.save_current_page();
        self.ensure_pcb_synced();
        let layout = ProjectFolderLayout::new(root.as_ref());
        layout.create_dirs()?;

        let mut cad = if let Some(cad) = self.analysis.pcb_cad.clone() {
            cad
        } else {
            let netlist = self.current_netlist();
            CadProjectData::from_schematic(&self.components, &netlist)
        };
        cad.board = Some(self.document.board.clone());
        cad.properties.insert(
            "document_revision".to_string(),
            self.analysis.circuit_revision.to_string(),
        );

        let schematic_json = serde_json::to_string_pretty(&SavedCircuit::from_app(self))
            .map_err(|error| error.to_string())?;
        let board_json = serde_json::to_string_pretty(&self.document.board)
            .map_err(|error| error.to_string())?;
        let project_json = serde_json::to_string_pretty(&cad).map_err(|error| error.to_string())?;

        write_with_backup_path(&layout.schematic_json, &schematic_json)?;
        write_with_backup_path(&layout.board_json, &board_json)?;
        write_with_backup_path(&layout.project_json, &project_json)?;
        Ok(())
    }

    pub(crate) fn load_project_folder_from(
        &mut self,
        root: impl AsRef<Path>,
    ) -> Result<(), String> {
        self.backup_dirty_work("project load");
        let layout = ProjectFolderLayout::new(root.as_ref());
        let schematic_json = fs::read_to_string(&layout.schematic_json)
            .map_err(|error| format!("Read {}: {error}", layout.schematic_json.display()))?;
        let board_json = fs::read_to_string(&layout.board_json)
            .map_err(|error| format!("Read {}: {error}", layout.board_json.display()))?;
        let project_json = fs::read_to_string(&layout.project_json)
            .map_err(|error| format!("Read {}: {error}", layout.project_json.display()))?;

        let saved = serde_json::from_str::<SavedCircuit>(&schematic_json)
            .map_err(|error| format!("Parse schematic.json: {error}"))?;
        let (snapshot, load_notes) = saved.into_snapshot()?;
        let mut board = serde_json::from_str::<Board>(&board_json)
            .map_err(|error| format!("Parse board.json: {error}"))?;
        board.rebuild_entity_index();
        let mut cad = serde_json::from_str::<CadProjectData>(&project_json)
            .map_err(|error| format!("Parse project.json: {error}"))?;
        cad.board = Some(board.clone());

        self.begin_snapshot_history_transaction("Load project folder");
        self.restore_snapshot(snapshot);
        self.document.board = board;
        let document_invariant_warnings = crate::model::validate_document_invariants(
            &self.document,
            &self.analysis.schematic_entity_index,
        )
        .err()
        .map_or(0, |violations| violations.len());
        let schematic_derived_warnings = usize::from(
            !self
                .analysis
                .attachment_index
                .is_consistent(&self.document.components, &self.document.wires),
        ) + usize::from(
            !self
                .analysis
                .schematic_spatial_index
                .is_consistent(&self.document.components, &self.document.wires),
        );
        let board_invariant_warnings = self
            .document
            .board
            .validate_invariants()
            .err()
            .map_or(0, |violations| violations.len());
        self.finish_history_transaction();
        self.dispatch_changes(crate::commands::ChangeSet::restored_document());
        self.analysis.pcb_cad = Some(cad.clone());
        self.pcb_ui.last_sync_revision = self.analysis.circuit_revision;
        self.pcb_ui.selected_drc_index = None;
        self.refresh_pcb_analysis(&cad);
        self.editor.history.dirty = false;
        self.last_autorecover_revision = self.analysis.revisions.persistence;
        self.analysis.dirty_flags.geometry_dirty = false;
        self.analysis.dirty_flags.connectivity_dirty = false;
        self.analysis.dirty_flags.validation_dirty = false;
        self.analysis.dirty_flags.simulation_dirty = true;
        self.analysis.dirty_flags.pcb_sync_dirty = false;
        self.pending_fit = true;
        let invariant_warnings =
            document_invariant_warnings + schematic_derived_warnings + board_invariant_warnings;
        if !load_notes.is_empty() || invariant_warnings > 0 {
            self.status = format!(
                "Loaded project with {} repair note(s) and {} invariant warning(s).",
                load_notes.len(),
                invariant_warnings,
            );
        } else {
            self.status = "Loaded project.".to_string();
        }
        Ok(())
    }

    pub(crate) fn select_pcb_drc_violation(&mut self, index: usize) {
        self.ensure_pcb_synced();
        let Some(violation) = self.analysis.pcb_drc.get(index).cloned() else {
            self.status = format!("PCB DRC item {index} is no longer available.");
            self.pcb_ui.selected_drc_index = None;
            return;
        };
        self.pcb_ui.selected_drc_index = Some(index);
        if let Some(object_id) = violation.object_id
            && let Some(component_id) = self
                .document
                .board
                .footprints
                .iter()
                .find(|footprint| footprint.id == object_id)
                .and_then(|footprint| footprint.symbol_instance_id)
        {
            self.editor.selected = Some(Selection::Component(component_id));
            if let Some(component) = self
                .components
                .iter()
                .find(|component| component.id == component_id)
            {
                self.pan =
                    self.canvas.rect.center().to_vec2() - component.pos.to_vec2() * self.zoom;
            }
            self.status = format!("Selected PCB DRC: {}.", violation.title);
            return;
        }
        self.status = if let Some(location) = violation.location {
            format!(
                "Selected PCB DRC: {} at {:.1},{:.1} mm.",
                violation.title, location.x, location.y
            )
        } else {
            format!("Selected PCB DRC: {}.", violation.title)
        };
    }

    fn ensure_pcb_synced(&mut self) {
        if self.analysis.pcb_cad.is_none()
            || self.pcb_ui.last_sync_revision != self.analysis.circuit_revision
            || self.analysis.dirty_flags.pcb_sync_dirty
        {
            self.update_pcb_from_schematic();
        }
    }

    fn refresh_pcb_analysis_from_current(&mut self) {
        if let Some(cad) = self.analysis.pcb_cad.clone() {
            self.refresh_pcb_analysis(&cad);
            self.analysis.pcb_cad = Some(cad);
        }
    }

    pub(crate) fn refresh_pcb_analysis(&mut self, cad: &CadProjectData) {
        self.rebuild_pcb_ratsnest_counts(cad);
        self.analysis.pcb_drc = run_drc_with_nets(&self.document.board, &cad.nets);
        self.analysis.dirty_flags.pcb_drc_dirty = false;
        if self
            .pcb_ui
            .selected_drc_index
            .is_some_and(|index| index >= self.analysis.pcb_drc.len())
        {
            self.pcb_ui.selected_drc_index = None;
        }
    }

    pub(crate) fn schedule_full_pcb_analysis(&mut self, cad: &CadProjectData) {
        let revision = self.analysis.revisions.persistence;
        if self.analysis.pending_full_drc_revision == Some(revision) {
            return;
        }
        self.rebuild_pcb_ratsnest_counts(cad);
        let request = crate::engine::worker::AnalysisRequest::FullDrc {
            board: Box::new(self.document.board.clone()),
            nets: cad.nets.clone(),
        };
        if self.analysis.worker.submit(revision, request).is_ok() {
            self.analysis.pending_full_drc_revision = Some(revision);
        }
    }

    pub(crate) fn refresh_local_pcb_analysis(
        &mut self,
        affected_track_ids: &std::collections::HashSet<u64>,
        affected_net_ids: &std::collections::HashSet<usize>,
    ) {
        let Some(cad) = self.analysis.pcb_cad.clone() else {
            return;
        };
        self.analysis.pcb_drc.retain(|violation| {
            !violation
                .object_id
                .is_some_and(|id| affected_track_ids.contains(&id))
        });
        self.analysis.pcb_drc.extend(crate::pcb::drc::run_local_drc(
            &self.document.board,
            affected_track_ids,
        ));
        let edges = self.document.board.ratsnest_edges(&cad.nets);
        for net_id in affected_net_ids {
            let routed = self
                .document
                .board
                .tracks
                .iter()
                .any(|track| track.net_id == *net_id)
                || self
                    .document
                    .board
                    .vias
                    .iter()
                    .any(|via| via.net_id == *net_id);
            let count = if routed {
                0
            } else {
                edges.iter().filter(|edge| edge.net_id == *net_id).count()
            };
            self.analysis.pcb_ratsnest_by_net.insert(*net_id, count);
        }
        self.pcb_ui.ratsnest_count = self.analysis.pcb_ratsnest_by_net.values().sum();
        self.analysis.dirty_flags.pcb_drc_dirty = false;
    }

    fn rebuild_pcb_ratsnest_counts(&mut self, cad: &CadProjectData) {
        self.analysis.pcb_ratsnest_by_net.clear();
        for edge in self.unrouted_pcb_ratsnest(cad) {
            *self
                .analysis
                .pcb_ratsnest_by_net
                .entry(edge.net_id)
                .or_default() += 1;
        }
        self.pcb_ui.ratsnest_count = self.analysis.pcb_ratsnest_by_net.values().sum();
    }

    fn unrouted_pcb_ratsnest(&self, cad: &CadProjectData) -> Vec<crate::pcb::board::RatsnestEdge> {
        self.document
            .board
            .ratsnest_edges(&cad.nets)
            .into_iter()
            .filter(|edge| {
                !self
                    .document
                    .board
                    .tracks
                    .iter()
                    .any(|track| track.net_id == edge.net_id)
                    && !self
                        .document
                        .board
                        .vias
                        .iter()
                        .any(|via| via.net_id == edge.net_id)
            })
            .collect()
    }

    pub(crate) fn pcb_dock_summary(&self) -> PcbDockSummary {
        let drc_errors = self
            .analysis
            .pcb_drc
            .iter()
            .filter(|violation| violation.severity == DrcSeverity::Error)
            .count();
        let drc_warnings = self
            .analysis
            .pcb_drc
            .iter()
            .filter(|violation| violation.severity == DrcSeverity::Warning)
            .count();
        PcbDockSummary {
            footprint_count: self.document.board.footprints.len(),
            unplaced_count: self
                .document
                .board
                .footprints
                .iter()
                .filter(|footprint| !footprint.placed)
                .count(),
            ratsnest_count: self.pcb_ui.ratsnest_count,
            drc_errors,
            drc_warnings,
            dirty: self.analysis.dirty_flags.pcb_sync_dirty
                || self.pcb_ui.last_sync_revision != self.analysis.circuit_revision,
            footprints: self
                .document
                .board
                .footprints
                .iter()
                .map(|footprint| {
                    let placed = if footprint.placed {
                        "placed"
                    } else {
                        "unplaced"
                    };
                    format!(
                        "{} {} ({:.1},{:.1})",
                        footprint.reference, placed, footprint.position.x, footprint.position.y
                    )
                })
                .collect(),
            ratsnest: self
                .analysis
                .pcb_cad
                .as_ref()
                .map(|cad| {
                    self.unrouted_pcb_ratsnest(cad)
                        .into_iter()
                        .map(|edge| {
                            format!(
                                "N{}: F{} -> F{}",
                                edge.net_id, edge.from_footprint_id, edge.to_footprint_id
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            drc: self
                .analysis
                .pcb_drc
                .iter()
                .enumerate()
                .map(|(index, violation)| PcbDrcRow {
                    index,
                    severity: pcb_drc_severity(violation.severity),
                    title: violation.title.clone(),
                    selected: self.pcb_ui.selected_drc_index == Some(index),
                })
                .collect(),
            preview: self.pcb_preview_data(),
        }
    }

    fn pcb_preview_data(&self) -> PcbPreviewData {
        let (width_mm, height_mm) = pcb_outline_size(&self.document.board.outline);
        let footprint_positions = self
            .document
            .board
            .footprints
            .iter()
            .map(|footprint| (footprint.id, footprint.position))
            .collect::<HashMap<_, _>>();
        let ratsnest = self
            .analysis
            .pcb_cad
            .as_ref()
            .map(|cad| {
                self.unrouted_pcb_ratsnest(cad)
                    .into_iter()
                    .filter_map(|edge| {
                        let start = footprint_positions.get(&edge.from_footprint_id)?;
                        let end = footprint_positions.get(&edge.to_footprint_id)?;
                        Some(PcbPreviewRatsnest {
                            start_x_mm: start.x,
                            start_y_mm: start.y,
                            end_x_mm: end.x,
                            end_y_mm: end.y,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        PcbPreviewData {
            width_mm,
            height_mm,
            footprints: self
                .document
                .board
                .footprints
                .iter()
                .map(|footprint| PcbPreviewFootprint {
                    reference: footprint.reference.clone(),
                    x_mm: footprint.position.x,
                    y_mm: footprint.position.y,
                    placed: footprint.placed,
                })
                .collect(),
            tracks: self
                .document
                .board
                .tracks
                .iter()
                .map(|track| PcbPreviewTrack {
                    start_x_mm: track.start.x,
                    start_y_mm: track.start.y,
                    end_x_mm: track.end.x,
                    end_y_mm: track.end.y,
                    front_layer: track.layer == BoardLayer::FrontCopper,
                })
                .collect(),
            ratsnest,
            diagnostics: self
                .analysis
                .pcb_drc
                .iter()
                .enumerate()
                .filter_map(|(index, violation)| {
                    let point = self.pcb_drc_point(violation)?;
                    Some(PcbPreviewDiagnostic {
                        x_mm: point.x,
                        y_mm: point.y,
                        severity: pcb_drc_severity(violation.severity),
                        selected: self.pcb_ui.selected_drc_index == Some(index),
                    })
                })
                .collect(),
        }
    }

    fn pcb_drc_point(&self, violation: &crate::pcb::drc::DrcViolation) -> Option<Point2> {
        if let Some(location) = violation.location {
            return Some(location);
        }
        let object_id = violation.object_id?;
        if let Some(footprint) = self
            .document
            .board
            .footprints
            .iter()
            .find(|footprint| footprint.id == object_id)
        {
            return Some(footprint.position);
        }
        if let Some(track) = self
            .document
            .board
            .tracks
            .iter()
            .find(|track| track.id == object_id)
        {
            return Some(Point2::new(
                (track.start.x + track.end.x) * 0.5,
                (track.start.y + track.end.y) * 0.5,
            ));
        }
        self.document
            .board
            .vias
            .iter()
            .find(|via| via.id == object_id)
            .map(|via| via.position)
    }

    pub(crate) fn pin_pos(&self, component_id: u64, label: &str) -> Option<Pos2> {
        let component = self
            .components
            .iter()
            .find(|component| component.id == component_id)?;
        component_pin_defs(component)
            .into_iter()
            .find(|pin| pin.label == label || pin.label.contains(label))
            .map(|pin| pin.pos)
    }

    pub(crate) fn push_wire_point(&mut self, pos: Pos2) {
        if self.orthogonal_wires
            && let Some(&last) = self.editor.draft_wire.last()
        {
            let dx = (pos.x - last.x).abs();
            let dy = (pos.y - last.y).abs();
            if dx > 0.1 && dy > 0.1 {
                let corner = if dx >= dy {
                    Pos2::new(pos.x, last.y)
                } else {
                    Pos2::new(last.x, pos.y)
                };
                crate::push_unique_point(&mut self.editor.draft_wire, corner);
            }
        }
        crate::push_unique_point(&mut self.editor.draft_wire, pos);
    }

    // ── Selection actions ────────────────────────────────────────────────────

    pub(crate) fn delete_selected(&mut self) {
        self.execute_editor_command(EditorCommand::Selection(SelectionCommand::Delete));
    }

    pub(crate) fn rotate_selected(&mut self) {
        self.execute_editor_command(EditorCommand::Selection(SelectionCommand::Rotate));
    }

    pub(crate) fn duplicate_selected(&mut self) {
        self.execute_editor_command(EditorCommand::Selection(SelectionCommand::Duplicate));
    }

    // ── Export ───────────────────────────────────────────────────────────────

    pub(crate) fn export_svg(&mut self) {
        match fs::write(
            "cluster_circuit.svg",
            crate::circuit_to_svg(&self.components, &self.wires),
        ) {
            Ok(()) => {
                self.status = "Saved cluster_circuit.svg.".to_string();
            }
            Err(err) => {
                self.status = format!("Export failed: {err}");
            }
        }
    }

    pub(crate) fn export_png(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        self.screenshot_pending = true;
        self.status = "Capturing screenshot...".to_string();
    }

    pub(crate) fn export_spice_netlist(&mut self) {
        let netlist = self.current_netlist();
        let output =
            crate::export::spice::export_spice_netlist_with_netlist(&self.components, &netlist);
        match fs::write("cluster_circuit.cir", output) {
            Ok(()) => {
                self.status = "Saved cluster_circuit.cir.".to_string();
            }
            Err(err) => {
                self.status = format!("SPICE export failed: {err}");
            }
        }
    }

    pub(crate) fn export_netlist_text(&mut self) {
        let netlist = self.current_netlist();
        match fs::write(
            "cluster_netlist.txt",
            crate::circuit_to_netlist_text(&netlist),
        ) {
            Ok(()) => {
                self.status = format!("Saved cluster_netlist.txt ({} nets).", netlist.nets.len())
            }
            Err(err) => self.status = format!("Netlist export failed: {err}"),
        }
    }

    pub(crate) fn export_arduino_code(&mut self) {
        let netlist = self.current_netlist();
        match fs::write(
            "cluster_arduino.ino",
            crate::generate_arduino_code(&netlist),
        ) {
            Ok(()) => self.status = "Saved cluster_arduino.ino.".to_string(),
            Err(err) => self.status = format!("Code export failed: {err}"),
        }
    }

    pub(crate) fn save_circuit_json(&mut self) {
        self.save_current_page();
        let path = crate::ui::app::default_save_path();
        match self.write_circuit_json(&path) {
            Ok(()) => {
                self.editor.history.dirty = false;
                self.last_autorecover_revision = self.analysis.revisions.persistence;
                self.status = format!("Saved {}.", path.display());
            }
            Err(err) => {
                self.status = format!("Save failed: {err}");
            }
        }
    }

    pub(crate) fn export_bom_csv(&mut self) {
        let pages = self.effective_pages();
        let csv = crate::circuit_to_bom_csv(&pages);
        let part_count = pages
            .iter()
            .flat_map(|(_, components, _, _, _)| components)
            .filter(|component| component.kind != ComponentKind::Ground)
            .count();
        match fs::write("cluster_bom.csv", csv) {
            Ok(()) => self.status = format!("Saved cluster_bom.csv ({part_count} parts)."),
            Err(e) => self.status = format!("BOM export failed: {e}"),
        }
    }

    // ── Align & distribute ───────────────────────────────────────────────────

    pub(crate) fn align_selected(&mut self, dir: AlignDir) {
        self.execute_editor_command(EditorCommand::Selection(SelectionCommand::Align(dir)));
    }

    pub(crate) fn distribute_selected(&mut self, vertical: bool) {
        self.execute_editor_command(EditorCommand::Selection(SelectionCommand::Distribute {
            vertical,
        }));
    }

    // ── Multi-page management ────────────────────────────────────────────────

    pub(crate) fn save_current_page(&mut self) {
        let current_page = self.document.current_page;
        let components = self.document.components.clone();
        let wires = self.document.wires.clone();
        let next_id = self.document.next_id;
        let counters = self.document.counters.clone();
        let annotations = self.document.annotations.clone();
        if let Some(page) = self.document.pages.get_mut(current_page) {
            page.components = components;
            page.wires = wires;
            page.next_id = next_id;
            page.counters = counters;
            page.annotations = annotations;
        }
    }

    pub(crate) fn load_page_state(&mut self, idx: usize) {
        let page = self.document.pages[idx].clone();
        self.document.components = page.components;
        self.document.wires = page.wires;
        self.document.next_id = page.next_id;
        self.document.counters = page.counters;
        self.document.annotations = page.annotations;
        self.editor.selected = None;
        self.editor.multi_selected.clear();
        self.editor.draft_wire.clear();
        self.editor.drag = None;
        self.editor.rect_select_start = None;
        self.hovered_net_wire = None;
        self.highlighted_net_wires.clear();
        self.editor.snap_target = None;
        self.invalidate_analysis_cache();
    }

    pub(crate) fn switch_page(&mut self, idx: usize) {
        if idx == self.current_page || idx >= self.pages.len() {
            return;
        }
        self.save_current_page();
        self.current_page = idx;
        self.load_page_state(idx);
        self.status = format!("Switched to {}", self.pages[idx].name);
    }

    pub(crate) fn add_page(&mut self) {
        self.save_current_page();
        self.begin_document_history_transaction("Add page");
        let n = self.pages.len() + 1;
        self.pages.push(ProjectPage::empty(format!("Page {n}")));
        let new_idx = self.pages.len() - 1;
        self.current_page = new_idx;
        self.load_page_state(new_idx);
        self.mark_dirty();
        self.status = format!("Added Page {n}.");
    }

    pub(crate) fn remove_current_page(&mut self) {
        if self.pages.len() <= 1 {
            self.status = "Cannot remove the only page.".to_string();
            return;
        }
        self.save_current_page();
        self.begin_document_history_transaction("Remove page");
        let current_page = self.document.current_page;
        self.document.pages.remove(current_page);
        let new_idx = self.current_page.saturating_sub(1);
        self.current_page = new_idx;
        self.load_page_state(new_idx);
        self.mark_dirty();
        self.status = "Page removed.".to_string();
    }

    // ── View ─────────────────────────────────────────────────────────────────

    pub(crate) fn zoom_to_fit(&mut self) {
        self.zoom_to_fit_silent();
        self.status = "Zoomed to fit.".to_string();
    }

    pub(crate) fn zoom_to_fit_silent(&mut self) {
        let Some(bounds) = crate::circuit_bounds(&self.components, &self.wires) else {
            return;
        };
        let canvas = self.canvas.rect;
        if canvas.width() < 1.0 || canvas.height() < 1.0 {
            return;
        }
        let margin = 80.0;
        let zoom_x = canvas.width() / (bounds.width() + margin * 2.0);
        let zoom_y = canvas.height() / (bounds.height() + margin * 2.0);
        self.zoom = zoom_x.min(zoom_y).clamp(0.2, 5.0);
        let origin = canvas.min;
        let world_center = bounds.center();
        let canvas_center = canvas.center();
        self.pan = (canvas_center - origin) - (world_center - origin) * self.zoom;
    }

    // ── Persistence ──────────────────────────────────────────────────────────

    pub(crate) fn load_circuit_json(&mut self) {
        self.load_circuit_json_from(&crate::ui::app::default_save_path(), false);
    }

    pub(crate) fn recover_autosave(&mut self) {
        self.load_circuit_json_from(&crate::ui::app::autorecover_path(self.document_id), true);
    }

    pub(crate) fn write_circuit_json(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Create {}: {error}", parent.display()))?;
        }
        let saved = SavedCircuit::from_app(self);
        let json = serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?;
        let path_text = path
            .to_str()
            .ok_or_else(|| format!("Path is not valid UTF-8: {}", path.display()))?;
        write_with_backup(path_text, &json)
    }

    pub(crate) fn backup_dirty_work(&mut self, reason: &str) {
        if !self.editor.history.dirty {
            return;
        }
        self.save_current_page();
        match self.write_circuit_json(&crate::ui::app::autorecover_path(self.document_id)) {
            Ok(()) => {
                self.status = format!("Auto-saved recovery before {reason}.");
            }
            Err(err) => {
                self.status = format!("Recovery save failed before {reason}: {err}");
            }
        }
    }

    pub(crate) fn load_circuit_json_from(&mut self, path: &std::path::Path, recovery: bool) {
        if !recovery {
            self.backup_dirty_work("load");
        }
        match std::fs::read_to_string(path)
            .map_err(|err| err.to_string())
            .and_then(|json| {
                serde_json::from_str::<SavedCircuit>(&json).map_err(|err| err.to_string())
            })
            .and_then(SavedCircuit::into_snapshot)
        {
            Ok((snapshot, load_notes)) => {
                self.begin_snapshot_history_transaction("Load circuit");
                self.restore_snapshot(snapshot);
                let invariant_warnings = crate::model::validate_document_invariants(
                    &self.document,
                    &self.analysis.schematic_entity_index,
                )
                .err()
                .map_or(0, |violations| violations.len());
                let invariant_warnings = invariant_warnings
                    + usize::from(
                        !self
                            .analysis
                            .attachment_index
                            .is_consistent(&self.document.components, &self.document.wires),
                    )
                    + usize::from(
                        !self
                            .analysis
                            .schematic_spatial_index
                            .is_consistent(&self.document.components, &self.document.wires),
                    );
                self.finish_history_transaction();
                self.dispatch_changes(crate::commands::ChangeSet::restored_document());
                if recovery {
                    self.editor.history.dirty = true;
                } else {
                    self.editor.history.dirty = false;
                    self.last_autorecover_revision = self.analysis.revisions.persistence;
                }
                self.status = if load_notes.is_empty() && invariant_warnings == 0 {
                    format!("Loaded {}.", path.display())
                } else {
                    format!(
                        "Loaded {} with {} repair(s) and {} invariant warning(s).",
                        path.display(),
                        load_notes.len(),
                        invariant_warnings,
                    )
                };
                self.pending_fit = true;
            }
            Err(err) => {
                self.status = format!("Load {} failed: {err}", path.display());
            }
        }
    }

    // ── Cache accessors ──────────────────────────────────────────────────────

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn current_simulation(
        &mut self,
    ) -> std::sync::Arc<crate::engine::simulation::Simulation> {
        if !self.simulate {
            self.simulation_run_state = crate::ui::app::SimulationRunState::Stopped;
            return std::sync::Arc::new(crate::engine::simulation::Simulation::default());
        }
        let ac_key = self.simulation_ui.ac_freq_hz.to_bits()
            ^ match self.simulation_ui.backend {
                crate::engine::backend::BackendKind::InternalMna => 0,
                crate::engine::backend::BackendKind::NgSpice => 0x8000_0000,
            };
        let revision = crate::ui::app::SimulationRevisionKey {
            connectivity: self.analysis.revisions.schematic_connectivity,
            topology: self.analysis.revisions.simulation_topology,
            parameters: self.analysis.revisions.simulation_parameters,
            electrical: self.analysis.revisions.electrical_parameters,
        };
        if let Some((cached_revision, cached_ac_key, simulation)) = &self.analysis.cached_simulation
            && *cached_revision == revision
            && *cached_ac_key == ac_key
        {
            self.performance.simulation_cache_hit = true;
            self.performance.simulation_cache_hits =
                self.performance.simulation_cache_hits.saturating_add(1);
            self.simulation_run_state = match simulation.status {
                crate::engine::simulation::SimulationStatus::Ok => {
                    crate::ui::app::SimulationRunState::Valid
                }
                crate::engine::simulation::SimulationStatus::Warning => {
                    crate::ui::app::SimulationRunState::Warning
                }
                crate::engine::simulation::SimulationStatus::Failed => {
                    crate::ui::app::SimulationRunState::Failed
                }
            };
            return std::sync::Arc::clone(simulation);
        }
        self.performance.simulation_cache_hit = false;
        self.performance.simulation_cache_misses =
            self.performance.simulation_cache_misses.saturating_add(1);
        self.simulation_run_state = crate::ui::app::SimulationRunState::Solving;
        let connectivity = self.current_connectivity();
        let mna_started = std::time::Instant::now();
        let mut simulation = simulation_engine::analyze_circuit_with_connectivity(
            &self.components,
            &self.wires,
            &connectivity,
        );
        simulation.ac = mna::solve_ac_with_connectivity(
            &self.components,
            &self.wires,
            self.simulation_ui.ac_freq_hz as f64,
            &connectivity,
        );
        simulation.transient = crate::engine::transient::solve_transient_with_netlist(
            &self.components,
            &connectivity.netlist,
        );
        self.performance.mna_ms = mna_started.elapsed().as_secs_f64() * 1_000.0;
        let erc_started = std::time::Instant::now();
        simulation.erc = crate::run_erc_with_netlist(
            &self.components,
            &self.wires,
            &simulation,
            &connectivity.netlist,
        );
        self.performance.erc_ms = erc_started.elapsed().as_secs_f64() * 1_000.0;
        let simulation = std::sync::Arc::new(simulation);
        self.analysis.cached_simulation =
            Some((revision, ac_key, std::sync::Arc::clone(&simulation)));
        self.analysis.simulation_revision = self.analysis.simulation_revision.saturating_add(1);
        self.analysis.dirty_flags.validation_dirty = false;
        self.analysis.dirty_flags.simulation_dirty = false;
        self.simulation_run_state = match simulation.status {
            crate::engine::simulation::SimulationStatus::Ok => {
                crate::ui::app::SimulationRunState::Valid
            }
            crate::engine::simulation::SimulationStatus::Warning => {
                crate::ui::app::SimulationRunState::Warning
            }
            crate::engine::simulation::SimulationStatus::Failed => {
                crate::ui::app::SimulationRunState::Failed
            }
        };
        simulation
    }

    /// Returns the newest completed simulation to the renderer and schedules
    /// stale work on the bounded worker. Direct callers such as exports and
    /// tests continue to use `current_simulation`, which is synchronous by
    /// design.
    pub(crate) fn simulation_for_frame(
        &mut self,
    ) -> std::sync::Arc<crate::engine::simulation::Simulation> {
        if !self.simulate {
            self.simulation_run_state = crate::ui::app::SimulationRunState::Stopped;
            self.analysis.pending_schematic = None;
            return std::sync::Arc::new(crate::engine::simulation::Simulation::default());
        }
        let ac_key = self.simulation_ui.ac_freq_hz.to_bits()
            ^ match self.simulation_ui.backend {
                crate::engine::backend::BackendKind::InternalMna => 0,
                crate::engine::backend::BackendKind::NgSpice => 0x8000_0000,
            };
        let revision_key = crate::ui::app::SimulationRevisionKey {
            connectivity: self.analysis.revisions.schematic_connectivity,
            topology: self.analysis.revisions.simulation_topology,
            parameters: self.analysis.revisions.simulation_parameters,
            electrical: self.analysis.revisions.electrical_parameters,
        };
        if let Some((cached_key, cached_ac_key, simulation)) = &self.analysis.cached_simulation
            && *cached_key == revision_key
            && *cached_ac_key == ac_key
        {
            self.performance.simulation_cache_hit = true;
            self.performance.simulation_cache_hits =
                self.performance.simulation_cache_hits.saturating_add(1);
            return std::sync::Arc::clone(simulation);
        }
        self.performance.simulation_cache_hit = false;
        self.performance.simulation_cache_misses =
            self.performance.simulation_cache_misses.saturating_add(1);

        let document_revision = self.analysis.revisions.persistence;
        let pending = (document_revision, revision_key, ac_key);
        if self.analysis.pending_schematic != Some(pending) {
            let request = crate::engine::worker::AnalysisRequest::Schematic {
                components: self.components.clone(),
                wires: self.wires.clone(),
                annotations: Box::new(self.annotations.netlist_annotations()),
                ac_frequency_hz: self.simulation_ui.ac_freq_hz as f64,
                backend: self.simulation_ui.backend,
                revision_key,
                ac_key,
            };
            if self
                .analysis
                .worker
                .submit(document_revision, request)
                .is_ok()
            {
                self.analysis.pending_schematic = Some(pending);
            }
        }
        self.simulation_run_state = crate::ui::app::SimulationRunState::Solving;
        self.analysis
            .cached_simulation
            .as_ref()
            .map(|(_, _, simulation)| std::sync::Arc::clone(simulation))
            .unwrap_or_else(|| {
                std::sync::Arc::new(crate::engine::simulation::Simulation {
                    summary: "Analyzing…".to_string(),
                    explanation: "Circuit analysis is running in the background.".to_string(),
                    ..crate::engine::simulation::Simulation::default()
                })
            })
    }

    pub(crate) fn current_netlist(&mut self) -> std::sync::Arc<CircuitNetlist> {
        let revision = self.analysis.revisions.schematic_connectivity;
        if let Some((cached_revision, netlist)) = &self.analysis.cached_netlist
            && *cached_revision == revision
        {
            return std::sync::Arc::clone(netlist);
        }
        let connectivity = self.current_connectivity();
        let netlist = std::sync::Arc::new(connectivity.netlist.clone());
        self.analysis.cached_netlist = Some((revision, std::sync::Arc::clone(&netlist)));
        netlist
    }

    pub(crate) fn current_connectivity(&mut self) -> std::sync::Arc<CanonicalConnectivity> {
        let revision = self.analysis.revisions.schematic_connectivity;
        if let Some((revision, connectivity)) = &self.analysis.cached_connectivity
            && *revision == self.analysis.revisions.schematic_connectivity
        {
            self.performance.netlist_cache_hit = true;
            self.performance.netlist_cache_hits =
                self.performance.netlist_cache_hits.saturating_add(1);
            return std::sync::Arc::clone(connectivity);
        }
        self.performance.netlist_cache_hit = false;
        self.performance.netlist_cache_misses =
            self.performance.netlist_cache_misses.saturating_add(1);
        let started = std::time::Instant::now();
        let annotations = self.annotations.netlist_annotations();
        let connectivity = build_canonical_connectivity_with_annotations(
            &self.components,
            &self.wires,
            &annotations,
        );
        self.performance.netlist_ms = started.elapsed().as_secs_f64() * 1_000.0;
        let connectivity = std::sync::Arc::new(connectivity);
        self.analysis.cached_connectivity = Some((revision, std::sync::Arc::clone(&connectivity)));
        self.analysis.cached_netlist =
            Some((revision, std::sync::Arc::new(connectivity.netlist.clone())));
        self.analysis.dirty_flags.connectivity_dirty = false;
        connectivity
    }

    pub(crate) fn current_connected_pins(&mut self) -> std::sync::Arc<Vec<(i32, i32)>> {
        let revision = self.analysis.revisions.schematic_connectivity;
        if let Some((revision, pins)) = &self.analysis.cached_connected_pins
            && *revision == self.analysis.revisions.schematic_connectivity
        {
            return std::sync::Arc::clone(pins);
        }
        let pins = self
            .current_connectivity()
            .netlist
            .pins
            .iter()
            .filter(|pin| pin.connected_by_wire)
            .map(|pin| (pin.position.x.round() as i32, pin.position.y.round() as i32))
            .collect::<Vec<_>>();
        let pins = std::sync::Arc::new(pins);
        self.analysis.cached_connected_pins = Some((revision, std::sync::Arc::clone(&pins)));
        pins
    }

    pub(crate) fn flush_autorecover_if_needed(&mut self) {
        if !self.editor.history.dirty
            || self.last_autorecover_revision == self.analysis.revisions.persistence
        {
            return;
        }
        self.save_current_page();
        let revision = self.analysis.revisions.persistence;
        let request = crate::engine::worker::AnalysisRequest::Autosave {
            saved: Box::new(SavedCircuit::from_app(self)),
            path: crate::ui::app::autorecover_path(self.document_id),
        };
        if self.analysis.worker.submit(revision, request).is_err() {
            self.status = "Auto backup queue is busy; retrying shortly.".to_string();
            return;
        }
        self.last_autorecover_revision = revision;
    }

    pub(crate) fn poll_analysis_worker(&mut self) {
        if let Some(error) = self.analysis.worker.take_failure() {
            self.status = error;
        }
        while let Some(result) = self.analysis.worker.try_recv() {
            if result.document_revision != self.analysis.revisions.persistence {
                continue;
            }
            let document_revision = result.document_revision;
            match result.payload {
                crate::engine::worker::AnalysisPayload::Schematic(result) => {
                    let expected = (document_revision, result.revision_key, result.ac_key);
                    if self.analysis.pending_schematic != Some(expected) {
                        continue;
                    }
                    self.performance.netlist_cache_hit = result.connectivity_reused;
                    if result.connectivity_reused {
                        self.performance.netlist_cache_hits =
                            self.performance.netlist_cache_hits.saturating_add(1);
                    } else {
                        self.performance.netlist_cache_misses =
                            self.performance.netlist_cache_misses.saturating_add(1);
                    }
                    let connectivity = result.connectivity;
                    self.analysis.cached_netlist = Some((
                        result.revision_key.connectivity,
                        std::sync::Arc::new(connectivity.netlist.clone()),
                    ));
                    self.analysis.cached_connectivity = Some((
                        result.revision_key.connectivity,
                        std::sync::Arc::clone(&connectivity),
                    ));
                    self.analysis.cached_simulation = Some((
                        result.revision_key,
                        result.ac_key,
                        std::sync::Arc::new(result.simulation),
                    ));
                    self.simulation_run_state = match self
                        .analysis
                        .cached_simulation
                        .as_ref()
                        .map(|(_, _, simulation)| simulation.status)
                        .unwrap_or_default()
                    {
                        crate::engine::simulation::SimulationStatus::Ok => {
                            crate::ui::app::SimulationRunState::Valid
                        }
                        crate::engine::simulation::SimulationStatus::Warning => {
                            crate::ui::app::SimulationRunState::Warning
                        }
                        crate::engine::simulation::SimulationStatus::Failed => {
                            crate::ui::app::SimulationRunState::Failed
                        }
                    };
                    self.performance.netlist_ms = result.connectivity_ms;
                    self.performance.mna_ms = result.simulation_ms;
                    self.performance.erc_ms = result.erc_ms;
                    self.analysis.pending_schematic = None;
                    self.analysis.simulation_revision =
                        self.analysis.simulation_revision.saturating_add(1);
                    self.analysis.dirty_flags.connectivity_dirty = false;
                    self.analysis.dirty_flags.validation_dirty = false;
                    self.analysis.dirty_flags.simulation_dirty = false;
                    self.workspace_state.repaint_requested = true;
                }
                crate::engine::worker::AnalysisPayload::FullDrc(violations) => {
                    if self.analysis.pending_full_drc_revision != Some(result.document_revision) {
                        continue;
                    }
                    self.analysis.pcb_drc = violations;
                    self.analysis.pending_full_drc_revision = None;
                    self.analysis.dirty_flags.pcb_drc_dirty = false;
                    self.workspace_state.repaint_requested = true;
                }
                crate::engine::worker::AnalysisPayload::Autosave(Err(error)) => {
                    self.status = format!("Auto backup failed: {error}");
                }
                crate::engine::worker::AnalysisPayload::Autosave(Ok(_)) => {}
            }
        }
    }

    pub(crate) fn effective_project_pages(&self) -> Vec<ProjectPage> {
        let mut pages = if self.pages.is_empty() {
            vec![ProjectPage::empty("Page 1".to_string())]
        } else {
            self.pages.clone()
        };

        let page_index = self.current_page.min(pages.len().saturating_sub(1));
        if let Some(page) = pages.get_mut(page_index) {
            page.components = self.components.clone();
            page.wires = self.wires.clone();
            page.next_id = self.next_id;
            page.counters = self.counters.clone();
            page.annotations = self.annotations.clone();
        }
        pages
    }

    #[allow(clippy::type_complexity)] // Export compatibility boundary.
    pub(crate) fn effective_pages(
        &self,
    ) -> Vec<(String, Vec<Component>, Vec<Wire>, u64, Counters)> {
        self.effective_project_pages()
            .into_iter()
            .map(|page| {
                (
                    page.name,
                    page.components,
                    page.wires,
                    page.next_id,
                    page.counters,
                )
            })
            .collect()
    }
}

fn i2c_mapping_for_kind(kind: ComponentKind) -> Option<(&'static str, &'static str, &'static str)> {
    match kind {
        ComponentKind::Esp32 => Some(("GPIO21 SDA", "GPIO22 SCL", "3V3")),
        ComponentKind::Esp32S3 => Some(("GPIO2 SDA", "GPIO3 SCL", "3V3")),
        ComponentKind::Esp32C3 => Some(("GPIO1 SDA", "GPIO2 SCL", "3V3")),
        ComponentKind::ArduinoUno => Some(("A4 SDA", "A5 SCL", "5V")),
        ComponentKind::RaspberryPiPico => Some(("GP4 SDA", "GP5 SCL", "3V3")),
        ComponentKind::Stm32BluePill => Some(("PB7 SDA", "PB6 SCL", "3V3")),
        ComponentKind::Stm32Nucleo64 => Some(("D14 PB9 SDA", "D15 PB8 SCL", "3V3")),
        _ => None,
    }
}

fn pcb_outline_size(outline: &BoardOutline) -> (f32, f32) {
    let max_x = outline
        .points
        .iter()
        .map(|point| point.x)
        .fold(0.0, f32::max);
    let max_y = outline
        .points
        .iter()
        .map(|point| point.y)
        .fold(0.0, f32::max);
    (max_x, max_y)
}

fn pcb_drc_severity(severity: DrcSeverity) -> PcbDrcSeverity {
    match severity {
        DrcSeverity::Error => PcbDrcSeverity::Error,
        DrcSeverity::Warning => PcbDrcSeverity::Warning,
    }
}

fn write_with_backup_path(path: &Path, content: &str) -> Result<(), String> {
    let path = path
        .to_str()
        .ok_or_else(|| format!("Path is not valid UTF-8: {}", path.display()))?;
    write_with_backup(path, content)
}

// ── SavedCircuit serialization helpers ──────────────────────────────────────

impl SavedCircuit {
    pub(crate) fn from_app(app: &crate::CircuitApp) -> Self {
        let pages = app
            .effective_project_pages()
            .into_iter()
            .map(|page| {
                let (junction_dots, no_connect_markers) = saved_annotations_from(&page.annotations);
                SavedPage {
                    name: page.name,
                    next_id: page.next_id,
                    counters: page.counters,
                    components: saved_components_from(&page.components),
                    wires: saved_wires_from(&page.wires),
                    junction_dots,
                    no_connect_markers,
                }
            })
            .collect::<Vec<_>>();
        let (junction_dots, no_connect_markers) = saved_annotations_from(&app.annotations);
        Self {
            schema_version: 4,
            next_id: app.next_id,
            counters: app.counters.clone(),
            components: saved_components_from(&app.components),
            wires: saved_wires_from(&app.wires),
            junction_dots,
            no_connect_markers,
            pages,
            current_page: app.current_page,
        }
    }

    pub(crate) fn into_snapshot(self) -> Result<(CircuitSnapshot, Vec<String>), String> {
        if self.schema_version > 4 {
            return Err(format!(
                "Unsupported schema version {}.",
                self.schema_version
            ));
        }
        let current_page = self.current_page;
        let mut load_notes = Vec::new();

        let mut pages = Vec::new();
        if self.pages.is_empty() {
            let annotations = repair_saved_annotations(
                self.junction_dots,
                self.no_connect_markers,
                "Page 1",
                &mut load_notes,
            );
            let page = repair_saved_page(
                "Page 1".to_string(),
                self.components,
                self.wires,
                self.next_id,
                self.counters,
                annotations,
                &mut load_notes,
            );
            pages.push(page);
        } else {
            for (idx, page) in self.pages.into_iter().enumerate() {
                let name = if page.name.trim().is_empty() {
                    load_notes.push(format!("Filled an empty page name on page {}.", idx + 1));
                    format!("Page {}", idx + 1)
                } else {
                    page.name
                };
                let annotations = repair_saved_annotations(
                    page.junction_dots,
                    page.no_connect_markers,
                    &name,
                    &mut load_notes,
                );
                pages.push(repair_saved_page(
                    name,
                    page.components,
                    page.wires,
                    page.next_id,
                    page.counters,
                    annotations,
                    &mut load_notes,
                ));
            }
        }

        if pages.is_empty() {
            pages.push(ProjectPage::empty("Page 1".to_string()));
        }

        let current_page = current_page.min(pages.len().saturating_sub(1));
        let current = pages[current_page].clone();
        Ok((
            CircuitSnapshot {
                components: current.components,
                wires: current.wires,
                next_id: current.next_id,
                counters: current.counters,
                annotations: current.annotations,
                pages,
                current_page,
                board: Board::new_two_layer(80.0, 50.0),
            },
            load_notes,
        ))
    }
}

fn saved_components_from(components: &[Component]) -> Vec<SavedComponent> {
    components
        .iter()
        .map(|component| SavedComponent {
            id: component.id,
            kind: component.kind,
            x: component.pos.x,
            y: component.pos.y,
            rotation: component.rotation,
            label: component.label.clone(),
            value: component.value.clone(),
            part_id: component.part_id.clone(),
        })
        .collect()
}

fn saved_wires_from(wires: &[Wire]) -> Vec<SavedWire> {
    wires
        .iter()
        .map(|wire| SavedWire {
            id: wire.id,
            points: wire
                .points
                .iter()
                .map(|point| SavedPoint {
                    x: point.x,
                    y: point.y,
                })
                .collect(),
            start: Some(wire.start.saved()),
            end: Some(wire.end.saved()),
        })
        .collect()
}

fn saved_annotations_from(
    annotations: &SchematicAnnotations,
) -> (Vec<SavedJunctionDot>, Vec<SavedNoConnectMarker>) {
    let junction_dots = annotations
        .junction_dots
        .iter()
        .map(|dot| SavedJunctionDot {
            id: dot.id.0,
            x: dot.position.x,
            y: dot.position.y,
        })
        .collect();
    let no_connect_markers = annotations
        .no_connect_markers
        .iter()
        .map(|marker| SavedNoConnectMarker {
            id: marker.id,
            x: marker.position.x,
            y: marker.position.y,
        })
        .collect();
    (junction_dots, no_connect_markers)
}

fn repair_saved_page(
    name: String,
    saved_components: Vec<SavedComponent>,
    saved_wires: Vec<SavedWire>,
    saved_next_id: u64,
    saved_counters: Counters,
    annotations: SchematicAnnotations,
    load_notes: &mut Vec<String>,
) -> ProjectPage {
    let annotation_max_id = annotations
        .junction_dots
        .iter()
        .map(|dot| dot.id.0)
        .chain(
            annotations
                .no_connect_markers
                .iter()
                .map(|marker| marker.id),
        )
        .max()
        .unwrap_or(0);
    let mut used_ids = HashSet::new();
    let mut repair_id = saved_components
        .iter()
        .map(|component| component.id)
        .chain(saved_wires.iter().map(|wire| wire.id))
        .max()
        .unwrap_or(0)
        .max(annotation_max_id)
        .max(saved_next_id)
        + 1;

    let mut components = Vec::new();
    for component in saved_components {
        if !component.x.is_finite() || !component.y.is_finite() {
            load_notes.push(format!(
                "Skipped {} with invalid position.",
                component.label
            ));
            continue;
        }
        let mut id = component.id;
        if id == 0 || !used_ids.insert(id) {
            id = repair_id;
            repair_id += 1;
            used_ids.insert(id);
            load_notes.push(format!(
                "Reassigned duplicate component id for {}.",
                component.label
            ));
        }
        if component.kind == ComponentKind::Custom
            && component.part_id.as_deref().and_then(custom_part).is_none()
        {
            load_notes.push(format!(
                "Custom part definition {} is not loaded; {} keeps its wiring but has no pins. \
                 Put the part's JSON file in {} and reload.",
                component.part_id.as_deref().unwrap_or("(missing id)"),
                component.label,
                CUSTOM_PARTS_DIR,
            ));
        }
        components.push(Component {
            id,
            kind: component.kind,
            pos: Pos2::new(component.x, component.y),
            rotation: component.rotation.rem_euclid(360),
            label: if component.label.trim().is_empty() {
                load_notes.push("Filled an empty component label.".to_string());
                crate::component_kind_label(component.kind).to_string()
            } else {
                component.label
            },
            value: component.value,
            part_id: component.part_id,
        });
    }

    let mut wires = Vec::new();
    for wire in saved_wires {
        let points = wire
            .points
            .into_iter()
            .filter_map(|point| {
                if point.x.is_finite() && point.y.is_finite() {
                    Some(Pos2::new(point.x, point.y))
                } else {
                    load_notes.push("Dropped an invalid wire point.".to_string());
                    None
                }
            })
            .collect::<Vec<_>>();
        let points = crate::simplify_wire(points);
        if points.len() < 2 {
            load_notes.push(format!(
                "Skipped wire {} with fewer than 2 points.",
                wire.id
            ));
            continue;
        }
        let mut id = wire.id;
        if id == 0 || !used_ids.insert(id) {
            id = repair_id;
            repair_id += 1;
            used_ids.insert(id);
            load_notes.push("Reassigned duplicate wire id.".to_string());
        }
        let start = wire
            .start
            .map(WireEndpoint::from_saved)
            .unwrap_or_else(|| infer_legacy_endpoint(points[0], &components, load_notes));
        let end = wire.end.map(WireEndpoint::from_saved).unwrap_or_else(|| {
            infer_legacy_endpoint(
                *points.last().unwrap_or(&points[0]),
                &components,
                load_notes,
            )
        });
        wires.push(Wire::with_endpoints(id, points, start, end));
    }

    let max_id = components
        .iter()
        .map(|component| component.id)
        .chain(wires.iter().map(|wire| wire.id))
        .max()
        .unwrap_or(0);
    let next_id = saved_next_id
        .max(max_id.max(annotation_max_id) + 1)
        .max(repair_id);
    ProjectPage {
        name,
        components,
        wires,
        next_id,
        counters: saved_counters,
        annotations,
    }
}

fn infer_legacy_endpoint(
    point: Pos2,
    components: &[Component],
    load_notes: &mut Vec<String>,
) -> WireEndpoint {
    let mut best: Option<(f32, PinRef)> = None;
    for component in components {
        for pin in component_pin_defs(component) {
            let distance = point.distance(pin.pos);
            if distance <= 1.0
                && best
                    .as_ref()
                    .is_none_or(|(best_distance, _)| distance < *best_distance)
            {
                best = Some((
                    distance,
                    PinRef {
                        component_id: component.id,
                        pin_name: pin.label.to_string(),
                    },
                ));
            }
        }
    }
    if let Some((_, pin)) = best {
        load_notes.push(format!(
            "Migrated legacy wire endpoint to explicit PinRef {}.{}.",
            pin.component_id, pin.pin_name
        ));
        WireEndpoint::Pin(pin)
    } else {
        WireEndpoint::FreePoint(point)
    }
}

fn repair_saved_annotations(
    junction_dots: Vec<SavedJunctionDot>,
    no_connect_markers: Vec<SavedNoConnectMarker>,
    page_name: &str,
    load_notes: &mut Vec<String>,
) -> SchematicAnnotations {
    let mut junction_ids = HashSet::new();
    let mut invalid_junctions = 0;
    let junction_dots = junction_dots
        .into_iter()
        .filter_map(|dot| {
            if dot.id == 0
                || !dot.x.is_finite()
                || !dot.y.is_finite()
                || !junction_ids.insert(dot.id)
            {
                invalid_junctions += 1;
                return None;
            }
            Some(JunctionDot {
                id: JunctionId(dot.id),
                position: Pos2::new(dot.x, dot.y),
            })
        })
        .collect();
    let mut no_connect_ids = HashSet::new();
    let mut invalid_no_connects = 0;
    let no_connect_markers = no_connect_markers
        .into_iter()
        .filter_map(|marker| {
            if marker.id == 0
                || !marker.x.is_finite()
                || !marker.y.is_finite()
                || !no_connect_ids.insert(marker.id)
            {
                invalid_no_connects += 1;
                return None;
            }
            Some(NoConnectDot {
                id: marker.id,
                position: Pos2::new(marker.x, marker.y),
            })
        })
        .collect();
    if invalid_junctions > 0 {
        load_notes.push(format!(
            "Dropped {invalid_junctions} invalid junction marker(s) on {page_name}."
        ));
    }
    if invalid_no_connects > 0 {
        load_notes.push(format!(
            "Dropped {invalid_no_connects} invalid no-connect marker(s) on {page_name}."
        ));
    }
    SchematicAnnotations {
        junction_dots,
        no_connect_markers,
    }
}
