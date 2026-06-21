#![allow(dead_code)]

use crate::model::cad::{FootprintId, NetClass, Point2};
use crate::pcb::footprint::Footprint;
use crate::pcb::layer::{BoardLayer, default_two_layer_stackup};
use crate::pcb::track::TrackSegment;
use crate::pcb::via::Via;
use serde::{Deserialize, Serialize};

pub(crate) const BOARD_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct DesignRules {
    pub(crate) default_clearance_mm: f32,
    pub(crate) min_track_width_mm: f32,
    pub(crate) min_via_diameter_mm: f32,
    pub(crate) min_via_drill_mm: f32,
    pub(crate) board_edge_clearance_mm: f32,
}

impl Default for DesignRules {
    fn default() -> Self {
        Self {
            default_clearance_mm: 0.2,
            min_track_width_mm: 0.2,
            min_via_diameter_mm: 0.6,
            min_via_drill_mm: 0.3,
            board_edge_clearance_mm: 0.25,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct BoardOutline {
    pub(crate) points: Vec<Point2>,
}

impl BoardOutline {
    pub(crate) fn rectangular(width_mm: f32, height_mm: f32) -> Self {
        Self {
            points: vec![
                Point2::new(0.0, 0.0),
                Point2::new(width_mm, 0.0),
                Point2::new(width_mm, height_mm),
                Point2::new(0.0, height_mm),
                Point2::new(0.0, 0.0),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct BoardFootprint {
    pub(crate) id: u64,
    pub(crate) symbol_instance_id: Option<u64>,
    pub(crate) reference: String,
    pub(crate) footprint_id: FootprintId,
    pub(crate) position: Point2,
    pub(crate) rotation_deg: f32,
    pub(crate) placed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Zone {
    pub(crate) id: u64,
    pub(crate) net_id: usize,
    pub(crate) layer: BoardLayer,
    pub(crate) outline: Vec<Point2>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Board {
    pub(crate) schema_version: u32,
    pub(crate) outline: BoardOutline,
    pub(crate) layers: Vec<BoardLayer>,
    pub(crate) tracks: Vec<TrackSegment>,
    pub(crate) vias: Vec<Via>,
    pub(crate) zones: Vec<Zone>,
    pub(crate) footprints: Vec<BoardFootprint>,
    pub(crate) footprint_library: Vec<Footprint>,
    pub(crate) design_rules: DesignRules,
    pub(crate) net_classes: Vec<NetClass>,
}

impl Board {
    pub(crate) fn new_two_layer(width_mm: f32, height_mm: f32) -> Self {
        Self {
            schema_version: BOARD_SCHEMA_VERSION,
            outline: BoardOutline::rectangular(width_mm, height_mm),
            layers: default_two_layer_stackup(),
            tracks: Vec::new(),
            vias: Vec::new(),
            zones: Vec::new(),
            footprints: Vec::new(),
            footprint_library: vec![Footprint::resistor_axial()],
            design_rules: DesignRules::default(),
            net_classes: vec![NetClass::default()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_board_has_two_copper_layers_and_edge_outline() {
        let board = Board::new_two_layer(50.0, 30.0);
        assert!(board.layers.contains(&BoardLayer::FrontCopper));
        assert!(board.layers.contains(&BoardLayer::BackCopper));
        assert!(board.layers.contains(&BoardLayer::EdgeCuts));
        assert_eq!(board.outline.points.first(), board.outline.points.last());
    }
}
