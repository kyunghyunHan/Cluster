#![allow(dead_code)]

use crate::model::cad::{FootprintId, Point2, Size2};
use crate::pcb::layer::BoardLayer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum PadShape {
    Circle,
    Oval,
    Rect,
    RoundRect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Pad {
    pub(crate) number: String,
    pub(crate) net_id: Option<usize>,
    pub(crate) position: Point2,
    pub(crate) size: Size2,
    pub(crate) drill_mm: Option<f32>,
    pub(crate) shape: PadShape,
    pub(crate) layers: Vec<BoardLayer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Footprint {
    pub(crate) footprint_id: FootprintId,
    pub(crate) display_name: String,
    pub(crate) pads: Vec<Pad>,
    pub(crate) courtyard: Vec<Point2>,
    pub(crate) silkscreen: Vec<Vec<Point2>>,
    pub(crate) fabrication: Vec<Vec<Point2>>,
    pub(crate) model_3d_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FootprintTransform {
    pub(crate) position: Point2,
    pub(crate) rotation_deg: f32,
    pub(crate) flipped: bool,
}

impl FootprintTransform {
    pub(crate) fn local_to_board(self, local: Point2) -> Point2 {
        let x = if self.flipped { -local.x } else { local.x };
        let radians = self.rotation_deg.to_radians();
        let (sin, cos) = radians.sin_cos();
        Point2::new(
            self.position.x + x * cos - local.y * sin,
            self.position.y + x * sin + local.y * cos,
        )
    }

    pub(crate) fn board_to_local(self, board: Point2) -> Point2 {
        let delta = Point2::new(board.x - self.position.x, board.y - self.position.y);
        let radians = (-self.rotation_deg).to_radians();
        let (sin, cos) = radians.sin_cos();
        let rotated = Point2::new(delta.x * cos - delta.y * sin, delta.x * sin + delta.y * cos);
        Point2::new(if self.flipped { -rotated.x } else { rotated.x }, rotated.y)
    }

    pub(crate) fn transform_pad(self, pad: &Pad) -> Pad {
        let mut transformed = pad.clone();
        transformed.position = self.local_to_board(pad.position);
        if (self.rotation_deg / 90.0).round() as i32 % 2 != 0 {
            std::mem::swap(&mut transformed.size.w, &mut transformed.size.h);
        }
        if self.flipped {
            transformed.layers = pad.layers.iter().copied().map(flip_layer).collect();
        }
        transformed
    }

    pub(crate) fn transform_polyline(self, points: &[Point2]) -> Vec<Point2> {
        points
            .iter()
            .copied()
            .map(|point| self.local_to_board(point))
            .collect()
    }

    pub(crate) fn transform_courtyard(self, footprint: &Footprint) -> Vec<Point2> {
        self.transform_polyline(&footprint.courtyard)
    }

    pub(crate) fn transform_silkscreen(self, footprint: &Footprint) -> Vec<Vec<Point2>> {
        footprint
            .silkscreen
            .iter()
            .map(|line| self.transform_polyline(line))
            .collect()
    }

    pub(crate) fn transform_fabrication(self, footprint: &Footprint) -> Vec<Vec<Point2>> {
        footprint
            .fabrication
            .iter()
            .map(|line| self.transform_polyline(line))
            .collect()
    }
}

fn flip_layer(layer: BoardLayer) -> BoardLayer {
    match layer {
        BoardLayer::FrontCopper => BoardLayer::BackCopper,
        BoardLayer::BackCopper => BoardLayer::FrontCopper,
        BoardLayer::FrontSilkscreen => BoardLayer::BackSilkscreen,
        BoardLayer::BackSilkscreen => BoardLayer::FrontSilkscreen,
        BoardLayer::FrontMask => BoardLayer::BackMask,
        BoardLayer::BackMask => BoardLayer::FrontMask,
        BoardLayer::EdgeCuts | BoardLayer::UserDwgs => layer,
    }
}

impl Footprint {
    pub(crate) fn resistor_axial() -> Self {
        Self {
            footprint_id: "R_THT_Axial".to_string(),
            display_name: "Resistor THT Axial".to_string(),
            pads: vec![
                Pad {
                    number: "1".to_string(),
                    net_id: None,
                    position: Point2::new(-5.08, 0.0),
                    size: Size2 { w: 1.7, h: 1.7 },
                    drill_mm: Some(0.8),
                    shape: PadShape::Circle,
                    layers: vec![BoardLayer::FrontCopper, BoardLayer::BackCopper],
                },
                Pad {
                    number: "2".to_string(),
                    net_id: None,
                    position: Point2::new(5.08, 0.0),
                    size: Size2 { w: 1.7, h: 1.7 },
                    drill_mm: Some(0.8),
                    shape: PadShape::Circle,
                    layers: vec![BoardLayer::FrontCopper, BoardLayer::BackCopper],
                },
            ],
            courtyard: vec![
                Point2::new(-6.5, -2.0),
                Point2::new(6.5, -2.0),
                Point2::new(6.5, 2.0),
                Point2::new(-6.5, 2.0),
            ],
            silkscreen: vec![vec![
                Point2::new(-3.5, -1.3),
                Point2::new(3.5, -1.3),
                Point2::new(3.5, 1.3),
                Point2::new(-3.5, 1.3),
                Point2::new(-3.5, -1.3),
            ]],
            fabrication: Vec::new(),
            model_3d_path: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_point(actual: Point2, expected: Point2) {
        assert!((actual.x - expected.x).abs() < 0.0001, "{actual:?}");
        assert!((actual.y - expected.y).abs() < 0.0001, "{actual:?}");
    }

    #[test]
    fn footprint_transform_has_golden_quadrant_and_flip_coordinates() {
        let local = Point2::new(2.0, 1.0);
        for (rotation, expected) in [
            (0.0, Point2::new(12.0, 21.0)),
            (90.0, Point2::new(9.0, 22.0)),
            (180.0, Point2::new(8.0, 19.0)),
            (270.0, Point2::new(11.0, 18.0)),
        ] {
            let transform = FootprintTransform {
                position: Point2::new(10.0, 20.0),
                rotation_deg: rotation,
                flipped: false,
            };
            let board = transform.local_to_board(local);
            assert_point(board, expected);
            assert_point(transform.board_to_local(board), local);
        }
        let flipped = FootprintTransform {
            position: Point2::new(10.0, 20.0),
            rotation_deg: 90.0,
            flipped: true,
        };
        assert_point(flipped.local_to_board(local), Point2::new(9.0, 18.0));
        assert_point(flipped.board_to_local(Point2::new(9.0, 18.0)), local);
    }
}
