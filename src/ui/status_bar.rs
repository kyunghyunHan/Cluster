use eframe::egui;
use egui::{Color32, Pos2};

pub(crate) struct StatusBarModel<'a> {
    pub(crate) active_tool: String,
    pub(crate) grid: f32,
    pub(crate) zoom: f32,
    pub(crate) snap: bool,
    pub(crate) simulation_text: String,
    pub(crate) simulation_color: Color32,
    pub(crate) selection: String,
    pub(crate) component_count: usize,
    pub(crate) wire_count: usize,
    pub(crate) cursor_world: Option<Pos2>,
    pub(crate) dirty: bool,
    pub(crate) page_name: &'a str,
}

pub(crate) fn render_status_bar(ui: &mut egui::Ui, model: StatusBarModel<'_>) {
    ui.horizontal_wrapped(|ui| {
        ui.monospace(format!("Tool: {}", model.active_tool));
        ui.separator();
        ui.monospace(format!("Page: {}", model.page_name));
        ui.separator();
        ui.monospace(format!("Grid: {:.0}px", model.grid));
        ui.separator();
        ui.monospace(format!("Zoom: {:.0}%", model.zoom * 100.0));
        ui.separator();
        ui.colored_label(
            if model.snap {
                Color32::from_rgb(100, 220, 160)
            } else {
                Color32::from_rgb(130, 130, 140)
            },
            if model.snap { "SNAP" } else { "snap off" },
        );
        ui.separator();
        ui.colored_label(model.simulation_color, model.simulation_text);
        ui.separator();
        ui.label(model.selection);
        ui.separator();
        ui.monospace(format!(
            "C:{} W:{}",
            model.component_count, model.wire_count
        ));
        if let Some(cursor) = model.cursor_world {
            ui.separator();
            ui.monospace(format!("({:.0}, {:.0})", cursor.x, cursor.y));
        }
        ui.separator();
        ui.colored_label(
            if model.dirty {
                Color32::from_rgb(255, 198, 92)
            } else {
                Color32::from_rgb(138, 190, 145)
            },
            if model.dirty {
                "● unsaved"
            } else {
                "✓ saved"
            },
        );
    });
}
