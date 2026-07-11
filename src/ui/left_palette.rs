use crate::app::Tool;
use crate::model::ComponentKind;
use eframe::egui;
use egui::{Color32, Stroke, Vec2};

pub(crate) enum PaletteAction {
    PlacePart {
        kind: ComponentKind,
        label: &'static str,
    },
    PlaceCustomPart {
        part_id: String,
    },
    ReloadCustomParts,
    CreateSamplePart,
    LoadTemplate(PaletteTemplate),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteTemplate {
    Esp32Led,
    Esp32Oled,
    Esp32Button,
    ArduinoLed,
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
    selected_custom_part: Option<&str>,
) -> Option<PaletteAction> {
    let mut action = None;
    template_section(ui, &mut action);
    my_parts_section(ui, filter, selected_custom_part, &mut action);
    if let Some(kind) = selected {
        part_section(
            ui,
            "Recently Used",
            PaletteSectionMode::Open,
            &[entry_for_kind(kind)],
            filter,
            selected,
            &mut action,
        );
    }
    part_section(
        ui,
        "Favorites",
        PaletteSectionMode::Open,
        &[
            part(
                "R",
                "Resistor",
                "limit current / divide voltage",
                ComponentKind::Resistor,
            ),
            part("LED", "LED", "visible output indicator", ComponentKind::Led),
            part(
                "GND",
                "Ground",
                "0V return reference",
                ComponentKind::Ground,
            ),
            part("ESP", "ESP32", "Wi-Fi MCU dev board", ComponentKind::Esp32),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "Basics",
        PaletteSectionMode::Open,
        &[
            part(
                "R",
                "Resistor",
                "sets current or voltage",
                ComponentKind::Resistor,
            ),
            part(
                "C",
                "Capacitor",
                "stores charge / filters",
                ComponentKind::Capacitor,
            ),
            part(
                "L",
                "Inductor",
                "coil / energy storage",
                ComponentKind::Inductor,
            ),
            part(
                "POT",
                "Potentiometer",
                "adjustable resistor",
                ComponentKind::Potentiometer,
            ),
            part("F", "Fuse", "overcurrent protection", ComponentKind::Fuse),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "Power",
        PaletteSectionMode::Open,
        &[
            part("GND", "Ground", "common return", ComponentKind::Ground),
            part(
                "V",
                "Voltage Source",
                "ideal DC supply",
                ComponentKind::VSource,
            ),
            part(
                "I",
                "Current Source",
                "ideal current source",
                ComponentKind::ISource,
            ),
            part("BAT", "Battery", "portable supply", ComponentKind::Battery),
            part(
                "REG",
                "Voltage Reg",
                "regulated output",
                ComponentKind::VoltageReg,
            ),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "Diodes",
        PaletteSectionMode::Open,
        &[
            part("D", "Diode", "one-way conduction", ComponentKind::Diode),
            part(
                "Z",
                "Zener",
                "voltage clamp/reference",
                ComponentKind::ZenerDiode,
            ),
            part("LED", "LED", "light output", ComponentKind::Led),
            part(
                "SBD",
                "Schottky",
                "low-drop diode",
                ComponentKind::SchottkyDiode,
            ),
            part("TVS", "TVS Diode", "surge clamp", ComponentKind::TvsDiode),
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
            part(
                "NPN",
                "NPN BJT",
                "low-side switch",
                ComponentKind::NpnTransistor,
            ),
            part(
                "PNP",
                "PNP BJT",
                "high-side switch",
                ComponentKind::PnpTransistor,
            ),
            part(
                "NMOS",
                "N-MOSFET",
                "logic power switch",
                ComponentKind::Nmosfet,
            ),
            part(
                "PMOS",
                "P-MOSFET",
                "high-side MOSFET",
                ComponentKind::Pmosfet,
            ),
            part(
                "OPTO",
                "Optocoupler",
                "isolated switching",
                ComponentKind::Optocoupler,
            ),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "MCU",
        PaletteSectionMode::Open,
        &[
            part("ESP", "ESP32 WROOM", "Wi-Fi MCU", ComponentKind::Esp32),
            part("S3", "ESP32-S3", "USB/Wi-Fi MCU", ComponentKind::Esp32S3),
            part("C3", "ESP32-C3", "RISC-V Wi-Fi MCU", ComponentKind::Esp32C3),
            part(
                "UNO",
                "Arduino UNO",
                "beginner MCU board",
                ComponentKind::ArduinoUno,
            ),
            part(
                "PICO",
                "Pi Pico",
                "RP2040 board",
                ComponentKind::RaspberryPiPico,
            ),
            part(
                "STM",
                "STM32 Blue Pill",
                "STM32 dev board",
                ComponentKind::Stm32BluePill,
            ),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "Sensors",
        PaletteSectionMode::Collapsed,
        &[
            part(
                "I2C",
                "Sensor I2C",
                "generic I2C module",
                ComponentKind::Sensor,
            ),
            part("DHT", "DHT11", "temp/humidity", ComponentKind::Dht11),
            part("DHT", "DHT22", "temp/humidity", ComponentKind::Dht22),
            part("US", "HC-SR04", "ultrasonic range", ComponentKind::HcSr04),
            part(
                "PIR",
                "PIR Motion",
                "motion detect",
                ComponentKind::PirSensor,
            ),
            part(
                "PT",
                "Phototransistor",
                "light sensor",
                ComponentKind::Phototransistor,
            ),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "Output",
        PaletteSectionMode::Open,
        &[
            part("LED", "LED", "simple indicator", ComponentKind::Led),
            part("OLED", "OLED I2C", "small display", ComponentKind::Oled),
            part("BUZ", "Buzzer", "sound output", ComponentKind::Buzzer),
            part(
                "PIX",
                "NeoPixel",
                "addressable RGB LED",
                ComponentKind::NeoPixel,
            ),
            part("LAMP", "Lamp", "larger load", ComponentKind::Lamp),
            part(
                "7SEG",
                "7-Seg Display",
                "numeric output",
                ComponentKind::Display7Seg,
            ),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "IC",
        PaletteSectionMode::Collapsed,
        &[
            part(
                "555",
                "555 Timer",
                "timer / oscillator",
                ComponentKind::Timer555,
            ),
            part("OP", "Op Amp", "analogue amplifier", ComponentKind::OpAmp),
            part(
                "IC",
                "Generic IC",
                "custom pin IC",
                ComponentKind::GenericIc,
            ),
            part("NOT", "NOT Gate", "inverter logic", ComponentKind::LogicNot),
            part("AND", "AND Gate", "logic gate", ComponentKind::LogicAnd),
            part("OR", "OR Gate", "logic gate", ComponentKind::LogicOr),
        ],
        filter,
        selected,
        &mut action,
    );
    part_section(
        ui,
        "PCB",
        PaletteSectionMode::Collapsed,
        &[
            part(
                "BB",
                "Breadboard",
                "prototype board",
                ComponentKind::Breadboard,
            ),
            part("XTAL", "Crystal", "clock source", ComponentKind::Crystal),
            part(
                "VR",
                "Voltage Ref",
                "precision reference",
                ComponentKind::VoltageRef,
            ),
            part("MOV", "Varistor", "surge absorber", ComponentKind::Varistor),
            part(
                "NTC",
                "Thermistor",
                "temperature resistor",
                ComponentKind::Thermistor,
            ),
            part(
                "XFMR",
                "Transformer",
                "coupled coils",
                ComponentKind::Transformer,
            ),
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
            part(
                "V",
                "Voltmeter",
                "measure voltage",
                ComponentKind::Voltmeter,
            ),
            part("A", "Ammeter", "measure current", ComponentKind::Ammeter),
            part(
                "NET",
                "Net Label",
                "name a connection",
                ComponentKind::NetLabel,
            ),
        ],
        filter,
        selected,
        &mut action,
    );
    action
}

#[derive(Debug, Clone, Copy)]
struct PartEntry {
    icon: &'static str,
    name: &'static str,
    description: &'static str,
    kind: ComponentKind,
}

const fn part(
    icon: &'static str,
    name: &'static str,
    description: &'static str,
    kind: ComponentKind,
) -> PartEntry {
    PartEntry {
        icon,
        name,
        description,
        kind,
    }
}

/// User parts loaded from `cluster_parts/*.json`. Always rendered (even when
/// empty) so the folder workflow is discoverable from the palette itself.
fn my_parts_section(
    ui: &mut egui::Ui,
    filter: &str,
    selected_custom_part: Option<&str>,
    action: &mut Option<PaletteAction>,
) {
    use crate::model::{custom_part, custom_part_list};

    let needle = filter.to_lowercase();
    palette_frame(ui, "My Parts", PaletteSectionMode::Open, |ui| {
        let parts = custom_part_list();
        if parts.is_empty() {
            ui.label(
                egui::RichText::new(format!(
                    "Drop part JSON files into {}/ and press Reload.",
                    crate::model::CUSTOM_PARTS_DIR
                ))
                .size(9.0)
                .color(Color32::from_rgb(145, 156, 168)),
            );
        }
        for (part_id, name) in &parts {
            if !needle.is_empty()
                && !name.to_lowercase().contains(&needle)
                && !part_id.to_lowercase().contains(&needle)
            {
                continue;
            }
            let description = custom_part(part_id)
                .map(|def| {
                    if def.description.is_empty() {
                        format!("{} pins", def.left_pins.len() + def.right_pins.len())
                    } else {
                        def.description
                    }
                })
                .unwrap_or_default();
            let is_selected = selected_custom_part == Some(part_id.as_str());
            if palette_card(ui, "USR", name, &description, is_selected).clicked() {
                *action = Some(PaletteAction::PlaceCustomPart {
                    part_id: part_id.clone(),
                });
            }
        }
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            if ui.small_button("Reload").clicked() {
                *action = Some(PaletteAction::ReloadCustomParts);
            }
            if ui.small_button("Create sample part").clicked() {
                *action = Some(PaletteAction::CreateSamplePart);
            }
        });
    });
}

fn template_section(ui: &mut egui::Ui, action: &mut Option<PaletteAction>) {
    palette_frame(ui, "Start from template", PaletteSectionMode::Open, |ui| {
        for (label, template) in [
            ("ESP32 LED", PaletteTemplate::Esp32Led),
            ("ESP32 OLED I2C", PaletteTemplate::Esp32Oled),
            ("ESP32 Button", PaletteTemplate::Esp32Button),
            ("Arduino LED", PaletteTemplate::ArduinoLed),
        ] {
            if compact_card(ui, "★", label, "beginner starter").clicked() {
                *action = Some(PaletteAction::LoadTemplate(template));
            }
        }
    });
}

fn part_section(
    ui: &mut egui::Ui,
    title: &str,
    mode: PaletteSectionMode,
    parts: &[PartEntry],
    filter: &str,
    selected: Option<ComponentKind>,
    action: &mut Option<PaletteAction>,
) {
    let needle = filter.to_lowercase();
    let filtered: Vec<_> = if needle.is_empty() {
        parts.to_vec()
    } else {
        parts
            .iter()
            .copied()
            .filter(|part| {
                part.name.to_lowercase().contains(&needle)
                    || part.description.to_lowercase().contains(&needle)
                    || part.icon.to_lowercase().contains(&needle)
            })
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
        for part in filtered {
            if part_button(ui, part, selected).clicked() {
                *action = Some(PaletteAction::PlacePart {
                    kind: part.kind,
                    label: part.name,
                });
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
    part: PartEntry,
    selected: Option<ComponentKind>,
) -> egui::Response {
    let is_selected = selected == Some(part.kind);
    palette_card(ui, part.icon, part.name, part.description, is_selected)
}

fn palette_card(
    ui: &mut egui::Ui,
    icon: &str,
    name: &str,
    description: &str,
    is_selected: bool,
) -> egui::Response {
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
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 2.0);
            egui::Frame::NONE
                .fill(fill)
                .stroke(stroke)
                .corner_radius(egui::CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            Vec2::new(32.0, 24.0),
                            egui::Label::new(
                                egui::RichText::new(icon).size(10.5).strong().color(color),
                            ),
                        );
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(name).size(10.5).strong().color(color));
                            ui.label(
                                egui::RichText::new(description)
                                    .size(9.0)
                                    .color(Color32::from_rgb(145, 156, 168)),
                            );
                        });
                    });
                })
                .response
        })
        .inner;
    response.interact(egui::Sense::click())
}

fn compact_card(
    ui: &mut egui::Ui,
    icon: &'static str,
    name: &'static str,
    description: &'static str,
) -> egui::Response {
    part_button(
        ui,
        part(icon, name, description, ComponentKind::TextNote),
        None,
    )
}

fn entry_for_kind(kind: ComponentKind) -> PartEntry {
    match kind {
        ComponentKind::Resistor => part("R", "Resistor", "sets current or voltage", kind),
        ComponentKind::Capacitor => part("C", "Capacitor", "stores charge / filters", kind),
        ComponentKind::Led => part("LED", "LED", "visible output indicator", kind),
        ComponentKind::Ground => part("GND", "Ground", "0V return reference", kind),
        ComponentKind::Esp32 => part("ESP", "ESP32", "Wi-Fi MCU dev board", kind),
        ComponentKind::ArduinoUno => part("UNO", "Arduino UNO", "beginner MCU board", kind),
        _ => part("●", "Selected part", "recently selected", kind),
    }
}
