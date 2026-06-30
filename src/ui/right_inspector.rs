use eframe::egui;
use egui::Color32;

pub(crate) fn render_inspector_header(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Inspector")
                .size(15.0)
                .strong()
                .color(Color32::from_rgb(225, 232, 240)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new("selection")
                    .size(10.0)
                    .color(Color32::from_rgb(135, 146, 158)),
            );
        });
    });
    ui.separator();
}
