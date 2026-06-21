#![allow(dead_code)]

use crate::model::cad::Point2;
use crate::pcb::layer::BoardLayer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct TrackSegment {
    pub(crate) id: u64,
    pub(crate) net_id: usize,
    pub(crate) layer: BoardLayer,
    pub(crate) start: Point2,
    pub(crate) end: Point2,
    pub(crate) width_mm: f32,
}
