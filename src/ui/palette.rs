use crate::app::Tool;
use crate::model::ComponentKind;
use eframe::egui;
use egui::{Color32, Stroke, Vec2};

pub(crate) enum PaletteAction {
    PlacePart {
        kind: ComponentKind,
        label: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaletteSectionMode {
    Open,
    Collapsed,
}

pub(crate) fn selected_part(tool: Tool) -> Option<ComponentKind> {
    match tool {
        Tool::Place(kind) => Some(kind),
        _ => None,
    }
}

pub(crate) fn render_parts_palette(
    ui: &mut egui::Ui,
    filter: &str,
    selected: Option<ComponentKind>,
) -> Option<PaletteAction> {
    let mut action = None;
    part_section(
        ui,
        "Passives",
        PaletteSectionMode::Open,
        &[
            ("Resistor  Q", ComponentKind::Resistor),
            ("Capacitor  A", ComponentKind::Capacitor),
            ("Inductor  I", ComponentKind::Inductor),
            ("Potentiometer", ComponentKind::Potentiometer),
            ("Lamp", ComponentKind::Lamp),
            ("Fuse", ComponentKind::Fuse),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "Semiconductors",
        PaletteSectionMode::Open,
        &[
            ("Diode  D", ComponentKind::Diode),
            ("Zener  Z", ComponentKind::ZenerDiode),
            ("LED  E", ComponentKind::Led),
            ("Op Amp", ComponentKind::OpAmp),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "Transistors",
        PaletteSectionMode::Open,
        &[
            ("NPN BJT  N", ComponentKind::NpnTransistor),
            ("PNP BJT  P", ComponentKind::PnpTransistor),
            ("N-MOSFET", ComponentKind::Nmosfet),
            ("P-MOSFET", ComponentKind::Pmosfet),
            ("Voltage Reg", ComponentKind::VoltageReg),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "Logic Gates",
        PaletteSectionMode::Collapsed,
        &[
            ("NOT", ComponentKind::LogicNot),
            ("AND", ComponentKind::LogicAnd),
            ("OR", ComponentKind::LogicOr),
            ("NAND", ComponentKind::LogicNand),
            ("NOR", ComponentKind::LogicNor),
            ("XOR", ComponentKind::LogicXor),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "Sources and IO",
        PaletteSectionMode::Open,
        &[
            ("Ground  G", ComponentKind::Ground),
            ("Voltage Source", ComponentKind::VSource),
            ("Current Source", ComponentKind::ISource),
            ("Battery  B", ComponentKind::Battery),
            ("Switch", ComponentKind::Switch),
            ("Push Button", ComponentKind::PushButton),
            ("Slide Switch", ComponentKind::SlideSwitch),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "Modules",
        PaletteSectionMode::Collapsed,
        &[
            ("ESP32 WROOM", ComponentKind::Esp32),
            ("ESP32-S3", ComponentKind::Esp32S3),
            ("ESP32-C3", ComponentKind::Esp32C3),
            ("Arduino UNO", ComponentKind::ArduinoUno),
            ("Pi Pico", ComponentKind::RaspberryPiPico),
            ("STM32 Blue Pill", ComponentKind::Stm32BluePill),
            ("STM32 Nucleo-64", ComponentKind::Stm32Nucleo64),
            ("Breadboard", ComponentKind::Breadboard),
            ("OLED I2C", ComponentKind::Oled),
            ("Sensor I2C", ComponentKind::Sensor),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "Sensors",
        PaletteSectionMode::Open,
        &[
            ("DHT11", ComponentKind::Dht11),
            ("DHT22", ComponentKind::Dht22),
            ("HC-SR04", ComponentKind::HcSr04),
            ("PIR Motion", ComponentKind::PirSensor),
            ("Buzzer", ComponentKind::Buzzer),
            ("NeoPixel", ComponentKind::NeoPixel),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "Actuators",
        PaletteSectionMode::Collapsed,
        &[
            ("Relay", ComponentKind::Relay),
            ("DC Motor", ComponentKind::DcMotor),
            ("Servo", ComponentKind::Servo),
            ("Motor Driver", ComponentKind::MotorDriver),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "Advanced",
        PaletteSectionMode::Collapsed,
        &[
            ("Net Label", ComponentKind::NetLabel),
            ("555 Timer", ComponentKind::Timer555),
            ("Crystal", ComponentKind::Crystal),
            ("Transformer", ComponentKind::Transformer),
            ("7-Seg Display", ComponentKind::Display7Seg),
            ("Thermistor", ComponentKind::Thermistor),
            ("Varistor", ComponentKind::Varistor),
            ("Voltage Ref", ComponentKind::VoltageRef),
            ("Schottky", ComponentKind::SchottkyDiode),
            ("TVS Diode", ComponentKind::TvsDiode),
            ("Phototransistor", ComponentKind::Phototransistor),
            ("Optocoupler", ComponentKind::Optocoupler),
            ("Generic IC", ComponentKind::GenericIc),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "Measurement",
        PaletteSectionMode::Collapsed,
        &[
            ("Voltmeter", ComponentKind::Voltmeter),
            ("Ammeter", ComponentKind::Ammeter),
        ],
        filter,
        selected,
        &mut action,
    );
    action
}

fn part_section(
    ui: &mut egui::Ui,
    title: &str,
    mode: PaletteSectionMode,
    parts: &[(&'static str, ComponentKind)],
    filter: &str,
    selected: Option<ComponentKind>,
    action: &mut Option<PaletteAction>,
) {
    let needle = filter.to_lowercase();
    let filtered: Vec<_> = if needle.is_empty() {
        parts.iter().copied().collect()
    } else {
        parts
            .iter()
            .copied()
            .filter(|(label, _)| label.to_lowercase().contains(&needle))
            .collect()
    };
    if filtered.is_empty() {
        return;
    }
    let effective_mode = if needle.is_empty() {
        mode
    } else {
        PaletteSectionMode::Open
    };
    palette_frame(ui, title, effective_mode, |ui| {
        for (label, kind) in filtered {
            if part_button(ui, label, kind, selected).clicked() {
                *action = Some(PaletteAction::PlacePart { kind, label });
            }
        }
    });
}

fn palette_frame(
    ui: &mut egui::Ui,
    title: &str,
    mode: PaletteSectionMode,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    ui.add_space(3.0);
    egui::Frame::NONE
        .fill(Color32::from_rgb(23, 28, 35))
        .stroke(Stroke::new(1.0, Color32::from_rgb(58, 68, 80)))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(5, 3))
        .show(ui, |ui| {
            let title = egui::RichText::new(title.to_uppercase())
                .size(10.0)
                .strong()
                .color(Color32::from_rgb(190, 204, 218));
            match mode {
                PaletteSectionMode::Open => {
                    ui.label(title);
                    ui.add_space(3.0);
                    add_contents(ui);
                }
                PaletteSectionMode::Collapsed => {
                    egui::CollapsingHeader::new(title)
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.add_space(2.0);
                            add_contents(ui);
                        });
                }
            }
        });
}

fn part_button(
    ui: &mut egui::Ui,
    label: &'static str,
    kind: ComponentKind,
    selected: Option<ComponentKind>,
) -> egui::Response {
    let is_selected = selected == Some(kind);
    let (fill, stroke, color) = if is_selected {
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
    ui.add_sized(
        Vec2::new(ui.available_width(), 22.0),
        egui::Button::new(egui::RichText::new(label).size(10.5).color(color))
            .fill(fill)
            .stroke(stroke),
    )
}
