use eframe::egui;
use egui::{Align2, Color32, Pos2, Rect, Stroke, Vec2};

pub(crate) fn draw_simulation_legend(
    painter: &egui::Painter,
    canvas: Rect,
    simulation_enabled: bool,
    has_dc_solution: bool,
) {
    if !simulation_enabled || !has_dc_solution {
        return;
    }
    let rect = Rect::from_min_size(
        Pos2::new(canvas.left() + 14.0, canvas.bottom() - 86.0),
        Vec2::new(190.0, 70.0),
    );
    painter.rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(18, 22, 28, 220));
    painter.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0_f32, Color32::from_rgb(58, 68, 80)),
        egui::StrokeKind::Outside,
    );
    painter.text(
        rect.left_top() + Vec2::new(10.0, 8.0),
        Align2::LEFT_TOP,
        "Simulation legend",
        egui::FontId::proportional(11.0),
        Color32::from_rgb(220, 228, 236),
    );

    let y1 = rect.top() + 32.0;
    painter.line_segment(
        [
            Pos2::new(rect.left() + 12.0, y1),
            Pos2::new(rect.left() + 54.0, y1),
        ],
        Stroke::new(3.0_f32, Color32::from_rgb(255, 176, 64)),
    );
    painter.text(
        Pos2::new(rect.left() + 64.0, y1 - 7.0),
        Align2::LEFT_TOP,
        "active current path",
        egui::FontId::proportional(10.0),
        Color32::from_rgb(190, 202, 214),
    );

    let y2 = rect.top() + 52.0;
    painter.line_segment(
        [
            Pos2::new(rect.left() + 12.0, y2),
            Pos2::new(rect.left() + 54.0, y2),
        ],
        Stroke::new(2.0_f32, Color32::from_rgb(80, 132, 220)),
    );
    painter.text(
        Pos2::new(rect.left() + 64.0, y2 - 7.0),
        Align2::LEFT_TOP,
        "voltage, 0A/no arrow",
        egui::FontId::proportional(10.0),
        Color32::from_rgb(190, 202, 214),
    );
}
