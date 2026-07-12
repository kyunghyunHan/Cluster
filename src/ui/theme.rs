use eframe::egui;
use egui::{Color32, Stroke, Vec2};

pub(crate) struct ClusterTheme {
    pub(crate) app_background: Color32,
    pub(crate) canvas_background: Color32,
    pub(crate) panel_background: Color32,
    pub(crate) panel_surface_2: Color32,
    pub(crate) panel_surface_3: Color32,
    pub(crate) raised_panel: Color32,
    pub(crate) border: Color32,
    pub(crate) strong_border: Color32,
    pub(crate) primary_text: Color32,
    pub(crate) secondary_text: Color32,
    pub(crate) accent: Color32,
    pub(crate) accent_hover: Color32,
    pub(crate) accent_pressed: Color32,
    pub(crate) warning: Color32,
    pub(crate) error: Color32,
    pub(crate) energized_wire: Color32,
    pub(crate) current_particle: Color32,
    pub(crate) wire_idle: Color32,
    pub(crate) wire_hover: Color32,
    pub(crate) wire_selected: Color32,
    pub(crate) pin: Color32,
    pub(crate) junction: Color32,
    pub(crate) pcb_front_copper: Color32,
    pub(crate) pcb_back_copper: Color32,
    pub(crate) ratsnest: Color32,
    pub(crate) selection_fill: Color32,
}

pub(crate) const CLUSTER_THEME: ClusterTheme = ClusterTheme {
    app_background: Color32::from_rgb(15, 18, 23),
    canvas_background: Color32::from_rgb(11, 14, 19),
    panel_background: Color32::from_rgb(23, 28, 35),
    panel_surface_2: Color32::from_rgb(18, 22, 28),
    panel_surface_3: Color32::from_rgb(28, 33, 39),
    raised_panel: Color32::from_rgb(28, 33, 39),
    border: Color32::from_rgb(58, 68, 80),
    strong_border: Color32::from_rgb(78, 92, 108),
    primary_text: Color32::from_rgb(225, 232, 240),
    secondary_text: Color32::from_rgb(156, 166, 176),
    accent: Color32::from_rgb(100, 178, 255),
    accent_hover: Color32::from_rgb(125, 195, 255),
    accent_pressed: Color32::from_rgb(70, 145, 220),
    warning: Color32::from_rgb(255, 200, 80),
    error: Color32::from_rgb(255, 110, 95),
    energized_wire: Color32::from_rgb(255, 174, 62),
    current_particle: Color32::from_rgb(255, 245, 178),
    wire_idle: Color32::from_rgb(105, 178, 255),
    wire_hover: Color32::from_rgb(140, 210, 255),
    wire_selected: Color32::from_rgb(90, 235, 170),
    pin: Color32::from_rgb(245, 205, 75),
    junction: Color32::from_rgb(150, 215, 255),
    pcb_front_copper: Color32::from_rgb(220, 75, 65),
    pcb_back_copper: Color32::from_rgb(65, 115, 220),
    ratsnest: Color32::from_rgb(210, 190, 75),
    selection_fill: Color32::from_rgba_premultiplied(70, 150, 220, 35),
};

pub(crate) const BG_APP: Color32 = CLUSTER_THEME.app_background;
pub(crate) const BG_PANEL: Color32 = CLUSTER_THEME.panel_background;
pub(crate) const BG_PANEL_DARK: Color32 = CLUSTER_THEME.panel_surface_2;
pub(crate) const BG_BUTTON: Color32 = CLUSTER_THEME.raised_panel;
pub(crate) const BG_BUTTON_HOVER: Color32 = Color32::from_rgb(36, 43, 51);
pub(crate) const BG_ACTIVE: Color32 = Color32::from_rgb(38, 70, 82);
pub(crate) const STROKE_PANEL: Color32 = CLUSTER_THEME.border;
pub(crate) const STROKE_MUTED: Color32 = Color32::from_rgb(48, 56, 64);
pub(crate) const TEXT_PRIMARY: Color32 = CLUSTER_THEME.primary_text;
pub(crate) const TEXT_SECONDARY: Color32 = CLUSTER_THEME.secondary_text;
pub(crate) const TEXT_MUTED: Color32 = Color32::from_rgb(120, 130, 140);
pub(crate) const ACCENT: Color32 = CLUSTER_THEME.accent;
pub(crate) const LIVE: Color32 = Color32::from_rgb(255, 198, 92);
pub(crate) const CURRENT_GLOW: Color32 = CLUSTER_THEME.energized_wire;
pub(crate) const CURRENT_PARTICLE: Color32 = CLUSTER_THEME.current_particle;
pub(crate) const GRID_MINOR: Color32 = Color32::from_rgb(36, 46, 58);
pub(crate) const GRID_MAJOR: Color32 = Color32::from_rgb(56, 72, 92);
pub(crate) const OK: Color32 = Color32::from_rgb(120, 200, 140);
pub(crate) const WARNING: Color32 = CLUSTER_THEME.warning;
pub(crate) const ERROR: Color32 = CLUSTER_THEME.error;

pub(crate) const PANEL_RADIUS: u8 = 4;
pub(crate) const CARD_RADIUS: u8 = 5;
pub(crate) const ROW_HEIGHT: f32 = 24.0;
pub(crate) const TOOL_HEIGHT: f32 = 28.0;
pub(crate) const TOOLBAR_HEIGHT: f32 = 32.0;
pub(crate) const PANEL_MARGIN: i8 = 6;
pub(crate) const SECTION_SPACING: f32 = 8.0;
pub(crate) const ICON_SIZE: f32 = 14.0;
pub(crate) const FAST_ANIMATION_MS: u64 = 120;
pub(crate) const SIMULATION_START_MS: u64 = 700;

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
