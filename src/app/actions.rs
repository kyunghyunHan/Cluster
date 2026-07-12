use crate::app::{AlignDir, Selection, Tool};
use crate::engine::validation::ErcAutoFix;
use crate::engine::{mna, netlist::build_circuit_netlist, simulation as simulation_engine};
use crate::model::cad::{CadProjectData, Point2};
use crate::model::*;
use crate::pcb::board::{Board, BoardOutline};
use crate::pcb::drc::{DrcSeverity, run_drc_with_nets};
use crate::pcb::layer::BoardLayer;
use crate::pcb::track::TrackSegment;
use crate::storage::save::{ProjectFolderLayout, write_with_backup};
use crate::ui::bottom_dock::{
    PcbDockSummary, PcbDrcRow, PcbDrcSeverity, PcbPreviewData, PcbPreviewDiagnostic,
    PcbPreviewFootprint, PcbPreviewRatsnest, PcbPreviewTrack,
};
use crate::ui::breadboard::BreadboardRoute;
use egui::{Pos2, Rect, Vec2};
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
        self.record_history();
        self.place_component(kind, pos);
        self.status = "Component placed. Drag to reposition, R to rotate.".to_string();
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
        id
    }

    pub(crate) fn add_custom_component(&mut self, part_id: &str, pos: Pos2) {
        self.record_history();
        self.place_custom_component(part_id, pos);
        self.status = "Custom part placed. Drag to reposition, R to rotate.".to_string();
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
        self.record_history();
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
                self.selected = Some(Selection::Component(id));
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
                self.selected = Some(Selection::Component(sda));
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
                self.selected = Some(Selection::Component(id));
                self.status = "Auto fix added a driver suggestion note.".to_string();
            }
            ErcAutoFix::AddRelayFlybackDiode { .. } => {
                let diode =
                    self.place_component(ComponentKind::Diode, base_pos + Vec2::new(-120.0, 0.0));
                self.add_wire_between(diode, "A", target_id, "COIL-");
                self.add_wire_between(diode, "B", target_id, "COIL+");
                self.selected = Some(Selection::Component(diode));
                self.status = "Auto fix added a flyback diode across the relay coil.".to_string();
            }
            ErcAutoFix::AddLevelShifterNote { .. } => {
                let id = self.place_note(
                    base_pos + Vec2::new(120.0, -100.0),
                    "Add level shifter or resistor divider before this 3.3V GPIO.",
                );
                self.selected = Some(Selection::Component(id));
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
        let points = crate::simplify_wire(points);
        if points.len() < 2 {
            return;
        }
        self.record_history();
        let endpoints: Vec<Pos2> = points
            .first()
            .copied()
            .into_iter()
            .chain(points.last().copied())
            .collect();
        for ep in endpoints {
            self.split_wire_at_point(ep);
        }
        let start = self.infer_wire_endpoint(points[0]);
        let end = self.infer_wire_endpoint(*points.last().unwrap_or(&points[0]));
        let id = self.next_id();
        self.wires
            .push(Wire::with_endpoints(id, points, start, end));
        self.status = "Wire placed.".to_string();
    }

    pub(crate) fn infer_wire_endpoint(&self, point: Pos2) -> WireEndpoint {
        for component in &self.components {
            for pin in component_pin_defs(component) {
                if point.distance(pin.pos) <= 1.0 {
                    return WireEndpoint::Pin(PinRef {
                        component_id: component.id,
                        pin_name: pin.label.to_string(),
                    });
                }
            }
        }
        WireEndpoint::FreePoint(point)
    }

    pub(crate) fn split_wire_at_point(&mut self, point: Pos2) {
        let mut split_target: Option<(usize, usize)> = None;
        'outer: for (wi, wire) in self.wires.iter().enumerate() {
            for si in 0..wire.points.len().saturating_sub(1) {
                let a = wire.points[si];
                let b = wire.points[si + 1];
                if distance_to_segment(point, a, b) < 2.5
                    && point.distance(a) > 5.0
                    && point.distance(b) > 5.0
                {
                    split_target = Some((wi, si));
                    break 'outer;
                }
            }
        }
        if let Some((wi, si)) = split_target {
            let old_start = self.wires[wi].start.clone();
            let old_end = self.wires[wi].end.clone();
            let mut first = self.wires[wi].points[..=si].to_vec();
            first.push(point);
            let mut second = vec![point];
            second.extend_from_slice(&self.wires[wi].points[si + 1..]);
            self.wires[wi].points = crate::simplify_wire(first);
            self.wires[wi].start = old_start;
            self.wires[wi].end = WireEndpoint::FreePoint(point);
            let new_id = self.next_id();
            self.wires.push(Wire {
                id: new_id,
                points: crate::simplify_wire(second),
                start: WireEndpoint::FreePoint(point),
                end: old_end,
            });
        }
    }

    pub(crate) fn same_net_wires(&self, wire_id: u64) -> HashSet<u64> {
        type Key = (i32, i32);
        let mut ep_to_wires: HashMap<Key, Vec<u64>> = HashMap::with_capacity(self.wires.len() * 2);
        let mut id_to_idx: HashMap<u64, usize> = HashMap::with_capacity(self.wires.len());
        for (idx, wire) in self.wires.iter().enumerate() {
            id_to_idx.insert(wire.id, idx);
            for pt in wire.points.first().into_iter().chain(wire.points.last()) {
                let key: Key = (pt.x.round() as i32, pt.y.round() as i32);
                ep_to_wires.entry(key).or_default().push(wire.id);
            }
        }
        if !id_to_idx.contains_key(&wire_id) {
            return HashSet::new();
        }

        let mut same_net: HashSet<u64> = HashSet::new();
        let mut queue: Vec<u64> = vec![wire_id];
        same_net.insert(wire_id);

        while let Some(wid) = queue.pop() {
            let wire = &self.wires[id_to_idx[&wid]];
            let wire_eps: Vec<Pos2> = wire
                .points
                .first()
                .copied()
                .into_iter()
                .chain(wire.points.last().copied())
                .collect();

            for &ep in &wire_eps {
                let key: Key = (ep.x.round() as i32, ep.y.round() as i32);
                for &candidate in ep_to_wires.get(&key).map(|v| v.as_slice()).unwrap_or(&[]) {
                    if same_net.insert(candidate) {
                        queue.push(candidate);
                    }
                }
            }
            for other in &self.wires {
                if same_net.contains(&other.id) {
                    continue;
                }
                let mut other_eps = other
                    .points
                    .first()
                    .copied()
                    .into_iter()
                    .chain(other.points.last().copied());
                let connected = other_eps.any(|oep| {
                    wire.points
                        .windows(2)
                        .any(|seg| distance_to_segment(oep, seg[0], seg[1]) < 2.5)
                }) || wire_eps.iter().any(|&ep| {
                    other
                        .points
                        .windows(2)
                        .any(|seg| distance_to_segment(ep, seg[0], seg[1]) < 2.5)
                });
                if connected && same_net.insert(other.id) {
                    queue.push(other.id);
                }
            }
        }
        same_net
    }

    pub(crate) fn reset_canvas(&mut self) {
        self.backup_dirty_work("reset");
        self.record_history();
        self.components.clear();
        self.wires.clear();
        self.selected = None;
        self.multi_selected.clear();
        self.drag = None;
        self.draft_wire.clear();
        self.wire_from_select = false;
        self.hovered_net_wire = None;
        self.highlighted_net_wires.clear();
        self.snap_target = None;
        self.inline_edit = None;
        self.context_menu = None;
        self.counters = Counters::default();
        self.next_id = 1;
        self.tool = Tool::Select;
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
        self.mark_dirty();
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
        self.selected = Some(Selection::Component(route.from_component_id));
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
        self.selected = Some(Selection::Component(route.from_component_id));
        self.status = format!(
            "Added jumper: {} {} -> {} {}.",
            route.from_label, from_pin, route.to_label, to_pin
        );
    }

    pub(crate) fn update_pcb_from_schematic(&mut self) {
        let netlist = self.current_netlist();
        let cad = CadProjectData::from_schematic(&self.components, &netlist);
        self.pcb_ui
            .board
            .update_from_schematic(&cad.symbols, &cad.nets);
        self.refresh_pcb_analysis(&cad);
        self.pcb_ui.cad = Some(cad);
        self.pcb_ui.last_sync_revision = self.circuit_revision;
        self.pcb_ui.selected_drc_index = None;
        self.dirty_flags.pcb_sync_dirty = false;

        let summary = self.pcb_dock_summary();
        self.status = format!(
            "PCB updated: {} footprint(s), {} ratsnest edge(s), {} DRC error(s).",
            summary.footprint_count, summary.ratsnest_count, summary.drc_errors
        );
    }

    pub(crate) fn auto_place_pcb_footprints(&mut self) {
        self.ensure_pcb_synced();
        let board_width = self
            .pcb_ui
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
        for (index, footprint) in self.pcb_ui.board.footprints.iter_mut().enumerate() {
            let col = index % per_row;
            let row = index / per_row;
            footprint.position = crate::model::cad::Point2::new(
                start_x + col as f32 * step_x,
                start_y + row as f32 * step_y,
            );
            footprint.placed = true;
        }
        self.refresh_pcb_analysis_from_current();
        self.status = format!(
            "Auto-placed {} PCB footprint(s).",
            self.pcb_ui.board.footprints.len()
        );
    }

    pub(crate) fn fit_pcb_board_to_contents(&mut self) {
        self.ensure_pcb_synced();
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        for footprint in &self.pcb_ui.board.footprints {
            min_x = min_x.min(footprint.position.x);
            min_y = min_y.min(footprint.position.y);
            max_x = max_x.max(footprint.position.x);
            max_y = max_y.max(footprint.position.y);
        }
        for track in &self.pcb_ui.board.tracks {
            for point in [track.start, track.end] {
                min_x = min_x.min(point.x);
                min_y = min_y.min(point.y);
                max_x = max_x.max(point.x);
                max_y = max_y.max(point.y);
            }
        }
        for via in &self.pcb_ui.board.vias {
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
        for footprint in &mut self.pcb_ui.board.footprints {
            footprint.position.x += shift_x;
            footprint.position.y += shift_y;
        }
        for track in &mut self.pcb_ui.board.tracks {
            track.start.x += shift_x;
            track.start.y += shift_y;
            track.end.x += shift_x;
            track.end.y += shift_y;
        }
        for via in &mut self.pcb_ui.board.vias {
            via.position.x += shift_x;
            via.position.y += shift_y;
        }

        let width = (max_x - min_x + margin * 2.0).max(25.0);
        let height = (max_y - min_y + margin * 2.0).max(20.0);
        self.pcb_ui.board.outline = BoardOutline::rectangular(width, height);
        self.refresh_pcb_analysis_from_current();
        self.status = format!("Fit PCB board to {:.1} x {:.1} mm.", width, height);
    }

    pub(crate) fn route_pcb_ratsnest(&mut self) {
        self.ensure_pcb_synced();
        let Some(cad) = self.pcb_ui.cad.clone() else {
            self.status = "Update PCB before routing ratsnest.".to_string();
            return;
        };
        let footprint_by_id = self
            .pcb_ui
            .board
            .footprints
            .iter()
            .map(|footprint| (footprint.id, footprint.position))
            .collect::<HashMap<_, _>>();
        let mut next_id = self
            .pcb_ui
            .board
            .tracks
            .iter()
            .map(|track| track.id)
            .max()
            .unwrap_or(0)
            + 1;
        let mut added = 0usize;
        for ratsnest in self.pcb_ui.board.ratsnest_edges(&cad.nets) {
            if self
                .pcb_ui
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
            self.pcb_ui.board.tracks.push(TrackSegment {
                id: next_id,
                net_id: ratsnest.net_id,
                layer: BoardLayer::FrontCopper,
                start,
                end,
                width_mm: self.pcb_ui.board.design_rules.min_track_width_mm.max(0.25),
            });
            next_id += 1;
            added += 1;
        }
        self.refresh_pcb_analysis(&cad);
        self.pcb_ui.cad = Some(cad);
        self.status = if added == 0 {
            "No unrouted ratsnest edges needed a new track.".to_string()
        } else {
            format!("Added {added} straight PCB track(s) from ratsnest.")
        };
    }

    pub(crate) fn export_pcb_fabrication_files(&mut self) {
        self.ensure_pcb_synced();
        let drc_errors = self
            .pcb_ui
            .drc
            .iter()
            .filter(|violation| violation.severity == DrcSeverity::Error)
            .count();
        if drc_errors > 0 {
            self.status = format!("PCB export blocked: fix {drc_errors} DRC error(s) first.");
            return;
        }
        let Some(mut cad) = self.pcb_ui.cad.clone() else {
            self.status = "Update PCB before fabrication export.".to_string();
            return;
        };
        cad.board = Some(self.pcb_ui.board.clone());
        let writes = [
            (
                "cluster_pcb_F_Cu.gbr",
                crate::export::gerber::gerber_for_layer(
                    &self.pcb_ui.board,
                    BoardLayer::FrontCopper,
                ),
            ),
            (
                "cluster_pcb_B_Cu.gbr",
                crate::export::gerber::gerber_for_layer(&self.pcb_ui.board, BoardLayer::BackCopper),
            ),
            (
                "cluster_pcb_Edge_Cuts.gbr",
                crate::export::gerber::gerber_for_layer(&self.pcb_ui.board, BoardLayer::EdgeCuts),
            ),
            (
                "cluster_pcb.drl",
                crate::export::gerber::excellon_drill(&self.pcb_ui.board),
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
                self.history_state.dirty = false;
                self.last_autorecover_revision = self.circuit_revision;
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

        let mut cad = if let Some(cad) = self.pcb_ui.cad.clone() {
            cad
        } else {
            let netlist = self.current_netlist();
            CadProjectData::from_schematic(&self.components, &netlist)
        };
        cad.board = Some(self.pcb_ui.board.clone());
        cad.properties.insert(
            "document_revision".to_string(),
            self.circuit_revision.to_string(),
        );

        let schematic_json = serde_json::to_string_pretty(&SavedCircuit::from_app(self))
            .map_err(|error| error.to_string())?;
        let board_json =
            serde_json::to_string_pretty(&self.pcb_ui.board).map_err(|error| error.to_string())?;
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
        let board = serde_json::from_str::<Board>(&board_json)
            .map_err(|error| format!("Parse board.json: {error}"))?;
        let mut cad = serde_json::from_str::<CadProjectData>(&project_json)
            .map_err(|error| format!("Parse project.json: {error}"))?;
        cad.board = Some(board.clone());

        self.record_history();
        self.restore_snapshot(snapshot);
        self.pcb_ui.board = board;
        self.pcb_ui.cad = Some(cad.clone());
        self.pcb_ui.last_sync_revision = self.circuit_revision;
        self.pcb_ui.selected_drc_index = None;
        self.refresh_pcb_analysis(&cad);
        self.history_state.dirty = false;
        self.cached_simulation = None;
        self.cached_netlist = None;
        self.last_autorecover_revision = self.circuit_revision;
        self.dirty_flags.geometry_dirty = false;
        self.dirty_flags.connectivity_dirty = false;
        self.dirty_flags.validation_dirty = false;
        self.dirty_flags.simulation_dirty = true;
        self.dirty_flags.pcb_sync_dirty = false;
        self.pending_fit = true;
        if !load_notes.is_empty() {
            self.status = format!(
                "Loaded project with {} schematic repair note(s).",
                load_notes.len()
            );
        }
        Ok(())
    }

    pub(crate) fn select_pcb_drc_violation(&mut self, index: usize) {
        self.ensure_pcb_synced();
        let Some(violation) = self.pcb_ui.drc.get(index).cloned() else {
            self.status = format!("PCB DRC item {index} is no longer available.");
            self.pcb_ui.selected_drc_index = None;
            return;
        };
        self.pcb_ui.selected_drc_index = Some(index);
        if let Some(object_id) = violation.object_id
            && let Some(component_id) = self
                .pcb_ui
                .board
                .footprints
                .iter()
                .find(|footprint| footprint.id == object_id)
                .and_then(|footprint| footprint.symbol_instance_id)
        {
            self.selected = Some(Selection::Component(component_id));
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
        if self.pcb_ui.cad.is_none()
            || self.pcb_ui.last_sync_revision != self.circuit_revision
            || self.dirty_flags.pcb_sync_dirty
        {
            self.update_pcb_from_schematic();
        }
    }

    fn refresh_pcb_analysis_from_current(&mut self) {
        if let Some(cad) = self.pcb_ui.cad.clone() {
            self.refresh_pcb_analysis(&cad);
            self.pcb_ui.cad = Some(cad);
        }
    }

    fn refresh_pcb_analysis(&mut self, cad: &CadProjectData) {
        self.pcb_ui.ratsnest_count = self.unrouted_pcb_ratsnest(cad).len();
        self.pcb_ui.drc = run_drc_with_nets(&self.pcb_ui.board, &cad.nets);
        if self
            .pcb_ui
            .selected_drc_index
            .is_some_and(|index| index >= self.pcb_ui.drc.len())
        {
            self.pcb_ui.selected_drc_index = None;
        }
    }

    fn unrouted_pcb_ratsnest(&self, cad: &CadProjectData) -> Vec<crate::pcb::board::RatsnestEdge> {
        self.pcb_ui
            .board
            .ratsnest_edges(&cad.nets)
            .into_iter()
            .filter(|edge| {
                !self
                    .pcb_ui
                    .board
                    .tracks
                    .iter()
                    .any(|track| track.net_id == edge.net_id)
                    && !self
                        .pcb_ui
                        .board
                        .vias
                        .iter()
                        .any(|via| via.net_id == edge.net_id)
            })
            .collect()
    }

    pub(crate) fn pcb_dock_summary(&self) -> PcbDockSummary {
        let drc_errors = self
            .pcb_ui
            .drc
            .iter()
            .filter(|violation| violation.severity == DrcSeverity::Error)
            .count();
        let drc_warnings = self
            .pcb_ui
            .drc
            .iter()
            .filter(|violation| violation.severity == DrcSeverity::Warning)
            .count();
        PcbDockSummary {
            footprint_count: self.pcb_ui.board.footprints.len(),
            unplaced_count: self
                .pcb_ui
                .board
                .footprints
                .iter()
                .filter(|footprint| !footprint.placed)
                .count(),
            ratsnest_count: self.pcb_ui.ratsnest_count,
            drc_errors,
            drc_warnings,
            dirty: self.dirty_flags.pcb_sync_dirty
                || self.pcb_ui.last_sync_revision != self.circuit_revision,
            footprints: self
                .pcb_ui
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
                .pcb_ui
                .cad
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
                .pcb_ui
                .drc
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
        let (width_mm, height_mm) = pcb_outline_size(&self.pcb_ui.board.outline);
        let footprint_positions = self
            .pcb_ui
            .board
            .footprints
            .iter()
            .map(|footprint| (footprint.id, footprint.position))
            .collect::<HashMap<_, _>>();
        let ratsnest = self
            .pcb_ui
            .cad
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
                .pcb_ui
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
                .pcb_ui
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
                .pcb_ui
                .drc
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
            .pcb_ui
            .board
            .footprints
            .iter()
            .find(|footprint| footprint.id == object_id)
        {
            return Some(footprint.position);
        }
        if let Some(track) = self
            .pcb_ui
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
        self.pcb_ui
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
            && let Some(&last) = self.draft_wire.last()
        {
            let dx = (pos.x - last.x).abs();
            let dy = (pos.y - last.y).abs();
            if dx > 0.1 && dy > 0.1 {
                let corner = if dx >= dy {
                    Pos2::new(pos.x, last.y)
                } else {
                    Pos2::new(last.x, pos.y)
                };
                crate::push_unique_point(&mut self.draft_wire, corner);
            }
        }
        crate::push_unique_point(&mut self.draft_wire, pos);
    }

    // ── Selection actions ────────────────────────────────────────────────────

    pub(crate) fn delete_selected(&mut self) {
        if !self.multi_selected.is_empty() {
            self.record_history();
            let count = self.multi_selected.len();
            self.components
                .retain(|c| !self.multi_selected.contains(&c.id));
            self.multi_selected.clear();
            self.selected = None;
            self.status = format!("Deleted {count} component(s).");
            return;
        }
        match self.selected.take() {
            Some(Selection::Component(id)) => {
                self.record_history();
                self.components.retain(|c| c.id != id);
                self.status = "Component deleted.".to_string();
            }
            Some(Selection::Wire(id)) => {
                self.record_history();
                self.wires.retain(|w| w.id != id);
                self.status = "Wire deleted.".to_string();
            }
            None => {
                self.status = "Nothing selected to delete.".to_string();
            }
        }
    }

    pub(crate) fn rotate_selected(&mut self) {
        let Some(Selection::Component(id)) = self.selected else {
            return;
        };
        let Some(index) = self.components.iter().position(|c| c.id == id) else {
            return;
        };
        self.record_history();
        let old_pins = component_pins(&self.components[index]);
        if let Some(component) = self.components.get_mut(index) {
            component.rotation = (component.rotation + 90) % 360;
        }
        let new_pins = component_pins(&self.components[index]);
        crate::move_attached_wire_endpoints(&mut self.wires, &old_pins, &new_pins);
        for wire in self.wires.iter_mut() {
            if wire.points.len() > 2 {
                let first = wire.points[0];
                let Some(&last) = wire.points.last() else {
                    continue;
                };
                if old_pins.iter().any(|pin| first.distance(*pin) <= 20.0)
                    || old_pins.iter().any(|pin| last.distance(*pin) <= 20.0)
                {
                    crate::tidy_wire_points(wire);
                }
            }
        }
        self.status = "Rotated and kept attached wires on pins.".to_string();
    }

    pub(crate) fn duplicate_selected(&mut self) {
        if !self.multi_selected.is_empty() {
            self.record_history();
            let offset = Vec2::new(self.grid * 2.0, self.grid * 2.0);
            let old_ids: Vec<u64> = self.multi_selected.iter().copied().collect();
            let mut new_ids = Vec::new();
            let srcs: Vec<Component> = self
                .components
                .iter()
                .filter(|c| old_ids.contains(&c.id))
                .cloned()
                .collect();
            for src in srcs {
                let mut dup = src;
                dup.id = self.next_id();
                dup.pos += offset;
                dup.label = self.next_label(dup.kind);
                new_ids.push(dup.id);
                self.components.push(dup);
            }
            self.multi_selected = new_ids.iter().copied().collect();
            self.status = format!("Duplicated {} component(s).", new_ids.len());
            return;
        }
        let Some(Selection::Component(id)) = self.selected else {
            self.status = "Select a component to duplicate.".to_string();
            return;
        };
        let Some(source) = self
            .components
            .iter()
            .find(|component| component.id == id)
            .cloned()
        else {
            self.status = "Selected component is missing.".to_string();
            return;
        };
        self.record_history();
        let mut duplicate = source;
        duplicate.id = self.next_id();
        duplicate.pos += Vec2::new(self.grid * 2.0, self.grid * 2.0);
        duplicate.label = self.next_label(duplicate.kind);
        let duplicate_id = duplicate.id;
        self.components.push(duplicate);
        self.selected = Some(Selection::Component(duplicate_id));
        self.status = "Component duplicated.".to_string();
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
        match fs::write(
            "cluster_circuit.cir",
            crate::circuit_to_spice_netlist(&self.components, &self.wires),
        ) {
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
        match self.write_circuit_json(crate::SAVE_PATH) {
            Ok(()) => {
                self.history_state.dirty = false;
                self.last_autorecover_revision = self.circuit_revision;
                self.status = format!("Saved {}.", crate::SAVE_PATH);
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
        let ids: Vec<u64> = if !self.multi_selected.is_empty() {
            self.multi_selected.iter().copied().collect()
        } else if let Some(Selection::Component(id)) = self.selected {
            vec![id]
        } else {
            return;
        };
        if ids.len() < 2 {
            return;
        }

        let positions: Vec<(u64, Pos2)> = self
            .components
            .iter()
            .filter(|c| ids.contains(&c.id))
            .map(|c| (c.id, c.pos))
            .collect();
        if positions.len() < 2 {
            self.status = "Select at least 2 valid components to align.".to_string();
            return;
        }

        let target = match dir {
            AlignDir::Left => positions
                .iter()
                .map(|p| p.1.x)
                .fold(f32::INFINITY, f32::min),
            AlignDir::Right => positions
                .iter()
                .map(|p| p.1.x)
                .fold(f32::NEG_INFINITY, f32::max),
            AlignDir::Top => positions
                .iter()
                .map(|p| p.1.y)
                .fold(f32::INFINITY, f32::min),
            AlignDir::Bottom => positions
                .iter()
                .map(|p| p.1.y)
                .fold(f32::NEG_INFINITY, f32::max),
            AlignDir::CenterH => {
                let sum: f32 = positions.iter().map(|p| p.1.x).sum();
                sum / positions.len() as f32
            }
            AlignDir::CenterV => {
                let sum: f32 = positions.iter().map(|p| p.1.y).sum();
                sum / positions.len() as f32
            }
        };

        self.record_history();
        for comp in self.components.iter_mut() {
            if ids.contains(&comp.id) {
                match dir {
                    AlignDir::Left | AlignDir::Right | AlignDir::CenterH => comp.pos.x = target,
                    AlignDir::Top | AlignDir::Bottom | AlignDir::CenterV => comp.pos.y = target,
                }
            }
        }
        self.status = format!("Aligned {} components.", positions.len());
    }

    pub(crate) fn distribute_selected(&mut self, vertical: bool) {
        let ids: Vec<u64> = if !self.multi_selected.is_empty() {
            self.multi_selected.iter().copied().collect()
        } else {
            return;
        };
        if ids.len() < 3 {
            return;
        }

        let mut ordered: Vec<(u64, f32)> = self
            .components
            .iter()
            .filter(|c| ids.contains(&c.id))
            .map(|c| (c.id, if vertical { c.pos.y } else { c.pos.x }))
            .collect();
        if ordered.len() < 3 {
            self.status = "Select at least 3 valid components to distribute.".to_string();
            return;
        }
        ordered.sort_by(|a, b| a.1.total_cmp(&b.1));

        let Some(first) = ordered.first().map(|(_, pos)| *pos) else {
            return;
        };
        let Some(last) = ordered.last().map(|(_, pos)| *pos) else {
            return;
        };
        let step = (last - first) / (ordered.len() as f32 - 1.0);

        self.record_history();
        for (i, (id, _)) in ordered.iter().enumerate() {
            let val = first + step * i as f32;
            if let Some(comp) = self.components.iter_mut().find(|c| c.id == *id) {
                if vertical {
                    comp.pos.y = val;
                } else {
                    comp.pos.x = val;
                }
            }
        }
        self.status = format!("Distributed {} components.", ids.len());
    }

    // ── Multi-page management ────────────────────────────────────────────────

    pub(crate) fn save_current_page(&mut self) {
        if let Some(page) = self.pages.get_mut(self.current_page) {
            page.1 = self.components.clone();
            page.2 = self.wires.clone();
            page.3 = self.next_id;
            page.4 = self.counters.clone();
        }
    }

    pub(crate) fn load_page_state(&mut self, idx: usize) {
        let (_, comps, wires, next_id, counters) = &self.pages[idx];
        self.components = comps.clone();
        self.wires = wires.clone();
        self.next_id = *next_id;
        self.counters = counters.clone();
        self.selected = None;
        self.multi_selected.clear();
        self.draft_wire.clear();
        self.drag = None;
        self.rect_select_start = None;
        self.hovered_net_wire = None;
        self.highlighted_net_wires.clear();
        self.snap_target = None;
        self.invalidate_analysis_cache();
    }

    pub(crate) fn switch_page(&mut self, idx: usize) {
        if idx == self.current_page || idx >= self.pages.len() {
            return;
        }
        self.save_current_page();
        self.current_page = idx;
        self.load_page_state(idx);
        self.status = format!("Switched to {}", self.pages[idx].0);
    }

    pub(crate) fn add_page(&mut self) {
        self.save_current_page();
        self.record_history();
        let n = self.pages.len() + 1;
        self.pages.push((
            format!("Page {n}"),
            Vec::new(),
            Vec::new(),
            1,
            Counters::default(),
        ));
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
        self.record_history();
        self.pages.remove(self.current_page);
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
        self.load_circuit_json_from(crate::SAVE_PATH, false);
    }

    pub(crate) fn recover_autosave(&mut self) {
        self.load_circuit_json_from(crate::AUTORECOVER_PATH, true);
    }

    pub(crate) fn write_circuit_json(&self, path: &str) -> Result<(), String> {
        let saved = SavedCircuit::from_app(self);
        let json = serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?;
        write_with_backup(path, &json)
    }

    pub(crate) fn backup_dirty_work(&mut self, reason: &str) {
        if !self.history_state.dirty || (self.components.is_empty() && self.wires.is_empty()) {
            return;
        }
        self.save_current_page();
        match self.write_circuit_json(crate::AUTORECOVER_PATH) {
            Ok(()) => {
                self.status = format!("Auto-saved recovery before {reason}.");
            }
            Err(err) => {
                self.status = format!("Recovery save failed before {reason}: {err}");
            }
        }
    }

    pub(crate) fn load_circuit_json_from(&mut self, path: &str, recovery: bool) {
        if path != crate::AUTORECOVER_PATH {
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
                self.record_history();
                self.restore_snapshot(snapshot);
                if recovery {
                    self.mark_dirty();
                } else {
                    self.history_state.dirty = false;
                    self.circuit_revision = self.circuit_revision.saturating_add(1);
                    self.cached_simulation = None;
                    self.last_autorecover_revision = self.circuit_revision;
                }
                self.status = if load_notes.is_empty() {
                    format!("Loaded {path}.")
                } else {
                    format!("Loaded {path} with {} repair(s).", load_notes.len())
                };
                self.pending_fit = true;
            }
            Err(err) => {
                self.status = format!("Load {path} failed: {err}");
            }
        }
    }

    // ── Cache accessors ──────────────────────────────────────────────────────

    pub(crate) fn current_simulation(&mut self) -> crate::engine::simulation::Simulation {
        if !self.simulate {
            return crate::engine::simulation::Simulation::default();
        }
        let ac_key = self.simulation_ui.ac_freq_hz.to_bits();
        if let Some((revision, cached_ac_key, simulation)) = &self.cached_simulation
            && *revision == self.circuit_revision
            && *cached_ac_key == ac_key
        {
            self.performance.simulation_cache_hit = true;
            return simulation.clone();
        }
        self.performance.simulation_cache_hit = false;
        let mna_started = std::time::Instant::now();
        let mut simulation = simulation_engine::analyze_circuit(&self.components, &self.wires);
        simulation.ac = mna::solve_ac(
            &self.components,
            &self.wires,
            self.simulation_ui.ac_freq_hz as f64,
        );
        simulation.transient =
            crate::engine::transient::solve_transient(&self.components, &self.wires);
        self.performance.mna_ms = mna_started.elapsed().as_secs_f64() * 1_000.0;
        let netlist = self.current_netlist();
        let erc_started = std::time::Instant::now();
        simulation.erc =
            crate::run_erc_with_netlist(&self.components, &self.wires, &simulation, &netlist);
        self.performance.erc_ms = erc_started.elapsed().as_secs_f64() * 1_000.0;
        self.cached_simulation = Some((self.circuit_revision, ac_key, simulation.clone()));
        self.dirty_flags.validation_dirty = false;
        self.dirty_flags.simulation_dirty = false;
        simulation
    }

    pub(crate) fn current_netlist(&mut self) -> CircuitNetlist {
        if let Some((revision, netlist)) = &self.cached_netlist
            && *revision == self.circuit_revision
        {
            self.performance.netlist_cache_hit = true;
            return netlist.clone();
        }
        self.performance.netlist_cache_hit = false;
        let started = std::time::Instant::now();
        let netlist = build_circuit_netlist(&self.components, &self.wires);
        self.performance.netlist_ms = started.elapsed().as_secs_f64() * 1_000.0;
        self.cached_netlist = Some((self.circuit_revision, netlist.clone()));
        self.dirty_flags.connectivity_dirty = false;
        netlist
    }

    pub(crate) fn current_connected_pins(&mut self) -> Vec<(i32, i32)> {
        if let Some((revision, pins)) = &self.cached_connected_pins
            && *revision == self.circuit_revision
        {
            return pins.clone();
        }
        let pins = crate::connected_pin_positions(&self.components, &self.wires);
        self.cached_connected_pins = Some((self.circuit_revision, pins.clone()));
        pins
    }

    pub(crate) fn flush_autorecover_if_needed(&mut self) {
        if !self.history_state.dirty
            || self.last_autorecover_revision == self.circuit_revision
            || (self.components.is_empty() && self.wires.is_empty())
        {
            return;
        }
        self.save_current_page();
        if let Err(err) = self.write_circuit_json(crate::AUTORECOVER_PATH) {
            self.status = format!("Auto backup failed: {err}");
            return;
        }
        self.last_autorecover_revision = self.circuit_revision;
    }

    pub(crate) fn effective_pages(
        &self,
    ) -> Vec<(String, Vec<Component>, Vec<Wire>, u64, Counters)> {
        let mut pages = if self.pages.is_empty() {
            vec![(
                "Page 1".to_string(),
                self.components.clone(),
                self.wires.clone(),
                self.next_id,
                self.counters.clone(),
            )]
        } else {
            self.pages.clone()
        };

        let page_index = self.current_page.min(pages.len().saturating_sub(1));
        if let Some(page) = pages.get_mut(page_index) {
            page.1 = self.components.clone();
            page.2 = self.wires.clone();
            page.3 = self.next_id;
            page.4 = self.counters.clone();
        }
        pages
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
            .effective_pages()
            .into_iter()
            .map(|(name, components, wires, next_id, counters)| SavedPage {
                name,
                next_id,
                counters,
                components: saved_components_from(&components),
                wires: saved_wires_from(&wires),
                junction_dots: Vec::new(),
                no_connect_markers: Vec::new(),
            })
            .collect::<Vec<_>>();
        Self {
            schema_version: 4,
            next_id: app.next_id,
            counters: app.counters.clone(),
            components: saved_components_from(&app.components),
            wires: saved_wires_from(&app.wires),
            junction_dots: Vec::new(),
            no_connect_markers: Vec::new(),
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
            let page = repair_saved_page(
                "Page 1".to_string(),
                self.components,
                self.wires,
                self.next_id,
                self.counters,
                &mut load_notes,
            );
            validate_saved_annotations(
                self.junction_dots,
                self.no_connect_markers,
                "Page 1",
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
                validate_saved_annotations(
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
                    &mut load_notes,
                ));
            }
        }

        if pages.is_empty() {
            pages.push((
                "Page 1".to_string(),
                Vec::new(),
                Vec::new(),
                1,
                Counters::default(),
            ));
        }

        let current_page = current_page.min(pages.len().saturating_sub(1));
        let (_, components, wires, next_id, counters) = pages[current_page].clone();
        Ok((
            CircuitSnapshot {
                components,
                wires,
                next_id,
                counters,
                pages,
                current_page,
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

fn repair_saved_page(
    name: String,
    saved_components: Vec<SavedComponent>,
    saved_wires: Vec<SavedWire>,
    saved_next_id: u64,
    saved_counters: Counters,
    load_notes: &mut Vec<String>,
) -> (String, Vec<Component>, Vec<Wire>, u64, Counters) {
    let mut used_ids = HashSet::new();
    let mut repair_id = saved_components
        .iter()
        .map(|component| component.id)
        .chain(saved_wires.iter().map(|wire| wire.id))
        .max()
        .unwrap_or(0)
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
    let next_id = saved_next_id.max(max_id + 1).max(repair_id);
    (name, components, wires, next_id, saved_counters)
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

fn validate_saved_annotations(
    junction_dots: Vec<SavedJunctionDot>,
    no_connect_markers: Vec<SavedNoConnectMarker>,
    page_name: &str,
    load_notes: &mut Vec<String>,
) {
    let invalid_junctions = junction_dots
        .iter()
        .filter(|dot| dot.id == 0 || !dot.x.is_finite() || !dot.y.is_finite())
        .count();
    let invalid_no_connects = no_connect_markers
        .iter()
        .filter(|marker| marker.id == 0 || !marker.x.is_finite() || !marker.y.is_finite())
        .count();
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
}
