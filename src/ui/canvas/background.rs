use super::CanvasView;
use crate::ui::theme;
use egui::{Color32, Painter, Pos2, Rect, Stroke};

pub(crate) fn draw_grid(painter: &Painter, rect: Rect, grid: f32, view: CanvasView) {
    painter.rect_filled(rect, 0.0, theme::CLUSTER_THEME.canvas_background);
    let screen_grid = (grid * view.zoom).max(3.0);
    let origin = view.to_screen(Pos2::ZERO);
    let start_x = rect.left() + (origin.x - rect.left()).rem_euclid(screen_grid);
    let start_y = rect.top() + (origin.y - rect.top()).rem_euclid(screen_grid);
    if screen_grid >= 5.0 {
        let mut x = start_x;
        while x <= rect.right() + 1.0 {
            let mut y = start_y;
            while y <= rect.bottom() + 1.0 {
                painter.circle_filled(
                    Pos2::new(x, y),
                    if screen_grid > 14.0 { 1.3 } else { 0.9 },
                    theme::GRID_MINOR,
                );
                y += screen_grid;
            }
            x += screen_grid;
        }
    }
    let major_grid = screen_grid * 5.0;
    let mut x = rect.left() + (origin.x - rect.left()).rem_euclid(major_grid);
    let start_y = rect.top() + (origin.y - rect.top()).rem_euclid(major_grid);
    while x <= rect.right() + 1.0 {
        let mut y = start_y;
        while y <= rect.bottom() + 1.0 {
            painter.circle_filled(Pos2::new(x, y), 2.0, theme::GRID_MAJOR);
            y += major_grid;
        }
        x += major_grid;
    }
    let world_origin = view.to_screen(Pos2::ZERO);
    if rect.contains(world_origin) {
        let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(80, 120, 160, 40));
        painter.line_segment(
            [
                Pos2::new(world_origin.x, rect.top()),
                Pos2::new(world_origin.x, rect.bottom()),
            ],
            stroke,
        );
        painter.line_segment(
            [
                Pos2::new(rect.left(), world_origin.y),
                Pos2::new(rect.right(), world_origin.y),
            ],
            stroke,
        );
    }
}
