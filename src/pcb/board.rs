#![allow(dead_code)]

use crate::model::cad::{CadNet, FootprintId, NetClass, Point2, SymbolInstance};
use crate::pcb::footprint::Footprint;
use crate::pcb::layer::{BoardLayer, default_two_layer_stackup};
use crate::pcb::track::TrackSegment;
use crate::pcb::via::Via;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub(crate) const BOARD_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct DesignRules {
    pub(crate) default_clearance_mm: f32,
    pub(crate) min_track_width_mm: f32,
    pub(crate) min_via_diameter_mm: f32,
    pub(crate) min_via_drill_mm: f32,
    pub(crate) board_edge_clearance_mm: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum BoardUnits {
    #[default]
    Millimeters,
    Mils,
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
    #[serde(default)]
    pub(crate) flipped: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemovedFootprintPolicy {
    KeepAsOrphan,
    RemoveFootprintKeepTracks,
    RemoveFootprintAndTracks,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct EcoReport {
    pub(crate) added_symbols: Vec<u64>,
    pub(crate) removed_footprints: Vec<u64>,
    pub(crate) changed_assignments: Vec<u64>,
    pub(crate) renamed_references: Vec<u64>,
    pub(crate) added_nets: Vec<usize>,
    pub(crate) removed_nets: Vec<usize>,
}

impl EcoReport {
    pub(crate) fn is_empty(&self) -> bool {
        self.added_symbols.is_empty()
            && self.removed_footprints.is_empty()
            && self.changed_assignments.is_empty()
            && self.renamed_references.is_empty()
            && self.added_nets.is_empty()
            && self.removed_nets.is_empty()
    }
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

    pub(crate) fn eco_report(&self, symbols: &[SymbolInstance], nets: &[CadNet]) -> EcoReport {
        let symbol_ids = symbols
            .iter()
            .map(|symbol| symbol.instance_id)
            .collect::<HashSet<_>>();
        let symbol_by_id = symbols
            .iter()
            .map(|symbol| (symbol.instance_id, symbol))
            .collect::<HashMap<_, _>>();
        let footprint_symbols = self
            .footprints
            .iter()
            .filter_map(|footprint| footprint.symbol_instance_id)
            .collect::<HashSet<_>>();
        let existing_nets = self
            .tracks
            .iter()
            .map(|track| track.net_id)
            .chain(self.vias.iter().map(|via| via.net_id))
            .collect::<HashSet<_>>();
        let incoming_nets = nets.iter().map(|net| net.net_id).collect::<HashSet<_>>();
        EcoReport {
            added_symbols: symbol_ids.difference(&footprint_symbols).copied().collect(),
            removed_footprints: self
                .footprints
                .iter()
                .filter(|footprint| {
                    footprint
                        .symbol_instance_id
                        .is_some_and(|id| !symbol_ids.contains(&id))
                })
                .map(|footprint| footprint.id)
                .collect(),
            changed_assignments: self
                .footprints
                .iter()
                .filter(|footprint| {
                    footprint.symbol_instance_id.is_some_and(|id| {
                        symbol_by_id.get(&id).is_some_and(|symbol| {
                            symbol.footprint_link.as_ref() != Some(&footprint.footprint_id)
                        })
                    })
                })
                .map(|footprint| footprint.id)
                .collect(),
            renamed_references: self
                .footprints
                .iter()
                .filter(|footprint| {
                    footprint.symbol_instance_id.is_some_and(|id| {
                        symbol_by_id
                            .get(&id)
                            .is_some_and(|symbol| symbol.reference != footprint.reference)
                    })
                })
                .map(|footprint| footprint.id)
                .collect(),
            added_nets: incoming_nets.difference(&existing_nets).copied().collect(),
            removed_nets: existing_nets.difference(&incoming_nets).copied().collect(),
        }
    }

    pub(crate) fn apply_eco(
        &mut self,
        symbols: &[SymbolInstance],
        nets: &[CadNet],
        removed_policy: RemovedFootprintPolicy,
    ) -> EcoReport {
        let report = self.eco_report(symbols, nets);
        let removed_ids = report
            .removed_footprints
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        if removed_policy != RemovedFootprintPolicy::KeepAsOrphan {
            self.footprints
                .retain(|footprint| !removed_ids.contains(&footprint.id));
        } else {
            for footprint in &mut self.footprints {
                if removed_ids.contains(&footprint.id) {
                    footprint.symbol_instance_id = None;
                }
            }
        }
        if removed_policy == RemovedFootprintPolicy::RemoveFootprintAndTracks {
            let removed_nets = report.removed_nets.iter().copied().collect::<HashSet<_>>();
            self.tracks
                .retain(|track| !removed_nets.contains(&track.net_id));
            self.vias.retain(|via| !removed_nets.contains(&via.net_id));
        }

        let symbol_by_id = symbols
            .iter()
            .map(|symbol| (symbol.instance_id, symbol))
            .collect::<HashMap<_, _>>();
        for footprint in &mut self.footprints {
            let Some(symbol) = footprint
                .symbol_instance_id
                .and_then(|id| symbol_by_id.get(&id))
            else {
                continue;
            };
            footprint.reference = symbol.reference.clone();
            if let Some(footprint_id) = &symbol.footprint_link {
                footprint.footprint_id = footprint_id.clone();
            }
            // Existing physical placement, side, and rotation are deliberate
            // layout work and must survive schematic synchronization.
        }

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
                flipped: false,
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
        report
    }

    pub(crate) fn update_from_schematic(&mut self, symbols: &[SymbolInstance], nets: &[CadNet]) {
        self.apply_eco(symbols, nets, RemovedFootprintPolicy::KeepAsOrphan);
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
            let islands = self.copper_islands(net.net_id, &footprint_ids);
            if islands.len() < 2 {
                continue;
            }
            let mut joined = vec![false; islands.len()];
            joined[0] = true;
            while joined.iter().any(|joined| !joined) {
                let mut best: Option<(f32, usize, usize, u64, u64)> = None;
                for (a_index, a) in islands.iter().enumerate().filter(|(i, _)| joined[*i]) {
                    for (b_index, b) in islands.iter().enumerate().filter(|(i, _)| !joined[*i]) {
                        for &a_id in a {
                            for &b_id in b {
                                let Some(a_pos) = self.footprint_position(a_id) else {
                                    continue;
                                };
                                let Some(b_pos) = self.footprint_position(b_id) else {
                                    continue;
                                };
                                let distance =
                                    (a_pos.x - b_pos.x).powi(2) + (a_pos.y - b_pos.y).powi(2);
                                if best.is_none_or(|candidate| distance < candidate.0) {
                                    best = Some((distance, a_index, b_index, a_id, b_id));
                                }
                            }
                        }
                    }
                }
                let Some((_, _, b_index, from, to)) = best else {
                    break;
                };
                joined[b_index] = true;
                edges.push(RatsnestEdge {
                    net_id: net.net_id,
                    from_footprint_id: from,
                    to_footprint_id: to,
                });
            }
        }
        edges
    }

    fn footprint_position(&self, id: u64) -> Option<Point2> {
        self.footprints
            .iter()
            .find(|footprint| footprint.id == id)
            .map(|footprint| footprint.position)
    }

    fn copper_islands(&self, net_id: usize, footprint_ids: &[u64]) -> Vec<Vec<u64>> {
        const CONTACT_MM: f32 = 0.05;
        let mut islands = footprint_ids
            .iter()
            .copied()
            .map(|id| vec![id])
            .collect::<Vec<_>>();
        let tracks = self
            .tracks
            .iter()
            .filter(|track| track.net_id == net_id)
            .collect::<Vec<_>>();
        let vias = self
            .vias
            .iter()
            .filter(|via| via.net_id == net_id)
            .collect::<Vec<_>>();
        loop {
            let mut merged = false;
            'outer: for a in 0..islands.len() {
                for b in (a + 1)..islands.len() {
                    if islands[a].iter().any(|&a_id| {
                        islands[b].iter().any(|&b_id| {
                            let (Some(a_pos), Some(b_pos)) =
                                (self.footprint_position(a_id), self.footprint_position(b_id))
                            else {
                                return false;
                            };
                            copper_path_exists(a_pos, b_pos, &tracks, &vias, CONTACT_MM)
                        })
                    }) {
                        let other = islands.remove(b);
                        islands[a].extend(other);
                        merged = true;
                        break 'outer;
                    }
                }
            }
            if !merged {
                break;
            }
        }
        islands
    }
}

fn copper_path_exists(
    start: Point2,
    end: Point2,
    tracks: &[&TrackSegment],
    vias: &[&Via],
    tolerance: f32,
) -> bool {
    let close =
        |a: Point2, b: Point2| (a.x - b.x).powi(2) + (a.y - b.y).powi(2) <= tolerance * tolerance;
    let mut reached = vec![false; tracks.len()];
    let mut frontier = tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| close(start, track.start) || close(start, track.end))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    while let Some(index) = frontier.pop() {
        if reached[index] {
            continue;
        }
        reached[index] = true;
        let track = tracks[index];
        if close(end, track.start) || close(end, track.end) {
            return true;
        }
        for (candidate_index, candidate) in tracks.iter().enumerate() {
            let same_layer_contact = track.layer == candidate.layer
                && (close(track.start, candidate.start)
                    || close(track.start, candidate.end)
                    || close(track.end, candidate.start)
                    || close(track.end, candidate.end));
            let via_contact = vias.iter().any(|via| {
                point_to_segment(via.position, track.start, track.end)
                    <= via.diameter_mm * 0.5 + tolerance
                    && point_to_segment(via.position, candidate.start, candidate.end)
                        <= via.diameter_mm * 0.5 + tolerance
            });
            if !reached[candidate_index] && (same_layer_contact || via_contact) {
                frontier.push(candidate_index);
            }
        }
    }
    false
}

fn point_to_segment(point: Point2, start: Point2, end: Point2) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared <= f32::EPSILON {
        return ((point.x - start.x).powi(2) + (point.y - start.y).powi(2)).sqrt();
    }
    let t =
        (((point.x - start.x) * dx + (point.y - start.y) * dy) / length_squared).clamp(0.0, 1.0);
    let closest = Point2::new(start.x + dx * t, start.y + dy * t);
    ((point.x - closest.x).powi(2) + (point.y - closest.y).powi(2)).sqrt()
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

    #[test]
    fn eco_preserves_layout_and_keeps_removed_routed_parts_as_orphans() {
        use crate::model::cad::SymbolInstance;

        let mut board = Board::new_two_layer(50.0, 30.0);
        let first = SymbolInstance {
            instance_id: 1,
            symbol_id: "Device:R".to_string(),
            reference: "R1".to_string(),
            value: "1k".to_string(),
            position: Point2::new(10.0, 10.0),
            rotation_deg: 0,
            fields: Default::default(),
            footprint_link: Some("R_THT_Axial".to_string()),
        };
        let second = SymbolInstance {
            instance_id: 2,
            symbol_id: "Device:LED".to_string(),
            reference: "LED1".to_string(),
            value: "red".to_string(),
            position: Point2::new(30.0, 10.0),
            rotation_deg: 0,
            fields: Default::default(),
            footprint_link: Some("LED_THT_5mm".to_string()),
        };

        board.update_from_schematic(&[first.clone(), second], &[]);
        assert_eq!(board.footprints.len(), 2);
        board.footprints[0].position = Point2::new(22.0, 18.0);
        board.footprints[0].rotation_deg = 180.0;
        board.footprints[0].placed = true;

        let mut moved = first;
        moved.reference = "R10".to_string();
        moved.position = Point2::new(80.0, 50.0);
        moved.rotation_deg = 90;
        board.update_from_schematic(&[moved], &[]);

        assert_eq!(board.footprints.len(), 2);
        let footprint = board
            .footprints
            .iter()
            .find(|footprint| footprint.symbol_instance_id == Some(1))
            .expect("existing footprint remains linked");
        assert_eq!(footprint.symbol_instance_id, Some(1));
        assert_eq!(footprint.reference, "R10");
        assert_eq!(footprint.position, Point2::new(22.0, 18.0));
        assert_eq!(footprint.rotation_deg, 180.0);
        assert!(board.footprints.iter().any(
            |footprint| footprint.reference == "LED1" && footprint.symbol_instance_id.is_none()
        ));
    }

    #[test]
    fn ratsnest_disappears_only_after_copper_connects_the_islands() {
        use crate::model::PinRef;
        use crate::pcb::track::TrackSegment;

        let mut board = Board::new_two_layer(50.0, 30.0);
        board.footprints = vec![
            BoardFootprint {
                id: 1,
                symbol_instance_id: Some(10),
                reference: "R1".to_string(),
                footprint_id: "R_THT_Axial".to_string(),
                position: Point2::new(5.0, 5.0),
                rotation_deg: 0.0,
                flipped: false,
                placed: true,
            },
            BoardFootprint {
                id: 2,
                symbol_instance_id: Some(20),
                reference: "R2".to_string(),
                footprint_id: "R_THT_Axial".to_string(),
                position: Point2::new(25.0, 5.0),
                rotation_deg: 0.0,
                flipped: false,
                placed: true,
            },
        ];
        let nets = vec![CadNet {
            net_id: 7,
            name: "SIGNAL".to_string(),
            connected_pins: vec![
                PinRef {
                    component_id: 10,
                    pin_name: "1".to_string(),
                },
                PinRef {
                    component_id: 20,
                    pin_name: "1".to_string(),
                },
            ],
            class_id: "Default".to_string(),
        }];
        assert_eq!(board.ratsnest_edges(&nets).len(), 1);

        board.tracks.push(TrackSegment {
            id: 1,
            net_id: 7,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(5.0, 5.0),
            end: Point2::new(15.0, 5.0),
            width_mm: 0.25,
        });
        assert_eq!(board.ratsnest_edges(&nets).len(), 1);

        board.tracks.push(TrackSegment {
            id: 2,
            net_id: 7,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(15.0, 5.0),
            end: Point2::new(25.0, 5.0),
            width_mm: 0.25,
        });
        assert!(board.ratsnest_edges(&nets).is_empty());
    }

    #[test]
    fn via_bridges_same_net_tracks_between_copper_layers() {
        use crate::model::PinRef;
        use crate::pcb::track::TrackSegment;

        let mut board = Board::new_two_layer(50.0, 30.0);
        board.footprints = vec![
            BoardFootprint {
                id: 1,
                symbol_instance_id: Some(10),
                reference: "U1".to_string(),
                footprint_id: "A".to_string(),
                position: Point2::new(5.0, 5.0),
                rotation_deg: 0.0,
                flipped: false,
                placed: true,
            },
            BoardFootprint {
                id: 2,
                symbol_instance_id: Some(20),
                reference: "U2".to_string(),
                footprint_id: "B".to_string(),
                position: Point2::new(25.0, 5.0),
                rotation_deg: 0.0,
                flipped: false,
                placed: true,
            },
        ];
        board.tracks = vec![
            TrackSegment {
                id: 1,
                net_id: 9,
                layer: BoardLayer::FrontCopper,
                start: Point2::new(5.0, 5.0),
                end: Point2::new(15.0, 5.0),
                width_mm: 0.25,
            },
            TrackSegment {
                id: 2,
                net_id: 9,
                layer: BoardLayer::BackCopper,
                start: Point2::new(15.0, 5.0),
                end: Point2::new(25.0, 5.0),
                width_mm: 0.25,
            },
        ];
        let nets = vec![CadNet {
            net_id: 9,
            name: "VIA_NET".to_string(),
            connected_pins: vec![
                PinRef {
                    component_id: 10,
                    pin_name: "1".to_string(),
                },
                PinRef {
                    component_id: 20,
                    pin_name: "1".to_string(),
                },
            ],
            class_id: "Default".to_string(),
        }];
        assert_eq!(board.ratsnest_edges(&nets).len(), 1);
        board.vias.push(Via {
            id: 3,
            net_id: 9,
            position: Point2::new(15.0, 5.0),
            diameter_mm: 0.6,
            drill_mm: 0.3,
        });
        assert!(board.ratsnest_edges(&nets).is_empty());
    }
}
