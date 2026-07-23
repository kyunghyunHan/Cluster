#![allow(dead_code)]

use crate::model::cad::{CadNet, FootprintId, NetClass, Point2, SymbolInstance};
use crate::pcb::footprint::Footprint;
use crate::pcb::layer::{BoardLayer, default_two_layer_stackup};
use crate::pcb::spatial_index::{PadRef, PcbSpatialIndex};
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

impl BoardFootprint {
    pub(crate) fn transform(&self) -> crate::pcb::footprint::FootprintTransform {
        crate::pcb::footprint::FootprintTransform {
            position: self.position,
            rotation_deg: self.rotation_deg,
            flipped: self.flipped,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Zone {
    pub(crate) id: u64,
    pub(crate) net_id: usize,
    pub(crate) layer: BoardLayer,
    pub(crate) outline: Vec<Point2>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(skip)]
    entity_index: BoardEntityIndex,
    #[serde(skip)]
    spatial_index: PcbSpatialIndex,
}

impl PartialEq for Board {
    fn eq(&self, other: &Self) -> bool {
        self.schema_version == other.schema_version
            && self.outline == other.outline
            && self.layers == other.layers
            && self.layer_visibility == other.layer_visibility
            && self.grid == other.grid
            && self.tracks == other.tracks
            && self.vias == other.vias
            && self.zones == other.zones
            && self.footprints == other.footprints
            && self.footprint_library == other.footprint_library
            && self.design_rules == other.design_rules
            && self.net_classes == other.net_classes
    }
}

#[derive(Debug, Clone, Default)]
struct BoardEntityIndex {
    footprints: HashMap<u64, usize>,
    tracks: HashMap<u64, usize>,
    vias: HashMap<u64, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BoardInvariantViolation {
    DuplicateFootprintId {
        id: u64,
    },
    DuplicateTrackId {
        id: u64,
    },
    DuplicateViaId {
        id: u64,
    },
    DuplicateZoneId {
        id: u64,
    },
    DuplicateFootprintDefinition {
        footprint_id: FootprintId,
    },
    MissingFootprintDefinition {
        id: u64,
        footprint_id: FootprintId,
    },
    DuplicatePadNumber {
        footprint_id: FootprintId,
        number: String,
    },
    EntityIndexMismatch,
    SpatialIndexMismatch,
    InvalidOutline,
    InvalidZonePolygon {
        id: u64,
    },
    NonFiniteGeometry {
        entity: &'static str,
        id: u64,
    },
    InvalidTrackWidth {
        id: u64,
    },
    InvalidViaDimensions {
        id: u64,
    },
    InvalidPadGeometry {
        footprint_id: FootprintId,
        number: String,
    },
    InvalidPadDrill {
        footprint_id: FootprintId,
        number: String,
    },
    MissingPadLayer {
        footprint_id: FootprintId,
        number: String,
    },
    MissingLayer {
        entity: &'static str,
        id: u64,
    },
    DuplicateNetClass {
        class_id: String,
    },
    InvalidNetClass {
        class_id: String,
    },
    InvalidDesignRules,
    UnknownNet {
        entity: &'static str,
        id: u64,
        net_id: usize,
    },
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EcoPatch {
    pub(crate) report: EcoReport,
    footprint_ids: HashSet<u64>,
    track_ids: HashSet<u64>,
    via_ids: HashSet<u64>,
    footprints_before: Vec<(usize, BoardFootprint)>,
    footprints_after: Vec<(usize, BoardFootprint)>,
    tracks_before: Vec<(usize, TrackSegment)>,
    tracks_after: Vec<(usize, TrackSegment)>,
    vias_before: Vec<(usize, Via)>,
    vias_after: Vec<(usize, Via)>,
    net_classes_before: Vec<NetClass>,
    net_classes_after: Vec<NetClass>,
}

impl EcoPatch {
    pub(crate) fn is_empty(&self) -> bool {
        self.report.is_empty()
            && self.footprints_before == self.footprints_after
            && self.tracks_before == self.tracks_after
            && self.vias_before == self.vias_after
            && self.net_classes_before == self.net_classes_after
    }

    pub(crate) fn rollback(&self, board: &mut Board) {
        restore_entities(
            &mut board.footprints,
            &self.footprint_ids,
            &self.footprints_before,
            |value| value.id,
        );
        restore_entities(
            &mut board.tracks,
            &self.track_ids,
            &self.tracks_before,
            |value| value.id,
        );
        restore_entities(&mut board.vias, &self.via_ids, &self.vias_before, |value| {
            value.id
        });
        board.net_classes.clone_from(&self.net_classes_before);
        board.rebuild_entity_index();
    }

    pub(crate) fn reapply(&self, board: &mut Board) {
        restore_entities(
            &mut board.footprints,
            &self.footprint_ids,
            &self.footprints_after,
            |value| value.id,
        );
        restore_entities(
            &mut board.tracks,
            &self.track_ids,
            &self.tracks_after,
            |value| value.id,
        );
        restore_entities(&mut board.vias, &self.via_ids, &self.vias_after, |value| {
            value.id
        });
        board.net_classes.clone_from(&self.net_classes_after);
        board.rebuild_entity_index();
    }
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
        let mut board = Self {
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
            footprint_library: vec![Footprint::resistor_axial(), Footprint::led_tht_5mm()],
            design_rules: DesignRules::default(),
            net_classes: vec![NetClass::default()],
            entity_index: BoardEntityIndex::default(),
            spatial_index: PcbSpatialIndex::default(),
        };
        board.rebuild_entity_index();
        board
    }

    pub(crate) fn rebuild_entity_index(&mut self) {
        self.entity_index.footprints = self
            .footprints
            .iter()
            .enumerate()
            .map(|(index, footprint)| (footprint.id, index))
            .collect();
        self.entity_index.tracks = self
            .tracks
            .iter()
            .enumerate()
            .map(|(index, track)| (track.id, index))
            .collect();
        self.entity_index.vias = self
            .vias
            .iter()
            .enumerate()
            .map(|(index, via)| (via.id, index))
            .collect();
        self.spatial_index = PcbSpatialIndex::build(
            &self.footprints,
            &self.tracks,
            &self.vias,
            &self.outline,
            &self.footprint_library,
        );
    }

    fn ensure_entity_index(&mut self) {
        if !self.entity_index_is_consistent() {
            self.rebuild_entity_index();
        }
    }

    pub(crate) fn footprint_mut(&mut self, id: u64) -> Option<&mut BoardFootprint> {
        self.ensure_entity_index();
        let index = *self.entity_index.footprints.get(&id)?;
        self.footprints.get_mut(index)
    }

    pub(crate) fn footprint(&self, id: u64) -> Option<&BoardFootprint> {
        self.entity_index
            .footprints
            .get(&id)
            .and_then(|index| self.footprints.get(*index))
    }

    pub(crate) fn track_mut(&mut self, id: u64) -> Option<&mut TrackSegment> {
        self.ensure_entity_index();
        let index = *self.entity_index.tracks.get(&id)?;
        self.tracks.get_mut(index)
    }

    pub(crate) fn track(&self, id: u64) -> Option<&TrackSegment> {
        self.entity_index
            .tracks
            .get(&id)
            .and_then(|index| self.tracks.get(*index))
    }

    pub(crate) fn via(&self, id: u64) -> Option<&Via> {
        self.entity_index
            .vias
            .get(&id)
            .and_then(|index| self.vias.get(*index))
    }

    pub(crate) fn via_mut(&mut self, id: u64) -> Option<&mut Via> {
        self.ensure_entity_index();
        let index = *self.entity_index.vias.get(&id)?;
        self.vias.get_mut(index)
    }

    pub(crate) fn add_track(&mut self, track: TrackSegment) {
        self.ensure_entity_index();
        let index = self.tracks.len();
        self.entity_index.tracks.insert(track.id, index);
        self.spatial_index.add_track(&track);
        self.tracks.push(track);
    }

    pub(crate) fn remove_track(&mut self, id: u64) -> Option<TrackSegment> {
        self.ensure_entity_index();
        let index = self.entity_index.tracks.remove(&id)?;
        let removed = self.tracks.swap_remove(index);
        if let Some(swapped) = self.tracks.get(index) {
            self.entity_index.tracks.insert(swapped.id, index);
        }
        self.spatial_index.remove_track(id);
        Some(removed)
    }

    pub(crate) fn add_via(&mut self, via: Via) {
        self.ensure_entity_index();
        let index = self.vias.len();
        self.entity_index.vias.insert(via.id, index);
        self.spatial_index.add_via(&via);
        self.vias.push(via);
    }

    pub(crate) fn remove_via(&mut self, id: u64) -> Option<Via> {
        self.ensure_entity_index();
        let index = self.entity_index.vias.remove(&id)?;
        let removed = self.vias.swap_remove(index);
        if let Some(swapped) = self.vias.get(index) {
            self.entity_index.vias.insert(swapped.id, index);
        }
        self.spatial_index.remove_via(id);
        Some(removed)
    }

    pub(crate) fn move_footprint(&mut self, id: u64, position: Point2) -> bool {
        self.ensure_entity_index();
        let Some(index) = self.entity_index.footprints.get(&id).copied() else {
            return false;
        };
        self.footprints[index].position = position;
        self.footprints[index].placed = true;
        let footprint = self.footprints[index].clone();
        self.spatial_index
            .update_footprint(&footprint, &self.footprint_library);
        true
    }

    pub(crate) fn rotate_footprint(&mut self, id: u64, delta_deg: f32) -> bool {
        self.ensure_entity_index();
        let Some(index) = self.entity_index.footprints.get(&id).copied() else {
            return false;
        };
        self.footprints[index].rotation_deg =
            (self.footprints[index].rotation_deg + delta_deg).rem_euclid(360.0);
        let footprint = self.footprints[index].clone();
        self.spatial_index
            .update_footprint(&footprint, &self.footprint_library);
        true
    }

    pub(crate) fn flip_footprint(&mut self, id: u64) -> bool {
        self.ensure_entity_index();
        let Some(index) = self.entity_index.footprints.get(&id).copied() else {
            return false;
        };
        self.footprints[index].flipped = !self.footprints[index].flipped;
        let footprint = self.footprints[index].clone();
        self.spatial_index
            .update_footprint(&footprint, &self.footprint_library);
        true
    }

    pub(crate) fn edit_track(&mut self, updated: TrackSegment) -> bool {
        self.ensure_entity_index();
        let Some(index) = self.entity_index.tracks.get(&updated.id).copied() else {
            return false;
        };
        self.tracks[index] = updated;
        self.spatial_index.update_track(&self.tracks[index]);
        true
    }

    pub(crate) fn edit_via(&mut self, updated: Via) -> bool {
        self.ensure_entity_index();
        let Some(index) = self.entity_index.vias.get(&updated.id).copied() else {
            return false;
        };
        self.vias[index] = updated;
        self.spatial_index.update_via(&self.vias[index]);
        true
    }

    pub(crate) fn set_outline(&mut self, outline: BoardOutline) -> bool {
        if self.outline == outline {
            return false;
        }
        self.outline = outline;
        self.spatial_index.update_outline(&self.outline);
        true
    }

    pub(crate) fn entity_index_is_consistent(&self) -> bool {
        self.entity_index.footprints.len() == self.footprints.len()
            && self.entity_index.tracks.len() == self.tracks.len()
            && self.entity_index.vias.len() == self.vias.len()
            && self
                .footprints
                .iter()
                .enumerate()
                .all(|(index, footprint)| {
                    self.entity_index.footprints.get(&footprint.id) == Some(&index)
                })
            && self
                .tracks
                .iter()
                .enumerate()
                .all(|(index, track)| self.entity_index.tracks.get(&track.id) == Some(&index))
            && self
                .vias
                .iter()
                .enumerate()
                .all(|(index, via)| self.entity_index.vias.get(&via.id) == Some(&index))
    }

    pub(crate) fn validate_invariants(&self) -> Result<(), Vec<BoardInvariantViolation>> {
        self.validate_invariants_with_nets(None)
    }

    pub(crate) fn validate_invariants_with_nets(
        &self,
        known_net_ids: Option<&HashSet<usize>>,
    ) -> Result<(), Vec<BoardInvariantViolation>> {
        let mut violations = Vec::new();
        collect_duplicate_ids(
            self.footprints.iter().map(|entity| entity.id),
            |id| BoardInvariantViolation::DuplicateFootprintId { id },
            &mut violations,
        );
        collect_duplicate_ids(
            self.tracks.iter().map(|entity| entity.id),
            |id| BoardInvariantViolation::DuplicateTrackId { id },
            &mut violations,
        );
        collect_duplicate_ids(
            self.vias.iter().map(|entity| entity.id),
            |id| BoardInvariantViolation::DuplicateViaId { id },
            &mut violations,
        );
        collect_duplicate_ids(
            self.zones.iter().map(|entity| entity.id),
            |id| BoardInvariantViolation::DuplicateZoneId { id },
            &mut violations,
        );
        if !self.entity_index_is_consistent() {
            violations.push(BoardInvariantViolation::EntityIndexMismatch);
        }
        if !self.spatial_index.is_consistent(
            &self.footprints,
            &self.tracks,
            &self.vias,
            &self.outline,
            &self.footprint_library,
        ) {
            violations.push(BoardInvariantViolation::SpatialIndexMismatch);
        }
        let mut definition_ids = HashSet::new();
        for definition in &self.footprint_library {
            if !definition_ids.insert(definition.footprint_id.clone()) {
                violations.push(BoardInvariantViolation::DuplicateFootprintDefinition {
                    footprint_id: definition.footprint_id.clone(),
                });
            }
            let mut pad_numbers = HashSet::new();
            for pad in &definition.pads {
                if !pad_numbers.insert(pad.number.clone()) {
                    violations.push(BoardInvariantViolation::DuplicatePadNumber {
                        footprint_id: definition.footprint_id.clone(),
                        number: pad.number.clone(),
                    });
                }
                if !finite(pad.position)
                    || !pad.size.w.is_finite()
                    || !pad.size.h.is_finite()
                    || pad.size.w <= 0.0
                    || pad.size.h <= 0.0
                {
                    violations.push(BoardInvariantViolation::InvalidPadGeometry {
                        footprint_id: definition.footprint_id.clone(),
                        number: pad.number.clone(),
                    });
                }
                if pad.drill_mm.is_some_and(|drill| {
                    !drill.is_finite() || drill <= 0.0 || drill >= pad.size.w.min(pad.size.h)
                }) {
                    violations.push(BoardInvariantViolation::InvalidPadDrill {
                        footprint_id: definition.footprint_id.clone(),
                        number: pad.number.clone(),
                    });
                }
                if pad.layers.is_empty()
                    || pad.layers.iter().any(|layer| !self.layers.contains(layer))
                {
                    violations.push(BoardInvariantViolation::MissingPadLayer {
                        footprint_id: definition.footprint_id.clone(),
                        number: pad.number.clone(),
                    });
                }
                if let (Some(known), Some(net_id)) = (known_net_ids, pad.net_id)
                    && !known.contains(&net_id)
                {
                    violations.push(BoardInvariantViolation::UnknownNet {
                        entity: "pad",
                        id: 0,
                        net_id,
                    });
                }
            }
        }
        if self.outline.points.len() < 4
            || self.outline.points.iter().any(|point| !finite(*point))
            || self.outline.points.first() != self.outline.points.last()
        {
            violations.push(BoardInvariantViolation::InvalidOutline);
        }
        for footprint in &self.footprints {
            if !finite(footprint.position) || !footprint.rotation_deg.is_finite() {
                violations.push(BoardInvariantViolation::NonFiniteGeometry {
                    entity: "footprint",
                    id: footprint.id,
                });
            }
            if !definition_ids.contains(&footprint.footprint_id) {
                violations.push(BoardInvariantViolation::MissingFootprintDefinition {
                    id: footprint.id,
                    footprint_id: footprint.footprint_id.clone(),
                });
            }
        }
        for track in &self.tracks {
            if !finite(track.start) || !finite(track.end) {
                violations.push(BoardInvariantViolation::NonFiniteGeometry {
                    entity: "track",
                    id: track.id,
                });
            }
            if !track.width_mm.is_finite() || track.width_mm <= 0.0 {
                violations.push(BoardInvariantViolation::InvalidTrackWidth { id: track.id });
            }
            if !self.layers.contains(&track.layer) {
                violations.push(BoardInvariantViolation::MissingLayer {
                    entity: "track",
                    id: track.id,
                });
            }
            validate_known_net(
                known_net_ids,
                "track",
                track.id,
                track.net_id,
                &mut violations,
            );
        }
        for via in &self.vias {
            if !finite(via.position) {
                violations.push(BoardInvariantViolation::NonFiniteGeometry {
                    entity: "via",
                    id: via.id,
                });
            }
            if !via.diameter_mm.is_finite()
                || !via.drill_mm.is_finite()
                || via.diameter_mm <= 0.0
                || via.drill_mm <= 0.0
                || via.drill_mm >= via.diameter_mm
            {
                violations.push(BoardInvariantViolation::InvalidViaDimensions { id: via.id });
            }
            validate_known_net(known_net_ids, "via", via.id, via.net_id, &mut violations);
        }
        for zone in &self.zones {
            if zone.outline.len() < 3 || zone.outline.iter().any(|point| !finite(*point)) {
                violations.push(BoardInvariantViolation::NonFiniteGeometry {
                    entity: "zone",
                    id: zone.id,
                });
            }
            if !valid_polygon(&zone.outline) {
                violations.push(BoardInvariantViolation::InvalidZonePolygon { id: zone.id });
            }
            if !self.layers.contains(&zone.layer) {
                violations.push(BoardInvariantViolation::MissingLayer {
                    entity: "zone",
                    id: zone.id,
                });
            }
            validate_known_net(known_net_ids, "zone", zone.id, zone.net_id, &mut violations);
        }
        let mut net_class_ids = HashSet::new();
        for class in &self.net_classes {
            if !net_class_ids.insert(class.class_id.clone()) {
                violations.push(BoardInvariantViolation::DuplicateNetClass {
                    class_id: class.class_id.clone(),
                });
            }
            if class.class_id.trim().is_empty()
                || !positive_finite(class.clearance_mm)
                || !positive_finite(class.track_width_mm)
                || !positive_finite(class.via_diameter_mm)
                || !positive_finite(class.via_drill_mm)
                || class.via_drill_mm >= class.via_diameter_mm
            {
                violations.push(BoardInvariantViolation::InvalidNetClass {
                    class_id: class.class_id.clone(),
                });
            }
        }
        if !positive_finite(self.design_rules.default_clearance_mm)
            || !positive_finite(self.design_rules.min_track_width_mm)
            || !positive_finite(self.design_rules.min_via_diameter_mm)
            || !positive_finite(self.design_rules.min_via_drill_mm)
            || !positive_finite(self.design_rules.board_edge_clearance_mm)
            || self.design_rules.min_via_drill_mm >= self.design_rules.min_via_diameter_mm
        {
            violations.push(BoardInvariantViolation::InvalidDesignRules);
        }
        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }

    pub(crate) fn footprint_candidates(&self, point: Point2) -> Vec<u64> {
        self.spatial_index.footprint_candidates(point)
    }

    pub(crate) fn footprints_in_rect(&self, min: Point2, max: Point2) -> Vec<u64> {
        self.spatial_index.footprints_in_rect(min, max)
    }

    pub(crate) fn track_candidates(&self, point: Point2) -> Vec<u64> {
        self.spatial_index.track_candidates(point)
    }

    pub(crate) fn track_candidates_in_bounds(&self, min: Point2, max: Point2) -> Vec<u64> {
        self.spatial_index.track_candidates_in_bounds(min, max)
    }

    pub(crate) fn track_candidate_pairs(&self) -> Vec<(u64, u64)> {
        self.spatial_index.track_candidate_pairs()
    }

    pub(crate) fn via_candidates(&self, point: Point2) -> Vec<u64> {
        self.spatial_index.via_candidates(point)
    }

    pub(crate) fn via_candidates_in_bounds(&self, min: Point2, max: Point2) -> Vec<u64> {
        self.spatial_index.via_candidates_in_bounds(min, max)
    }

    pub(crate) fn pad_candidates(&self, point: Point2) -> Vec<PadRef> {
        self.spatial_index.pad_candidates(point)
    }

    pub(crate) fn pad_position(&self, pad: &PadRef) -> Option<Point2> {
        let footprint = self.footprint(pad.footprint_id)?;
        let definition = self
            .footprint_library
            .iter()
            .find(|definition| definition.footprint_id == footprint.footprint_id)?;
        let pad = definition
            .pads
            .iter()
            .find(|item| item.number == pad.number)?;
        Some(footprint.transform().local_to_board(pad.position))
    }

    pub(crate) fn edge_candidates(&self, point: Point2) -> Vec<usize> {
        self.spatial_index.edge_candidates(point)
    }

    pub(crate) fn edges_in_rect(&self, min: Point2, max: Point2) -> Vec<usize> {
        self.spatial_index.edges_in_rect(min, max)
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

    pub(crate) fn apply_eco_patch(
        &mut self,
        symbols: &[SymbolInstance],
        nets: &[CadNet],
        removed_policy: RemovedFootprintPolicy,
    ) -> EcoPatch {
        let report = self.eco_report(symbols, nets);
        let footprint_ids_before = self
            .footprints
            .iter()
            .map(|footprint| footprint.id)
            .collect::<HashSet<_>>();
        let mut footprint_ids = report
            .removed_footprints
            .iter()
            .chain(&report.changed_assignments)
            .chain(&report.renamed_references)
            .copied()
            .collect::<HashSet<_>>();
        let removed_nets = report.removed_nets.iter().copied().collect::<HashSet<_>>();
        let track_ids = if removed_policy == RemovedFootprintPolicy::RemoveFootprintAndTracks {
            self.tracks
                .iter()
                .filter(|track| removed_nets.contains(&track.net_id))
                .map(|track| track.id)
                .collect()
        } else {
            HashSet::new()
        };
        let via_ids = if removed_policy == RemovedFootprintPolicy::RemoveFootprintAndTracks {
            self.vias
                .iter()
                .filter(|via| removed_nets.contains(&via.net_id))
                .map(|via| via.id)
                .collect()
        } else {
            HashSet::new()
        };
        let footprints_before =
            capture_entities(&self.footprints, &footprint_ids, |value| value.id);
        let tracks_before = capture_entities(&self.tracks, &track_ids, |value| value.id);
        let vias_before = capture_entities(&self.vias, &via_ids, |value| value.id);
        let net_classes_before = self.net_classes.clone();

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
        self.rebuild_entity_index();
        footprint_ids.extend(
            self.footprints
                .iter()
                .filter(|footprint| !footprint_ids_before.contains(&footprint.id))
                .map(|footprint| footprint.id),
        );
        EcoPatch {
            report,
            footprints_before,
            footprints_after: capture_entities(&self.footprints, &footprint_ids, |value| value.id),
            tracks_before,
            tracks_after: capture_entities(&self.tracks, &track_ids, |value| value.id),
            vias_before,
            vias_after: capture_entities(&self.vias, &via_ids, |value| value.id),
            net_classes_before,
            net_classes_after: self.net_classes.clone(),
            footprint_ids,
            track_ids,
            via_ids,
        }
    }

    pub(crate) fn apply_eco(
        &mut self,
        symbols: &[SymbolInstance],
        nets: &[CadNet],
        removed_policy: RemovedFootprintPolicy,
    ) -> EcoReport {
        self.apply_eco_patch(symbols, nets, removed_policy).report
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

fn finite(point: Point2) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn positive_finite(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

fn validate_known_net(
    known_net_ids: Option<&HashSet<usize>>,
    entity: &'static str,
    id: u64,
    net_id: usize,
    output: &mut Vec<BoardInvariantViolation>,
) {
    if known_net_ids.is_some_and(|known| !known.contains(&net_id)) {
        output.push(BoardInvariantViolation::UnknownNet { entity, id, net_id });
    }
}

fn valid_polygon(points: &[Point2]) -> bool {
    if points.len() < 3 || points.iter().any(|point| !finite(*point)) {
        return false;
    }
    let unique_len = if points.first() == points.last() {
        points.len().saturating_sub(1)
    } else {
        points.len()
    };
    if unique_len < 3 {
        return false;
    }
    let polygon = &points[..unique_len];
    let signed_twice_area = polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(unique_len)
        .map(|(left, right)| left.x * right.y - right.x * left.y)
        .sum::<f32>();
    if signed_twice_area.abs() <= 1.0e-6 {
        return false;
    }
    for left in 0..unique_len {
        let left_next = (left + 1) % unique_len;
        for right in (left + 1)..unique_len {
            let right_next = (right + 1) % unique_len;
            if left == right
                || left_next == right
                || right_next == left
                || (left == 0 && right_next == 0)
            {
                continue;
            }
            if segments_intersect(
                polygon[left],
                polygon[left_next],
                polygon[right],
                polygon[right_next],
            ) {
                return false;
            }
        }
    }
    true
}

fn segments_intersect(a: Point2, b: Point2, c: Point2, d: Point2) -> bool {
    fn cross(a: Point2, b: Point2, c: Point2) -> f32 {
        (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
    }
    fn on_segment(a: Point2, b: Point2, point: Point2) -> bool {
        const EPSILON: f32 = 1.0e-6;
        point.x >= a.x.min(b.x) - EPSILON
            && point.x <= a.x.max(b.x) + EPSILON
            && point.y >= a.y.min(b.y) - EPSILON
            && point.y <= a.y.max(b.y) + EPSILON
    }
    const EPSILON: f32 = 1.0e-6;
    let ab_c = cross(a, b, c);
    let ab_d = cross(a, b, d);
    let cd_a = cross(c, d, a);
    let cd_b = cross(c, d, b);
    (ab_c * ab_d < 0.0 && cd_a * cd_b < 0.0)
        || (ab_c.abs() <= EPSILON && on_segment(a, b, c))
        || (ab_d.abs() <= EPSILON && on_segment(a, b, d))
        || (cd_a.abs() <= EPSILON && on_segment(c, d, a))
        || (cd_b.abs() <= EPSILON && on_segment(c, d, b))
}

fn collect_duplicate_ids(
    ids: impl IntoIterator<Item = u64>,
    violation: impl Fn(u64) -> BoardInvariantViolation,
    output: &mut Vec<BoardInvariantViolation>,
) {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            output.push(violation(id));
        }
    }
}

fn capture_entities<T: Clone>(
    values: &[T],
    ids: &HashSet<u64>,
    id: impl Fn(&T) -> u64,
) -> Vec<(usize, T)> {
    values
        .iter()
        .enumerate()
        .filter(|(_, value)| ids.contains(&id(value)))
        .map(|(index, value)| (index, value.clone()))
        .collect()
}

fn restore_entities<T: Clone>(
    values: &mut Vec<T>,
    ids: &HashSet<u64>,
    captured: &[(usize, T)],
    id: impl Fn(&T) -> u64,
) {
    values.retain(|value| !ids.contains(&id(value)));
    let mut captured = captured.to_vec();
    captured.sort_by_key(|(index, _)| *index);
    for (index, value) in captured {
        values.insert(index.min(values.len()), value);
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
        let patch = board.apply_eco_patch(&[moved], &[], RemovedFootprintPolicy::KeepAsOrphan);

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

        patch.rollback(&mut board);
        assert!(board.footprints.iter().any(
            |footprint| footprint.reference == "R1" && footprint.symbol_instance_id == Some(1)
        ));
        assert!(
            board
                .footprints
                .iter()
                .any(|footprint| footprint.reference == "LED1"
                    && footprint.symbol_instance_id == Some(2))
        );
        patch.reapply(&mut board);
        assert!(board.footprints.iter().any(
            |footprint| footprint.reference == "R10" && footprint.symbol_instance_id == Some(1)
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

    #[test]
    fn entity_indices_follow_swap_remove_and_updates() {
        let mut board = Board::new_two_layer(50.0, 30.0);
        for id in 1..=3 {
            board.add_track(TrackSegment {
                id,
                net_id: id as usize,
                layer: BoardLayer::FrontCopper,
                start: Point2::new(id as f32, 1.0),
                end: Point2::new(id as f32 + 1.0, 1.0),
                width_mm: 0.25,
            });
            board.add_via(Via {
                id: id + 10,
                net_id: id as usize,
                position: Point2::new(id as f32, 2.0),
                diameter_mm: 0.6,
                drill_mm: 0.3,
            });
        }
        board.rebuild_entity_index();
        assert!(board.entity_index_is_consistent());

        assert_eq!(board.remove_track(2).map(|track| track.id), Some(2));
        assert_eq!(board.remove_via(11).map(|via| via.id), Some(11));
        assert!(board.track_mut(3).is_some());
        assert!(board.entity_index_is_consistent());

        let json = serde_json::to_string(&board).expect("serialize board");
        let mut restored: Board = serde_json::from_str(&json).expect("deserialize board");
        assert!(!restored.entity_index_is_consistent());
        restored.rebuild_entity_index();
        assert!(restored.entity_index_is_consistent());
    }

    #[test]
    fn spatial_index_updates_incrementally_with_board_entities() {
        let mut board = Board::new_two_layer(80.0, 50.0);
        board.add_track(TrackSegment {
            id: 41,
            net_id: 1,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(8.0, 8.0),
            end: Point2::new(16.0, 8.0),
            width_mm: 0.25,
        });
        assert!(board.track_candidates(Point2::new(12.0, 8.0)).contains(&41));
        board.edit_track(TrackSegment {
            id: 41,
            net_id: 1,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(48.0, 40.0),
            end: Point2::new(56.0, 40.0),
            width_mm: 0.25,
        });
        assert!(!board.track_candidates(Point2::new(12.0, 8.0)).contains(&41));
        assert!(
            board
                .track_candidates(Point2::new(52.0, 40.0))
                .contains(&41)
        );
        assert_eq!(board.remove_track(41).map(|track| track.id), Some(41));
        assert!(board.track_candidates(Point2::new(52.0, 40.0)).is_empty());
        assert!(board.entity_index_is_consistent());
    }

    #[test]
    fn board_validator_reports_corrupt_geometry_and_index() {
        let mut board = Board::new_two_layer(80.0, 50.0);
        board.tracks.push(TrackSegment {
            id: 9,
            net_id: 1,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(f32::NAN, 1.0),
            end: Point2::new(2.0, 1.0),
            width_mm: 0.0,
        });
        board.tracks.push(board.tracks[0].clone());

        let violations = board
            .validate_invariants()
            .expect_err("malformed board must be rejected");
        assert!(violations.contains(&BoardInvariantViolation::DuplicateTrackId { id: 9 }));
        assert!(violations.contains(&BoardInvariantViolation::EntityIndexMismatch));
        assert!(
            violations.contains(&BoardInvariantViolation::NonFiniteGeometry {
                entity: "track",
                id: 9,
            })
        );
        assert!(violations.contains(&BoardInvariantViolation::InvalidTrackWidth { id: 9 }));
    }

    #[test]
    fn board_validator_reports_library_net_and_rule_corruption() {
        let mut board = Board::new_two_layer(80.0, 50.0);
        board.footprints.push(BoardFootprint {
            id: 20,
            symbol_instance_id: Some(5),
            reference: "U1".to_string(),
            footprint_id: "missing".to_string(),
            position: Point2::new(10.0, 10.0),
            rotation_deg: 0.0,
            flipped: false,
            placed: true,
        });
        board.tracks.push(TrackSegment {
            id: 21,
            net_id: 99,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(10.0, 10.0),
            end: Point2::new(20.0, 10.0),
            width_mm: 0.25,
        });
        board.zones.push(Zone {
            id: 22,
            net_id: 99,
            layer: BoardLayer::FrontCopper,
            outline: vec![
                Point2::new(0.0, 0.0),
                Point2::new(5.0, 5.0),
                Point2::new(0.0, 5.0),
                Point2::new(5.0, 0.0),
            ],
        });
        board.net_classes.push(NetClass {
            class_id: "broken".to_string(),
            clearance_mm: -0.1,
            ..NetClass::default()
        });
        board.rebuild_entity_index();

        let known_nets = HashSet::from([1]);
        let violations = board
            .validate_invariants_with_nets(Some(&known_nets))
            .expect_err("cross-reference corruption must be diagnosed");
        assert!(
            violations.contains(&BoardInvariantViolation::MissingFootprintDefinition {
                id: 20,
                footprint_id: "missing".to_string(),
            })
        );
        assert!(violations.contains(&BoardInvariantViolation::UnknownNet {
            entity: "track",
            id: 21,
            net_id: 99,
        }));
        assert!(violations.contains(&BoardInvariantViolation::InvalidZonePolygon { id: 22 }));
        assert!(
            violations.contains(&BoardInvariantViolation::InvalidNetClass {
                class_id: "broken".to_string(),
            })
        );
    }

    #[test]
    fn board_validator_detects_stale_spatial_index() {
        let mut board = Board::new_two_layer(80.0, 50.0);
        board.add_track(TrackSegment {
            id: 30,
            net_id: 1,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(10.0, 10.0),
            end: Point2::new(20.0, 10.0),
            width_mm: 0.25,
        });
        board.tracks[0].end = Point2::new(70.0, 40.0);

        let violations = board
            .validate_invariants()
            .expect_err("out-of-band geometry mutation must stale the spatial index");
        assert!(violations.contains(&BoardInvariantViolation::SpatialIndexMismatch));

        board.rebuild_entity_index();
        assert!(board.validate_invariants().is_ok());
    }
}
