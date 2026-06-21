#![allow(dead_code)]

use crate::model::cad::Point2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Via {
    pub(crate) id: u64,
    pub(crate) net_id: usize,
    pub(crate) position: Point2,
    pub(crate) diameter_mm: f32,
    pub(crate) drill_mm: f32,
}
