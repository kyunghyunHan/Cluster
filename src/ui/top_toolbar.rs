use crate::app::{AlignDir, Tool};
use eframe::egui;
use egui::{Color32, Stroke, Vec2};

pub(crate) struct TopToolbarModel<'a> {
    pub(crate) tool: Tool,
    pub(crate) zoom: f32,
    pub(crate) simulation_summary: &'a str,
    pub(crate) snap: &'a mut bool,
    pub(crate) orthogonal_wires: &'a mut bool,
    pub(crate) show_pins: &'a mut bool,
    pub(crate) simulate: &'a mut bool,
    pub(crate) show_breadboard_view: &'a mut bool,
    pub(crate) show_voltage_labels: &'a mut bool,
    pub(crate) show_dc_overlay: &'a mut bool,
    pub(crate) show_oscilloscope: &'a mut bool,
    pub(crate) grid: &'a mut f32,
    pub(crate) ac_freq_hz: &'a mut f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TopToolbarAction {
    SelectTool,
    WireTool,
    Undo,
    Redo,
    Rotate,
    Duplicate,
    Delete,
    Align(AlignDir),
    Distribute { vertical: bool },
    ToggleFind,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    SaveJson,
    LoadJson,
    RecoverAutosave,
    TidyWires,
    ExportSvg,
    ExportPng,
    ExportCir,
    ExportNetlistText,
    ExportBomCsv,
    ExportArduinoCode,
    BlankSchematic,
    AddPage,
    RemoveCurrentPage,
}

pub(crate) fn render_top_toolbar(
    ui: &mut egui::Ui,
    model: TopToolbarModel<'_>,
) -> Option<TopToolbarAction> {
    let mut action = None;
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
        if tool_button(ui, model.tool == Tool::Select, "Select").clicked() {
            action = Some(TopToolbarAction::SelectTool);
        }
        if tool_button(ui, model.tool == Tool::Wire, "Wire").clicked() {
            action = Some(TopToolbarAction::WireTool);
        }
        ui.separator();
        for (label, next) in [
            ("Undo", TopToolbarAction::Undo),
            ("Redo", TopToolbarAction::Redo),
            ("Rotate", TopToolbarAction::Rotate),
            ("Duplicate", TopToolbarAction::Duplicate),
            ("Delete", TopToolbarAction::Delete),
        ] {
            if compact_button(ui, label).clicked() {
                action = Some(next);
            }
        }
        ui.separator();
        toolbar_menu(ui, "Align", |ui| {
            for (label, dir) in [
                ("Left edges", AlignDir::Left),
                ("Right edges", AlignDir::Right),
                ("Top edges", AlignDir::Top),
                ("Bottom edges", AlignDir::Bottom),
                ("Center horizontally", AlignDir::CenterH),
                ("Center vertically", AlignDir::CenterV),
            ] {
                if menu_action(ui, label).clicked() {
                    action = Some(TopToolbarAction::Align(dir));
                    ui.close();
                }
            }
            ui.separator();
            if menu_action(ui, "Distribute horizontally").clicked() {
                action = Some(TopToolbarAction::Distribute { vertical: false });
                ui.close();
            }
            if menu_action(ui, "Distribute vertically").clicked() {
                action = Some(TopToolbarAction::Distribute { vertical: true });
                ui.close();
            }
        });
        if compact_button(ui, "Find  Ctrl+F").clicked() {
            action = Some(TopToolbarAction::ToggleFind);
        }
        ui.separator();
        if compact_button(ui, "-").clicked() {
            action = Some(TopToolbarAction::ZoomOut);
        }
        ui.label(
            egui::RichText::new(format!("{:.0}%", model.zoom * 100.0))
                .size(11.0)
                .monospace()
                .color(Color32::from_rgb(180, 190, 200)),
        );
        if compact_button(ui, "+").clicked() {
            action = Some(TopToolbarAction::ZoomIn);
        }
        if compact_button(ui, "Fit").clicked() {
            action = Some(TopToolbarAction::ZoomFit);
        }
        ui.separator();
        toolbar_menu(ui, "View", |ui| {
            ui.checkbox(model.snap, "Snap to grid");
            ui.checkbox(model.orthogonal_wires, "90° wires");
            ui.checkbox(model.show_pins, "Show pins");
            ui.checkbox(model.simulate, "Live simulation");
            ui.checkbox(model.show_breadboard_view, "Breadboard View");
            ui.checkbox(model.show_voltage_labels, "Voltage labels on wires");
            ui.checkbox(model.show_dc_overlay, "V/I badges on components");
            ui.checkbox(model.show_oscilloscope, "DC/AC Analysis panel");
            ui.add_sized(
                Vec2::new(180.0, 18.0),
                egui::Slider::new(model.grid, 10.0..=40.0).text("Grid"),
            );
            ui.add_sized(
                Vec2::new(180.0, 18.0),
                egui::Slider::new(model.ac_freq_hz, 1.0..=1_000_000.0)
                    .text("AC freq (Hz)")
                    .logarithmic(true),
            );
        });
        toolbar_menu(ui, "Actions", |ui| {
            for (label, next) in [
                ("Save JSON", TopToolbarAction::SaveJson),
                ("Load JSON", TopToolbarAction::LoadJson),
                ("Recover Auto Backup", TopToolbarAction::RecoverAutosave),
            ] {
                if menu_action(ui, label).clicked() {
                    action = Some(next);
                    ui.close();
                }
            }
            ui.separator();
            if menu_action(ui, "Tidy all wires  Ctrl+T").clicked() {
                action = Some(TopToolbarAction::TidyWires);
                ui.close();
            }
            ui.separator();
            for (label, next) in [
                ("Export SVG", TopToolbarAction::ExportSvg),
                ("Export PNG (screenshot)", TopToolbarAction::ExportPng),
                ("Export CIR", TopToolbarAction::ExportCir),
                ("Export Netlist TXT", TopToolbarAction::ExportNetlistText),
                ("Export BOM CSV", TopToolbarAction::ExportBomCsv),
                ("Generate Arduino Code", TopToolbarAction::ExportArduinoCode),
            ] {
                if menu_action(ui, label).clicked() {
                    action = Some(next);
                    ui.close();
                }
            }
            ui.separator();
            if menu_action(ui, "Blank schematic").clicked() {
                action = Some(TopToolbarAction::BlankSchematic);
                ui.close();
            }
            ui.separator();
            if menu_action(ui, "Add page").clicked() {
                action = Some(TopToolbarAction::AddPage);
                ui.close();
            }
            if menu_action(ui, "Remove current page").clicked() {
                action = Some(TopToolbarAction::RemoveCurrentPage);
                ui.close();
            }
        });
        ui.separator();
        status_pill(ui, model.simulation_summary);
    });
    action
}

fn tool_button(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let (fill, stroke, text) = if selected {
        (
            Color32::from_rgb(42, 78, 92),
            Stroke::new(1.0, Color32::from_rgb(100, 178, 255)),
            Color32::from_rgb(240, 248, 255),
        )
    } else {
        (
            Color32::from_rgb(28, 33, 39),
            Stroke::new(1.0, Color32::from_rgb(48, 56, 64)),
            Color32::from_rgb(214, 222, 230),
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

fn menu_action(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add_sized(
        Vec2::new(180.0, 27.0),
        egui::Button::new(egui::RichText::new(label).color(Color32::from_rgb(216, 224, 232)))
            .fill(Color32::from_rgb(28, 33, 39))
            .stroke(Stroke::new(1.0, Color32::from_rgb(48, 56, 64))),
    )
}

fn toolbar_menu(ui: &mut egui::Ui, label: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.menu_button(
        egui::RichText::new(label)
            .size(11.0)
            .color(Color32::from_rgb(215, 222, 230)),
        add_contents,
    );
}

fn status_pill(ui: &mut egui::Ui, text: &str) {
    egui::Frame::NONE
        .fill(Color32::from_rgb(54, 42, 22))
        .stroke(Stroke::new(1.0, Color32::from_rgb(132, 92, 34)))
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .size(11.0)
                    .strong()
                    .color(Color32::from_rgb(255, 198, 92)),
            );
        });
}
