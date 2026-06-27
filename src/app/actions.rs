use crate::app::{AlignDir, Selection, Tool};
use crate::engine::{mna, netlist::build_circuit_netlist, simulation as simulation_engine};
use crate::model::*;
use crate::storage::save::write_with_backup;
use egui::{Pos2, Rect, Vec2};
use std::collections::{HashMap, HashSet};
use std::fs;

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
        }
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
        let id = self.next_id();
        self.wires.push(Wire { id, points });
        self.status = "Wire placed.".to_string();
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
            let mut first = self.wires[wi].points[..=si].to_vec();
            first.push(point);
            let mut second = vec![point];
            second.extend_from_slice(&self.wires[wi].points[si + 1..]);
            self.wires[wi].points = crate::simplify_wire(first);
            let new_id = self.next_id();
            self.wires.push(Wire {
                id: new_id,
                points: crate::simplify_wire(second),
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
                let last = *wire.points.last().unwrap();
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
        self.status = "Capturing screenshot…".to_string();
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
                self.dirty = false;
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
        self.status = format!("Aligned {} components.", ids.len());
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
        ordered.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let first = ordered.first().unwrap().1;
        let last = ordered.last().unwrap().1;
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
        let canvas = self.canvas_rect;
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
        if !self.dirty || (self.components.is_empty() && self.wires.is_empty()) {
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
                    self.dirty = false;
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
        let ac_key = self.ac_freq_hz.to_bits();
        if let Some((revision, cached_ac_key, simulation)) = &self.cached_simulation
            && *revision == self.circuit_revision
            && *cached_ac_key == ac_key
        {
            return simulation.clone();
        }
        let mut simulation = simulation_engine::analyze_circuit(&self.components, &self.wires);
        simulation.ac = mna::solve_ac(&self.components, &self.wires, self.ac_freq_hz as f64);
        let netlist = self.current_netlist();
        simulation.erc =
            crate::run_erc_with_netlist(&self.components, &self.wires, &simulation, &netlist);
        self.cached_simulation = Some((self.circuit_revision, ac_key, simulation.clone()));
        simulation
    }

    pub(crate) fn current_netlist(&mut self) -> CircuitNetlist {
        if let Some((revision, netlist)) = &self.cached_netlist
            && *revision == self.circuit_revision
        {
            return netlist.clone();
        }
        let netlist = build_circuit_netlist(&self.components, &self.wires);
        self.cached_netlist = Some((self.circuit_revision, netlist.clone()));
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
        if !self.dirty
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
            schema_version: 3,
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
        if self.schema_version > 3 {
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
        wires.push(Wire { id, points });
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
