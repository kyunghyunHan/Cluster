use eframe::egui;
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Place(ComponentKind),
    Wire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
enum ComponentKind {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    Led,
    Switch,
    PushButton,
    SlideSwitch,
    Ground,
    VSource,
    ISource,
    Battery,
    OpAmp,
    Lamp,
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
}

#[derive(Debug, Clone)]
struct Component {
    id: u64,
    kind: ComponentKind,
    pos: Pos2,
    rotation: i32,
    label: String,
    value: String,
}

#[derive(Debug, Clone)]
struct Wire {
    id: u64,
    points: Vec<Pos2>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PinRole {
    Passive,
    Positive,
    Ground,
    Digital,
    I2c,
    Control,
    Output,
}

#[derive(Debug, Clone)]
struct CircuitPin {
    label: &'static str,
    role: PinRole,
    pos: Pos2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Selection {
    Component(u64),
    Wire(u64),
}

#[derive(Debug, Clone)]
enum DragState {
    Component { id: u64, offset: Vec2 },
    WirePoint { wire_id: u64, point_index: usize },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Counters {
    resistor: usize,
    capacitor: usize,
    inductor: usize,
    diode: usize,
    led: usize,
    switch: usize,
    ground: usize,
    vsource: usize,
    isource: usize,
    battery: usize,
    opamp: usize,
    lamp: usize,
    esp32: usize,
    arduino: usize,
    pico: usize,
    breadboard: usize,
    relay: usize,
    motor: usize,
    servo: usize,
    oled: usize,
    sensor: usize,
}

struct CircuitApp {
    components: Vec<Component>,
    wires: Vec<Wire>,
    tool: Tool,
    selected: Option<Selection>,
    drag: Option<DragState>,
    draft_wire: Vec<Pos2>,
    grid: f32,
    snap: bool,
    orthogonal_wires: bool,
    show_pins: bool,
    simulate: bool,
    status: String,
    next_id: u64,
    counters: Counters,
    history: Vec<CircuitSnapshot>,
    redo_history: Vec<CircuitSnapshot>,
    dirty: bool,
}

#[derive(Debug, Clone)]
struct CircuitSnapshot {
    components: Vec<Component>,
    wires: Vec<Wire>,
    next_id: u64,
    counters: Counters,
}

#[derive(Debug, Serialize, Deserialize)]
struct SavedCircuit {
    schema_version: u32,
    next_id: u64,
    counters: Counters,
    components: Vec<SavedComponent>,
    wires: Vec<SavedWire>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SavedComponent {
    id: u64,
    kind: ComponentKind,
    x: f32,
    y: f32,
    rotation: i32,
    label: String,
    value: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SavedWire {
    id: u64,
    points: Vec<SavedPoint>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SavedPoint {
    x: f32,
    y: f32,
}

impl CircuitApp {
    fn new() -> Self {
        Self {
            components: Vec::new(),
            wires: Vec::new(),
            tool: Tool::Select,
            selected: None,
            drag: None,
            draft_wire: Vec::new(),
            grid: 20.0,
            snap: true,
            orthogonal_wires: true,
            show_pins: true,
            simulate: true,
            status: String::new(),
            next_id: 1,
            counters: Counters::default(),
            history: Vec::new(),
            redo_history: Vec::new(),
            dirty: false,
        }
    }

    fn snapshot(&self) -> CircuitSnapshot {
        CircuitSnapshot {
            components: self.components.clone(),
            wires: self.wires.clone(),
            next_id: self.next_id,
            counters: self.counters.clone(),
        }
    }

    fn restore_snapshot(&mut self, snapshot: CircuitSnapshot) {
        self.components = snapshot.components;
        self.wires = snapshot.wires;
        self.next_id = snapshot.next_id;
        self.counters = snapshot.counters;
        self.selected = None;
        self.drag = None;
        self.draft_wire.clear();
    }

    fn record_history(&mut self) {
        self.history.push(self.snapshot());
        if self.history.len() > 80 {
            self.history.remove(0);
        }
        self.redo_history.clear();
        self.dirty = true;
    }

    fn undo(&mut self) {
        let Some(snapshot) = self.history.pop() else {
            self.status = "Nothing to undo.".to_string();
            return;
        };
        self.redo_history.push(self.snapshot());
        self.restore_snapshot(snapshot);
        self.dirty = true;
        self.status = "Undo.".to_string();
    }

    fn redo(&mut self) {
        let Some(snapshot) = self.redo_history.pop() else {
            self.status = "Nothing to redo.".to_string();
            return;
        };
        self.history.push(self.snapshot());
        self.restore_snapshot(snapshot);
        self.dirty = true;
        self.status = "Redo.".to_string();
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn next_label(&mut self, kind: ComponentKind) -> String {
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
        }
    }

    fn default_value(kind: ComponentKind) -> String {
        match kind {
            ComponentKind::Resistor => "10k".to_string(),
            ComponentKind::Capacitor => "100nF".to_string(),
            ComponentKind::Inductor => "10uH".to_string(),
            ComponentKind::Diode => "1N4148".to_string(),
            ComponentKind::Led => "red".to_string(),
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
        }
    }

    fn add_component(&mut self, kind: ComponentKind, pos: Pos2) {
        self.record_history();
        self.place_component(kind, pos);
        self.status = "Component placed. Drag to reposition, R to rotate.".to_string();
    }

    fn place_component(&mut self, kind: ComponentKind, pos: Pos2) -> u64 {
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

    fn add_wire(&mut self, points: Vec<Pos2>) {
        let points = simplify_wire(points);
        if points.len() < 2 {
            return;
        }
        self.record_history();
        let id = self.next_id();
        self.wires.push(Wire { id, points });
        self.status = "Wire placed.".to_string();
    }

    fn reset_canvas(&mut self) {
        self.record_history();
        self.components.clear();
        self.wires.clear();
        self.selected = None;
        self.drag = None;
        self.draft_wire.clear();
        self.counters = Counters::default();
        self.next_id = 1;
        self.tool = Tool::Select;
        self.dirty = true;
    }

    fn add_wire_between(&mut self, a_id: u64, a_pin: &str, b_id: u64, b_pin: &str) {
        let Some(a) = self.pin_pos(a_id, a_pin) else {
            return;
        };
        let Some(b) = self.pin_pos(b_id, b_pin) else {
            return;
        };
        let corner = if (a.x - b.x).abs() >= (a.y - b.y).abs() {
            Pos2::new(b.x, a.y)
        } else {
            Pos2::new(a.x, b.y)
        };
        self.add_wire(vec![a, corner, b]);
    }

    fn pin_pos(&self, component_id: u64, label: &str) -> Option<Pos2> {
        let component = self
            .components
            .iter()
            .find(|component| component.id == component_id)?;
        component_pin_defs(component)
            .into_iter()
            .find(|pin| pin.label == label || pin.label.contains(label))
            .map(|pin| pin.pos)
    }

    fn load_led_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(180.0, 390.0));
        let resistor = self.place_component(ComponentKind::Resistor, Pos2::new(360.0, 220.0));
        let led = self.place_component(ComponentKind::Led, Pos2::new(540.0, 220.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(720.0, 400.0));

        self.add_wire_between(battery, "+", resistor, "A");
        self.add_wire_between(resistor, "B", led, "A");
        self.add_wire_between(led, "B", ground, "GND");
        self.add_wire_between(battery, "-", ground, "GND");
        self.status = "Loaded LED current-limiting demo.".to_string();
    }

    fn load_esp32_oled_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(170.0, 380.0));
        let esp32 = self.place_component(ComponentKind::Esp32, Pos2::new(430.0, 310.0));
        let oled = self.place_component(ComponentKind::Oled, Pos2::new(720.0, 300.0));

        self.add_wire_between(battery, "+", esp32, "VIN");
        self.add_wire_between(battery, "-", esp32, "GND");
        self.add_wire_between(esp32, "3V3", oled, "VCC");
        self.add_wire_between(esp32, "GND", oled, "GND");
        self.add_wire_between(esp32, "GPIO21", oled, "SDA");
        self.add_wire_between(esp32, "GPIO22", oled, "SCL");
        self.status = "Loaded ESP32 + OLED I2C demo.".to_string();
    }

    fn load_motor_relay_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(170.0, 380.0));
        let button = self.place_component(ComponentKind::PushButton, Pos2::new(360.0, 470.0));
        let relay = self.place_component(ComponentKind::Relay, Pos2::new(560.0, 360.0));
        let motor = self.place_component(ComponentKind::DcMotor, Pos2::new(780.0, 280.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(780.0, 500.0));

        self.add_wire_between(battery, "+", relay, "COIL+");
        self.add_wire_between(relay, "COIL-", button, "A");
        self.add_wire_between(button, "B", ground, "GND");
        self.add_wire_between(battery, "-", ground, "GND");
        self.add_wire_between(battery, "+", motor, "+");
        self.add_wire_between(motor, "-", relay, "COM");
        self.add_wire_between(relay, "NO", ground, "GND");
        self.status = "Loaded relay-controlled motor demo.".to_string();
    }

    fn push_wire_point(&mut self, pos: Pos2) {
        if self.orthogonal_wires {
            if let Some(&last) = self.draft_wire.last() {
                let dx = (pos.x - last.x).abs();
                let dy = (pos.y - last.y).abs();
                if dx > 0.1 && dy > 0.1 {
                    let corner = if dx >= dy {
                        Pos2::new(pos.x, last.y)
                    } else {
                        Pos2::new(last.x, pos.y)
                    };
                    push_unique_point(&mut self.draft_wire, corner);
                }
            }
        }
        push_unique_point(&mut self.draft_wire, pos);
    }

    fn delete_selected(&mut self) {
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

    fn rotate_selected(&mut self) {
        let Some(Selection::Component(id)) = self.selected else {
            return;
        };
        let Some(index) = self.components.iter().position(|c| c.id == id) else {
            return;
        };
        if component_is_module(&self.components[index]) {
            self.status = "Modules stay upright so pin labels remain readable.".to_string();
            return;
        }
        self.record_history();
        if let Some(component) = self.components.get_mut(index) {
            component.rotation = (component.rotation + 90) % 360;
            self.status = "Rotated.".to_string();
        }
    }

    fn duplicate_selected(&mut self) {
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
        duplicate.pos += Vec2::new(40.0, 40.0);
        duplicate.label = self.next_label(duplicate.kind);
        let duplicate_id = duplicate.id;
        self.components.push(duplicate);
        self.selected = Some(Selection::Component(duplicate_id));
        self.status = "Component duplicated.".to_string();
    }

    fn export_svg(&mut self) {
        match fs::write(
            "cluster_circuit.svg",
            circuit_to_svg(&self.components, &self.wires),
        ) {
            Ok(()) => {
                self.status = "Saved cluster_circuit.svg.".to_string();
            }
            Err(err) => {
                self.status = format!("Export failed: {err}");
            }
        }
    }

    fn export_spice_netlist(&mut self) {
        match fs::write(
            "cluster_circuit.cir",
            circuit_to_spice_netlist(&self.components, &self.wires),
        ) {
            Ok(()) => {
                self.status = "Saved cluster_circuit.cir.".to_string();
            }
            Err(err) => {
                self.status = format!("SPICE export failed: {err}");
            }
        }
    }

    fn save_circuit_json(&mut self) {
        let saved = SavedCircuit::from_app(self);
        match serde_json::to_string_pretty(&saved)
            .map_err(|err| err.to_string())
            .and_then(|json| fs::write("cluster_circuit.json", json).map_err(|err| err.to_string()))
        {
            Ok(()) => {
                self.dirty = false;
                self.status = "Saved cluster_circuit.json.".to_string();
            }
            Err(err) => {
                self.status = format!("Save failed: {err}");
            }
        }
    }

    fn load_circuit_json(&mut self) {
        match fs::read_to_string("cluster_circuit.json")
            .map_err(|err| err.to_string())
            .and_then(|json| {
                serde_json::from_str::<SavedCircuit>(&json).map_err(|err| err.to_string())
            })
            .and_then(SavedCircuit::into_snapshot)
        {
            Ok(snapshot) => {
                self.record_history();
                self.restore_snapshot(snapshot);
                self.dirty = false;
                self.status = "Loaded cluster_circuit.json.".to_string();
            }
            Err(err) => {
                self.status = format!("Load failed: {err}");
            }
        }
    }
}

impl SavedCircuit {
    fn from_app(app: &CircuitApp) -> Self {
        Self {
            schema_version: 1,
            next_id: app.next_id,
            counters: app.counters.clone(),
            components: app
                .components
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
                .collect(),
            wires: app
                .wires
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
                .collect(),
        }
    }

    fn into_snapshot(self) -> Result<CircuitSnapshot, String> {
        if self.schema_version > 1 {
            return Err(format!(
                "Unsupported schema version {}.",
                self.schema_version
            ));
        }
        let components = self
            .components
            .into_iter()
            .map(|component| Component {
                id: component.id,
                kind: component.kind,
                pos: Pos2::new(component.x, component.y),
                rotation: component.rotation.rem_euclid(360),
                label: component.label,
                value: component.value,
            })
            .collect::<Vec<_>>();
        let wires = self
            .wires
            .into_iter()
            .map(|wire| Wire {
                id: wire.id,
                points: wire
                    .points
                    .into_iter()
                    .map(|point| Pos2::new(point.x, point.y))
                    .collect(),
            })
            .collect::<Vec<_>>();
        let max_id = components
            .iter()
            .map(|component| component.id)
            .chain(wires.iter().map(|wire| wire.id))
            .max()
            .unwrap_or(0);
        Ok(CircuitSnapshot {
            components,
            wires,
            next_id: self.next_id.max(max_id + 1),
            counters: self.counters,
        })
    }
}

impl eframe::App for CircuitApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_app_style(ctx);

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("Cluster")
                        .size(18.0)
                        .strong()
                        .color(Color32::from_rgb(245, 248, 252)),
                );
                ui.label(
                    egui::RichText::new("workbench")
                        .size(12.0)
                        .color(Color32::from_rgb(160, 170, 180)),
                );
                ui.separator();
                if tool_button(ui, self.tool == Tool::Select, "Select").clicked() {
                    self.tool = Tool::Select;
                    self.draft_wire.clear();
                }
                if tool_button(ui, self.tool == Tool::Wire, "Wire").clicked() {
                    self.tool = Tool::Wire;
                    self.draft_wire.clear();
                }
                ui.separator();
                if compact_button(ui, "Undo").clicked() {
                    self.undo();
                }
                if compact_button(ui, "Redo").clicked() {
                    self.redo();
                }
                if compact_button(ui, "Rotate").clicked() {
                    self.rotate_selected();
                }
                if compact_button(ui, "Duplicate").clicked() {
                    self.duplicate_selected();
                }
                if compact_button(ui, "Delete").clicked() {
                    self.delete_selected();
                }
                ui.separator();
                toolbar_menu(ui, "View", |ui| {
                    ui.checkbox(&mut self.snap, "Snap to grid");
                    ui.checkbox(&mut self.orthogonal_wires, "90 degree wires");
                    ui.checkbox(&mut self.show_pins, "Show pins");
                    ui.checkbox(&mut self.simulate, "Live check");
                    ui.add_sized(
                        Vec2::new(180.0, 18.0),
                        egui::Slider::new(&mut self.grid, 10.0..=40.0).text("Grid"),
                    );
                });
                toolbar_menu(ui, "Actions", |ui| {
                    if menu_action(ui, "Save JSON").clicked() {
                        self.save_circuit_json();
                        ui.close();
                    }
                    if menu_action(ui, "Load JSON").clicked() {
                        self.load_circuit_json();
                        ui.close();
                    }
                    ui.separator();
                    if menu_action(ui, "Export SVG").clicked() {
                        self.export_svg();
                        ui.close();
                    }
                    if menu_action(ui, "Export CIR").clicked() {
                        self.export_spice_netlist();
                        ui.close();
                    }
                    ui.separator();
                    if menu_action(ui, "Blank schematic").clicked() {
                        self.reset_canvas();
                        self.status = "Blank schematic ready.".to_string();
                        ui.close();
                    }
                });
            });
            if !self.status.is_empty() {
                ui.label(
                    egui::RichText::new(&self.status)
                        .size(12.0)
                        .color(Color32::from_rgb(210, 218, 226)),
                );
            }
            ui.add_space(4.0);
        });

        egui::SidePanel::left("palette")
            .default_width(220.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Parts");
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        part_section(
                            ui,
                            self,
                            "Passives",
                            SectionMode::Open,
                            &[
                                ("Resistor", ComponentKind::Resistor),
                                ("Capacitor", ComponentKind::Capacitor),
                                ("Inductor", ComponentKind::Inductor),
                                ("Lamp", ComponentKind::Lamp),
                            ],
                        );
                        part_section(
                            ui,
                            self,
                            "Semiconductors",
                            SectionMode::Open,
                            &[
                                ("Diode", ComponentKind::Diode),
                                ("LED", ComponentKind::Led),
                                ("Op Amp", ComponentKind::OpAmp),
                            ],
                        );
                        part_section(
                            ui,
                            self,
                            "Sources and IO",
                            SectionMode::Open,
                            &[
                                ("Ground", ComponentKind::Ground),
                                ("Voltage Source", ComponentKind::VSource),
                                ("Current Source", ComponentKind::ISource),
                                ("Battery", ComponentKind::Battery),
                                ("Switch", ComponentKind::Switch),
                                ("Push Button", ComponentKind::PushButton),
                                ("Slide Switch", ComponentKind::SlideSwitch),
                            ],
                        );
                        part_section(
                            ui,
                            self,
                            "Modules",
                            SectionMode::Collapsed,
                            &[
                                ("ESP32 WROOM", ComponentKind::Esp32),
                                ("ESP32-S3", ComponentKind::Esp32S3),
                                ("ESP32-C3", ComponentKind::Esp32C3),
                                ("Arduino UNO", ComponentKind::ArduinoUno),
                                ("Pi Pico", ComponentKind::RaspberryPiPico),
                                ("Breadboard", ComponentKind::Breadboard),
                                ("OLED I2C", ComponentKind::Oled),
                                ("Sensor", ComponentKind::Sensor),
                            ],
                        );
                        part_section(
                            ui,
                            self,
                            "Actuators",
                            SectionMode::Collapsed,
                            &[
                                ("Relay", ComponentKind::Relay),
                                ("DC Motor", ComponentKind::DcMotor),
                                ("Servo", ComponentKind::Servo),
                            ],
                        );

                        palette_section(ui, "Examples", SectionMode::Open, |ui| {
                            if palette_action(ui, "LED Circuit").clicked() {
                                self.load_led_demo();
                            }
                            if palette_action(ui, "ESP32 + OLED").clicked() {
                                self.load_esp32_oled_demo();
                            }
                            if palette_action(ui, "Relay + Motor").clicked() {
                                self.load_motor_relay_demo();
                            }
                            if palette_action(ui, "Blank").clicked() {
                                self.reset_canvas();
                                self.status = "Blank schematic ready.".to_string();
                            }
                        });

                        palette_section(ui, "Circuit", SectionMode::Open, |ui| {
                            metric_row(ui, "Parts", self.components.len().to_string());
                            metric_row(ui, "Wires", self.wires.len().to_string());
                            if self.simulate {
                                let simulation = analyze_circuit(&self.components, &self.wires);
                                let tone = if simulation.shorted {
                                    StatusTone::Error
                                } else if simulation.closed {
                                    StatusTone::Live
                                } else {
                                    StatusTone::Muted
                                };
                                status_pill(ui, &simulation.summary, tone);
                                if let Some(voltage) = simulation.voltage {
                                    metric_row(ui, "Voltage", format!("{:.2} V", voltage));
                                }
                                if let Some(resistance) = simulation.resistance {
                                    metric_row(ui, "Resistance", format_resistance(resistance));
                                }
                                if let Some(current) = simulation.current {
                                    metric_row(ui, "Current", format_current(current));
                                }
                                for detail in simulation.details.iter().take(4) {
                                    ui.label(
                                        egui::RichText::new(detail)
                                            .size(11.0)
                                            .color(Color32::from_rgb(150, 160, 170)),
                                    );
                                }
                            }
                        });

                        palette_section(ui, "Shortcuts", SectionMode::Collapsed, |ui| {
                            metric_row(ui, "R", "rotate");
                            metric_row(ui, "Del", "delete");
                            metric_row(ui, "Enter", "finish wire");
                            metric_row(ui, "Esc", "select");
                        });
                    });
            });

        egui::SidePanel::right("inspector")
            .default_width(248.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Inspector");
                ui.separator();
                match self.selected {
                    Some(Selection::Component(id)) => {
                        if let Some(component) = self.components.iter_mut().find(|c| c.id == id) {
                            status_pill(
                                ui,
                                component_kind_label(component.kind),
                                StatusTone::Neutral,
                            );
                            ui.add_space(8.0);
                            if edit_row(ui, "Label", &mut component.label)
                                || edit_row(ui, "Value", &mut component.value)
                            {
                                self.dirty = true;
                            }
                            if component_is_switch(component.kind) {
                                let mut closed =
                                    component_conductance(component) != Conductance::Open;
                                if ui.checkbox(&mut closed, "Closed").changed() {
                                    component.value = if closed {
                                        "closed".to_string()
                                    } else {
                                        "open".to_string()
                                    };
                                }
                            }
                            metric_row(ui, "Rotation", format!("{}°", component.rotation));
                            metric_row(
                                ui,
                                "Position",
                                format!("{:.0}, {:.0}", component.pos.x, component.pos.y),
                            );
                            if component_is_module(component) {
                                ui.add_space(8.0);
                                section_title(ui, "Pins");
                                for pin in component_pin_defs(component) {
                                    metric_row(ui, pin.label, format!("{:?}", pin.role));
                                }
                            }
                        }
                    }
                    Some(Selection::Wire(id)) => {
                        if let Some(wire) = self.wires.iter().find(|w| w.id == id) {
                            status_pill(ui, "Wire", StatusTone::Neutral);
                            ui.add_space(8.0);
                            metric_row(ui, "Points", wire.points.len().to_string());
                            metric_row(ui, "Length", format!("{:.0}px", wire_length(wire)));
                        }
                    }
                    None => {
                        ui.label(
                            egui::RichText::new("Nothing selected")
                                .color(Color32::from_rgb(145, 154, 164)),
                        );
                    }
                }
            });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.monospace(format!("Tool: {:?}", self.tool));
                ui.separator();
                ui.monospace(format!("Grid: {:.0}px", self.grid));
                ui.separator();
                ui.label(selection_summary(
                    self.selected,
                    &self.components,
                    &self.wires,
                ));
                ui.separator();
                ui.colored_label(
                    if self.dirty {
                        Color32::from_rgb(255, 198, 92)
                    } else {
                        Color32::from_rgb(138, 190, 145)
                    },
                    if self.dirty { "Unsaved" } else { "Saved" },
                );
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let (response, painter) = ui.allocate_painter(available, Sense::click_and_drag());
            let rect = response.rect;
            let simulation = if self.simulate {
                analyze_circuit(&self.components, &self.wires)
            } else {
                Simulation::default()
            };
            let flow_phase = ctx.input(|i| i.time) as f32 * 90.0;
            let show_flow = self.simulate && simulation.closed && !simulation.shorted;
            if show_flow {
                ctx.request_repaint();
            }

            draw_grid(&painter, rect, self.grid);
            if self.components.is_empty() && self.wires.is_empty() {
                draw_empty_canvas_hint(&painter, rect);
            }

            for wire in &self.wires {
                let energized = simulation.energized_wires.contains(&wire.id);
                draw_wire(
                    &painter,
                    wire,
                    self.selected == Some(Selection::Wire(wire.id)),
                    energized,
                    show_flow && energized,
                    flow_phase,
                );
            }

            for component in &self.components {
                draw_component(
                    &painter,
                    component,
                    self.selected == Some(Selection::Component(component.id)),
                    self.show_pins,
                    simulation.energized_components.contains(&component.id),
                );
            }

            draw_junctions(&painter, &self.wires);
            draw_title_block(&painter, rect, &self.components, &self.wires, &simulation);

            let hover_pos = ui.input(|i| i.pointer.hover_pos());
            let pointer_in_rect = hover_pos.filter(|pos| rect.contains(*pos));

            if let Some(pos) = pointer_in_rect {
                let mut pos = snap_pos(pos, rect, self.grid, self.snap);
                if self.tool == Tool::Wire {
                    pos = snap_to_nearest_pin(pos, &self.components).unwrap_or(pos);
                }
                if self.tool == Tool::Wire && !self.draft_wire.is_empty() {
                    let preview = preview_wire_points(&self.draft_wire, pos, self.orthogonal_wires);
                    draw_wire_preview(&painter, &preview);
                }
            }

            if response.clicked_by(egui::PointerButton::Primary) {
                if let Some(raw_pos) = pointer_in_rect {
                    let pos = snap_pos(raw_pos, rect, self.grid, self.snap);
                    match self.tool {
                        Tool::Select => {
                            if let Some(selection) =
                                hit_test(raw_pos, &self.components, &self.wires)
                            {
                                self.selected = Some(selection);
                            } else {
                                self.selected = None;
                            }
                        }
                        Tool::Place(kind) => {
                            self.add_component(kind, pos);
                        }
                        Tool::Wire => {
                            let pos = snap_to_nearest_pin(pos, &self.components).unwrap_or(pos);
                            self.push_wire_point(pos);
                        }
                    }
                }
            }

            if response.clicked_by(egui::PointerButton::Secondary) {
                if self.tool == Tool::Wire {
                    if !self.draft_wire.is_empty() {
                        let points = std::mem::take(&mut self.draft_wire);
                        self.add_wire(points);
                    }
                }
            }

            if response.double_clicked() {
                if self.tool == Tool::Wire && self.draft_wire.len() >= 2 {
                    let points = std::mem::take(&mut self.draft_wire);
                    self.add_wire(points);
                }
            }

            if response.drag_started() {
                if self.tool == Tool::Select {
                    if let Some(pos) = pointer_in_rect {
                        if let Some((wire_id, point_index)) =
                            hit_test_wire_control_point(pos, &self.wires)
                        {
                            self.record_history();
                            self.drag = Some(DragState::WirePoint {
                                wire_id,
                                point_index,
                            });
                            self.selected = Some(Selection::Wire(wire_id));
                        } else {
                            self.record_history();
                            if let Some((wire_id, point_index)) =
                                insert_wire_control_point(pos, &mut self.wires)
                            {
                                self.drag = Some(DragState::WirePoint {
                                    wire_id,
                                    point_index,
                                });
                                self.selected = Some(Selection::Wire(wire_id));
                            } else if let Some(Selection::Component(id)) =
                                hit_test_component(pos, &self.components)
                            {
                                if let Some(component) = self.components.iter().find(|c| c.id == id)
                                {
                                    self.drag = Some(DragState::Component {
                                        id,
                                        offset: pos - component.pos,
                                    });
                                    self.selected = Some(Selection::Component(id));
                                }
                            } else {
                                let _ = self.history.pop();
                            }
                        }
                    }
                }
            }

            if response.dragged() {
                if let (Some(drag), Some(pos)) = (self.drag.clone(), pointer_in_rect) {
                    match drag {
                        DragState::Component { id, offset } => {
                            if let Some(index) = self.components.iter().position(|c| c.id == id) {
                                let old_pins = component_pins(&self.components[index]);
                                let pos = snap_pos(pos, rect, self.grid, self.snap);
                                self.components[index].pos = pos - offset;
                                let new_pins = component_pins(&self.components[index]);
                                move_attached_wire_endpoints(&mut self.wires, &old_pins, &new_pins);
                            }
                        }
                        DragState::WirePoint {
                            wire_id,
                            point_index,
                        } => {
                            let pos = snap_pos(pos, rect, self.grid, self.snap);
                            move_wire_control_point(&mut self.wires, wire_id, point_index, pos);
                        }
                    }
                }
            }

            let primary_down = ctx.input(|i| i.pointer.primary_down());
            if !primary_down {
                self.drag = None;
            }
        });

        let delete_pressed = ctx.input(|i| i.key_pressed(egui::Key::Delete))
            || ctx.input(|i| i.key_pressed(egui::Key::Backspace));
        if delete_pressed {
            self.delete_selected();
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.tool = Tool::Select;
            self.draft_wire.clear();
        }

        if ctx.input(|i| i.key_pressed(egui::Key::R)) {
            self.rotate_selected();
        }

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z)) {
            self.undo();
        }

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Y)) {
            self.redo();
        }

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::D)) {
            self.duplicate_selected();
        }

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            self.save_circuit_json();
        }

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::O)) {
            self.load_circuit_json();
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            if self.tool == Tool::Wire && self.draft_wire.len() >= 2 {
                let points = std::mem::take(&mut self.draft_wire);
                self.add_wire(points);
            }
        }
    }
}

fn snap_pos(pos: Pos2, rect: Rect, grid: f32, snap: bool) -> Pos2 {
    let mut pos = pos;
    if snap {
        let x = ((pos.x - rect.left()) / grid).round() * grid + rect.left();
        let y = ((pos.y - rect.top()) / grid).round() * grid + rect.top();
        pos = Pos2::new(x, y);
    }
    pos
}

fn hit_test(pos: Pos2, components: &[Component], wires: &[Wire]) -> Option<Selection> {
    if let Some(selection) = hit_test_component(pos, components) {
        return Some(selection);
    }
    if let Some(selection) = hit_test_wire(pos, wires) {
        return Some(selection);
    }
    None
}

fn selection_summary(
    selected: Option<Selection>,
    components: &[Component],
    wires: &[Wire],
) -> String {
    match selected {
        Some(Selection::Component(id)) => components
            .iter()
            .find(|component| component.id == id)
            .map(|component| format!("Selected: {}", component.label))
            .unwrap_or_else(|| "Selected: missing component".to_string()),
        Some(Selection::Wire(id)) => wires
            .iter()
            .find(|wire| wire.id == id)
            .map(|wire| format!("Selected: wire {:.0}px", wire_length(wire)))
            .unwrap_or_else(|| "Selected: missing wire".to_string()),
        None => "Selected: none".to_string(),
    }
}

fn hit_test_component(pos: Pos2, components: &[Component]) -> Option<Selection> {
    for component in components.iter().rev() {
        if component_bounds(component).contains(pos) {
            return Some(Selection::Component(component.id));
        }
    }
    None
}

fn hit_test_wire(pos: Pos2, wires: &[Wire]) -> Option<Selection> {
    let threshold = 10.0;
    for wire in wires.iter().rev() {
        for segment in wire.points.windows(2) {
            let a = segment[0];
            let b = segment[1];
            if distance_to_segment(pos, a, b) <= threshold {
                return Some(Selection::Wire(wire.id));
            }
        }
    }
    None
}

fn hit_test_wire_control_point(pos: Pos2, wires: &[Wire]) -> Option<(u64, usize)> {
    let threshold = 9.0;
    for wire in wires.iter().rev() {
        for (index, point) in wire.points.iter().enumerate() {
            if pos.distance(*point) <= threshold {
                return Some((wire.id, index));
            }
        }
    }
    None
}

fn insert_wire_control_point(pos: Pos2, wires: &mut [Wire]) -> Option<(u64, usize)> {
    let threshold = 10.0;
    for wire in wires.iter_mut().rev() {
        for index in 0..wire.points.len().saturating_sub(1) {
            let a = wire.points[index];
            let b = wire.points[index + 1];
            if distance_to_segment(pos, a, b) <= threshold {
                let horizontal = (a.y - b.y).abs() <= 0.5;
                let vertical = (a.x - b.x).abs() <= 0.5;
                let inserted = if horizontal {
                    Pos2::new(pos.x.clamp(a.x.min(b.x), a.x.max(b.x)), a.y)
                } else if vertical {
                    Pos2::new(a.x, pos.y.clamp(a.y.min(b.y), a.y.max(b.y)))
                } else {
                    closest_point_on_segment(pos, a, b)
                };
                wire.points.insert(index + 1, inserted);
                return Some((wire.id, index + 1));
            }
        }
    }
    None
}

fn move_wire_control_point(wires: &mut [Wire], wire_id: u64, point_index: usize, pos: Pos2) {
    let Some(wire) = wires.iter_mut().find(|wire| wire.id == wire_id) else {
        return;
    };
    if point_index >= wire.points.len() {
        return;
    }
    wire.points[point_index] = pos;
    straighten_neighbor_segments(wire, point_index);
}

fn straighten_neighbor_segments(wire: &mut Wire, point_index: usize) {
    let point = wire.points[point_index];
    if point_index > 0 {
        let prev = wire.points[point_index - 1];
        let dx = (point.x - prev.x).abs();
        let dy = (point.y - prev.y).abs();
        if dx <= dy {
            wire.points[point_index - 1].x = point.x;
        } else {
            wire.points[point_index - 1].y = point.y;
        }
    }
    if point_index + 1 < wire.points.len() {
        let next = wire.points[point_index + 1];
        let dx = (point.x - next.x).abs();
        let dy = (point.y - next.y).abs();
        if dx <= dy {
            wire.points[point_index + 1].x = point.x;
        } else {
            wire.points[point_index + 1].y = point.y;
        }
    }
}

fn snap_to_nearest_pin(pos: Pos2, components: &[Component]) -> Option<Pos2> {
    let mut best = None;
    let mut best_distance = 14.0;
    for component in components {
        for pin in component_pin_defs(component) {
            let distance = pin.pos.distance(pos);
            if distance <= best_distance {
                best_distance = distance;
                best = Some(pin.pos);
            }
        }
    }
    best
}

fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let ab_len_sq = ab.x * ab.x + ab.y * ab.y;
    if ab_len_sq == 0.0 {
        return ap.length();
    }
    let t = ((ap.x * ab.x) + (ap.y * ab.y)) / ab_len_sq;
    let t = t.clamp(0.0, 1.0);
    let closest = a + ab * t;
    (p - closest).length()
}

fn closest_point_on_segment(p: Pos2, a: Pos2, b: Pos2) -> Pos2 {
    let ab = b - a;
    let ap = p - a;
    let ab_len_sq = ab.x * ab.x + ab.y * ab.y;
    if ab_len_sq == 0.0 {
        return a;
    }
    let t = ((ap.x * ab.x) + (ap.y * ab.y)) / ab_len_sq;
    a + ab * t.clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Copy)]
enum StatusTone {
    Neutral,
    Muted,
    Live,
    Error,
}

fn apply_app_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = Color32::from_rgb(18, 21, 26);
    visuals.panel_fill = Color32::from_rgb(18, 21, 26);
    visuals.extreme_bg_color = Color32::from_rgb(12, 14, 18);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(31, 36, 43);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(43, 50, 59);
    visuals.widgets.active.bg_fill = Color32::from_rgb(46, 58, 68);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(52, 58, 66));
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.visuals = ctx.style().visuals.clone();
    ctx.set_style(style);
}

fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(11.0)
            .strong()
            .color(Color32::from_rgb(138, 149, 160)),
    );
}

fn toolbar_menu(ui: &mut egui::Ui, label: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.menu_button(
        egui::RichText::new(format!("{label} v"))
            .strong()
            .color(Color32::from_rgb(230, 236, 242)),
        add_contents,
    );
}

fn tool_button(ui: &mut egui::Ui, active: bool, label: &str) -> egui::Response {
    let (fill, stroke, text) = if active {
        (
            Color32::from_rgb(38, 70, 82),
            Stroke::new(1.0, Color32::from_rgb(105, 178, 255)),
            Color32::from_rgb(235, 246, 255),
        )
    } else {
        (
            Color32::from_rgb(30, 34, 40),
            Stroke::new(1.0, Color32::from_rgb(52, 60, 68)),
            Color32::from_rgb(190, 198, 206),
        )
    };
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(text))
            .fill(fill)
            .stroke(stroke)
            .min_size(Vec2::new(72.0, 28.0)),
    )
}

fn compact_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(Color32::from_rgb(215, 222, 230)))
            .fill(Color32::from_rgb(31, 36, 43))
            .stroke(Stroke::new(1.0, Color32::from_rgb(56, 64, 74)))
            .min_size(Vec2::new(74.0, 26.0)),
    )
}

fn palette_action(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add_sized(
        Vec2::new(ui.available_width(), 27.0),
        egui::Button::new(egui::RichText::new(label).color(Color32::from_rgb(216, 224, 232)))
            .fill(Color32::from_rgb(28, 33, 39))
            .stroke(Stroke::new(1.0, Color32::from_rgb(48, 56, 64))),
    )
}

fn menu_action(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add_sized(
        Vec2::new(180.0, 27.0),
        egui::Button::new(egui::RichText::new(label).color(Color32::from_rgb(216, 224, 232)))
            .fill(Color32::from_rgb(28, 33, 39))
            .stroke(Stroke::new(1.0, Color32::from_rgb(48, 56, 64))),
    )
}

fn status_pill(ui: &mut egui::Ui, text: &str, tone: StatusTone) {
    let (fill, stroke, color) = match tone {
        StatusTone::Neutral => (
            Color32::from_rgb(30, 36, 43),
            Color32::from_rgb(58, 68, 78),
            Color32::from_rgb(210, 218, 226),
        ),
        StatusTone::Muted => (
            Color32::from_rgb(28, 31, 36),
            Color32::from_rgb(62, 68, 76),
            Color32::from_rgb(152, 162, 172),
        ),
        StatusTone::Live => (
            Color32::from_rgb(54, 42, 22),
            Color32::from_rgb(132, 92, 34),
            Color32::from_rgb(255, 198, 92),
        ),
        StatusTone::Error => (
            Color32::from_rgb(58, 30, 30),
            Color32::from_rgb(142, 64, 58),
            Color32::from_rgb(255, 128, 112),
        ),
    };
    egui::Frame::NONE
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(11.0).strong().color(color));
        });
}

fn metric_row(ui: &mut egui::Ui, label: impl Into<String>, value: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new(label.into())
                .size(11.0)
                .color(Color32::from_rgb(135, 146, 156)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value.into())
                    .size(11.0)
                    .color(Color32::from_rgb(210, 218, 226)),
            );
        });
    });
}

fn edit_row(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    ui.label(
        egui::RichText::new(label)
            .size(11.0)
            .color(Color32::from_rgb(135, 146, 156)),
    );
    ui.add_sized(
        Vec2::new(ui.available_width(), 25.0),
        egui::TextEdit::singleline(value)
            .text_color(Color32::from_rgb(230, 235, 240))
            .background_color(Color32::from_rgb(12, 15, 19)),
    )
    .changed()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SectionMode {
    Open,
    Collapsed,
}

fn part_section(
    ui: &mut egui::Ui,
    app: &mut CircuitApp,
    title: &str,
    mode: SectionMode,
    parts: &[(&str, ComponentKind)],
) {
    palette_section(ui, title, mode, |ui| {
        for (label, kind) in parts {
            part_button(ui, app, label, *kind);
        }
    });
}

fn palette_section(
    ui: &mut egui::Ui,
    title: &str,
    mode: SectionMode,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    ui.add_space(5.0);
    egui::Frame::NONE
        .fill(Color32::from_rgb(23, 28, 35))
        .stroke(Stroke::new(1.0, Color32::from_rgb(58, 68, 80)))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(7, 5))
        .show(ui, |ui| {
            let title = egui::RichText::new(title.to_uppercase())
                .size(11.0)
                .strong()
                .color(Color32::from_rgb(190, 204, 218));
            match mode {
                SectionMode::Open => {
                    ui.label(title);
                    ui.add_space(5.0);
                    add_contents(ui);
                }
                SectionMode::Collapsed => {
                    egui::CollapsingHeader::new(title)
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.add_space(4.0);
                            add_contents(ui);
                        });
                }
            }
        });
}

fn part_button(ui: &mut egui::Ui, app: &mut CircuitApp, label: &str, kind: ComponentKind) {
    let selected = app.tool == Tool::Place(kind);
    let (fill, stroke, color) = if selected {
        (
            Color32::from_rgb(38, 70, 82),
            Stroke::new(1.0, Color32::from_rgb(105, 178, 255)),
            Color32::from_rgb(235, 246, 255),
        )
    } else {
        (
            Color32::from_rgb(25, 29, 35),
            Stroke::new(1.0, Color32::from_rgb(43, 50, 58)),
            Color32::from_rgb(198, 207, 216),
        )
    };
    let response = ui.add_sized(
        Vec2::new(ui.available_width(), 27.0),
        egui::Button::new(egui::RichText::new(label).color(color))
            .fill(fill)
            .stroke(stroke),
    );
    if response.clicked() {
        app.tool = Tool::Place(kind);
        app.draft_wire.clear();
        app.status = format!("Placing {label}. Click the canvas.");
    }
}

fn push_unique_point(points: &mut Vec<Pos2>, pos: Pos2) {
    if points.last().is_some_and(|last| last.distance(pos) < 0.5) {
        return;
    }
    points.push(pos);
}

fn move_attached_wire_endpoints(wires: &mut [Wire], old_pins: &[Pos2], new_pins: &[Pos2]) {
    for wire in wires {
        if wire.points.is_empty() {
            continue;
        }

        if let Some(new_pos) = moved_pin_for_point(wire.points[0], old_pins, new_pins) {
            wire.points[0] = new_pos;
            keep_wire_end_orthogonal(wire, true);
        }

        let last_index = wire.points.len() - 1;
        if let Some(new_pos) = moved_pin_for_point(wire.points[last_index], old_pins, new_pins) {
            wire.points[last_index] = new_pos;
            keep_wire_end_orthogonal(wire, false);
        }
    }
}

fn moved_pin_for_point(point: Pos2, old_pins: &[Pos2], new_pins: &[Pos2]) -> Option<Pos2> {
    old_pins
        .iter()
        .zip(new_pins)
        .find(|(old_pin, _)| point.distance(**old_pin) <= 12.0)
        .map(|(_, &new_pin)| new_pin)
}

fn keep_wire_end_orthogonal(wire: &mut Wire, first: bool) {
    if wire.points.len() < 2 {
        return;
    }
    let (end_index, neighbor_index) = if first {
        (0, 1)
    } else {
        (wire.points.len() - 1, wire.points.len() - 2)
    };
    let end = wire.points[end_index];
    let neighbor = wire.points[neighbor_index];
    let dx = (end.x - neighbor.x).abs();
    let dy = (end.y - neighbor.y).abs();
    if dx <= dy {
        wire.points[neighbor_index].x = end.x;
    } else {
        wire.points[neighbor_index].y = end.y;
    }
}

fn simplify_wire(points: Vec<Pos2>) -> Vec<Pos2> {
    let mut deduped = Vec::new();
    for point in points {
        push_unique_point(&mut deduped, point);
    }

    let mut simplified: Vec<Pos2> = Vec::new();
    for point in deduped {
        simplified.push(point);
        while simplified.len() >= 3 {
            let len = simplified.len();
            let a = simplified[len - 3];
            let b = simplified[len - 2];
            let c = simplified[len - 1];
            let horizontal = (a.y - b.y).abs() < 0.5 && (b.y - c.y).abs() < 0.5;
            let vertical = (a.x - b.x).abs() < 0.5 && (b.x - c.x).abs() < 0.5;
            if horizontal || vertical {
                simplified.remove(len - 2);
            } else {
                break;
            }
        }
    }
    simplified
}

fn preview_wire_points(points: &[Pos2], cursor: Pos2, orthogonal: bool) -> Vec<Pos2> {
    let mut preview = points.to_vec();
    if orthogonal {
        if let Some(&last) = preview.last() {
            let dx = (cursor.x - last.x).abs();
            let dy = (cursor.y - last.y).abs();
            if dx > 0.1 && dy > 0.1 {
                let corner = if dx >= dy {
                    Pos2::new(cursor.x, last.y)
                } else {
                    Pos2::new(last.x, cursor.y)
                };
                push_unique_point(&mut preview, corner);
            }
        }
    }
    push_unique_point(&mut preview, cursor);
    preview
}

fn wire_length(wire: &Wire) -> f32 {
    wire.points
        .windows(2)
        .map(|segment| segment[0].distance(segment[1]))
        .sum()
}

#[derive(Default)]
struct Simulation {
    closed: bool,
    shorted: bool,
    energized_components: HashSet<u64>,
    energized_wires: HashSet<u64>,
    summary: String,
    details: Vec<String>,
    voltage: Option<f32>,
    resistance: Option<f32>,
    current: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Conductance {
    Open,
    Conductor,
    Load,
}

fn analyze_circuit(components: &[Component], wires: &[Wire]) -> Simulation {
    let mut nodes = CircuitNodes::default();
    let mut graph: Vec<HashSet<usize>> = Vec::new();
    let mut wire_graph: Vec<HashSet<usize>> = Vec::new();
    let mut positive_nodes = Vec::new();
    let mut return_nodes = Vec::new();
    let mut component_edges = Vec::new();
    let mut powered_module_edges = Vec::new();
    let mut wire_edges = Vec::new();

    for wire in wires {
        for segment in wire.points.windows(2) {
            let a = nodes.node_for(segment[0]);
            let b = nodes.node_for(segment[1]);
            connect(&mut graph, a, b);
            connect(&mut wire_graph, a, b);
            wire_edges.push((wire.id, a, b));
        }
    }

    for component in components {
        let pins = component_pin_defs(component);
        let pin_nodes: Vec<usize> = pins.iter().map(|pin| nodes.node_for(pin.pos)).collect();
        match component.kind {
            ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource => {
                for (pin, &node) in pins.iter().zip(&pin_nodes) {
                    match pin.role {
                        PinRole::Positive => positive_nodes.push(node),
                        PinRole::Ground => return_nodes.push(node),
                        _ => {}
                    }
                }
                if positive_nodes.is_empty() && return_nodes.is_empty() && pin_nodes.len() >= 2 {
                    return_nodes.push(pin_nodes[0]);
                    positive_nodes.push(pin_nodes[1]);
                }
            }
            ComponentKind::Ground => {
                for &node in &pin_nodes {
                    return_nodes.push(node);
                }
            }
            _ => {
                let conductance = component_conductance(component);
                if conductance != Conductance::Open && pin_nodes.len() >= 2 {
                    let a = pin_nodes[0];
                    let b = pin_nodes[1];
                    connect(&mut graph, a, b);
                    component_edges.push((component.id, a, b, conductance == Conductance::Load));
                } else if component_is_powered_module(component) {
                    let positives: Vec<usize> = pins
                        .iter()
                        .zip(&pin_nodes)
                        .filter(|(pin, _)| pin.role == PinRole::Positive)
                        .map(|(_, &node)| node)
                        .collect();
                    let grounds: Vec<usize> = pins
                        .iter()
                        .zip(&pin_nodes)
                        .filter(|(pin, _)| pin.role == PinRole::Ground)
                        .map(|(_, &node)| node)
                        .collect();
                    for &positive in &positives {
                        for &ground in &grounds {
                            connect(&mut graph, positive, ground);
                            powered_module_edges.push((component.id, positive, ground));
                        }
                    }
                }
            }
        }
    }

    let mut details = validate_i2c_links(components, &nodes, &wire_graph);

    if positive_nodes.is_empty() || return_nodes.is_empty() {
        details.push("Add a source/battery and GND return to run live simulation.".to_string());
        return Simulation {
            summary: "No source or return".to_string(),
            details,
            ..Simulation::default()
        };
    }

    let from_positive = reachable_nodes(&graph, &positive_nodes);
    let from_return = reachable_nodes(&graph, &return_nodes);
    let loop_nodes: HashSet<usize> = from_positive.intersection(&from_return).copied().collect();
    if loop_nodes.is_empty() {
        details.push("No closed path between source + and return/GND.".to_string());
        return Simulation {
            summary: "Open circuit".to_string(),
            details,
            ..Simulation::default()
        };
    }

    let energized_component_edges: Vec<(u64, bool)> = component_edges
        .into_iter()
        .filter(|(_, a, b, _)| loop_nodes.contains(a) && loop_nodes.contains(b))
        .map(|(id, _, _, is_load)| (id, is_load))
        .collect();
    let energized_loads: HashSet<u64> = energized_component_edges
        .iter()
        .filter(|(_, is_load)| *is_load)
        .map(|(id, _)| *id)
        .chain(
            powered_module_edges
                .iter()
                .filter(|(_, a, b)| loop_nodes.contains(a) && loop_nodes.contains(b))
                .map(|(id, _, _)| *id),
        )
        .collect();

    let energized_components = energized_component_edges
        .into_iter()
        .map(|(id, _)| id)
        .chain(
            powered_module_edges
                .into_iter()
                .filter(|(_, a, b)| loop_nodes.contains(a) && loop_nodes.contains(b))
                .map(|(id, _, _)| id),
        )
        .chain(
            components
                .iter()
                .filter(|component| {
                    matches!(
                        component.kind,
                        ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource
                    ) && component_pin_defs(component)
                        .iter()
                        .map(|pin| nodes.find_existing(pin.pos))
                        .all(|node| node.is_some_and(|node| loop_nodes.contains(&node)))
                })
                .map(|component| component.id),
        )
        .collect();

    let energized_wires = wire_edges
        .into_iter()
        .filter(|(_, a, b)| loop_nodes.contains(a) && loop_nodes.contains(b))
        .map(|(id, _, _)| id)
        .collect();

    let wire_only_from_positive = reachable_nodes(&wire_graph, &positive_nodes);
    let wire_only_from_return = reachable_nodes(&wire_graph, &return_nodes);
    let direct_wire_short = wire_only_from_positive
        .intersection(&wire_only_from_return)
        .next()
        .is_some();
    let shorted = energized_loads.is_empty() || (direct_wire_short && energized_loads.is_empty());

    let voltage = estimate_loop_voltage(components, &nodes, &loop_nodes);
    let resistance = estimate_loop_resistance(components, &energized_loads);
    let current = match (voltage, resistance) {
        (Some(v), Some(r)) if r > 0.0 && !shorted => Some(v / r),
        _ => None,
    };

    if shorted {
        details.push("Source + reaches return/GND without a resistive load.".to_string());
    } else {
        details.push(format!("{} energized load(s).", energized_loads.len()));
    }

    Simulation {
        closed: true,
        shorted,
        energized_components,
        energized_wires,
        summary: if shorted {
            "Short circuit".to_string()
        } else {
            "Current flowing".to_string()
        },
        details,
        voltage,
        resistance,
        current,
    }
}

#[derive(Default)]
struct CircuitNodes {
    positions: Vec<Pos2>,
}

impl CircuitNodes {
    fn node_for(&mut self, pos: Pos2) -> usize {
        if let Some(index) = self.find_existing(pos) {
            return index;
        }
        self.positions.push(pos);
        self.positions.len() - 1
    }

    fn find_existing(&self, pos: Pos2) -> Option<usize> {
        self.positions
            .iter()
            .position(|existing| existing.distance(pos) <= 12.0)
    }
}

fn connect(graph: &mut Vec<HashSet<usize>>, a: usize, b: usize) {
    let needed = a.max(b) + 1;
    if graph.len() < needed {
        graph.resize_with(needed, HashSet::new);
    }
    graph[a].insert(b);
    graph[b].insert(a);
}

fn reachable_nodes(graph: &[HashSet<usize>], starts: &[usize]) -> HashSet<usize> {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::new();
    for &start in starts {
        if seen.insert(start) {
            queue.push_back(start);
        }
    }

    while let Some(node) = queue.pop_front() {
        if let Some(neighbors) = graph.get(node) {
            for &neighbor in neighbors {
                if seen.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
    }
    seen
}

fn nodes_connected(graph: &[HashSet<usize>], a: usize, b: usize) -> bool {
    reachable_nodes(graph, &[a]).contains(&b)
}

fn validate_i2c_links(
    components: &[Component],
    nodes: &CircuitNodes,
    wire_graph: &[HashSet<usize>],
) -> Vec<String> {
    let mut esp_sda = Vec::new();
    let mut esp_scl = Vec::new();
    for component in components {
        if !component_is_esp32(component.kind) {
            continue;
        }
        for pin in component_pin_defs(component) {
            if pin.label.contains("GPIO21") || pin.label.contains("SDA") {
                if let Some(node) = nodes.find_existing(pin.pos) {
                    esp_sda.push(node);
                }
            }
            if pin.label.contains("GPIO22") || pin.label.contains("SCL") {
                if let Some(node) = nodes.find_existing(pin.pos) {
                    esp_scl.push(node);
                }
            }
        }
    }

    let mut details = Vec::new();
    for component in components {
        if !matches!(component.kind, ComponentKind::Oled | ComponentKind::Sensor) {
            continue;
        }
        let mut sda_ok = false;
        let mut scl_ok = false;
        for pin in component_pin_defs(component) {
            let Some(node) = nodes.find_existing(pin.pos) else {
                continue;
            };
            if pin.label.contains("SDA") {
                sda_ok = esp_sda
                    .iter()
                    .any(|&esp_node| nodes_connected(wire_graph, node, esp_node));
            }
            if pin.label.contains("SCL") {
                scl_ok = esp_scl
                    .iter()
                    .any(|&esp_node| nodes_connected(wire_graph, node, esp_node));
            }
        }
        if sda_ok && scl_ok {
            details.push(format!(
                "{} I2C linked to ESP32 GPIO21/22.",
                component.label
            ));
        } else if !esp_sda.is_empty() || !esp_scl.is_empty() {
            details.push(format!(
                "{} I2C incomplete: SDA {}, SCL {}.",
                component.label,
                if sda_ok { "ok" } else { "missing" },
                if scl_ok { "ok" } else { "missing" }
            ));
        }
    }
    details
}

fn estimate_loop_voltage(
    components: &[Component],
    nodes: &CircuitNodes,
    loop_nodes: &HashSet<usize>,
) -> Option<f32> {
    components
        .iter()
        .filter(|component| {
            matches!(
                component.kind,
                ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource
            )
        })
        .filter(|component| {
            component_pin_defs(component)
                .iter()
                .filter_map(|pin| nodes.find_existing(pin.pos))
                .any(|node| loop_nodes.contains(&node))
        })
        .filter_map(|component| parse_metric_value(&component.value, "v"))
        .next()
}

fn estimate_loop_resistance(
    components: &[Component],
    energized_loads: &HashSet<u64>,
) -> Option<f32> {
    let resistance = components
        .iter()
        .filter(|component| energized_loads.contains(&component.id))
        .filter_map(|component| match component.kind {
            ComponentKind::Resistor => parse_metric_value(&component.value, "ohm"),
            ComponentKind::Led | ComponentKind::Diode => Some(220.0),
            ComponentKind::Lamp => Some(60.0),
            _ => None,
        })
        .sum::<f32>();
    (resistance > 0.0).then_some(resistance)
}

fn parse_metric_value(value: &str, unit_hint: &str) -> Option<f32> {
    let normalized = value
        .trim()
        .to_lowercase()
        .replace('\u{03a9}', "ohm")
        .replace('\u{00b5}', "u");
    let number_end = normalized
        .char_indices()
        .find(|(_, ch)| !(ch.is_ascii_digit() || *ch == '.'))
        .map(|(idx, _)| idx)
        .unwrap_or(normalized.len());
    let number = normalized.get(..number_end)?.parse::<f32>().ok()?;
    let suffix = normalized.get(number_end..)?.trim();
    let multiplier = if suffix.starts_with('m') && unit_hint == "v" {
        0.001
    } else if suffix.starts_with('k') {
        1_000.0
    } else if suffix.starts_with("meg") || suffix.starts_with('m') && unit_hint == "ohm" {
        1_000_000.0
    } else if suffix.starts_with('u') {
        0.000_001
    } else {
        1.0
    };
    Some(number * multiplier)
}

fn format_resistance(ohms: f32) -> String {
    if ohms >= 1_000_000.0 {
        format!("{:.2} Mohm", ohms / 1_000_000.0)
    } else if ohms >= 1_000.0 {
        format!("{:.2} kohm", ohms / 1_000.0)
    } else {
        format!("{:.1} ohm", ohms)
    }
}

fn format_current(amps: f32) -> String {
    if amps >= 1.0 {
        format!("{amps:.2} A")
    } else if amps >= 0.001 {
        format!("{:.2} mA", amps * 1_000.0)
    } else {
        format!("{:.2} uA", amps * 1_000_000.0)
    }
}

fn component_conductance(component: &Component) -> Conductance {
    match component.kind {
        ComponentKind::Resistor
        | ComponentKind::Diode
        | ComponentKind::Led
        | ComponentKind::Lamp => Conductance::Load,
        ComponentKind::Inductor => Conductance::Conductor,
        ComponentKind::Switch | ComponentKind::PushButton | ComponentKind::SlideSwitch => {
            let value = component.value.to_lowercase();
            if value.contains("open") || value.contains("off") {
                Conductance::Open
            } else {
                Conductance::Conductor
            }
        }
        ComponentKind::DcMotor => Conductance::Load,
        ComponentKind::Relay => Conductance::Conductor,
        ComponentKind::Capacitor | ComponentKind::OpAmp | ComponentKind::Breadboard => {
            Conductance::Open
        }
        ComponentKind::Esp32
        | ComponentKind::Esp32S3
        | ComponentKind::Esp32C3
        | ComponentKind::ArduinoUno
        | ComponentKind::RaspberryPiPico
        | ComponentKind::Servo
        | ComponentKind::Oled
        | ComponentKind::Sensor => Conductance::Open,
        ComponentKind::Ground
        | ComponentKind::VSource
        | ComponentKind::ISource
        | ComponentKind::Battery => Conductance::Open,
    }
}

fn component_is_powered_module(component: &Component) -> bool {
    matches!(
        component.kind,
        ComponentKind::Esp32
            | ComponentKind::Esp32S3
            | ComponentKind::Esp32C3
            | ComponentKind::ArduinoUno
            | ComponentKind::RaspberryPiPico
            | ComponentKind::Servo
            | ComponentKind::Oled
            | ComponentKind::Sensor
    )
}

fn component_is_esp32(kind: ComponentKind) -> bool {
    matches!(
        kind,
        ComponentKind::Esp32 | ComponentKind::Esp32S3 | ComponentKind::Esp32C3
    )
}

fn component_is_switch(kind: ComponentKind) -> bool {
    matches!(
        kind,
        ComponentKind::Switch | ComponentKind::PushButton | ComponentKind::SlideSwitch
    )
}

fn component_bounds(component: &Component) -> Rect {
    let size = component_size(component);
    Rect::from_center_size(component.pos, size)
}

fn component_size(component: &Component) -> Vec2 {
    let (w, h) = match component.kind {
        ComponentKind::Resistor | ComponentKind::Inductor | ComponentKind::Diode => (72.0, 28.0),
        ComponentKind::Capacitor
        | ComponentKind::Switch
        | ComponentKind::PushButton
        | ComponentKind::SlideSwitch
        | ComponentKind::Battery => (64.0, 32.0),
        ComponentKind::Ground => (40.0, 40.0),
        ComponentKind::VSource
        | ComponentKind::ISource
        | ComponentKind::Lamp
        | ComponentKind::Led => (56.0, 56.0),
        ComponentKind::OpAmp => (82.0, 68.0),
        ComponentKind::Esp32 | ComponentKind::Esp32S3 => (140.0, 160.0),
        ComponentKind::Esp32C3 => (118.0, 138.0),
        ComponentKind::ArduinoUno => (150.0, 190.0),
        ComponentKind::RaspberryPiPico => (110.0, 180.0),
        ComponentKind::Breadboard => (280.0, 160.0),
        ComponentKind::Relay => (92.0, 70.0),
        ComponentKind::DcMotor => (80.0, 64.0),
        ComponentKind::Servo => (96.0, 72.0),
        ComponentKind::Oled => (100.0, 120.0),
        ComponentKind::Sensor => (96.0, 100.0),
    };
    if component.rotation % 180 == 0 {
        Vec2::new(w, h)
    } else {
        Vec2::new(h, w)
    }
}

fn draw_grid(painter: &egui::Painter, rect: Rect, grid: f32) {
    painter.rect_filled(rect, 0.0, Color32::from_rgb(18, 22, 28));
    let line_color = Color32::from_rgb(44, 52, 62);
    let stroke = Stroke::new(1.0, line_color);
    let mut x = rect.left();
    while x <= rect.right() {
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            stroke,
        );
        x += grid;
    }
    let mut y = rect.top();
    while y <= rect.bottom() {
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            stroke,
        );
        y += grid;
    }
    let major_stroke = Stroke::new(1.0, Color32::from_rgb(67, 78, 92));
    let major = grid * 5.0;
    let mut x = rect.left();
    while x <= rect.right() {
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            major_stroke,
        );
        x += major;
    }
    let mut y = rect.top();
    while y <= rect.bottom() {
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            major_stroke,
        );
        y += major;
    }
}

fn draw_title_block(
    painter: &egui::Painter,
    canvas: Rect,
    components: &[Component],
    wires: &[Wire],
    simulation: &Simulation,
) {
    let size = Vec2::new(250.0, 108.0);
    let rect = Rect::from_min_size(canvas.right_bottom() - size - Vec2::new(18.0, 18.0), size);
    painter.rect_filled(rect, 3.0, Color32::from_rgba_unmultiplied(18, 21, 25, 232));
    painter.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0, Color32::from_rgb(72, 80, 88)),
        StrokeKind::Outside,
    );
    let status_color = if simulation.shorted {
        Color32::from_rgb(255, 95, 80)
    } else if simulation.closed {
        Color32::from_rgb(255, 185, 80)
    } else {
        Color32::from_rgb(150, 155, 165)
    };
    painter.text(
        rect.left_top() + Vec2::new(12.0, 10.0),
        Align2::LEFT_TOP,
        "CLUSTER SCHEMATIC",
        egui::FontId::proportional(13.0),
        Color32::from_rgb(230, 234, 238),
    );
    painter.line_segment(
        [
            Pos2::new(rect.left() + 10.0, rect.top() + 32.0),
            Pos2::new(rect.right() - 10.0, rect.top() + 32.0),
        ],
        Stroke::new(1.0, Color32::from_rgb(64, 70, 78)),
    );
    painter.text(
        rect.left_top() + Vec2::new(12.0, 42.0),
        Align2::LEFT_TOP,
        format!(
            "Parts  {:>2}    Wires  {:>2}",
            components.len(),
            wires.len()
        ),
        egui::FontId::monospace(11.0),
        Color32::from_rgb(185, 195, 205),
    );
    painter.text(
        rect.left_top() + Vec2::new(12.0, 62.0),
        Align2::LEFT_TOP,
        format!("Status {}", simulation.summary),
        egui::FontId::monospace(11.0),
        status_color,
    );
    if let Some(current) = simulation.current {
        painter.text(
            rect.left_top() + Vec2::new(12.0, 82.0),
            Align2::LEFT_TOP,
            format!("Loop   {}", format_current(current)),
            egui::FontId::monospace(11.0),
            Color32::from_rgb(185, 195, 205),
        );
    }
}

fn draw_empty_canvas_hint(painter: &egui::Painter, canvas: Rect) {
    let rect = Rect::from_center_size(canvas.center(), Vec2::new(360.0, 120.0));
    painter.rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(20, 24, 30, 225));
    painter.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(58, 66, 76)),
        StrokeKind::Outside,
    );
    painter.text(
        rect.center_top() + Vec2::new(0.0, 24.0),
        Align2::CENTER_TOP,
        "Start a schematic",
        egui::FontId::proportional(18.0),
        Color32::from_rgb(228, 234, 240),
    );
    painter.text(
        rect.center() + Vec2::new(0.0, 6.0),
        Align2::CENTER_CENTER,
        "Pick a part on the left, then click the grid.",
        egui::FontId::proportional(12.0),
        Color32::from_rgb(156, 166, 176),
    );
    painter.text(
        rect.center_bottom() - Vec2::new(0.0, 22.0),
        Align2::CENTER_BOTTOM,
        "Use Wire to connect pins. Enter finishes a wire.",
        egui::FontId::proportional(12.0),
        Color32::from_rgb(156, 166, 176),
    );
}

fn draw_component(
    painter: &egui::Painter,
    component: &Component,
    selected: bool,
    show_pins: bool,
    energized: bool,
) {
    let stroke = if selected {
        Stroke::new(2.2, Color32::from_rgb(90, 235, 170))
    } else if energized {
        Stroke::new(2.8, Color32::from_rgb(255, 185, 80))
    } else {
        Stroke::new(2.0, Color32::from_rgb(222, 226, 232))
    };
    let size = component_size(component);
    let rect = Rect::from_center_size(component.pos, size);

    if selected {
        painter.rect_stroke(
            rect.expand(8.0),
            4.0,
            Stroke::new(1.0, Color32::from_rgb(70, 140, 125)),
            StrokeKind::Outside,
        );
    }

    match component.kind {
        ComponentKind::Resistor => draw_resistor(painter, rect, component.rotation, stroke),
        ComponentKind::Capacitor => draw_capacitor(painter, rect, component.rotation, stroke),
        ComponentKind::Inductor => draw_inductor(painter, rect, component.rotation, stroke),
        ComponentKind::Diode => draw_diode(painter, rect, component.rotation, stroke, false),
        ComponentKind::Led => draw_led(painter, rect, component.rotation, stroke),
        ComponentKind::Switch | ComponentKind::SlideSwitch => {
            draw_switch(painter, rect, component.rotation, stroke)
        }
        ComponentKind::PushButton => draw_push_button(painter, rect, component.rotation, stroke),
        ComponentKind::Ground => draw_ground(painter, rect, component.rotation, stroke),
        ComponentKind::VSource => draw_vsource(painter, rect, component.rotation, stroke),
        ComponentKind::ISource => draw_isource(painter, rect, component.rotation, stroke),
        ComponentKind::Battery => draw_battery(painter, rect, component.rotation, stroke),
        ComponentKind::OpAmp => draw_opamp(painter, rect, component.rotation, stroke),
        ComponentKind::Lamp => draw_lamp(painter, rect, component.rotation, stroke),
        ComponentKind::Esp32 => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            "ESP32",
            &[
                "3V3",
                "GND",
                "GPIO23",
                "GPIO22 SCL",
                "GPIO21 SDA",
                "TX0",
                "RX0",
            ],
            &["VIN", "GND", "GPIO18", "GPIO19", "GPIO5", "EN", "RST"],
        ),
        ComponentKind::Esp32S3 => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            "ESP32-S3",
            &[
                "3V3",
                "GND",
                "GPIO1",
                "GPIO2 SDA",
                "GPIO3 SCL",
                "GPIO4",
                "TX0",
                "RX0",
            ],
            &[
                "VIN", "GND", "GPIO8", "GPIO9", "GPIO10", "GPIO11", "EN", "RST",
            ],
        ),
        ComponentKind::Esp32C3 => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            "ESP32-C3",
            &["3V3", "GND", "GPIO0", "GPIO1 SDA", "GPIO2 SCL", "TX", "RX"],
            &["5V", "GND", "GPIO3", "GPIO4", "GPIO5", "EN", "BOOT"],
        ),
        ComponentKind::ArduinoUno => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            "ARDUINO UNO",
            &["VIN", "5V", "3V3", "GND", "GND", "A4 SDA", "A5 SCL"],
            &[
                "D2", "D3 PWM", "D5 PWM", "D6 PWM", "D9 PWM", "D10", "D11", "D13",
            ],
        ),
        ComponentKind::RaspberryPiPico => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            "PI PICO",
            &[
                "VSYS", "3V3", "GND", "GP0 TX", "GP1 RX", "GP4 SDA", "GP5 SCL",
            ],
            &["VBUS", "GND", "GP14", "GP15", "GP16", "GP17", "RUN"],
        ),
        ComponentKind::Breadboard => draw_breadboard(painter, rect, stroke),
        ComponentKind::Relay => draw_relay(painter, rect, component.rotation, stroke),
        ComponentKind::DcMotor => draw_dc_motor(painter, rect, component.rotation, stroke),
        ComponentKind::Servo => draw_servo(painter, rect, stroke, energized),
        ComponentKind::Oled => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            "OLED",
            &["GND", "VCC", "SCL", "SDA"],
            &[],
        ),
        ComponentKind::Sensor => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            "SENSOR",
            &["GND", "VCC", "SCL"],
            &["SDA"],
        ),
    }

    if show_pins {
        for pin in component_pin_defs(component) {
            painter.circle_filled(pin.pos, 3.0, Color32::from_rgb(250, 205, 95));
            painter.circle_stroke(
                pin.pos,
                4.0,
                Stroke::new(1.0, Color32::from_rgb(40, 35, 20)),
            );
            if should_draw_pin_label(component.kind, &pin) {
                draw_pin_label(painter, component.pos, &pin);
            }
        }
    }

    painter.text(
        rect.center_bottom() + Vec2::new(0.0, 6.0),
        Align2::CENTER_TOP,
        &component.label,
        egui::FontId::proportional(12.0),
        if energized {
            Color32::from_rgb(255, 210, 130)
        } else {
            Color32::from_rgb(225, 228, 232)
        },
    );
    if !component.value.trim().is_empty() {
        painter.text(
            rect.center_top() - Vec2::new(0.0, 6.0),
            Align2::CENTER_BOTTOM,
            &component.value,
            egui::FontId::proportional(11.0),
            Color32::from_rgb(160, 170, 180),
        );
    }
}

fn draw_wire(
    painter: &egui::Painter,
    wire: &Wire,
    selected: bool,
    energized: bool,
    show_flow: bool,
    flow_phase: f32,
) {
    let stroke = if selected {
        Stroke::new(3.0, Color32::from_rgb(90, 235, 170))
    } else if energized {
        Stroke::new(3.2, Color32::from_rgb(255, 170, 55))
    } else {
        Stroke::new(2.0, Color32::from_rgb(105, 178, 255))
    };
    for segment in wire.points.windows(2) {
        painter.line_segment([segment[0], segment[1]], stroke);
    }
    for point in &wire.points {
        painter.circle_filled(*point, 2.8, stroke.color);
    }
    if show_flow {
        draw_flow_markers(painter, &wire.points, flow_phase);
    }
}

fn draw_flow_markers(painter: &egui::Painter, points: &[Pos2], flow_phase: f32) {
    let total = polyline_length(points);
    if total <= 1.0 {
        return;
    }

    let spacing = 28.0;
    let radius = 3.6;
    let mut distance = flow_phase % spacing;
    while distance < total {
        if let Some(point) = point_on_polyline(points, distance) {
            painter.circle_filled(point, radius, Color32::from_rgb(255, 235, 150));
            painter.circle_stroke(
                point,
                radius + 1.2,
                Stroke::new(1.0, Color32::from_rgb(120, 70, 15)),
            );
        }
        distance += spacing;
    }
}

fn polyline_length(points: &[Pos2]) -> f32 {
    points
        .windows(2)
        .map(|segment| segment[0].distance(segment[1]))
        .sum()
}

fn point_on_polyline(points: &[Pos2], mut distance: f32) -> Option<Pos2> {
    for segment in points.windows(2) {
        let a = segment[0];
        let b = segment[1];
        let length = a.distance(b);
        if length <= 0.1 {
            continue;
        }
        if distance <= length {
            let t = distance / length;
            return Some(a + (b - a) * t);
        }
        distance -= length;
    }
    points.last().copied()
}

fn should_draw_pin_label(kind: ComponentKind, pin: &CircuitPin) -> bool {
    if matches!(kind, ComponentKind::Battery | ComponentKind::Ground) {
        return false;
    }
    matches!(pin.role, PinRole::Positive | PinRole::Ground)
}

fn draw_pin_label(painter: &egui::Painter, component_center: Pos2, pin: &CircuitPin) {
    let outward = pin.pos - component_center;
    let horizontal = outward.x.abs() >= outward.y.abs();
    let offset = if horizontal {
        Vec2::new(if outward.x >= 0.0 { 10.0 } else { -10.0 }, -10.0)
    } else {
        Vec2::new(10.0, if outward.y >= 0.0 { 10.0 } else { -10.0 })
    };
    let align = if horizontal && outward.x < 0.0 {
        Align2::RIGHT_CENTER
    } else if horizontal {
        Align2::LEFT_CENTER
    } else if outward.y < 0.0 {
        Align2::LEFT_BOTTOM
    } else {
        Align2::LEFT_TOP
    };
    let color = match pin.role {
        PinRole::Positive => Color32::from_rgb(255, 210, 120),
        PinRole::Ground => Color32::from_rgb(170, 210, 255),
        _ => Color32::from_rgb(220, 225, 230),
    };
    painter.text(
        pin.pos + offset,
        align,
        pin.label,
        egui::FontId::proportional(12.0),
        color,
    );
}

fn draw_wire_preview(painter: &egui::Painter, points: &[Pos2]) {
    let stroke = Stroke::new(1.8, Color32::from_rgb(130, 200, 255));
    for segment in points.windows(2) {
        painter.line_segment([segment[0], segment[1]], stroke);
    }
    for point in points {
        painter.circle_filled(*point, 3.0, Color32::from_rgb(130, 200, 255));
    }
}

fn rotate_point(point: Pos2, center: Pos2, rotation: i32) -> Pos2 {
    let rot = ((rotation % 360) + 360) % 360;
    let translated = point - center;
    match rot {
        90 => Pos2::new(center.x - translated.y, center.y + translated.x),
        180 => Pos2::new(center.x - translated.x, center.y - translated.y),
        270 => Pos2::new(center.x + translated.y, center.y - translated.x),
        _ => point,
    }
}

fn component_pins(component: &Component) -> Vec<Pos2> {
    component_pin_defs(component)
        .into_iter()
        .map(|pin| pin.pos)
        .collect()
}

fn component_pin_defs(component: &Component) -> Vec<CircuitPin> {
    let rect = Rect::from_center_size(component.pos, component_size(component));
    let center = rect.center();
    let base = match component.kind {
        ComponentKind::Ground => vec![CircuitPin {
            label: "GND",
            role: PinRole::Ground,
            pos: Pos2::new(center.x, rect.top()),
        }],
        ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource => vec![
            CircuitPin {
                label: "-",
                role: PinRole::Ground,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "+",
                role: PinRole::Positive,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
        ComponentKind::OpAmp => vec![
            CircuitPin {
                label: "-IN",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y - rect.height() * 0.22),
            },
            CircuitPin {
                label: "+IN",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y + rect.height() * 0.22),
            },
            CircuitPin {
                label: "OUT",
                role: PinRole::Output,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
        ComponentKind::Esp32 => module_pin_defs(
            rect,
            &[
                ("3V3", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GPIO23", PinRole::Digital),
                ("GPIO22 SCL", PinRole::I2c),
                ("GPIO21 SDA", PinRole::I2c),
                ("TX0", PinRole::Digital),
                ("RX0", PinRole::Digital),
            ],
            &[
                ("VIN", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GPIO18", PinRole::Digital),
                ("GPIO19", PinRole::Digital),
                ("GPIO5", PinRole::Digital),
                ("EN", PinRole::Control),
                ("RST", PinRole::Control),
            ],
        ),
        ComponentKind::Esp32S3 => module_pin_defs(
            rect,
            &[
                ("3V3", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GPIO1", PinRole::Digital),
                ("GPIO2 SDA", PinRole::I2c),
                ("GPIO3 SCL", PinRole::I2c),
                ("GPIO4", PinRole::Digital),
                ("TX0", PinRole::Digital),
                ("RX0", PinRole::Digital),
            ],
            &[
                ("VIN", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GPIO8", PinRole::Digital),
                ("GPIO9", PinRole::Digital),
                ("GPIO10", PinRole::Digital),
                ("GPIO11", PinRole::Digital),
                ("EN", PinRole::Control),
                ("RST", PinRole::Control),
            ],
        ),
        ComponentKind::Esp32C3 => module_pin_defs(
            rect,
            &[
                ("3V3", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GPIO0", PinRole::Digital),
                ("GPIO1 SDA", PinRole::I2c),
                ("GPIO2 SCL", PinRole::I2c),
                ("TX", PinRole::Digital),
                ("RX", PinRole::Digital),
            ],
            &[
                ("5V", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GPIO3", PinRole::Digital),
                ("GPIO4", PinRole::Digital),
                ("GPIO5", PinRole::Digital),
                ("EN", PinRole::Control),
                ("BOOT", PinRole::Control),
            ],
        ),
        ComponentKind::ArduinoUno => module_pin_defs(
            rect,
            &[
                ("VIN", PinRole::Positive),
                ("5V", PinRole::Positive),
                ("3V3", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GND", PinRole::Ground),
                ("A4 SDA", PinRole::I2c),
                ("A5 SCL", PinRole::I2c),
            ],
            &[
                ("D2", PinRole::Digital),
                ("D3 PWM", PinRole::Digital),
                ("D5 PWM", PinRole::Digital),
                ("D6 PWM", PinRole::Digital),
                ("D9 PWM", PinRole::Digital),
                ("D10", PinRole::Digital),
                ("D11", PinRole::Digital),
                ("D13", PinRole::Digital),
            ],
        ),
        ComponentKind::RaspberryPiPico => module_pin_defs(
            rect,
            &[
                ("VSYS", PinRole::Positive),
                ("3V3", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GP0 TX", PinRole::Digital),
                ("GP1 RX", PinRole::Digital),
                ("GP4 SDA", PinRole::I2c),
                ("GP5 SCL", PinRole::I2c),
            ],
            &[
                ("VBUS", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GP14", PinRole::Digital),
                ("GP15", PinRole::Digital),
                ("GP16", PinRole::Digital),
                ("GP17", PinRole::Digital),
                ("RUN", PinRole::Control),
            ],
        ),
        ComponentKind::Breadboard => breadboard_pin_defs(rect),
        ComponentKind::Relay => vec![
            CircuitPin {
                label: "COIL+",
                role: PinRole::Positive,
                pos: Pos2::new(rect.left(), center.y - rect.height() * 0.25),
            },
            CircuitPin {
                label: "COIL-",
                role: PinRole::Ground,
                pos: Pos2::new(rect.left(), center.y + rect.height() * 0.25),
            },
            CircuitPin {
                label: "COM",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y - rect.height() * 0.28),
            },
            CircuitPin {
                label: "NO",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y),
            },
            CircuitPin {
                label: "NC",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y + rect.height() * 0.28),
            },
        ],
        ComponentKind::DcMotor => vec![
            CircuitPin {
                label: "-",
                role: PinRole::Ground,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "+",
                role: PinRole::Positive,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
        ComponentKind::Servo => vec![
            CircuitPin {
                label: "GND",
                role: PinRole::Ground,
                pos: Pos2::new(rect.left(), center.y - rect.height() * 0.24),
            },
            CircuitPin {
                label: "VCC",
                role: PinRole::Positive,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "SIG",
                role: PinRole::Digital,
                pos: Pos2::new(rect.left(), center.y + rect.height() * 0.24),
            },
        ],
        ComponentKind::Oled => vec![
            module_pin(rect, "GND", PinRole::Ground, false, 4, 0),
            module_pin(rect, "VCC", PinRole::Positive, false, 4, 1),
            module_pin(rect, "SCL", PinRole::I2c, false, 4, 2),
            module_pin(rect, "SDA", PinRole::I2c, false, 4, 3),
        ],
        ComponentKind::Sensor => vec![
            module_pin(rect, "GND", PinRole::Ground, false, 4, 0),
            module_pin(rect, "VCC", PinRole::Positive, false, 4, 1),
            module_pin(rect, "SCL", PinRole::I2c, false, 4, 2),
            module_pin(rect, "SDA", PinRole::I2c, true, 4, 2),
        ],
        _ => vec![
            CircuitPin {
                label: "A",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "B",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
    };
    base.into_iter()
        .map(|pin| CircuitPin {
            pos: rotate_point(pin.pos, center, component.rotation),
            ..pin
        })
        .collect()
}

fn module_pin_defs(
    rect: Rect,
    left: &[(&'static str, PinRole)],
    right: &[(&'static str, PinRole)],
) -> Vec<CircuitPin> {
    let mut pins = Vec::new();
    for (index, (label, role)) in left.iter().copied().enumerate() {
        pins.push(module_pin(rect, label, role, false, left.len(), index));
    }
    for (index, (label, role)) in right.iter().copied().enumerate() {
        pins.push(module_pin(rect, label, role, true, right.len(), index));
    }
    pins
}

fn module_pin(
    rect: Rect,
    label: &'static str,
    role: PinRole,
    right_side: bool,
    count: usize,
    index: usize,
) -> CircuitPin {
    CircuitPin {
        label,
        role,
        pos: Pos2::new(
            if right_side {
                rect.right()
            } else {
                rect.left()
            },
            module_pin_y(rect, count, index),
        ),
    }
}

fn breadboard_pin_defs(rect: Rect) -> Vec<CircuitPin> {
    vec![
        CircuitPin {
            label: "+",
            role: PinRole::Positive,
            pos: Pos2::new(rect.left(), rect.top() + 24.0),
        },
        CircuitPin {
            label: "-",
            role: PinRole::Ground,
            pos: Pos2::new(rect.left(), rect.top() + 44.0),
        },
        CircuitPin {
            label: "+",
            role: PinRole::Positive,
            pos: Pos2::new(rect.right(), rect.top() + 24.0),
        },
        CircuitPin {
            label: "-",
            role: PinRole::Ground,
            pos: Pos2::new(rect.right(), rect.top() + 44.0),
        },
        CircuitPin {
            label: "A",
            role: PinRole::Passive,
            pos: Pos2::new(rect.center().x - 50.0, rect.center().y),
        },
        CircuitPin {
            label: "B",
            role: PinRole::Passive,
            pos: Pos2::new(rect.center().x + 50.0, rect.center().y),
        },
    ]
}

fn draw_junctions(painter: &egui::Painter, wires: &[Wire]) {
    let mut points = Vec::new();
    for wire in wires {
        points.extend(wire.points.iter().copied());
    }

    for i in 0..points.len() {
        let connected = points
            .iter()
            .enumerate()
            .filter(|(idx, point)| *idx != i && point.distance(points[i]) < 1.0)
            .count();
        if connected > 0 {
            painter.circle_filled(points[i], 4.2, Color32::from_rgb(105, 178, 255));
        }
    }
}

fn draw_resistor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), rect.center().y);
    let right = Pos2::new(rect.right(), rect.center().y);
    let mut points = Vec::new();
    let zig_count = 6;
    let step = rect.width() / (zig_count as f32 + 1.0);
    points.push(left);
    for i in 1..=zig_count {
        let x = rect.left() + step * i as f32;
        let y = if i % 2 == 0 {
            rect.center().y - rect.height() * 0.35
        } else {
            rect.center().y + rect.height() * 0.35
        };
        points.push(Pos2::new(x, y));
    }
    points.push(right);

    let rotated: Vec<Pos2> = points
        .into_iter()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    for segment in rotated.windows(2) {
        painter.line_segment([segment[0], segment[1]], stroke);
    }
}

fn draw_capacitor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), rect.center().y);
    let right = Pos2::new(rect.right(), rect.center().y);
    let plate_offset = rect.width() * 0.2;
    let plate_height = rect.height() * 0.5;
    let p1 = Pos2::new(center.x - plate_offset, rect.center().y - plate_height);
    let p2 = Pos2::new(center.x - plate_offset, rect.center().y + plate_height);
    let p3 = Pos2::new(center.x + plate_offset, rect.center().y - plate_height);
    let p4 = Pos2::new(center.x + plate_offset, rect.center().y + plate_height);

    let points = [left, right, p1, p2, p3, p4];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    let left = rotated[0];
    let right = rotated[1];
    let p1 = rotated[2];
    let p2 = rotated[3];
    let p3 = rotated[4];
    let p4 = rotated[5];

    painter.line_segment([left, p1.lerp(p2, 0.5)], stroke);
    painter.line_segment([p3.lerp(p4, 0.5), right], stroke);
    painter.line_segment([p1, p2], stroke);
    painter.line_segment([p3, p4], stroke);
}

fn draw_inductor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), rect.center().y);
    let right = Pos2::new(rect.right(), rect.center().y);
    let turns = 4;
    let step = rect.width() / (turns as f32 + 1.0);
    let radius = rect.height() * 0.25;

    let mut points = Vec::new();
    points.push(left);
    for i in 0..turns {
        let x = rect.left() + step * (i as f32 + 1.0);
        let y_top = rect.center().y - radius;
        let y_bottom = rect.center().y + radius;
        points.push(Pos2::new(x - step * 0.25, y_bottom));
        points.push(Pos2::new(x, y_top));
        points.push(Pos2::new(x + step * 0.25, y_bottom));
    }
    points.push(right);

    let rotated: Vec<Pos2> = points
        .into_iter()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    for segment in rotated.windows(2) {
        painter.line_segment([segment[0], segment[1]], stroke);
    }
}

fn draw_diode(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke, filled: bool) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let anode = Pos2::new(center.x - rect.width() * 0.2, center.y);
    let cathode = Pos2::new(center.x + rect.width() * 0.2, center.y);
    let tri_top = Pos2::new(
        center.x - rect.width() * 0.18,
        center.y - rect.height() * 0.42,
    );
    let tri_bottom = Pos2::new(
        center.x - rect.width() * 0.18,
        center.y + rect.height() * 0.42,
    );
    let cathode_top = Pos2::new(cathode.x, center.y - rect.height() * 0.42);
    let cathode_bottom = Pos2::new(cathode.x, center.y + rect.height() * 0.42);

    let points = [
        left,
        right,
        anode,
        cathode,
        tri_top,
        tri_bottom,
        cathode_top,
        cathode_bottom,
    ];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[2]], stroke);
    painter.line_segment([rotated[3], rotated[1]], stroke);
    let triangle = vec![rotated[4], rotated[5], rotated[3]];
    if filled {
        painter.add(egui::Shape::convex_polygon(
            triangle.clone(),
            Color32::from_rgba_unmultiplied(
                stroke.color.r(),
                stroke.color.g(),
                stroke.color.b(),
                40,
            ),
            stroke,
        ));
    } else {
        painter.line_segment([triangle[0], triangle[1]], stroke);
        painter.line_segment([triangle[1], triangle[2]], stroke);
        painter.line_segment([triangle[2], triangle[0]], stroke);
    }
    painter.line_segment([rotated[6], rotated[7]], stroke);
}

fn draw_led(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    draw_diode(painter, rect, rotation, stroke, true);
    let center = rect.center();
    let arrow_a = [
        Pos2::new(
            center.x + rect.width() * 0.12,
            center.y - rect.height() * 0.5,
        ),
        Pos2::new(
            center.x + rect.width() * 0.34,
            center.y - rect.height() * 0.72,
        ),
    ];
    let arrow_b = [
        Pos2::new(
            center.x + rect.width() * 0.26,
            center.y - rect.height() * 0.32,
        ),
        Pos2::new(
            center.x + rect.width() * 0.48,
            center.y - rect.height() * 0.54,
        ),
    ];
    for arrow in [arrow_a, arrow_b] {
        let start = rotate_point(arrow[0], center, rotation);
        let end = rotate_point(arrow[1], center, rotation);
        painter.line_segment([start, end], stroke);
        painter.circle_filled(end, 2.0, stroke.color);
    }
}

fn draw_switch(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let left_contact = Pos2::new(center.x - rect.width() * 0.25, center.y);
    let right_contact = Pos2::new(center.x + rect.width() * 0.25, center.y);
    let blade_end = Pos2::new(
        center.x + rect.width() * 0.18,
        center.y - rect.height() * 0.32,
    );
    let points = [left, right, left_contact, right_contact, blade_end];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[2]], stroke);
    painter.line_segment([rotated[3], rotated[1]], stroke);
    painter.circle_filled(rotated[2], 3.2, stroke.color);
    painter.circle_filled(rotated[3], 3.2, stroke.color);
    painter.line_segment([rotated[2], rotated[4]], stroke);
}

fn draw_push_button(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let left_contact = Pos2::new(center.x - rect.width() * 0.24, center.y);
    let right_contact = Pos2::new(center.x + rect.width() * 0.24, center.y);
    let bar_left = Pos2::new(
        center.x - rect.width() * 0.18,
        center.y - rect.height() * 0.18,
    );
    let bar_right = Pos2::new(
        center.x + rect.width() * 0.18,
        center.y - rect.height() * 0.18,
    );
    let stem_top = Pos2::new(center.x, rect.top() + rect.height() * 0.08);
    let stem_bottom = Pos2::new(center.x, center.y - rect.height() * 0.18);
    let points = [
        left,
        right,
        left_contact,
        right_contact,
        bar_left,
        bar_right,
        stem_top,
        stem_bottom,
    ];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[2]], stroke);
    painter.line_segment([rotated[3], rotated[1]], stroke);
    painter.circle_filled(rotated[2], 3.2, stroke.color);
    painter.circle_filled(rotated[3], 3.2, stroke.color);
    painter.line_segment([rotated[4], rotated[5]], stroke);
    painter.line_segment([rotated[6], rotated[7]], stroke);
}

fn draw_breadboard(painter: &egui::Painter, rect: Rect, stroke: Stroke) {
    painter.rect_filled(rect, 4.0, Color32::from_rgb(28, 31, 35));
    painter.rect_stroke(rect, 4.0, stroke, StrokeKind::Outside);
    let plus_y = rect.top() + 24.0;
    let minus_y = rect.top() + 44.0;
    painter.line_segment(
        [
            Pos2::new(rect.left() + 14.0, plus_y),
            Pos2::new(rect.right() - 14.0, plus_y),
        ],
        Stroke::new(2.0, Color32::from_rgb(255, 185, 80)),
    );
    painter.line_segment(
        [
            Pos2::new(rect.left() + 14.0, minus_y),
            Pos2::new(rect.right() - 14.0, minus_y),
        ],
        Stroke::new(2.0, Color32::from_rgb(120, 190, 255)),
    );
    painter.line_segment(
        [
            Pos2::new(rect.center().x, rect.top() + 66.0),
            Pos2::new(rect.center().x, rect.bottom() - 14.0),
        ],
        Stroke::new(1.4, Color32::from_rgb(80, 85, 92)),
    );

    let hole = Color32::from_rgb(70, 76, 84);
    let mut x = rect.left() + 28.0;
    while x <= rect.right() - 28.0 {
        for row in 0..5 {
            let y = rect.top() + 78.0 + row as f32 * 14.0;
            painter.circle_filled(Pos2::new(x, y), 2.2, hole);
            painter.circle_filled(Pos2::new(x, y + 82.0), 2.2, hole);
        }
        x += 18.0;
    }
    painter.text(
        rect.left_top() + Vec2::new(12.0, 8.0),
        Align2::LEFT_TOP,
        "+  -",
        egui::FontId::proportional(12.0),
        Color32::from_rgb(220, 225, 230),
    );
}

fn draw_relay(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let box_rect =
        Rect::from_center_size(center, Vec2::new(rect.width() * 0.72, rect.height() * 0.72));
    painter.rect_stroke(box_rect, 4.0, stroke, StrokeKind::Outside);
    painter.text(
        center,
        Align2::CENTER_CENTER,
        "RELAY",
        egui::FontId::proportional(12.0),
        stroke.color,
    );
    let pins = [
        Pos2::new(rect.left(), center.y - rect.height() * 0.25),
        Pos2::new(box_rect.left(), center.y - rect.height() * 0.25),
        Pos2::new(rect.left(), center.y + rect.height() * 0.25),
        Pos2::new(box_rect.left(), center.y + rect.height() * 0.25),
        Pos2::new(box_rect.right(), center.y - rect.height() * 0.28),
        Pos2::new(rect.right(), center.y - rect.height() * 0.28),
        Pos2::new(box_rect.right(), center.y),
        Pos2::new(rect.right(), center.y),
        Pos2::new(box_rect.right(), center.y + rect.height() * 0.28),
        Pos2::new(rect.right(), center.y + rect.height() * 0.28),
    ];
    let rotated: Vec<Pos2> = pins
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();
    for segment in rotated.chunks(2) {
        painter.line_segment([segment[0], segment[1]], stroke);
    }
}

fn draw_dc_motor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let radius = rect.height() * 0.34;
    let rotated_left = rotate_point(left, center, rotation);
    let rotated_right = rotate_point(right, center, rotation);
    painter.line_segment(
        [
            rotated_left,
            rotate_point(Pos2::new(center.x - radius, center.y), center, rotation),
        ],
        stroke,
    );
    painter.line_segment(
        [
            rotate_point(Pos2::new(center.x + radius, center.y), center, rotation),
            rotated_right,
        ],
        stroke,
    );
    painter.circle_stroke(center, radius, stroke);
    painter.text(
        center,
        Align2::CENTER_CENTER,
        "M",
        egui::FontId::proportional(18.0),
        stroke.color,
    );
}

fn draw_servo(painter: &egui::Painter, rect: Rect, stroke: Stroke, energized: bool) {
    let fill = if energized {
        Color32::from_rgb(48, 38, 20)
    } else {
        Color32::from_rgb(26, 31, 36)
    };
    painter.rect_filled(rect.shrink(8.0), 4.0, fill);
    painter.rect_stroke(rect.shrink(8.0), 4.0, stroke, StrokeKind::Outside);
    let horn_center = Pos2::new(rect.right() - 24.0, rect.center().y);
    painter.circle_stroke(horn_center, 10.0, stroke);
    painter.line_segment([horn_center, horn_center + Vec2::new(24.0, -12.0)], stroke);
    painter.text(
        rect.center() - Vec2::new(12.0, 0.0),
        Align2::CENTER_CENTER,
        "SERVO",
        egui::FontId::proportional(11.0),
        stroke.color,
    );
}

fn draw_ground(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let stem_top = Pos2::new(rect.center().x, rect.top());
    let stem_bottom = Pos2::new(rect.center().x, rect.center().y);
    let line1_left = Pos2::new(rect.center().x - rect.width() * 0.3, rect.center().y);
    let line1_right = Pos2::new(rect.center().x + rect.width() * 0.3, rect.center().y);
    let line2_left = Pos2::new(
        rect.center().x - rect.width() * 0.2,
        rect.center().y + rect.height() * 0.2,
    );
    let line2_right = Pos2::new(
        rect.center().x + rect.width() * 0.2,
        rect.center().y + rect.height() * 0.2,
    );
    let line3_left = Pos2::new(
        rect.center().x - rect.width() * 0.1,
        rect.center().y + rect.height() * 0.35,
    );
    let line3_right = Pos2::new(
        rect.center().x + rect.width() * 0.1,
        rect.center().y + rect.height() * 0.35,
    );

    let points = [
        stem_top,
        stem_bottom,
        line1_left,
        line1_right,
        line2_left,
        line2_right,
        line3_left,
        line3_right,
    ];

    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[1]], stroke);
    painter.line_segment([rotated[2], rotated[3]], stroke);
    painter.line_segment([rotated[4], rotated[5]], stroke);
    painter.line_segment([rotated[6], rotated[7]], stroke);
}

fn draw_vsource(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.4;
    let left = Pos2::new(rect.left(), rect.center().y);
    let right = Pos2::new(rect.right(), rect.center().y);
    let plus_top = Pos2::new(center.x, center.y - radius * 0.4);
    let plus_bottom = Pos2::new(center.x, center.y + radius * 0.4);
    let plus_left = Pos2::new(center.x - radius * 0.2, center.y);
    let plus_right = Pos2::new(center.x + radius * 0.2, center.y);
    let minus_left = Pos2::new(center.x - radius * 0.2, center.y + radius * 0.45);
    let minus_right = Pos2::new(center.x + radius * 0.2, center.y + radius * 0.45);

    let points = [
        left,
        right,
        plus_top,
        plus_bottom,
        plus_left,
        plus_right,
        minus_left,
        minus_right,
    ];

    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    let left = rotated[0];
    let right = rotated[1];
    let plus_top = rotated[2];
    let plus_bottom = rotated[3];
    let plus_left = rotated[4];
    let plus_right = rotated[5];
    let minus_left = rotated[6];
    let minus_right = rotated[7];

    painter.line_segment([left, center], stroke);
    painter.line_segment([center, right], stroke);
    painter.circle_stroke(center, radius, stroke);
    painter.line_segment([plus_top, plus_bottom], stroke);
    painter.line_segment([plus_left, plus_right], stroke);
    painter.line_segment([minus_left, minus_right], stroke);
}

fn draw_isource(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.4;
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let arrow_start = Pos2::new(center.x - radius * 0.35, center.y);
    let arrow_end = Pos2::new(center.x + radius * 0.35, center.y);
    let head_a = Pos2::new(center.x + radius * 0.1, center.y - radius * 0.22);
    let head_b = Pos2::new(center.x + radius * 0.1, center.y + radius * 0.22);
    let points = [left, right, arrow_start, arrow_end, head_a, head_b];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], center], stroke);
    painter.line_segment([center, rotated[1]], stroke);
    painter.circle_stroke(center, radius, stroke);
    painter.line_segment([rotated[2], rotated[3]], stroke);
    painter.line_segment([rotated[3], rotated[4]], stroke);
    painter.line_segment([rotated[3], rotated[5]], stroke);
}

fn draw_battery(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let short_x = center.x - rect.width() * 0.16;
    let long_x = center.x + rect.width() * 0.12;
    let short_top = Pos2::new(short_x, center.y - rect.height() * 0.26);
    let short_bottom = Pos2::new(short_x, center.y + rect.height() * 0.26);
    let long_top = Pos2::new(long_x, center.y - rect.height() * 0.46);
    let long_bottom = Pos2::new(long_x, center.y + rect.height() * 0.46);
    let points = [left, right, short_top, short_bottom, long_top, long_bottom];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], midpoint(rotated[2], rotated[3])], stroke);
    painter.line_segment([midpoint(rotated[4], rotated[5]), rotated[1]], stroke);
    painter.line_segment([rotated[2], rotated[3]], stroke);
    painter.line_segment([rotated[4], rotated[5]], stroke);
    let minus_pos = rotate_point(
        Pos2::new(
            center.x - rect.width() * 0.32,
            center.y - rect.height() * 0.42,
        ),
        center,
        rotation,
    );
    let plus_pos = rotate_point(
        Pos2::new(
            center.x + rect.width() * 0.32,
            center.y - rect.height() * 0.42,
        ),
        center,
        rotation,
    );
    painter.text(
        minus_pos,
        Align2::CENTER_CENTER,
        "-",
        egui::FontId::proportional(15.0),
        Color32::from_rgb(170, 210, 255),
    );
    painter.text(
        plus_pos,
        Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(15.0),
        Color32::from_rgb(255, 210, 120),
    );
}

fn draw_opamp(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left_top = Pos2::new(rect.left(), rect.top());
    let left_bottom = Pos2::new(rect.left(), rect.bottom());
    let right = Pos2::new(rect.right(), center.y);
    let in_minus = Pos2::new(rect.left(), center.y - rect.height() * 0.22);
    let in_plus = Pos2::new(rect.left(), center.y + rect.height() * 0.22);
    let minus_text = Pos2::new(
        rect.left() + rect.width() * 0.25,
        center.y - rect.height() * 0.22,
    );
    let plus_text = Pos2::new(
        rect.left() + rect.width() * 0.25,
        center.y + rect.height() * 0.22,
    );
    let points = [
        left_top,
        left_bottom,
        right,
        in_minus,
        in_plus,
        minus_text,
        plus_text,
    ];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[1]], stroke);
    painter.line_segment([rotated[1], rotated[2]], stroke);
    painter.line_segment([rotated[2], rotated[0]], stroke);
    let minus_lead = rotate_point(Pos2::new(rect.left() - 8.0, in_minus.y), center, rotation);
    let plus_lead = rotate_point(Pos2::new(rect.left() - 8.0, in_plus.y), center, rotation);
    let out_lead = rotate_point(Pos2::new(rect.right() + 8.0, center.y), center, rotation);
    painter.line_segment([minus_lead, rotated[3]], stroke);
    painter.line_segment([plus_lead, rotated[4]], stroke);
    painter.line_segment([rotated[2], out_lead], stroke);
    painter.text(
        rotated[5],
        Align2::CENTER_CENTER,
        "-",
        egui::FontId::proportional(14.0),
        stroke.color,
    );
    painter.text(
        rotated[6],
        Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(14.0),
        stroke.color,
    );
}

fn draw_lamp(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.34;
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let a = Pos2::new(center.x - radius * 0.7, center.y - radius * 0.7);
    let b = Pos2::new(center.x + radius * 0.7, center.y + radius * 0.7);
    let c = Pos2::new(center.x + radius * 0.7, center.y - radius * 0.7);
    let d = Pos2::new(center.x - radius * 0.7, center.y + radius * 0.7);
    let points = [left, right, a, b, c, d];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], center], stroke);
    painter.line_segment([center, rotated[1]], stroke);
    painter.circle_stroke(center, radius, stroke);
    painter.line_segment([rotated[2], rotated[3]], stroke);
    painter.line_segment([rotated[4], rotated[5]], stroke);
}

fn draw_module(
    painter: &egui::Painter,
    component: &Component,
    rect: Rect,
    stroke: Stroke,
    energized: bool,
    title: &str,
    left_labels: &[&str],
    right_labels: &[&str],
) {
    let center = rect.center();
    let body_fill = if energized {
        Color32::from_rgb(62, 46, 22)
    } else {
        Color32::from_rgb(24, 30, 38)
    };
    let outline = Stroke::new(stroke.width, stroke.color);
    painter.rect_filled(rect, 4.0, body_fill);
    painter.rect_stroke(rect, 4.0, outline, StrokeKind::Outside);

    painter.text(
        center + Vec2::new(0.0, -7.0),
        Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(14.0),
        stroke.color,
    );
    painter.text(
        center + Vec2::new(0.0, 10.0),
        Align2::CENTER_CENTER,
        &component.value,
        egui::FontId::proportional(10.0),
        Color32::from_rgb(150, 160, 170),
    );

    for (i, label) in left_labels.iter().enumerate() {
        let y = module_pin_y(rect, left_labels.len(), i);
        let pin = Pos2::new(rect.left(), y);
        painter.line_segment([pin, pin + Vec2::new(10.0, 0.0)], stroke);
        painter.text(
            pin + Vec2::new(13.0, 0.0),
            Align2::LEFT_CENTER,
            *label,
            egui::FontId::proportional(9.0),
            Color32::from_rgb(185, 195, 205),
        );
    }

    for (i, label) in right_labels.iter().enumerate() {
        let y = module_pin_y(rect, right_labels.len(), i);
        let pin = Pos2::new(rect.right(), y);
        painter.line_segment([pin - Vec2::new(10.0, 0.0), pin], stroke);
        painter.text(
            pin - Vec2::new(13.0, 0.0),
            Align2::RIGHT_CENTER,
            *label,
            egui::FontId::proportional(9.0),
            Color32::from_rgb(185, 195, 205),
        );
    }
}

fn module_pin_y(rect: Rect, count: usize, index: usize) -> f32 {
    if count <= 1 {
        return rect.center().y;
    }
    let middle = count / 2;
    rect.center().y + (index as f32 - middle as f32) * 20.0
}

fn midpoint(a: Pos2, b: Pos2) -> Pos2 {
    Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}

#[derive(Default)]
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn ensure(&mut self, index: usize) {
        while self.parent.len() <= index {
            self.parent.push(self.parent.len());
        }
    }

    fn find(&mut self, index: usize) -> usize {
        self.ensure(index);
        if self.parent[index] != index {
            self.parent[index] = self.find(self.parent[index]);
        }
        self.parent[index]
    }

    fn union(&mut self, a: usize, b: usize) {
        let a = self.find(a);
        let b = self.find(b);
        if a != b {
            self.parent[b] = a;
        }
    }
}

fn circuit_to_spice_netlist(components: &[Component], wires: &[Wire]) -> String {
    let mut nodes = CircuitNodes::default();
    let mut nets = UnionFind::default();

    for wire in wires {
        for point in &wire.points {
            let node = nodes.node_for(*point);
            nets.ensure(node);
        }
        for segment in wire.points.windows(2) {
            let a = nodes.node_for(segment[0]);
            let b = nodes.node_for(segment[1]);
            nets.union(a, b);
        }
    }

    for component in components {
        for pin in component_pin_defs(component) {
            let node = nodes.node_for(pin.pos);
            nets.ensure(node);
        }
    }

    let mut ground_roots = HashSet::new();
    for component in components {
        if component.kind != ComponentKind::Ground {
            continue;
        }
        for pin in component_pin_defs(component) {
            let node = nodes.node_for(pin.pos);
            ground_roots.insert(nets.find(node));
        }
    }

    let mut named_roots = HashMap::new();
    for index in 0..nodes.positions.len() {
        let root = nets.find(index);
        if ground_roots.contains(&root) {
            named_roots.insert(root, "0".to_string());
        }
    }

    let mut roots = (0..nodes.positions.len())
        .map(|index| nets.find(index))
        .collect::<Vec<_>>();
    roots.sort_unstable();
    roots.dedup();

    let mut next_net = 1;
    for root in roots {
        named_roots.entry(root).or_insert_with(|| {
            let name = format!("N{next_net:03}");
            next_net += 1;
            name
        });
    }

    let mut net_name = |pos: Pos2| {
        let node = nodes.node_for(pos);
        let root = nets.find(node);
        named_roots
            .entry(root)
            .or_insert_with(|| {
                let name = format!("N{next_net:03}");
                next_net += 1;
                name
            })
            .clone()
    };

    let mut used_names = HashSet::new();
    let mut lines = Vec::new();
    let mut skipped = Vec::new();
    let mut uses_diode_model = false;
    let mut uses_led_model = false;

    for component in components {
        let pins = component_pin_defs(component);
        let line = match component.kind {
            ComponentKind::Resistor
            | ComponentKind::Capacitor
            | ComponentKind::Inductor
            | ComponentKind::Diode
            | ComponentKind::Led
            | ComponentKind::VSource
            | ComponentKind::ISource
            | ComponentKind::Battery => {
                let Some((a, b)) = spice_two_pin_nets(component, &pins, &mut net_name) else {
                    skipped.push(format!("* skipped {}: missing pins", component.label));
                    continue;
                };
                match component.kind {
                    ComponentKind::Resistor => Some(format!(
                        "{} {a} {b} {}",
                        unique_spice_name("R", &component.label, component.id, &mut used_names),
                        spice_value(component, "1k")
                    )),
                    ComponentKind::Capacitor => Some(format!(
                        "{} {a} {b} {}",
                        unique_spice_name("C", &component.label, component.id, &mut used_names),
                        spice_value(component, "100n")
                    )),
                    ComponentKind::Inductor => Some(format!(
                        "{} {a} {b} {}",
                        unique_spice_name("L", &component.label, component.id, &mut used_names),
                        spice_value(component, "10u")
                    )),
                    ComponentKind::VSource | ComponentKind::Battery => Some(format!(
                        "{} {a} {b} DC {}",
                        unique_spice_name("V", &component.label, component.id, &mut used_names),
                        spice_value(component, "5")
                    )),
                    ComponentKind::ISource => Some(format!(
                        "{} {a} {b} DC {}",
                        unique_spice_name("I", &component.label, component.id, &mut used_names),
                        spice_value(component, "1m")
                    )),
                    ComponentKind::Diode => {
                        uses_diode_model = true;
                        Some(format!(
                            "{} {a} {b} DGEN",
                            unique_spice_name("D", &component.label, component.id, &mut used_names)
                        ))
                    }
                    ComponentKind::Led => {
                        uses_led_model = true;
                        Some(format!(
                            "{} {a} {b} LEDGEN",
                            unique_spice_name("D", &component.label, component.id, &mut used_names)
                        ))
                    }
                    _ => None,
                }
            }
            ComponentKind::Ground => None,
            _ => {
                skipped.push(format!(
                    "* skipped {}: {} has no SPICE primitive yet",
                    component.label,
                    component_kind_label(component.kind)
                ));
                None
            }
        };
        if let Some(line) = line {
            lines.push(line);
        }
    }

    let mut output = String::new();
    output.push_str("* Cluster SPICE netlist\n");
    output.push_str("* Generated from the schematic connectivity graph.\n");
    if lines.is_empty() {
        output.push_str("* No supported SPICE primitives in this schematic.\n");
    } else {
        for line in lines {
            output.push_str(&line);
            output.push('\n');
        }
    }
    if uses_diode_model {
        output.push_str(".model DGEN D(Is=2n Rs=0.6 N=1.8)\n");
    }
    if uses_led_model {
        output.push_str(".model LEDGEN D(Is=10n Rs=4 N=2.0 Eg=2.0)\n");
    }
    for line in skipped {
        output.push_str(&line);
        output.push('\n');
    }
    output.push_str(".op\n.end\n");
    output
}

fn spice_two_pin_nets(
    component: &Component,
    pins: &[CircuitPin],
    net_name: &mut impl FnMut(Pos2) -> String,
) -> Option<(String, String)> {
    match component.kind {
        ComponentKind::VSource
        | ComponentKind::Battery
        | ComponentKind::ISource
        | ComponentKind::DcMotor => {
            let positive = pins.iter().find(|pin| pin.role == PinRole::Positive)?;
            let negative = pins.iter().find(|pin| pin.role == PinRole::Ground)?;
            Some((net_name(positive.pos), net_name(negative.pos)))
        }
        _ => {
            let a = pins.first()?;
            let b = pins.get(1)?;
            Some((net_name(a.pos), net_name(b.pos)))
        }
    }
}

fn unique_spice_name(prefix: &str, label: &str, id: u64, used: &mut HashSet<String>) -> String {
    let mut name = label
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    if name.is_empty() {
        name = format!("{prefix}{id}");
    }
    if !name
        .chars()
        .next()
        .is_some_and(|ch| ch.eq_ignore_ascii_case(&prefix.chars().next().unwrap_or('X')))
    {
        name = format!("{prefix}{name}");
    }
    if used.insert(name.clone()) {
        return name;
    }
    let with_id = format!("{name}_{id}");
    used.insert(with_id.clone());
    with_id
}

fn spice_value(component: &Component, fallback: &str) -> String {
    let normalized = component.value.trim().replace(' ', "");
    if normalized.is_empty() {
        return fallback.to_string();
    }
    let lower = normalized.to_lowercase();
    let stripped = match component.kind {
        ComponentKind::Resistor => lower.strip_suffix("ohm").unwrap_or(&normalized),
        ComponentKind::Capacitor => lower.strip_suffix('f').unwrap_or(&normalized),
        ComponentKind::Inductor => lower.strip_suffix('h').unwrap_or(&normalized),
        ComponentKind::VSource | ComponentKind::Battery => lower
            .strip_suffix("volts")
            .or_else(|| lower.strip_suffix("volt"))
            .or_else(|| lower.strip_suffix('v'))
            .unwrap_or(&normalized),
        ComponentKind::ISource => lower
            .strip_suffix("amps")
            .or_else(|| lower.strip_suffix("amp"))
            .or_else(|| lower.strip_suffix('a'))
            .unwrap_or(&normalized),
        _ => &normalized,
    };
    if stripped.trim().is_empty() {
        fallback.to_string()
    } else {
        stripped.to_string()
    }
}

fn circuit_to_svg(components: &[Component], wires: &[Wire]) -> String {
    let bounds = circuit_bounds(components, wires)
        .unwrap_or_else(|| Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(960.0, 640.0)));
    let margin = 40.0;
    let min_x = bounds.left() - margin;
    let min_y = bounds.top() - margin;
    let width = (bounds.width() + margin * 2.0).max(480.0);
    let height = (bounds.height() + margin * 2.0).max(320.0);
    let simulation = analyze_circuit(components, wires);

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{:.1} {:.1} {:.1} {:.1}" width="{:.1}" height="{:.1}">
<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="#101216"/>
<g fill="none" stroke-linecap="round" stroke-linejoin="round">
"##,
        min_x, min_y, width, height, width, height, min_x, min_y, width, height
    ));

    for wire in wires {
        if wire.points.len() < 2 {
            continue;
        }
        let color = if simulation.energized_wires.contains(&wire.id) {
            "#ffaa37"
        } else {
            "#69b2ff"
        };
        let points = wire
            .points
            .iter()
            .map(|p| format!("{:.1},{:.1}", p.x, p.y))
            .collect::<Vec<_>>()
            .join(" ");
        svg.push_str(&format!(
            r##"<polyline points="{}" stroke="{}" stroke-width="2.4"/>"##,
            points, color
        ));
        svg.push('\n');
    }

    for component in components {
        let rect = component_bounds(component);
        let energized = simulation.energized_components.contains(&component.id);
        let stroke = if energized { "#ffb950" } else { "#dee2e8" };
        let fill = if component_is_module(component) {
            if energized { "#3e2e16" } else { "#181e26" }
        } else {
            "none"
        };
        svg.push_str(&format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="4" fill="{}" stroke="{}" stroke-width="2"/>"##,
            rect.left(),
            rect.top(),
            rect.width(),
            rect.height(),
            fill,
            stroke
        ));
        svg.push('\n');
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" fill="{}" font-family="Arial, sans-serif" font-size="12" text-anchor="middle">{}</text>"##,
            rect.center().x,
            rect.center().y - 2.0,
            stroke,
            escape_xml(component_kind_label(component.kind))
        ));
        svg.push('\n');
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" fill="#e1e4e8" font-family="Arial, sans-serif" font-size="11" text-anchor="middle">{}</text>"##,
            rect.center().x,
            rect.bottom() + 15.0,
            escape_xml(&component.label)
        ));
        svg.push('\n');
        if !component.value.trim().is_empty() {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" fill="#9aa4ae" font-family="Arial, sans-serif" font-size="10" text-anchor="middle">{}</text>"##,
                rect.center().x,
                rect.top() - 7.0,
                escape_xml(&component.value)
            ));
            svg.push('\n');
        }
        for pin in component_pins(component) {
            svg.push_str(&format!(
                r##"<circle cx="{:.1}" cy="{:.1}" r="3.2" fill="#facd5f" stroke="#281f14" stroke-width="1"/>"##,
                pin.x, pin.y
            ));
            svg.push('\n');
        }
    }

    svg.push_str("</g>\n</svg>\n");
    svg
}

fn circuit_bounds(components: &[Component], wires: &[Wire]) -> Option<Rect> {
    let mut min = Pos2::new(f32::INFINITY, f32::INFINITY);
    let mut max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut has_content = false;

    for component in components {
        let rect = component_bounds(component);
        min.x = min.x.min(rect.left());
        min.y = min.y.min(rect.top());
        max.x = max.x.max(rect.right());
        max.y = max.y.max(rect.bottom());
        has_content = true;
    }

    for wire in wires {
        for point in &wire.points {
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
            has_content = true;
        }
    }

    has_content.then(|| Rect::from_min_max(min, max))
}

fn component_kind_label(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Resistor => "Resistor",
        ComponentKind::Capacitor => "Capacitor",
        ComponentKind::Inductor => "Inductor",
        ComponentKind::Diode => "Diode",
        ComponentKind::Led => "LED",
        ComponentKind::Switch => "Switch",
        ComponentKind::PushButton => "Push Button",
        ComponentKind::SlideSwitch => "Slide Switch",
        ComponentKind::Ground => "Ground",
        ComponentKind::VSource => "V Source",
        ComponentKind::ISource => "I Source",
        ComponentKind::Battery => "Battery",
        ComponentKind::OpAmp => "Op Amp",
        ComponentKind::Lamp => "Lamp",
        ComponentKind::Esp32 => "ESP32 WROOM",
        ComponentKind::Esp32S3 => "ESP32-S3",
        ComponentKind::Esp32C3 => "ESP32-C3",
        ComponentKind::ArduinoUno => "Arduino UNO",
        ComponentKind::RaspberryPiPico => "Pi Pico",
        ComponentKind::Breadboard => "Breadboard",
        ComponentKind::Relay => "Relay",
        ComponentKind::DcMotor => "DC Motor",
        ComponentKind::Servo => "Servo",
        ComponentKind::Oled => "OLED I2C",
        ComponentKind::Sensor => "Sensor",
    }
}

fn component_is_module(component: &Component) -> bool {
    matches!(
        component.kind,
        ComponentKind::Esp32
            | ComponentKind::Esp32S3
            | ComponentKind::Esp32C3
            | ComponentKind::ArduinoUno
            | ComponentKind::RaspberryPiPico
            | ComponentKind::Oled
            | ComponentKind::Sensor
    )
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spice_export_names_connected_nets_and_ground() {
        let mut app = CircuitApp::new();
        app.load_led_demo();

        let netlist = circuit_to_spice_netlist(&app.components, &app.wires);

        assert!(netlist.contains("VBAT1"));
        assert!(netlist.contains("R1"));
        assert!(netlist.contains("DLED1"));
        assert!(netlist.contains(" 0 "));
        assert!(netlist.contains(".model LEDGEN"));
        assert!(netlist.ends_with(".op\n.end\n"));
    }

    #[test]
    fn spice_export_reports_empty_schematic_without_panicking() {
        let netlist = circuit_to_spice_netlist(&[], &[]);

        assert!(netlist.contains("No supported SPICE primitives"));
        assert!(netlist.contains(".end"));
    }

    #[test]
    fn saved_circuit_round_trips_components_and_wires() {
        let mut app = CircuitApp::new();
        app.load_led_demo();

        let json = serde_json::to_string(&SavedCircuit::from_app(&app)).unwrap();
        let saved = serde_json::from_str::<SavedCircuit>(&json).unwrap();
        let snapshot = saved.into_snapshot().unwrap();

        assert_eq!(snapshot.components.len(), app.components.len());
        assert_eq!(snapshot.wires.len(), app.wires.len());
        assert!(snapshot.next_id > app.components.len() as u64);
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Cluster Circuits")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1180.0, 760.0]),
        run_and_return: false,
        ..Default::default()
    };
    eframe::run_native(
        "Cluster Circuits",
        options,
        Box::new(|_cc| Ok(Box::new(CircuitApp::new()))),
    )
}

/*
This is an egui/eframe Rust circuit editor.
Refactor it toward a professional SPICE frontend.

Do not rewrite the whole app.
First add:
1. SPICE netlist export
2. net naming from connected wire/pin graph
3. Export .cir button
4. Keep existing analyze_circuit() as quick live check
5. Support Resistor, Capacitor, Inductor, Battery/VSource, ISource, Diode, LED, Ground

Use serde only if needed.
Keep the UI working.
Explain changes after patch.
*/
