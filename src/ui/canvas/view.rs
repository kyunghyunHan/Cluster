use egui::{Pos2, Vec2};

/// Immutable world/screen transform for one canvas frame.
#[derive(Clone, Copy)]
pub(crate) struct CanvasView {
    pub(crate) zoom: f32,
    pub(crate) pan: Vec2,
    pub(crate) origin: Pos2,
}

impl CanvasView {
    pub(crate) fn to_screen(self, world: Pos2) -> Pos2 {
        self.origin + (world - self.origin) * self.zoom + self.pan
    }

    pub(crate) fn to_world(self, screen: Pos2) -> Pos2 {
        self.origin + ((screen - self.origin) - self.pan) / self.zoom
    }

    pub(crate) fn scale_f(self, value: f32) -> f32 {
        (value * self.zoom).clamp(value * 0.4, value * 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_screen_round_trip_is_stable() {
        let view = CanvasView {
            zoom: 1.75,
            pan: Vec2::new(23.0, -9.0),
            origin: Pos2::new(80.0, 40.0),
        };
        let world = Pos2::new(240.0, 125.0);
        assert!(view.to_world(view.to_screen(world)).distance(world) < 1.0e-4);
    }
}
