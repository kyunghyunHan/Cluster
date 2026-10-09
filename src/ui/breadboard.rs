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
    let controllers: Vec<_> = components
        .iter()
        .filter(|c| {
            matches!(
                c.kind,
                ComponentKind::Esp32
                    | ComponentKind::Esp32S3
                    | ComponentKind::Esp32C3
                    | ComponentKind::ArduinoUno
                    | ComponentKind::RaspberryPiPico
                    | ComponentKind::Stm32BluePill
                    | ComponentKind::Stm32Nucleo64
            )
        })
        .collect();
    let peripherals: Vec<_> = components
        .iter()
        .filter(|c| matches!(c.kind, ComponentKind::Oled | ComponentKind::Sensor))
        .collect();
    let mut guide = BreadboardGuide {
        title: "Breadboard wiring assistant".into(),
        controller: controllers
            .first()
            .map(|c| format!("{} ({})", c.label, crate::component_kind_label(c.kind))),
        peripheral: peripherals.first().map(|c| {
            format!(
                "{} ({}){}",
                c.label,
                crate::component_kind_label(c.kind),
                if peripherals.len() > 1 {
                    format!(" + {} module(s)", peripherals.len() - 1)
                } else {
                    String::new()
                }
            )
        }),
        routes: Vec::new(),
        notes: Vec::new(),
    };
    if controllers.is_empty() || peripherals.is_empty() {
        guide
            .notes
            .push("Place a controller and an OLED or I2C sensor to get guided wiring.".into());
        return guide;
    }
    if controllers.len() != 1 {
        guide.controller = Some(format!("{} controllers", controllers.len()));
        guide.notes.push(
            "Guided wiring requires one controller per page; separate controllers into pages."
                .into(),
        );
        return guide;
    }
    let controller = controllers[0];
    let power = if controller.kind == ComponentKind::ArduinoUno {
        "5V"
    } else {
        "3V3"
    };
    for peripheral in peripherals {
        for (from_query, to_pin, purpose) in [
            (power, "VCC", "Power rail"),
            ("GND", "GND", "Common ground"),
            ("SDA", "SDA", "I2C data"),
            ("SCL", "SCL", "I2C clock"),
        ] {
            if let Some(route) =
                breadboard_route_for(netlist, controller, from_query, peripheral, to_pin, purpose)
            {
                guide.routes.push(route);
            } else {
                guide.notes.push(format!(
                    "Missing pin mapping: {} {from_query} -> {} {to_pin}.",
                    controller.label, peripheral.label
                ));
            }
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
    let to = find_netlist_pin(netlist, to_component.id, to_pin)?;
    // Preserve a wired alternate GPIO or duplicate GND pin before suggesting defaults.
    let is_signal = matches!(from_pin, "SDA" | "SCL");
    let matches_default = |pin: &&NetlistPin| {
        pin.component_id == from_component.id
            && pin
                .pin_name
                .split_whitespace()
                .any(|token| token == from_pin)
    };
    let from = netlist
        .pins
        .iter()
        .find(|pin| {
            pin.component_id == from_component.id
                && pin.net_id == to.net_id
                && (matches_default(pin)
                    || (is_signal
                        && crate::engine::validation::pin_is_microcontroller_gpio(pin)
                        && !crate::engine::validation::pin_is_i2c_named(&pin.pin_name)))
        })
        .or_else(|| netlist.pins.iter().find(matches_default))?;
    Some(BreadboardRoute {
        from_component_id: from_component.id,
        from_label: from_component.label.clone(),
        from_pin: from.pin_name.clone(),
        to_component_id: to_component.id,
        to_label: to_component.label.clone(),
        to_pin: to.pin_name.clone(),
        net_id: from.net_id,
        connected: from.net_id == to.net_id,
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
            && pin
                .pin_name
                .split_whitespace()
                .any(|token| token == pin_query)
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

    let (board_rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 150.0), Sense::hover());
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
        ui.horizontal_wrapped(|ui| {
            status_pill(ui, if route.connected { "OK" } else { "TODO" }, tone);
            let label = format!(
                "{} {}  ->  {} {}",
                route.from_label, route.from_pin, route.to_label, route.to_pin
            );
            if ui
                .add_sized(
                    Vec2::new((ui.available_width() - 132.0).clamp(80.0, 380.0), 20.0),
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
        Stroke::new(1.0_f32, Color32::from_rgb(70, 78, 88)),
        StrokeKind::Outside,
    );

    let rail_top = rect.top() + 22.0;
    let rail_bottom = rect.bottom() - 22.0;
    painter.line_segment(
        [
            Pos2::new(rect.left() + 18.0, rail_top),
            Pos2::new(rect.right() - 18.0, rail_top),
        ],
        Stroke::new(3.0_f32, Color32::from_rgb(220, 80, 80)),
    );
    painter.line_segment(
        [
            Pos2::new(rect.left() + 18.0, rail_bottom),
            Pos2::new(rect.right() - 18.0, rail_bottom),
        ],
        Stroke::new(3.0_f32, Color32::from_rgb(90, 145, 235)),
    );

    let module_width = (rect.width() * 0.3).min(120.0);
    let left_module = Rect::from_min_size(
        Pos2::new(rect.left() + 14.0, rect.top() + 48.0),
        Vec2::new(module_width, 58.0),
    );
    let right_module = Rect::from_min_size(
        Pos2::new(rect.right() - 14.0 - module_width, rect.top() + 48.0),
        Vec2::new(module_width, 58.0),
    );
    painter.rect_filled(left_module, 4.0, Color32::from_rgb(38, 48, 58));
    painter.rect_filled(right_module, 4.0, Color32::from_rgb(38, 48, 58));
    painter.rect_stroke(
        left_module,
        4.0,
        Stroke::new(1.0_f32, Color32::from_rgb(95, 120, 145)),
        StrokeKind::Outside,
    );
    painter.rect_stroke(
        right_module,
        4.0,
        Stroke::new(1.0_f32, Color32::from_rgb(95, 120, 145)),
        StrokeKind::Outside,
    );
    painter.with_clip_rect(left_module).text(
        left_module.center(),
        Align2::CENTER_CENTER,
        guide.controller.as_deref().unwrap_or("Controller"),
        egui::FontId::proportional(10.5),
        Color32::from_rgb(215, 222, 230),
    );
    painter.with_clip_rect(right_module).text(
        right_module.center(),
        Align2::CENTER_CENTER,
        guide.peripheral.as_deref().unwrap_or("Peripheral"),
        egui::FontId::proportional(10.5),
        Color32::from_rgb(215, 222, 230),
    );

    let route_y_start = rect.top() + 58.0;
    // The preview shows the first module; all modules have actionable rows below.
    for (index, route) in guide.routes.iter().take(4).enumerate() {
        let y = route_y_start + index as f32 * 18.0;
        let color = match route.purpose {
            "Power rail" => Color32::from_rgb(220, 80, 80),
            "Common ground" => Color32::from_rgb(90, 145, 235),
            "I2C data" => Color32::from_rgb(80, 210, 150),
            "I2C clock" => Color32::from_rgb(240, 190, 85),
            _ => Color32::from_rgb(190, 200, 210),
        };
        let stroke = Stroke::new(if route.connected { 2.2_f32 } else { 1.4_f32 }, color);
        let from = Pos2::new(left_module.right(), y);
        let to = Pos2::new(right_module.left(), y);
        painter.line_segment([from, to], stroke);
        if !route.connected {
            painter.circle_stroke(
                Pos2::new((from.x + to.x) * 0.5, y),
                5.0,
                Stroke::new(1.4_f32, Color32::from_rgb(255, 190, 80)),
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
        .stroke(Stroke::new(1.0_f32, stroke))
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
