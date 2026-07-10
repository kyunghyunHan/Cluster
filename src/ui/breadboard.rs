use crate::model::{CircuitNetlist, Component, ComponentKind, NetlistPin};
use eframe::egui;
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BreadboardRoute {
    pub(crate) from_component_id: u64,
    pub(crate) from_label: String,
    pub(crate) from_pin: String,
    pub(crate) to_component_id: u64,
    pub(crate) to_label: String,
    pub(crate) to_pin: String,
    pub(crate) net_id: usize,
    pub(crate) connected: bool,
    pub(crate) purpose: &'static str,
}

#[derive(Debug, Clone)]
pub(crate) struct BreadboardGuide {
    pub(crate) title: String,
    pub(crate) controller: Option<String>,
    pub(crate) peripheral: Option<String>,
    pub(crate) routes: Vec<BreadboardRoute>,
    pub(crate) notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BreadboardAction {
    Select(BreadboardRoute),
    AddJumper(BreadboardRoute),
}

pub(crate) fn build_breadboard_guide(
    components: &[Component],
    netlist: &CircuitNetlist,
) -> BreadboardGuide {
    let controller = components.iter().find(|component| {
        matches!(
            component.kind,
            ComponentKind::Esp32
                | ComponentKind::Esp32S3
                | ComponentKind::Esp32C3
                | ComponentKind::ArduinoUno
                | ComponentKind::RaspberryPiPico
                | ComponentKind::Stm32BluePill
                | ComponentKind::Stm32Nucleo64
        )
    });
    let peripheral = components
        .iter()
        .find(|component| matches!(component.kind, ComponentKind::Oled | ComponentKind::Sensor));

    let mut guide = BreadboardGuide {
        title: "Breadboard wiring assistant".to_string(),
        controller: controller.map(|component| {
            format!(
                "{} ({})",
                component.label,
                crate::component_kind_label(component.kind)
            )
        }),
        peripheral: peripheral.map(|component| {
            format!(
                "{} ({})",
                component.label,
                crate::component_kind_label(component.kind)
            )
        }),
        routes: Vec::new(),
        notes: Vec::new(),
    };

    let (Some(controller), Some(peripheral)) = (controller, peripheral) else {
        guide.notes.push(
            "Place an ESP32/Arduino and an OLED or sensor to get guided jumper wiring.".to_string(),
        );
        guide
            .notes
            .push("Power rails are shown for VCC and GND once pins exist.".to_string());
        return guide;
    };

    let route_specs: Vec<(&str, &str, &'static str)> = match (controller.kind, peripheral.kind) {
        (
            ComponentKind::Esp32 | ComponentKind::Esp32S3 | ComponentKind::Esp32C3,
            ComponentKind::Oled,
        )
        | (
            ComponentKind::Esp32 | ComponentKind::Esp32S3 | ComponentKind::Esp32C3,
            ComponentKind::Sensor,
        ) => vec![
            ("3V3", "VCC", "Power rail"),
            ("GND", "GND", "Common ground"),
            ("GPIO21", "SDA", "I2C data"),
            ("GPIO22", "SCL", "I2C clock"),
        ],
        (ComponentKind::ArduinoUno, ComponentKind::Oled)
        | (ComponentKind::ArduinoUno, ComponentKind::Sensor) => vec![
            ("5V", "VCC", "Power rail"),
            ("GND", "GND", "Common ground"),
            ("A4 SDA", "SDA", "I2C data"),
            ("A5 SCL", "SCL", "I2C clock"),
        ],
        (ComponentKind::Stm32BluePill, ComponentKind::Oled)
        | (ComponentKind::Stm32BluePill, ComponentKind::Sensor) => vec![
            ("3V3", "VCC", "Power rail"),
            ("GND", "GND", "Common ground"),
            ("PB7 SDA", "SDA", "I2C data"),
            ("PB6 SCL", "SCL", "I2C clock"),
        ],
        (ComponentKind::Stm32Nucleo64, ComponentKind::Oled)
        | (ComponentKind::Stm32Nucleo64, ComponentKind::Sensor) => vec![
            ("3V3", "VCC", "Power rail"),
            ("GND", "GND", "Common ground"),
            ("D14 PB9 SDA", "SDA", "I2C data"),
            ("D15 PB8 SCL", "SCL", "I2C clock"),
        ],
        _ => {
            guide.notes.push(format!(
                "Breadboard guidance for {} + {} is not mapped yet.",
                crate::component_kind_label(controller.kind),
                crate::component_kind_label(peripheral.kind)
            ));
            Vec::new()
        }
    };

    for (from_pin, to_pin, purpose) in route_specs {
        if let Some(route) =
            breadboard_route_for(netlist, controller, from_pin, peripheral, to_pin, purpose)
        {
            guide.routes.push(route);
        } else {
            guide.notes.push(format!(
                "Missing pin mapping for {} {} -> {} {}.",
                controller.label, from_pin, peripheral.label, to_pin
            ));
        }
    }

    let missing = guide.routes.iter().filter(|route| !route.connected).count();
    if missing == 0 && !guide.routes.is_empty() {
        guide
            .notes
            .push("All guided jumpers are connected in the schematic.".to_string());
    } else if missing > 0 {
        guide.notes.push(format!(
            "{missing} jumper(s) still need wiring or pin correction."
        ));
    }

    guide
}

fn breadboard_route_for(
    netlist: &CircuitNetlist,
    from_component: &Component,
    from_pin: &str,
    to_component: &Component,
    to_pin: &str,
    purpose: &'static str,
) -> Option<BreadboardRoute> {
    let from = find_netlist_pin(netlist, from_component.id, from_pin)?;
    let to = find_netlist_pin(netlist, to_component.id, to_pin)?;
    Some(BreadboardRoute {
        from_component_id: from_component.id,
        from_label: from_component.label.clone(),
        from_pin: from.pin_name.clone(),
        to_component_id: to_component.id,
        to_label: to_component.label.clone(),
        to_pin: to.pin_name.clone(),
        net_id: from.net_id,
        connected: from.net_id == to.net_id && from.connected_by_wire && to.connected_by_wire,
        purpose,
    })
}

fn find_netlist_pin<'a>(
    netlist: &'a CircuitNetlist,
    component_id: u64,
    pin_query: &str,
) -> Option<&'a NetlistPin> {
    netlist.pins.iter().find(|pin| {
        pin.component_id == component_id
            && (pin.pin_name == pin_query || pin.pin_name.contains(pin_query))
    })
}

pub(crate) fn render_breadboard_view(
    ui: &mut egui::Ui,
    guide: &BreadboardGuide,
) -> Option<BreadboardAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        status_pill(ui, "Schematic synced", BreadboardTone::Live);
        ui.label(
            egui::RichText::new(&guide.title)
                .size(12.0)
                .color(Color32::from_rgb(190, 200, 210)),
        );
    });
    ui.add_space(6.0);

    let (board_rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(360.0), 150.0),
        Sense::hover(),
    );
    draw_breadboard_preview(ui.painter(), board_rect, guide);
    ui.add_space(8.0);

    if let Some(controller) = &guide.controller {
        metric_row(ui, "Controller", controller);
    }
    if let Some(peripheral) = &guide.peripheral {
        metric_row(ui, "Peripheral", peripheral);
    }

    ui.add_space(8.0);
    section_title(ui, "Guided Jumpers");
    if guide.routes.is_empty() {
        ui.label(
            egui::RichText::new("No mapped jumper routes for the current schematic.")
                .size(11.0)
                .color(Color32::from_rgb(150, 160, 170)),
        );
    }
    for route in &guide.routes {
        let tone = if route.connected {
            BreadboardTone::Live
        } else {
            BreadboardTone::Warning
        };
        ui.horizontal(|ui| {
            status_pill(ui, if route.connected { "OK" } else { "TODO" }, tone);
            let label = format!(
                "{} {}  ->  {} {}",
                route.from_label, route.from_pin, route.to_label, route.to_pin
            );
            if ui
                .add_sized(
                    Vec2::new((ui.available_width() - 132.0).max(160.0), 20.0),
                    egui::Button::new(egui::RichText::new(label).size(11.0)),
                )
                .clicked()
            {
                action = Some(BreadboardAction::Select(route.clone()));
            }
            if route.connected {
                ui.add_enabled(
                    false,
                    egui::Button::new(egui::RichText::new("Wired").size(10.5)),
                );
            } else if ui
                .add_sized(
                    Vec2::new(42.0, 20.0),
                    egui::Button::new(egui::RichText::new("Wire").size(10.5)),
                )
                .clicked()
            {
                action = Some(BreadboardAction::AddJumper(route.clone()));
            }
            ui.label(
                egui::RichText::new(route.purpose)
                    .size(10.5)
                    .color(Color32::from_rgb(150, 160, 170)),
            );
        });
    }

    if !guide.notes.is_empty() {
        ui.add_space(8.0);
        section_title(ui, "Notes");
        for note in &guide.notes {
            ui.label(
                egui::RichText::new(note)
                    .size(10.5)
                    .color(Color32::from_rgb(170, 178, 186)),
            );
        }
    }

    action
}

fn draw_breadboard_preview(painter: &egui::Painter, rect: Rect, guide: &BreadboardGuide) {
    painter.rect_filled(rect, 6.0, Color32::from_rgb(30, 34, 39));
    painter.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(70, 78, 88)),
        StrokeKind::Outside,
    );

    let rail_top = rect.top() + 22.0;
    let rail_bottom = rect.bottom() - 22.0;
    painter.line_segment(
        [
            Pos2::new(rect.left() + 18.0, rail_top),
            Pos2::new(rect.right() - 18.0, rail_top),
        ],
        Stroke::new(3.0, Color32::from_rgb(220, 80, 80)),
    );
    painter.line_segment(
        [
            Pos2::new(rect.left() + 18.0, rail_bottom),
            Pos2::new(rect.right() - 18.0, rail_bottom),
        ],
        Stroke::new(3.0, Color32::from_rgb(90, 145, 235)),
    );

    let left_module = Rect::from_min_size(
        Pos2::new(rect.left() + 28.0, rect.top() + 48.0),
        Vec2::new(120.0, 58.0),
    );
    let right_module = Rect::from_min_size(
        Pos2::new(rect.right() - 148.0, rect.top() + 48.0),
        Vec2::new(120.0, 58.0),
    );
    painter.rect_filled(left_module, 4.0, Color32::from_rgb(38, 48, 58));
    painter.rect_filled(right_module, 4.0, Color32::from_rgb(38, 48, 58));
    painter.rect_stroke(
        left_module,
        4.0,
        Stroke::new(1.0, Color32::from_rgb(95, 120, 145)),
        StrokeKind::Outside,
    );
    painter.rect_stroke(
        right_module,
        4.0,
        Stroke::new(1.0, Color32::from_rgb(95, 120, 145)),
        StrokeKind::Outside,
    );
    painter.text(
        left_module.center(),
        Align2::CENTER_CENTER,
        guide.controller.as_deref().unwrap_or("Controller"),
        egui::FontId::proportional(10.5),
        Color32::from_rgb(215, 222, 230),
    );
    painter.text(
        right_module.center(),
        Align2::CENTER_CENTER,
        guide.peripheral.as_deref().unwrap_or("Peripheral"),
        egui::FontId::proportional(10.5),
        Color32::from_rgb(215, 222, 230),
    );

    let route_y_start = rect.top() + 58.0;
    for (index, route) in guide.routes.iter().enumerate() {
        let y = route_y_start + index as f32 * 18.0;
        let color = match route.purpose {
            "Power rail" => Color32::from_rgb(220, 80, 80),
            "Common ground" => Color32::from_rgb(90, 145, 235),
            "I2C data" => Color32::from_rgb(80, 210, 150),
            "I2C clock" => Color32::from_rgb(240, 190, 85),
            _ => Color32::from_rgb(190, 200, 210),
        };
        let stroke = Stroke::new(if route.connected { 2.2 } else { 1.4 }, color);
        let from = Pos2::new(left_module.right(), y);
        let to = Pos2::new(right_module.left(), y);
        painter.line_segment([from, to], stroke);
        if !route.connected {
            painter.circle_stroke(
                Pos2::new((from.x + to.x) * 0.5, y),
                5.0,
                Stroke::new(1.4, Color32::from_rgb(255, 190, 80)),
            );
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum BreadboardTone {
    Live,
    Warning,
}

fn status_pill(ui: &mut egui::Ui, text: &str, tone: BreadboardTone) {
    let (fg, bg, stroke) = match tone {
        BreadboardTone::Live => (
            Color32::from_rgb(150, 245, 185),
            Color32::from_rgb(20, 56, 38),
            Color32::from_rgb(45, 120, 74),
        ),
        BreadboardTone::Warning => (
            Color32::from_rgb(255, 210, 120),
            Color32::from_rgb(68, 50, 22),
            Color32::from_rgb(140, 98, 34),
        ),
    };
    egui::Frame::NONE
        .fill(bg)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(10.0).color(fg));
        });
}

fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .strong()
            .size(11.0)
            .color(Color32::from_rgb(190, 200, 210)),
    );
}

fn metric_row(ui: &mut egui::Ui, label: impl Into<String>, value: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label.into())
                .size(10.5)
                .color(Color32::from_rgb(130, 140, 150)),
        );
        ui.label(
            egui::RichText::new(value.into())
                .size(10.5)
                .color(Color32::from_rgb(212, 218, 226)),
        );
    });
}
