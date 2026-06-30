use eframe::egui;
use egui::{Color32, Stroke, Vec2};

pub(crate) const BG_APP: Color32 = Color32::from_rgb(15, 18, 23);
pub(crate) const BG_PANEL: Color32 = Color32::from_rgb(23, 28, 35);
pub(crate) const BG_PANEL_DARK: Color32 = Color32::from_rgb(18, 22, 28);
pub(crate) const BG_BUTTON: Color32 = Color32::from_rgb(28, 33, 39);
pub(crate) const BG_BUTTON_HOVER: Color32 = Color32::from_rgb(36, 43, 51);
pub(crate) const BG_ACTIVE: Color32 = Color32::from_rgb(38, 70, 82);
pub(crate) const STROKE_PANEL: Color32 = Color32::from_rgb(58, 68, 80);
pub(crate) const STROKE_MUTED: Color32 = Color32::from_rgb(48, 56, 64);
pub(crate) const TEXT_PRIMARY: Color32 = Color32::from_rgb(225, 232, 240);
pub(crate) const TEXT_SECONDARY: Color32 = Color32::from_rgb(156, 166, 176);
pub(crate) const TEXT_MUTED: Color32 = Color32::from_rgb(120, 130, 140);
pub(crate) const ACCENT: Color32 = Color32::from_rgb(100, 178, 255);
pub(crate) const LIVE: Color32 = Color32::from_rgb(255, 198, 92);
pub(crate) const OK: Color32 = Color32::from_rgb(120, 200, 140);
pub(crate) const WARNING: Color32 = Color32::from_rgb(255, 200, 80);
pub(crate) const ERROR: Color32 = Color32::from_rgb(255, 110, 95);

pub(crate) const PANEL_RADIUS: u8 = 4;
pub(crate) const CARD_RADIUS: u8 = 5;
pub(crate) const ROW_HEIGHT: f32 = 24.0;
pub(crate) const TOOL_HEIGHT: f32 = 28.0;

pub(crate) fn panel_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(BG_PANEL)
        .stroke(Stroke::new(1.0, STROKE_PANEL))
        .corner_radius(egui::CornerRadius::same(PANEL_RADIUS))
        .inner_margin(egui::Margin::symmetric(6, 4))
}

pub(crate) fn card_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(BG_PANEL_DARK)
        .stroke(Stroke::new(1.0, STROKE_MUTED))
        .corner_radius(egui::CornerRadius::same(CARD_RADIUS))
        .inner_margin(egui::Margin::symmetric(6, 5))
}

pub(crate) fn tool_button(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let (fill, stroke, text) = if active {
        (BG_ACTIVE, Stroke::new(1.0, ACCENT), Color32::WHITE)
    } else {
        (BG_BUTTON, Stroke::new(1.0, STROKE_MUTED), TEXT_PRIMARY)
    };
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(11.0).color(text))
            .fill(fill)
            .stroke(stroke)
            .min_size(Vec2::new(72.0, TOOL_HEIGHT)),
    )
}
