#![allow(dead_code)]

use crate::model::cad::{CadNet, FootprintId, NetClass, Point2, SymbolInstance};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum BoardUnits {
    Millimeters,
    Mils,
}

impl Default for BoardUnits {
    fn default() -> Self {
        Self::Millimeters
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct GridSettings {
    pub(crate) units: BoardUnits,
    pub(crate) grid_mm: f32,
    pub(crate) snap_enabled: bool,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            units: BoardUnits::Millimeters,
            grid_mm: 0.25,
            snap_enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LayerVisibility {
    pub(crate) layer: BoardLayer,
    pub(crate) visible: bool,
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
    #[serde(default)]
    pub(crate) layer_visibility: Vec<LayerVisibility>,
    #[serde(default)]
    pub(crate) grid: GridSettings,
    pub(crate) tracks: Vec<TrackSegment>,
    pub(crate) vias: Vec<Via>,
    pub(crate) zones: Vec<Zone>,
    pub(crate) footprints: Vec<BoardFootprint>,
    pub(crate) footprint_library: Vec<Footprint>,
    pub(crate) design_rules: DesignRules,
    pub(crate) net_classes: Vec<NetClass>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct RatsnestEdge {
    pub(crate) net_id: usize,
    pub(crate) from_footprint_id: u64,
    pub(crate) to_footprint_id: u64,
}

impl Board {
    pub(crate) fn new_two_layer(width_mm: f32, height_mm: f32) -> Self {
        Self {
            schema_version: BOARD_SCHEMA_VERSION,
            outline: BoardOutline::rectangular(width_mm, height_mm),
            layers: default_two_layer_stackup(),
            layer_visibility: default_two_layer_stackup()
                .into_iter()
                .map(|layer| LayerVisibility {
                    layer,
                    visible: true,
                })
                .collect(),
            grid: GridSettings::default(),
            tracks: Vec::new(),
            vias: Vec::new(),
            zones: Vec::new(),
            footprints: Vec::new(),
            footprint_library: vec![Footprint::resistor_axial()],
            design_rules: DesignRules::default(),
            net_classes: vec![NetClass::default()],
        }
    }

    pub(crate) fn update_from_schematic(&mut self, symbols: &[SymbolInstance], nets: &[CadNet]) {
        let mut next_id = self
            .footprints
            .iter()
            .map(|footprint| footprint.id)
            .max()
            .unwrap_or(0)
            + 1;
        for symbol in symbols {
            if self
                .footprints
                .iter()
                .any(|footprint| footprint.symbol_instance_id == Some(symbol.instance_id))
            {
                continue;
            }
            let Some(footprint_id) = symbol.footprint_link.clone() else {
                continue;
            };
            self.footprints.push(BoardFootprint {
                id: next_id,
                symbol_instance_id: Some(symbol.instance_id),
                reference: symbol.reference.clone(),
                footprint_id,
                position: Point2::new(symbol.position.x * 0.1, symbol.position.y * 0.1),
                rotation_deg: symbol.rotation_deg as f32,
                placed: false,
            });
            next_id += 1;
        }
        for net in nets {
            if !self
                .net_classes
                .iter()
                .any(|class| class.class_id == net.class_id)
            {
                self.net_classes.push(NetClass {
                    class_id: net.class_id.clone(),
                    ..NetClass::default()
                });
            }
        }
    }

    pub(crate) fn ratsnest_edges(&self, nets: &[CadNet]) -> Vec<RatsnestEdge> {
        let symbol_to_footprint = self
            .footprints
            .iter()
            .filter_map(|footprint| footprint.symbol_instance_id.map(|id| (id, footprint.id)))
            .collect::<std::collections::HashMap<_, _>>();
        let mut edges = Vec::new();
        for net in nets {
            let mut footprint_ids = net
                .connected_pins
                .iter()
                .filter_map(|pin| symbol_to_footprint.get(&pin.component_id).copied())
                .collect::<Vec<_>>();
            footprint_ids.sort_unstable();
            footprint_ids.dedup();
            for pair in footprint_ids.windows(2) {
                edges.push(RatsnestEdge {
                    net_id: net.net_id,
                    from_footprint_id: pair[0],
                    to_footprint_id: pair[1],
                });
            }
        }
        edges
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
        assert_eq!(board.grid.units, BoardUnits::Millimeters);
        assert!(board.grid.snap_enabled);
        assert!(
            board
                .layer_visibility
                .iter()
                .any(|entry| entry.layer == BoardLayer::FrontCopper && entry.visible)
        );
    }

    #[test]
    fn update_from_schematic_adds_unplaced_footprints_and_ratsnest() {
        use crate::model::PinRef;
        use crate::model::cad::{CadNet, SymbolInstance};

        let mut board = Board::new_two_layer(50.0, 30.0);
        let symbols = vec![
            SymbolInstance {
                instance_id: 1,
                symbol_id: "Device:R".to_string(),
                reference: "R1".to_string(),
                value: "1k".to_string(),
                position: Point2::new(10.0, 10.0),
                rotation_deg: 0,
                fields: Default::default(),
                footprint_link: Some("R_THT_Axial".to_string()),
            },
            SymbolInstance {
                instance_id: 2,
                symbol_id: "Device:R".to_string(),
                reference: "R2".to_string(),
                value: "1k".to_string(),
                position: Point2::new(40.0, 10.0),
                rotation_deg: 0,
                fields: Default::default(),
                footprint_link: Some("R_THT_Axial".to_string()),
            },
        ];
        let nets = vec![CadNet {
            net_id: 1,
            name: "NET_001".to_string(),
            connected_pins: vec![
                PinRef {
                    component_id: 1,
                    pin_name: "A".to_string(),
                },
                PinRef {
                    component_id: 2,
                    pin_name: "A".to_string(),
                },
            ],
            class_id: "Default".to_string(),
        }];

        board.update_from_schematic(&symbols, &nets);

        assert_eq!(board.footprints.len(), 2);
        assert!(board.footprints.iter().all(|footprint| !footprint.placed));
        assert_eq!(board.ratsnest_edges(&nets).len(), 1);
    }
}
