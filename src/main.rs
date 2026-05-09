use eframe::egui;
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use std::collections::{HashSet, VecDeque};
use std::fs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Place(ComponentKind),
    Wire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ComponentKind {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    Led,
    Switch,
    Ground,
    VSource,
    ISource,
    Battery,
    OpAmp,
    Lamp,
    Esp32,
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
struct DragState {
    id: u64,
    offset: Vec2,
}

#[derive(Default)]
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
        }
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
            ComponentKind::Switch => {
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
            ComponentKind::Esp32 => {
                self.counters.esp32 += 1;
                format!("ESP{}", self.counters.esp32)
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
            ComponentKind::Ground => "0V".to_string(),
            ComponentKind::VSource => "5V".to_string(),
            ComponentKind::ISource => "10mA".to_string(),
            ComponentKind::Battery => "9V".to_string(),
            ComponentKind::OpAmp => "LM358".to_string(),
            ComponentKind::Lamp => "12V".to_string(),
            ComponentKind::Esp32 => "ESP32-WROOM".to_string(),
            ComponentKind::Oled => "0.96 I2C".to_string(),
            ComponentKind::Sensor => "I2C sensor".to_string(),
        }
    }

    fn add_component(&mut self, kind: ComponentKind, pos: Pos2) {
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
        self.status = "Component placed. Drag to reposition, R to rotate.".to_string();
    }

    fn add_wire(&mut self, points: Vec<Pos2>) {
        let points = simplify_wire(points);
        if points.len() < 2 {
            return;
        }
        let id = self.next_id();
        self.wires.push(Wire { id, points });
        self.status = "Wire placed.".to_string();
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
                self.components.retain(|c| c.id != id);
                self.status = "Component deleted.".to_string();
            }
            Some(Selection::Wire(id)) => {
                self.wires.retain(|w| w.id != id);
                self.status = "Wire deleted.".to_string();
            }
            None => {}
        }
    }

    fn rotate_selected(&mut self) {
        let Some(Selection::Component(id)) = self.selected else {
            return;
        };
        if let Some(component) = self.components.iter_mut().find(|c| c.id == id) {
            if component_is_module(component) {
                self.status = "Modules stay upright so pin labels remain readable.".to_string();
                return;
            }
            component.rotation = (component.rotation + 90) % 360;
            self.status = "Rotated.".to_string();
        }
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
}

impl eframe::App for CircuitApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(self.tool == Tool::Select, "Select")
                    .clicked()
                {
                    self.tool = Tool::Select;
                    self.draft_wire.clear();
                }
                if ui
                    .selectable_label(self.tool == Tool::Wire, "Wire")
                    .clicked()
                {
                    self.tool = Tool::Wire;
                    self.draft_wire.clear();
                }
                ui.separator();
                ui.checkbox(&mut self.snap, "Snap");
                ui.checkbox(&mut self.orthogonal_wires, "90° wires");
                ui.checkbox(&mut self.show_pins, "Pins");
                ui.checkbox(&mut self.simulate, "Live");
                ui.add(egui::Slider::new(&mut self.grid, 10.0..=40.0).text("Grid"));
                if ui.button("Rotate").clicked() {
                    self.rotate_selected();
                }
                if ui.button("Delete").clicked() {
                    self.delete_selected();
                }
                if ui.button("Export SVG").clicked() {
                    self.export_svg();
                }
                if !self.status.is_empty() {
                    ui.separator();
                    ui.label(&self.status);
                }
            });
        });

        egui::SidePanel::left("palette").show(ctx, |ui| {
            ui.heading("Parts");
            ui.label("Passives");
            part_button(ui, self, "Resistor", ComponentKind::Resistor);
            part_button(ui, self, "Capacitor", ComponentKind::Capacitor);
            part_button(ui, self, "Inductor", ComponentKind::Inductor);
            part_button(ui, self, "Lamp", ComponentKind::Lamp);
            ui.separator();
            ui.label("Semiconductors");
            part_button(ui, self, "Diode", ComponentKind::Diode);
            part_button(ui, self, "LED", ComponentKind::Led);
            part_button(ui, self, "Op Amp", ComponentKind::OpAmp);
            ui.separator();
            ui.label("Sources and IO");
            part_button(ui, self, "Ground", ComponentKind::Ground);
            part_button(ui, self, "Voltage Source", ComponentKind::VSource);
            part_button(ui, self, "Current Source", ComponentKind::ISource);
            part_button(ui, self, "Battery", ComponentKind::Battery);
            part_button(ui, self, "Switch", ComponentKind::Switch);
            ui.separator();
            ui.label("Modules");
            part_button(ui, self, "ESP32", ComponentKind::Esp32);
            part_button(ui, self, "OLED I2C", ComponentKind::Oled);
            part_button(ui, self, "Sensor", ComponentKind::Sensor);
            ui.separator();
            ui.label(format!("{} parts", self.components.len()));
            ui.label(format!("{} wires", self.wires.len()));
            if self.simulate {
                let simulation = analyze_circuit(&self.components, &self.wires);
                if simulation.closed {
                    ui.colored_label(Color32::from_rgb(255, 185, 80), "Current flowing");
                } else {
                    ui.colored_label(Color32::from_rgb(150, 155, 165), "Open circuit");
                }
            }
            ui.separator();
            ui.label("Shortcuts");
            ui.label("R rotate");
            ui.label("Del delete");
            ui.label("Enter finish wire");
            ui.label("Esc select");
        });

        egui::SidePanel::right("inspector").show(ctx, |ui| {
            ui.heading("Inspector");
            match self.selected {
                Some(Selection::Component(id)) => {
                    if let Some(component) = self.components.iter_mut().find(|c| c.id == id) {
                        ui.label(format!("Kind: {:?}", component.kind));
                        ui.horizontal(|ui| {
                            ui.label("Label");
                            ui.text_edit_singleline(&mut component.label);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Value");
                            ui.text_edit_singleline(&mut component.value);
                        });
                        ui.label(format!("Rotation: {}°", component.rotation));
                        ui.label(format!(
                            "Position: {:.0}, {:.0}",
                            component.pos.x, component.pos.y
                        ));
                        if component_is_module(component) {
                            ui.separator();
                            ui.label("Pins");
                            for pin in component_pin_defs(component) {
                                ui.label(format!("{}  {:?}", pin.label, pin.role));
                            }
                        }
                    }
                }
                Some(Selection::Wire(id)) => {
                    if let Some(wire) = self.wires.iter().find(|w| w.id == id) {
                        ui.label("Kind: Wire");
                        ui.label(format!("Points: {}", wire.points.len()));
                        ui.label(format!("Length: {:.0}px", wire_length(wire)));
                    }
                }
                None => {
                    ui.label("Nothing selected");
                }
            }
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

            draw_grid(&painter, rect, self.grid);

            for wire in &self.wires {
                draw_wire(
                    &painter,
                    wire,
                    self.selected == Some(Selection::Wire(wire.id)),
                    simulation.energized_wires.contains(&wire.id),
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
                if let Some(pos) = pointer_in_rect {
                    let pos = snap_pos(pos, rect, self.grid, self.snap);
                    match self.tool {
                        Tool::Select => {
                            if let Some(selection) = hit_test(pos, &self.components, &self.wires) {
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
                        let pos = snap_pos(pos, rect, self.grid, self.snap);
                        if let Some(Selection::Component(id)) =
                            hit_test_component(pos, &self.components)
                        {
                            if let Some(component) = self.components.iter().find(|c| c.id == id) {
                                self.drag = Some(DragState {
                                    id,
                                    offset: pos - component.pos,
                                });
                                self.selected = Some(Selection::Component(id));
                            }
                        }
                    }
                }
            }

            if response.dragged() {
                if let (Some(drag), Some(pos)) = (self.drag.clone(), pointer_in_rect) {
                    if let Some(component) = self.components.iter_mut().find(|c| c.id == drag.id) {
                        let pos = snap_pos(pos, rect, self.grid, self.snap);
                        component.pos = pos - drag.offset;
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

fn hit_test_component(pos: Pos2, components: &[Component]) -> Option<Selection> {
    for component in components.iter().rev() {
        if component_bounds(component).contains(pos) {
            return Some(Selection::Component(component.id));
        }
    }
    None
}

fn hit_test_wire(pos: Pos2, wires: &[Wire]) -> Option<Selection> {
    let threshold = 6.0;
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

fn part_button(ui: &mut egui::Ui, app: &mut CircuitApp, label: &str, kind: ComponentKind) {
    if ui
        .selectable_label(app.tool == Tool::Place(kind), label)
        .clicked()
    {
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
    energized_components: HashSet<u64>,
    energized_wires: HashSet<u64>,
}

fn analyze_circuit(components: &[Component], wires: &[Wire]) -> Simulation {
    let mut nodes = CircuitNodes::default();
    let mut graph: Vec<HashSet<usize>> = Vec::new();
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
                if component_conducts(component) && pin_nodes.len() >= 2 {
                    connect(&mut graph, pin_nodes[0], pin_nodes[1]);
                    component_edges.push((component.id, pin_nodes[0], pin_nodes[1]));
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

    if positive_nodes.is_empty() || return_nodes.is_empty() {
        return Simulation::default();
    }

    let from_positive = reachable_nodes(&graph, &positive_nodes);
    let from_return = reachable_nodes(&graph, &return_nodes);
    let loop_nodes: HashSet<usize> = from_positive.intersection(&from_return).copied().collect();
    if loop_nodes.is_empty() {
        return Simulation::default();
    }

    let energized_components = component_edges
        .into_iter()
        .filter(|(_, a, b)| loop_nodes.contains(a) && loop_nodes.contains(b))
        .map(|(id, _, _)| id)
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

    Simulation {
        closed: true,
        energized_components,
        energized_wires,
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

fn component_conducts(component: &Component) -> bool {
    match component.kind {
        ComponentKind::Resistor
        | ComponentKind::Inductor
        | ComponentKind::Diode
        | ComponentKind::Led
        | ComponentKind::Lamp => true,
        ComponentKind::Switch => {
            let value = component.value.to_lowercase();
            !(value.contains("open") || value.contains("off"))
        }
        ComponentKind::Capacitor | ComponentKind::OpAmp => false,
        ComponentKind::Esp32 | ComponentKind::Oled | ComponentKind::Sensor => false,
        ComponentKind::Ground
        | ComponentKind::VSource
        | ComponentKind::ISource
        | ComponentKind::Battery => false,
    }
}

fn component_is_powered_module(component: &Component) -> bool {
    matches!(
        component.kind,
        ComponentKind::Esp32 | ComponentKind::Oled | ComponentKind::Sensor
    )
}

fn component_bounds(component: &Component) -> Rect {
    let size = component_size(component);
    Rect::from_center_size(component.pos, size)
}

fn component_size(component: &Component) -> Vec2 {
    let (w, h) = match component.kind {
        ComponentKind::Resistor | ComponentKind::Inductor | ComponentKind::Diode => (72.0, 28.0),
        ComponentKind::Capacitor | ComponentKind::Switch | ComponentKind::Battery => (64.0, 32.0),
        ComponentKind::Ground => (40.0, 40.0),
        ComponentKind::VSource
        | ComponentKind::ISource
        | ComponentKind::Lamp
        | ComponentKind::Led => (56.0, 56.0),
        ComponentKind::OpAmp => (82.0, 68.0),
        ComponentKind::Esp32 => (140.0, 160.0),
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
    painter.rect_filled(rect, 0.0, Color32::from_rgb(16, 18, 22));
    let line_color = Color32::from_gray(38);
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
    let major_stroke = Stroke::new(1.0, Color32::from_gray(55));
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
        ComponentKind::Switch => draw_switch(painter, rect, component.rotation, stroke),
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
        for pin in component_pins(component) {
            painter.circle_filled(pin, 3.0, Color32::from_rgb(250, 205, 95));
            painter.circle_stroke(pin, 4.0, Stroke::new(1.0, Color32::from_rgb(40, 35, 20)));
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

fn draw_wire(painter: &egui::Painter, wire: &Wire, selected: bool, energized: bool) {
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
        ComponentKind::Ground => "Ground",
        ComponentKind::VSource => "V Source",
        ComponentKind::ISource => "I Source",
        ComponentKind::Battery => "Battery",
        ComponentKind::OpAmp => "Op Amp",
        ComponentKind::Lamp => "Lamp",
        ComponentKind::Esp32 => "ESP32",
        ComponentKind::Oled => "OLED I2C",
        ComponentKind::Sensor => "Sensor",
    }
}

fn component_is_module(component: &Component) -> bool {
    matches!(
        component.kind,
        ComponentKind::Esp32 | ComponentKind::Oled | ComponentKind::Sensor
    )
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Cluster Circuits",
        options,
        Box::new(|_cc| Ok(Box::new(CircuitApp::new()))),
    )
}
