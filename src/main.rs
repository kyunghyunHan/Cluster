use eframe::egui;
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};

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
    Ground,
    VSource,
}

#[derive(Debug, Clone)]
struct Component {
    id: u64,
    kind: ComponentKind,
    pos: Pos2,
    rotation: i32,
    label: String,
}

#[derive(Debug, Clone)]
struct Wire {
    id: u64,
    points: Vec<Pos2>,
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
    ground: usize,
    vsource: usize,
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
            ComponentKind::Ground => {
                self.counters.ground += 1;
                format!("GND{}", self.counters.ground)
            }
            ComponentKind::VSource => {
                self.counters.vsource += 1;
                format!("V{}", self.counters.vsource)
            }
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
        });
        self.status = "Component placed.".to_string();
    }

    fn add_wire(&mut self, points: Vec<Pos2>) {
        if points.len() < 2 {
            return;
        }
        let id = self.next_id();
        self.wires.push(Wire {
            id,
            points,
        });
        self.status = "Wire placed.".to_string();
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
            component.rotation = (component.rotation + 90) % 360;
            self.status = "Rotated.".to_string();
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
                if ui.selectable_label(self.tool == Tool::Wire, "Wire").clicked() {
                    self.tool = Tool::Wire;
                    self.draft_wire.clear();
                }
                ui.separator();
                ui.checkbox(&mut self.snap, "Snap");
                ui.add(egui::Slider::new(&mut self.grid, 10.0..=40.0).text("Grid"));
                if ui.button("Rotate").clicked() {
                    self.rotate_selected();
                }
                if ui.button("Delete").clicked() {
                    self.delete_selected();
                }
                if !self.status.is_empty() {
                    ui.separator();
                    ui.label(&self.status);
                }
            });
        });

        egui::SidePanel::left("palette").show(ctx, |ui| {
            ui.heading("Palette");
            for (label, kind) in [
                ("Resistor", ComponentKind::Resistor),
                ("Capacitor", ComponentKind::Capacitor),
                ("Inductor", ComponentKind::Inductor),
                ("Ground", ComponentKind::Ground),
                ("V Source", ComponentKind::VSource),
            ] {
                if ui
                    .selectable_label(self.tool == Tool::Place(kind), label)
                    .clicked()
                {
                    self.tool = Tool::Place(kind);
                    self.draft_wire.clear();
                }
            }
            ui.separator();
            ui.label("Tips:");
            ui.label("- Click to place or select");
            ui.label("- Drag to move components");
            ui.label("- Double click to finish wire");
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
                        ui.label(format!("Rotation: {}°", component.rotation));
                    }
                }
                Some(Selection::Wire(id)) => {
                    if let Some(wire) = self.wires.iter().find(|w| w.id == id) {
                        ui.label("Kind: Wire");
                        ui.label(format!("Points: {}", wire.points.len()));
                    }
                }
                None => {
                    ui.label("Nothing selected");
                }
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let (response, painter) =
                ui.allocate_painter(available, Sense::click_and_drag());
            let rect = response.rect;

            draw_grid(&painter, rect, self.grid);

            for wire in &self.wires {
                draw_wire(&painter, wire, self.selected == Some(Selection::Wire(wire.id)));
            }

            for component in &self.components {
                draw_component(
                    &painter,
                    component,
                    self.selected == Some(Selection::Component(component.id)),
                );
            }

            let hover_pos = ui.input(|i| i.pointer.hover_pos());
            let pointer_in_rect = hover_pos.filter(|pos| rect.contains(*pos));

            if let Some(pos) = pointer_in_rect {
                let pos = snap_pos(pos, rect, self.grid, self.snap);
                if self.tool == Tool::Wire && !self.draft_wire.is_empty() {
                    let mut preview = self.draft_wire.clone();
                    preview.push(pos);
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
                            self.draft_wire.push(pos);
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
                        if let Some(Selection::Component(id)) = hit_test_component(pos, &self.components)
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

fn component_bounds(component: &Component) -> Rect {
    let size = component_size(component);
    Rect::from_center_size(component.pos, size)
}

fn component_size(component: &Component) -> Vec2 {
    let (w, h) = match component.kind {
        ComponentKind::Resistor | ComponentKind::Inductor => (70.0, 26.0),
        ComponentKind::Capacitor => (60.0, 30.0),
        ComponentKind::Ground => (40.0, 40.0),
        ComponentKind::VSource => (50.0, 50.0),
    };
    if component.rotation % 180 == 0 {
        Vec2::new(w, h)
    } else {
        Vec2::new(h, w)
    }
}

fn draw_grid(painter: &egui::Painter, rect: Rect, grid: f32) {
    let line_color = Color32::from_gray(40);
    let stroke = Stroke::new(1.0, line_color);
    let mut x = rect.left();
    while x <= rect.right() {
        painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], stroke);
        x += grid;
    }
    let mut y = rect.top();
    while y <= rect.bottom() {
        painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], stroke);
        y += grid;
    }
}

fn draw_component(painter: &egui::Painter, component: &Component, selected: bool) {
    let stroke = if selected {
        Stroke::new(2.0, Color32::LIGHT_GREEN)
    } else {
        Stroke::new(2.0, Color32::LIGHT_GRAY)
    };
    let size = component_size(component);
    let rect = Rect::from_center_size(component.pos, size);

    match component.kind {
        ComponentKind::Resistor => draw_resistor(painter, rect, component.rotation, stroke),
        ComponentKind::Capacitor => draw_capacitor(painter, rect, component.rotation, stroke),
        ComponentKind::Inductor => draw_inductor(painter, rect, component.rotation, stroke),
        ComponentKind::Ground => draw_ground(painter, rect, component.rotation, stroke),
        ComponentKind::VSource => draw_vsource(painter, rect, component.rotation, stroke),
    }

    painter.text(
        rect.center_bottom() + Vec2::new(0.0, 6.0),
        Align2::CENTER_TOP,
        &component.label,
        egui::FontId::proportional(12.0),
        Color32::LIGHT_GRAY,
    );
}

fn draw_wire(painter: &egui::Painter, wire: &Wire, selected: bool) {
    let stroke = if selected {
        Stroke::new(2.5, Color32::LIGHT_GREEN)
    } else {
        Stroke::new(2.0, Color32::LIGHT_BLUE)
    };
    for segment in wire.points.windows(2) {
        painter.line_segment([segment[0], segment[1]], stroke);
    }
}

fn draw_wire_preview(painter: &egui::Painter, points: &[Pos2]) {
    let stroke = Stroke::new(1.5, Color32::from_rgb(120, 180, 255));
    for segment in points.windows(2) {
        painter.line_segment([segment[0], segment[1]], stroke);
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

fn draw_ground(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let stem_top = Pos2::new(rect.center().x, rect.top());
    let stem_bottom = Pos2::new(rect.center().x, rect.center().y);
    let line1_left = Pos2::new(rect.center().x - rect.width() * 0.3, rect.center().y);
    let line1_right = Pos2::new(rect.center().x + rect.width() * 0.3, rect.center().y);
    let line2_left = Pos2::new(rect.center().x - rect.width() * 0.2, rect.center().y + rect.height() * 0.2);
    let line2_right = Pos2::new(rect.center().x + rect.width() * 0.2, rect.center().y + rect.height() * 0.2);
    let line3_left = Pos2::new(rect.center().x - rect.width() * 0.1, rect.center().y + rect.height() * 0.35);
    let line3_right = Pos2::new(rect.center().x + rect.width() * 0.1, rect.center().y + rect.height() * 0.35);

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

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Cluster Circuits",
        options,
        Box::new(|_cc| Ok(Box::new(CircuitApp::new()))),
    )
}
