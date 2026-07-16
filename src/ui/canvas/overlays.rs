//! Canvas labels, snap indicators, minimap, and probe overlay boundary.

use crate::ui::theme;
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};

pub(crate) fn draw_probe_card(
    painter: &Painter,
    viewport: Rect,
    anchor: Pos2,
    lines: &[String],
    accent: Color32,
) {
    if lines.is_empty() {
        return;
    }
    let width = lines
        .iter()
        .map(|line| line.chars().count() as f32 * 6.2 + 16.0)
        .fold(120.0, f32::max)
        .min(viewport.width() - 16.0);
    let size = Vec2::new(width, lines.len() as f32 * 16.0 + 10.0);
    let desired = anchor + Vec2::new(14.0, -8.0);
    let min = Pos2::new(
        desired.x.clamp(
            viewport.left() + 8.0,
            (viewport.right() - size.x - 8.0).max(viewport.left() + 8.0),
        ),
        desired.y.clamp(
            viewport.top() + 8.0,
            (viewport.bottom() - size.y - 8.0).max(viewport.top() + 8.0),
        ),
    );
    let card = Rect::from_min_size(min, size);
    painter.rect_filled(card, 3.0, Color32::from_rgba_unmultiplied(15, 20, 28, 235));
    painter.rect_stroke(
        card,
        3.0,
        Stroke::new(1.0_f32, theme::STROKE_PANEL),
        StrokeKind::Outside,
    );
    for (index, line) in lines.iter().enumerate() {
        painter.text(
            card.min + Vec2::new(7.0, 5.0 + index as f32 * 16.0),
            Align2::LEFT_TOP,
            line,
            FontId::proportional(11.0),
            if index == 0 {
                theme::TEXT_PRIMARY
            } else {
                accent
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_probe_is_a_noop() {
        let ctx = egui::Context::default();
        let painter = ctx.layer_painter(egui::LayerId::background());
        draw_probe_card(
            &painter,
            Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)),
            Pos2::ZERO,
            &[],
            theme::ACCENT,
        );
    }
}
