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
