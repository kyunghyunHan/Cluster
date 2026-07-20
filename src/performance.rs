//! Deterministic workloads used by Criterion and performance regression checks.

use crate::editor::delta::{DocumentDelta, UndoableCommand};
use crate::engine::mna;
use crate::engine::netlist::build_canonical_connectivity_with_annotations;
use crate::engine::simulation::Simulation;
use crate::model::cad::{CadNet, Point2, Size2};
use crate::model::{
    CircuitSnapshot, Component, ComponentKind, JunctionDot, JunctionId, NetlistAnnotations, PinRef,
    SavedCircuit, SchematicAnnotations, Wire,
};
use crate::pcb::board::{Board, BoardFootprint};
use crate::pcb::footprint::{Footprint, Pad, PadShape};
use crate::pcb::layer::BoardLayer;
use crate::pcb::track::TrackSegment;
use crate::pcb::via::Via;
use crate::ui::canvas::{hit_test_component, hit_test_wire};
use crate::ui::current_flow::{CurrentFlowCache, FlowCacheKey};
use egui::{Pos2, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchematicSize {
    Small,
    Medium,
    Large,
}

impl SchematicSize {
    const fn counts(self) -> (usize, usize, usize, usize) {
        match self {
            Self::Small => (100, 300, 20, 10),
            Self::Medium => (500, 2_000, 100, 100),
            Self::Large => (1_000, 5_000, 250, 300),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcbSize {
    Small,
    Medium,
}

impl PcbSize {
    const fn counts(self) -> (usize, usize, usize, usize) {
        match self {
            Self::Small => (50, 200, 150, 10),
            Self::Medium => (250, 1_500, 2_000, 150),
        }
    }
}

pub struct SchematicFixture {
    components: Vec<Component>,
    wires: Vec<Wire>,
    annotations: NetlistAnnotations,
    expected: (usize, usize, usize, usize),
}

impl SchematicFixture {
    pub fn generate(size: SchematicSize) -> Self {
        let expected = size.counts();
        let (component_count, segment_count, label_count, junction_count) = expected;
        let mut components = Vec::with_capacity(component_count + label_count);
        for index in 0..component_count {
            components.push(Component {
                id: index as u64 + 1,
                kind: ComponentKind::Resistor,
                pos: Pos2::new((index % 25) as f32 * 80.0, (index / 25) as f32 * 80.0),
                rotation: 0,
                label: format!("R{}", index + 1),
                value: "1k".to_string(),
                part_id: None,
            });
        }
        for index in 0..label_count {
            components.push(Component {
                id: component_count as u64 + index as u64 + 1,
                kind: ComponentKind::NetLabel,
                pos: Pos2::new(index as f32 * 80.0, -120.0),
                rotation: 0,
                label: format!("N{}", index + 1),
                value: format!("FIXTURE_NET_{:03}", index % 17),
                part_id: None,
            });
        }
        let wires = (0..segment_count)
            .map(|index| {
                let row = index / 40;
                let column = index % 40;
                let x = column as f32 * 48.0;
                let y = row as f32 * 24.0 + 600.0;
                let end_y = if index % 11 == 0 { y + 24.0 } else { y };
                Wire::new(
                    10_000 + index as u64,
                    vec![Pos2::new(x, y), Pos2::new(x + 32.0, end_y)],
                )
            })
            .collect::<Vec<_>>();
        let annotations = SchematicAnnotations {
            junction_dots: (0..junction_count)
                .map(|index| {
                    let wire = &wires[index % wires.len()];
                    JunctionDot {
                        id: JunctionId(index as u64 + 1),
                        position: wire.points[0].lerp(wire.points[1], 0.5),
                    }
                })
                .collect(),
            no_connect_markers: Vec::new(),
        }
        .netlist_annotations();
        let fixture = Self {
            components,
            wires,
            annotations,
            expected,
        };
        fixture.assert_counts();
        fixture
    }

    fn assert_counts(&self) {
        let labels = self
            .components
            .iter()
            .filter(|component| component.kind == ComponentKind::NetLabel)
            .count();
        assert_eq!(
            (
                self.components.len() - labels,
                self.wires
                    .iter()
                    .map(|wire| wire.points.len().saturating_sub(1))
                    .sum::<usize>(),
                labels,
                self.annotations.junction_endpoints.len(),
            ),
            self.expected
        );
    }

    fn connectivity(&self) -> crate::CanonicalConnectivity {
        build_canonical_connectivity_with_annotations(
            &self.components,
            &self.wires,
            &self.annotations,
        )
    }

    pub fn connectivity_checksum(&self) -> usize {
        let result = self.connectivity();
        result.netlist.nets.len()
            + result.netlist.pins.len()
            + result.wire_segment_nets.len()
            + result.diagnostics.len()
    }

    pub fn erc_checksum(&self) -> usize {
        let connectivity = self.connectivity();
        crate::run_erc_with_netlist(
            &self.components,
            &self.wires,
            &Simulation::default(),
            &connectivity.netlist,
        )
        .len()
    }

    pub fn mna_checksum(&self) -> usize {
        let connectivity = self.connectivity();
        mna::solve_dc_detailed_with_connectivity(&self.components, &self.wires, &connectivity)
            .map_or_else(
                |_| connectivity.netlist.nets.len(),
                |dc| dc.node_voltages.len(),
            )
    }

    pub fn hit_test_checksum(&self) -> u64 {
        let miss = Pos2::new(-10_000.0, -10_000.0);
        u64::from(hit_test_component(miss, &self.components).is_some())
            + u64::from(hit_test_wire(miss, &self.wires).is_some())
    }

    pub fn viewport_query_checksum(&self) -> usize {
        let viewport = Rect::from_min_max(Pos2::ZERO, Pos2::new(640.0, 480.0));
        self.components
            .iter()
            .filter(|component| crate::ui::app::component_bounds(component).intersects(viewport))
            .count()
            + self
                .wires
                .iter()
                .filter(|wire| {
                    wire.points.windows(2).any(|segment| {
                        Rect::from_two_pos(segment[0], segment[1]).intersects(viewport)
                    })
                })
                .count()
    }

    pub fn flow_path_checksum(&self) -> usize {
        let mut dc = mna::DcResult::default();
        for wire in &self.wires {
            dc.wire_current_known.insert(wire.id);
            dc.wire_current.insert(wire.id, 0.001);
        }
        let mut cache = CurrentFlowCache::default();
        cache.rebuild_if_needed(
            FlowCacheKey {
                geometry_revision: 1,
                simulation_revision: 1,
            },
            &self.wires,
            Some(&dc),
        );
        cache
            .wires
            .iter()
            .map(|wire| wire.segments.len() + wire.runs.len())
            .sum()
    }

    pub fn flow_animation_checksum(&self, phase: f32) -> f32 {
        self.wires
            .iter()
            .take(3_000)
            .flat_map(|wire| wire.points.windows(2))
            .map(|segment| (phase * 90.0).rem_euclid(segment[0].distance(segment[1]).max(0.001)))
            .sum()
    }

    /// Deterministic CPU-side work representative of a cached schematic
    /// frame: viewport culling, hit testing and visible flow phase updates.
    pub fn frame_checksum(&self) -> usize {
        self.viewport_query_checksum()
            + self.hit_test_checksum() as usize
            + self.flow_animation_checksum(0.016) as usize
    }

    pub fn serialization_len(&self) -> usize {
        let mut app = crate::CircuitApp::new();
        app.document.components.clone_from(&self.components);
        app.document.wires.clone_from(&self.wires);
        serde_json::to_vec(&SavedCircuit::from_app(&app)).map_or(0, |json| json.len())
    }
}

pub struct PcbFixture {
    board: Board,
    nets: Vec<CadNet>,
    expected: (usize, usize, usize, usize),
}

impl PcbFixture {
    pub fn generate(size: PcbSize) -> Self {
        let expected = size.counts();
        let (footprint_count, pad_count, track_count, via_count) = expected;
        let mut board = Board::new_two_layer(500.0, 500.0);
        board.footprints = (0..footprint_count)
            .map(|index| BoardFootprint {
                id: index as u64 + 1,
                symbol_instance_id: Some(index as u64 + 1),
                reference: format!("U{}", index + 1),
                footprint_id: format!("PERF:{}", index + 1),
                position: Point2::new(
                    5.0 + (index % 25) as f32 * 18.0,
                    5.0 + (index / 25) as f32 * 18.0,
                ),
                rotation_deg: 0.0,
                flipped: false,
                placed: true,
            })
            .collect();
        board.footprint_library = (0..footprint_count)
            .map(|index| Footprint {
                footprint_id: format!("PERF:{}", index + 1),
                display_name: format!("Performance {}", index + 1),
                pads: (0..(pad_count / footprint_count
                    + usize::from(index < pad_count % footprint_count)))
                    .map(|pad| Pad {
                        number: (pad + 1).to_string(),
                        net_id: Some(index % 32),
                        position: Point2::new(pad as f32 * 1.27, 0.0),
                        size: Size2 { w: 1.0, h: 1.0 },
                        drill_mm: Some(0.5),
                        shape: PadShape::Circle,
                        layers: vec![BoardLayer::FrontCopper, BoardLayer::BackCopper],
                    })
                    .collect(),
                courtyard: Vec::new(),
                silkscreen: Vec::new(),
                fabrication: Vec::new(),
                model_3d_path: None,
            })
            .collect();
        board.tracks = (0..track_count)
            .map(|index| {
                let start = Point2::new(
                    2.0 + (index % 50) as f32 * 9.0,
                    150.0 + (index / 50) as f32 * 4.0,
                );
                TrackSegment {
                    id: 20_000 + index as u64,
                    net_id: index % 32,
                    layer: if index % 2 == 0 {
                        BoardLayer::FrontCopper
                    } else {
                        BoardLayer::BackCopper
                    },
                    start,
                    end: Point2::new(start.x + 6.0, start.y),
                    width_mm: 0.25,
                }
            })
            .collect();
        board.vias = (0..via_count)
            .map(|index| Via {
                id: 50_000 + index as u64,
                net_id: index % 32,
                position: Point2::new(10.0 + index as f32 * 2.0, 100.0),
                diameter_mm: 0.6,
                drill_mm: 0.3,
            })
            .collect();
        board.rebuild_entity_index();
        let nets = (0..32)
            .map(|net_id| CadNet {
                net_id,
                name: format!("PCB_NET_{net_id}"),
                connected_pins: board
                    .footprints
                    .iter()
                    .filter(|footprint| footprint.id as usize % 32 == net_id)
                    .map(|footprint| PinRef {
                        component_id: footprint.symbol_instance_id.unwrap_or(footprint.id),
                        pin_name: "1".to_string(),
                    })
                    .collect(),
                class_id: "Default".to_string(),
            })
            .collect();
        let fixture = Self {
            board,
            nets,
            expected,
        };
        fixture.assert_counts();
        fixture
    }

    fn assert_counts(&self) {
        assert_eq!(
            (
                self.board.footprints.len(),
                self.board
                    .footprint_library
                    .iter()
                    .map(|footprint| footprint.pads.len())
                    .sum::<usize>(),
                self.board.tracks.len(),
                self.board.vias.len(),
            ),
            self.expected
        );
    }

    pub fn hit_test_checksum(&self) -> usize {
        let point = Point2::new(-1_000.0, -1_000.0);
        self.board
            .footprint_candidates(point)
            .into_iter()
            .filter_map(|id| self.board.footprint(id))
            .filter(|footprint| {
                (footprint.position.x - point.x).abs() <= 5.0
                    && (footprint.position.y - point.y).abs() <= 5.0
            })
            .count()
            + self
                .board
                .track_candidates(point)
                .into_iter()
                .filter_map(|id| self.board.track(id))
                .filter(|track| point_segment_distance(point, track.start, track.end) <= 0.5)
                .count()
    }

    pub fn ratsnest_checksum(&self) -> usize {
        self.board.ratsnest_edges(&self.nets).len()
    }

    pub fn local_drc_checksum(&self) -> usize {
        let affected = self
            .board
            .tracks
            .iter()
            .take(128)
            .map(|track| track.id)
            .collect();
        crate::pcb::drc::run_local_drc(&self.board, &affected).len()
    }

    pub fn full_drc_checksum(&self) -> usize {
        crate::pcb::drc::run_drc_with_nets(&self.board, &self.nets).len()
    }
}

fn point_segment_distance(point: Point2, start: Point2, end: Point2) -> f32 {
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

pub struct HistoryFixture {
    before: CircuitSnapshot,
    after: CircuitSnapshot,
}

impl HistoryFixture {
    pub fn generate(size: SchematicSize) -> Self {
        let fixture = SchematicFixture::generate(size);
        let mut app = crate::CircuitApp::new();
        app.document.components = fixture.components;
        app.document.wires = fixture.wires;
        let before = app.snapshot();
        if let Some(component) = app.document.components.first_mut() {
            component.pos.x += 20.0;
        }
        let after = app.snapshot();
        Self { before, after }
    }

    pub fn snapshot_checksum(&self) -> usize {
        let cloned = self.before.clone();
        cloned.components.len() + cloned.wires.len()
    }

    pub fn delta_checksum(&self) -> usize {
        DocumentDelta::between(&self.before, &self.after).memory_cost()
    }

    pub fn undo_redo_checksum(&self) -> usize {
        let mut app = crate::CircuitApp::new();
        app.restore_snapshot(self.before.clone());
        let delta = DocumentDelta::between(&self.before, &self.after);
        app.push_history_delta(delta, "Benchmark edit", None);
        app.undo();
        app.redo();
        app.document.components.len() + app.document.wires.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_fixture_counts_are_exact_and_deterministic() {
        for size in [
            SchematicSize::Small,
            SchematicSize::Medium,
            SchematicSize::Large,
        ] {
            let first = SchematicFixture::generate(size);
            let second = SchematicFixture::generate(size);
            first.assert_counts();
            assert_eq!(
                first.connectivity_checksum(),
                second.connectivity_checksum()
            );
        }
        for size in [PcbSize::Small, PcbSize::Medium] {
            PcbFixture::generate(size).assert_counts();
        }
    }
}
