//! Deterministic workloads used by Criterion and performance regression checks.

use crate::editor::delta::{DocumentDelta, UndoableCommand};
use crate::engine::mna;
use crate::engine::netlist::{
    build_canonical_connectivity_profiled, build_canonical_connectivity_with_annotations,
    build_multi_page_circuit_netlist_with_annotations,
};
use crate::engine::simulation::Simulation;
use crate::model::cad::{CadNet, Point2, Size2};
use crate::model::{
    CircuitSnapshot, Component, ComponentKind, JunctionDot, JunctionId, NetlistAnnotations, PinRef,
    SavedCircuit, SchematicAnnotations, SchematicEntityIndex, Wire, component_pin_defs,
};
use crate::pcb::board::{Board, BoardFootprint};
use crate::pcb::footprint::{Footprint, Pad, PadShape};
use crate::pcb::layer::BoardLayer;
use crate::pcb::track::TrackSegment;
use crate::pcb::via::Via;
use crate::ui::canvas::spatial_index::SchematicSpatialIndex;
use crate::ui::canvas::{hit_test_component, hit_test_wire};
use crate::ui::current_flow::{CurrentFlowCache, FlowCacheKey};
use egui::{Pos2, Rect};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealisticFixtureKind {
    DenseEsp32I2c,
    BranchHeavyPower,
    MultiPage,
    MixedSimulation,
    DenseCrossing,
}

pub struct RealisticFixture {
    components: Vec<Component>,
    wires: Vec<Wire>,
    annotations: NetlistAnnotations,
    pages: Vec<(Vec<Component>, Vec<Wire>)>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MnaStageMetrics {
    pub circuit_compilation_ms: f64,
    pub node_indexing_ms: f64,
    pub matrix_allocation_ms: f64,
    pub matrix_stamping_ms: f64,
    pub nonlinear_iteration_ms: f64,
    pub factorization_solve_ms: f64,
    pub convergence_test_ms: f64,
    pub result_mapping_ms: f64,
    pub wire_segment_current_mapping_ms: f64,
    pub total_ms: f64,
}

impl RealisticFixture {
    pub fn generate(kind: RealisticFixtureKind) -> Self {
        match kind {
            RealisticFixtureKind::DenseEsp32I2c => dense_i2c_fixture(),
            RealisticFixtureKind::BranchHeavyPower => branch_heavy_power_fixture(),
            RealisticFixtureKind::MultiPage => multi_page_fixture(),
            RealisticFixtureKind::MixedSimulation => mixed_simulation_fixture(),
            RealisticFixtureKind::DenseCrossing => dense_crossing_fixture(),
        }
    }

    fn connectivity(&self) -> crate::model::CanonicalConnectivity {
        build_canonical_connectivity_with_annotations(
            &self.components,
            &self.wires,
            &self.annotations,
        )
    }

    pub fn connectivity_checksum(&self) -> usize {
        if !self.pages.is_empty() {
            let pages = self
                .pages
                .iter()
                .map(|(components, wires)| (components.as_slice(), wires.as_slice()))
                .collect::<Vec<_>>();
            let netlist =
                build_multi_page_circuit_netlist_with_annotations(&pages, &self.annotations);
            return netlist.nets.len() + netlist.pins.len() + netlist.wire_segments.len();
        }
        let connectivity = self.connectivity();
        connectivity.netlist.nets.len()
            + connectivity.netlist.pins.len()
            + connectivity.wire_segment_nets.len()
            + connectivity.diagnostics.len()
    }

    pub fn erc_checksum(&self) -> usize {
        if !self.pages.is_empty() {
            let pages = self
                .pages
                .iter()
                .map(|(components, wires)| (components.as_slice(), wires.as_slice()))
                .collect::<Vec<_>>();
            return crate::engine::validation::validate_beginner_rules(
                &build_multi_page_circuit_netlist_with_annotations(&pages, &self.annotations),
            )
            .len();
        }
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
        if !self.pages.is_empty() {
            return self.connectivity_checksum();
        }
        let connectivity = self.connectivity();
        mna::solve_dc_detailed_with_connectivity(&self.components, &self.wires, &connectivity)
            .map_or(connectivity.netlist.nets.len(), |dc| dc.node_voltages.len())
    }

    pub fn mna_stage_profile(&self) -> MnaStageMetrics {
        if !self.pages.is_empty() {
            return MnaStageMetrics::default();
        }
        let connectivity = self.connectivity();
        let (_, profile) =
            mna::solve_dc_detailed_profiled(&self.components, &self.wires, &connectivity);
        MnaStageMetrics {
            circuit_compilation_ms: profile.circuit_compilation_ms,
            node_indexing_ms: profile.node_indexing_ms,
            matrix_allocation_ms: profile.matrix_allocation_ms,
            matrix_stamping_ms: profile.matrix_stamping_ms,
            nonlinear_iteration_ms: profile.nonlinear_iteration_ms,
            factorization_solve_ms: profile.factorization_solve_ms,
            convergence_test_ms: profile.convergence_test_ms,
            result_mapping_ms: profile.result_mapping_ms,
            wire_segment_current_mapping_ms: profile.wire_segment_current_mapping_ms,
            total_ms: profile.total_ms,
        }
    }

    pub fn prepare_single_page_analysis(&self) -> Option<PreparedSchematicAnalysis<'_>> {
        self.pages.is_empty().then(|| PreparedSchematicAnalysis {
            components: &self.components,
            wires: &self.wires,
            connectivity: self.connectivity(),
        })
    }
}

fn fixture_component(id: u64, kind: ComponentKind, position: Pos2) -> Component {
    Component {
        id,
        kind,
        pos: position,
        rotation: 0,
        label: format!("{kind:?}{id}"),
        value: match kind {
            ComponentKind::Resistor => "4.7k",
            ComponentKind::Capacitor => "100nF",
            ComponentKind::VSource | ComponentKind::Battery => "3.3V",
            ComponentKind::DcMotor => "100",
            _ => "1",
        }
        .to_string(),
        part_id: None,
    }
}

fn endpoint_wire(id: u64, component: &Component, pin_index: usize, target: Pos2) -> Wire {
    let pins = component_pin_defs(component);
    let pin = &pins[pin_index.min(pins.len().saturating_sub(1))];
    Wire::with_endpoints(
        id,
        vec![pin.pos, Pos2::new(target.x, pin.pos.y), target],
        crate::model::WireEndpoint::Pin(PinRef {
            component_id: component.id,
            pin_name: pin.label.to_string(),
        }),
        crate::model::WireEndpoint::FreePoint(target),
    )
}

fn dense_i2c_fixture() -> RealisticFixture {
    let mut components = vec![
        fixture_component(1, ComponentKind::Esp32, Pos2::new(0.0, 0.0)),
        fixture_component(2, ComponentKind::ArduinoUno, Pos2::new(300.0, 0.0)),
    ];
    for index in 0..24 {
        components.push(fixture_component(
            index + 3,
            if index % 3 == 0 {
                ComponentKind::Oled
            } else {
                ComponentKind::Sensor
            },
            Pos2::new(
                (index % 8) as f32 * 180.0,
                180.0 + (index / 8) as f32 * 150.0,
            ),
        ));
    }
    components.push(fixture_component(
        100,
        ComponentKind::Resistor,
        Pos2::new(80.0, 580.0),
    ));
    components.push(fixture_component(
        101,
        ComponentKind::Resistor,
        Pos2::new(220.0, 580.0),
    ));
    let mut wires = Vec::new();
    let mut junctions = HashMap::new();
    for component in &components {
        for pin_index in 0..component_pin_defs(component).len().min(4) {
            let target = Pos2::new(component.pos.x, 720.0 + pin_index as f32 * 50.0);
            let wire_id = 10_000 + wires.len() as u64;
            wires.push(endpoint_wire(wire_id, component, pin_index, target));
            junctions.insert(JunctionId(wire_id), target);
        }
    }
    RealisticFixture {
        components,
        wires,
        annotations: NetlistAnnotations {
            junction_endpoints: junctions,
            ..Default::default()
        },
        pages: Vec::new(),
    }
}

fn branch_heavy_power_fixture() -> RealisticFixture {
    let mut components = vec![
        fixture_component(1, ComponentKind::VSource, Pos2::new(-180.0, 0.0)),
        fixture_component(2, ComponentKind::Ground, Pos2::new(-180.0, 140.0)),
    ];
    for index in 0..240 {
        components.push(fixture_component(
            index + 3,
            ComponentKind::Resistor,
            Pos2::new((index % 40) as f32 * 70.0, (index / 40) as f32 * 100.0),
        ));
    }
    let mut wires = Vec::new();
    let mut junctions = HashMap::new();
    for component in &components[2..] {
        for (pin_index, rail_y) in [(0, -120.0), (1, 720.0)] {
            let target = Pos2::new(component.pos.x, rail_y);
            let id = 20_000 + wires.len() as u64;
            wires.push(endpoint_wire(id, component, pin_index, target));
            junctions.insert(JunctionId(id), target);
        }
    }
    wires.push(Wire::new(
        29_998,
        vec![Pos2::new(-200.0, -120.0), Pos2::new(2_800.0, -120.0)],
    ));
    wires.push(Wire::new(
        29_999,
        vec![Pos2::new(-200.0, 720.0), Pos2::new(2_800.0, 720.0)],
    ));
    RealisticFixture {
        components,
        wires,
        annotations: NetlistAnnotations {
            junction_endpoints: junctions,
            ..Default::default()
        },
        pages: Vec::new(),
    }
}

fn multi_page_fixture() -> RealisticFixture {
    let mut pages = Vec::new();
    let mut scopes = HashMap::new();
    for page in 0..10u64 {
        let mut components = Vec::new();
        let mut wires = Vec::new();
        for index in 0..12u64 {
            let resistor_id = page * 100 + index * 2 + 1;
            let label_id = resistor_id + 1;
            let resistor = fixture_component(
                resistor_id,
                ComponentKind::Resistor,
                Pos2::new(index as f32 * 100.0, 100.0),
            );
            let mut label = fixture_component(
                label_id,
                ComponentKind::NetLabel,
                Pos2::new(index as f32 * 100.0, 200.0),
            );
            label.value = format!("BUS_{}", index % 4);
            scopes.insert(
                label_id,
                if index % 3 == 0 {
                    crate::model::NetLabelScope::Page
                } else {
                    crate::model::NetLabelScope::Global
                },
            );
            wires.push(endpoint_wire(40_000 + resistor_id, &resistor, 0, label.pos));
            components.push(resistor);
            components.push(label);
        }
        pages.push((components, wires));
    }
    RealisticFixture {
        components: Vec::new(),
        wires: Vec::new(),
        annotations: NetlistAnnotations {
            net_label_scopes: scopes,
            ..Default::default()
        },
        pages,
    }
}

fn mixed_simulation_fixture() -> RealisticFixture {
    let kinds = [
        ComponentKind::VSource,
        ComponentKind::Resistor,
        ComponentKind::Diode,
        ComponentKind::NpnTransistor,
        ComponentKind::Nmosfet,
        ComponentKind::Relay,
        ComponentKind::DcMotor,
        ComponentKind::Capacitor,
        ComponentKind::Battery,
        ComponentKind::Pmosfet,
    ];
    let mut components = (0..120u64)
        .map(|index| {
            fixture_component(
                index + 1,
                kinds[index as usize % kinds.len()],
                Pos2::new((index % 20) as f32 * 100.0, (index / 20) as f32 * 120.0),
            )
        })
        .collect::<Vec<_>>();
    // Give every device pin a nearby global net label. This keeps the mixed
    // nonlinear workload electrically well-defined without long fixture wires
    // accidentally touching unrelated pins as the grid becomes dense.
    let devices = components.clone();
    let mut wires = Vec::new();
    let mut label_scopes = HashMap::new();
    let mut next_label_id = 10_000u64;
    let mut power_domain = 0u64;
    for device in &devices {
        if matches!(device.kind, ComponentKind::VSource | ComponentKind::Battery) {
            power_domain += 1;
        }
        for (pin_index, pin) in component_pin_defs(device).iter().enumerate() {
            let ground_side = matches!(pin.label, "-" | "K" | "E" | "S" | "COIL-" | "GND")
                || pin.role == crate::model::PinRole::Ground;
            let mut label = fixture_component(
                next_label_id,
                ComponentKind::NetLabel,
                pin.pos
                    + egui::vec2(
                        if pin_index % 2 == 0 { -24.0 } else { 24.0 },
                        if ground_side { 28.0 } else { -28.0 },
                    ),
            );
            label.value = if ground_side {
                "GND".to_string()
            } else {
                format!("PWR_{power_domain}")
            };
            label_scopes.insert(label.id, crate::model::NetLabelScope::Global);
            let label_pin = component_pin_defs(&label)[0].clone();
            wires.push(Wire::with_endpoints(
                50_000 + wires.len() as u64,
                vec![pin.pos, label_pin.pos],
                crate::model::WireEndpoint::Pin(PinRef {
                    component_id: device.id,
                    pin_name: pin.label.to_string(),
                }),
                crate::model::WireEndpoint::Pin(PinRef {
                    component_id: label.id,
                    pin_name: label_pin.label.to_string(),
                }),
            ));
            components.push(label);
            next_label_id += 1;
        }
    }
    RealisticFixture {
        components,
        wires,
        annotations: NetlistAnnotations {
            net_label_scopes: label_scopes,
            ..Default::default()
        },
        pages: Vec::new(),
    }
}

fn dense_crossing_fixture() -> RealisticFixture {
    let mut wires = Vec::new();
    for index in 0..80u64 {
        let offset = 20.0 + index as f32 * 18.0;
        wires.push(Wire::new(
            60_000 + index,
            vec![Pos2::new(0.0, offset), Pos2::new(1_500.0, offset)],
        ));
        wires.push(Wire::new(
            61_000 + index,
            vec![Pos2::new(offset, 0.0), Pos2::new(offset, 1_500.0)],
        ));
    }
    let mut junction_endpoints = HashMap::new();
    let mut no_connects = Vec::new();
    for index in (0..80u64).step_by(8) {
        let position = Pos2::new(20.0 + index as f32 * 18.0, 20.0 + index as f32 * 18.0);
        junction_endpoints.insert(JunctionId(70_000 + index), position);
        no_connects.push(position + egui::vec2(9.0, 0.0));
    }
    RealisticFixture {
        components: Vec::new(),
        wires,
        annotations: NetlistAnnotations {
            junction_endpoints,
            no_connects,
            ..Default::default()
        },
        pages: Vec::new(),
    }
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
    entity_index: SchematicEntityIndex,
    spatial_index: SchematicSpatialIndex,
}

/// A schematic fixture whose canonical connectivity has already been built.
///
/// Keeping this boundary explicit prevents ERC and MNA micro-benchmarks from
/// accidentally charging connectivity construction to the rule evaluator or
/// solver. End-to-end benchmarks remain available on `SchematicFixture`.
pub struct PreparedSchematicAnalysis<'a> {
    components: &'a [Component],
    wires: &'a [Wire],
    connectivity: crate::CanonicalConnectivity,
}

pub struct PreparedSaveFixture {
    saved: SavedCircuit,
    json: String,
    path: PathBuf,
}

impl PreparedSaveFixture {
    pub fn serialization_len(&self) -> usize {
        serde_json::to_vec(&self.saved).map_or(0, |json| json.len())
    }

    pub fn atomic_write_len(&self) -> usize {
        let Some(path) = self.path.to_str() else {
            return 0;
        };
        crate::storage::save::write_with_backup(path, &self.json)
            .map(|()| self.json.len())
            .unwrap_or(0)
    }
}

impl Drop for PreparedSaveFixture {
    fn drop(&mut self) {
        for suffix in ["", ".tmp", ".bak", ".bak.1", ".bak.2", ".bak.3"] {
            let _ = std::fs::remove_file(format!("{}{}", self.path.display(), suffix));
        }
    }
}

pub struct PreparedAutosaveFixture {
    app: crate::CircuitApp,
}

impl PreparedAutosaveFixture {
    /// Measures the work still performed on the UI thread before enqueueing an
    /// autosave job: materializing the schema DTO from the live document.
    pub fn ui_thread_snapshot_len(&self) -> usize {
        let saved = SavedCircuit::from_app(&self.app);
        saved.components.len()
            + saved.wires.len()
            + saved
                .pages
                .iter()
                .map(|page| page.components.len() + page.wires.len())
                .sum::<usize>()
    }
}

impl PreparedSchematicAnalysis<'_> {
    pub fn erc_evaluation_checksum(&self) -> usize {
        crate::run_erc_with_netlist(
            self.components,
            self.wires,
            &Simulation::default(),
            &self.connectivity.netlist,
        )
        .len()
    }

    /// Evaluates only rules whose result can change when component values or
    /// switch state change. Connectivity is prepared and topology rules are
    /// intentionally excluded.
    pub fn erc_values_only_checksum(&self) -> usize {
        crate::engine::validation::validate_beginner_rules_for(
            &self.connectivity.netlist,
            &[crate::engine::erc::ErcDependency::Values],
        )
        .len()
    }

    /// Evaluates the topology-dependent rule set against a prepared netlist.
    pub fn erc_topology_only_checksum(&self) -> usize {
        crate::engine::validation::validate_beginner_rules_for(
            &self.connectivity.netlist,
            &[crate::engine::erc::ErcDependency::Topology],
        )
        .len()
    }

    pub fn mna_attempt_checksum(&self) -> usize {
        mna::solve_dc_detailed_with_connectivity(self.components, self.wires, &self.connectivity)
            .map_or_else(
                |_| self.connectivity.netlist.nets.len(),
                |dc| dc.node_voltages.len(),
            )
    }

    /// Analysis attempt with canonical connectivity already reused. Callers
    /// must use a solvable fixture before presenting this as solver latency.
    pub fn reused_connectivity_analysis_checksum(&self) -> usize {
        let mna = self.mna_attempt_checksum();
        mna.saturating_add(self.erc_values_only_checksum())
    }

    pub fn mna_stage_profile(&self) -> MnaStageMetrics {
        let (_, profile) =
            mna::solve_dc_detailed_profiled(self.components, self.wires, &self.connectivity);
        MnaStageMetrics {
            circuit_compilation_ms: profile.circuit_compilation_ms,
            node_indexing_ms: profile.node_indexing_ms,
            matrix_allocation_ms: profile.matrix_allocation_ms,
            matrix_stamping_ms: profile.matrix_stamping_ms,
            nonlinear_iteration_ms: profile.nonlinear_iteration_ms,
            factorization_solve_ms: profile.factorization_solve_ms,
            convergence_test_ms: profile.convergence_test_ms,
            result_mapping_ms: profile.result_mapping_ms,
            wire_segment_current_mapping_ms: profile.wire_segment_current_mapping_ms,
            total_ms: profile.total_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ConnectivityStageMetrics {
    pub endpoint_extraction_ms: f64,
    pub segment_spatial_index_build_ms: f64,
    pub pin_spatial_index_build_ms: f64,
    pub intersection_candidate_lookup_ms: f64,
    pub exact_intersection_checks_ms: f64,
    pub junction_application_ms: f64,
    pub endpoint_on_segment_contacts_ms: f64,
    pub union_find_ms: f64,
    pub label_merge_ms: f64,
    pub net_construction_ms: f64,
    pub deterministic_sorting_ms: f64,
    pub diagnostics_ms: f64,
    pub total_ms: f64,
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
        let schematic_annotations = SchematicAnnotations {
            junction_dots: annotations
                .junction_endpoints
                .iter()
                .map(|(&id, &position)| JunctionDot { id, position })
                .collect(),
            no_connect_markers: Vec::new(),
        };
        let mut entity_index = SchematicEntityIndex::default();
        entity_index.rebuild(&components, &wires, &schematic_annotations);
        let mut spatial_index = SchematicSpatialIndex::default();
        spatial_index.sync(&components, &wires);
        let fixture = Self {
            components,
            wires,
            annotations,
            expected,
            entity_index,
            spatial_index,
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

    pub fn prepare_analysis(&self) -> PreparedSchematicAnalysis<'_> {
        PreparedSchematicAnalysis {
            components: &self.components,
            wires: &self.wires,
            connectivity: self.connectivity(),
        }
    }

    fn save_app(&self) -> crate::CircuitApp {
        let mut app = crate::CircuitApp::new();
        app.document.components.clone_from(&self.components);
        app.document.wires.clone_from(&self.wires);
        app.document.next_id = self
            .components
            .iter()
            .map(|component| component.id)
            .chain(self.wires.iter().map(|wire| wire.id))
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        app
    }

    pub fn prepare_save(&self) -> PreparedSaveFixture {
        static SAVE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
        let app = self.save_app();
        let saved = SavedCircuit::from_app(&app);
        let json = serde_json::to_string(&saved).unwrap_or_default();
        let sequence = SAVE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "cluster-performance-{}-{sequence}.json",
            std::process::id()
        ));
        PreparedSaveFixture { saved, json, path }
    }

    pub fn prepare_autosave(&self) -> PreparedAutosaveFixture {
        PreparedAutosaveFixture {
            app: self.save_app(),
        }
    }

    pub fn connectivity_checksum(&self) -> usize {
        let result = self.connectivity();
        result.netlist.nets.len()
            + result.netlist.pins.len()
            + result.wire_segment_nets.len()
            + result.diagnostics.len()
    }

    pub fn connectivity_stage_profile(&self) -> ConnectivityStageMetrics {
        let (_, profile) =
            build_canonical_connectivity_profiled(&self.components, &self.wires, &self.annotations);
        ConnectivityStageMetrics {
            endpoint_extraction_ms: profile.endpoint_extraction_ms,
            segment_spatial_index_build_ms: profile.segment_spatial_index_build_ms,
            pin_spatial_index_build_ms: profile.pin_spatial_index_build_ms,
            intersection_candidate_lookup_ms: profile.intersection_candidate_lookup_ms,
            exact_intersection_checks_ms: profile.exact_intersection_checks_ms,
            junction_application_ms: profile.junction_application_ms,
            endpoint_on_segment_contacts_ms: profile.endpoint_on_segment_contacts_ms,
            union_find_ms: profile.union_find_ms,
            label_merge_ms: profile.label_merge_ms,
            net_construction_ms: profile.net_construction_ms,
            deterministic_sorting_ms: profile.deterministic_sorting_ms,
            diagnostics_ms: profile.diagnostics_ms,
            total_ms: profile.total_ms,
        }
    }

    pub fn connectivity_plus_erc_checksum(&self) -> usize {
        let connectivity = self.connectivity();
        crate::run_erc_with_netlist(
            &self.components,
            &self.wires,
            &Simulation::default(),
            &connectivity.netlist,
        )
        .len()
    }

    pub fn connectivity_plus_mna_checksum(&self) -> usize {
        let connectivity = self.connectivity();
        mna::solve_dc_detailed_with_connectivity(&self.components, &self.wires, &connectivity)
            .map_or_else(
                |_| connectivity.netlist.nets.len(),
                |dc| dc.node_voltages.len(),
            )
    }

    #[deprecated(note = "use connectivity_plus_erc_checksum for an explicit timing boundary")]
    pub fn erc_checksum(&self) -> usize {
        self.connectivity_plus_erc_checksum()
    }

    #[deprecated(note = "use connectivity_plus_mna_checksum for an explicit timing boundary")]
    pub fn mna_checksum(&self) -> usize {
        self.connectivity_plus_mna_checksum()
    }

    pub fn hit_test_checksum(&self) -> u64 {
        let miss = Pos2::new(-10_000.0, -10_000.0);
        u64::from(hit_test_component(miss, &self.components).is_some())
            + u64::from(hit_test_wire(miss, &self.wires).is_some())
    }

    pub fn component_hit_checksum(&self, index: usize) -> u64 {
        let position = self.components[index.min(self.components.len() - 1)].pos;
        match hit_test_component(position, &self.components) {
            Some(crate::app::Selection::Component(id)) => id,
            _ => 0,
        }
    }

    pub fn component_miss_checksum(&self) -> u64 {
        u64::from(hit_test_component(Pos2::new(-10_000.0, -10_000.0), &self.components).is_some())
    }

    pub fn pin_hit_dense_checksum(&self) -> u64 {
        let component = &self.components[self.components.len() / 2];
        let pins = component_pin_defs(component);
        let Some(pin) = pins.first() else {
            return 0;
        };
        self.spatial_index
            .nearest_pin(pin.pos, 1.0)
            .map_or(0, |pin| pin.component_id)
    }

    pub fn wire_hit_dense_checksum(&self) -> u64 {
        let wire = &self.wires[self.wires.len() / 2];
        let position = wire.points[0].lerp(wire.points[1], 0.5);
        match hit_test_wire(position, &self.wires) {
            Some(crate::app::Selection::Wire(id)) => id,
            _ => 0,
        }
    }

    pub fn wire_miss_checksum(&self) -> u64 {
        u64::from(hit_test_wire(Pos2::new(-10_000.0, -10_000.0), &self.wires).is_some())
    }

    pub fn indexed_hit_test_checksum(&self) -> u64 {
        let component = &self.components[self.components.len() / 2];
        self.spatial_index
            .query_components(component.pos, component.pos)
            .into_iter()
            .filter_map(|id| self.entity_index.component(id).map(|index| (index, id)))
            .max_by_key(|(index, _)| *index)
            .map_or(0, |(_, id)| id)
    }

    pub fn indexed_viewport_checksum(&self, viewport: Rect) -> usize {
        self.spatial_index.components_in_viewport(viewport).len()
            + self.spatial_index.wire_segments_in_viewport(viewport).len()
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
    pub fn synthetic_canvas_cpu_checksum(&self) -> usize {
        self.viewport_query_checksum()
            + self.hit_test_checksum() as usize
            + self.flow_animation_checksum(0.016) as usize
    }

    pub fn serialization_len(&self) -> usize {
        self.prepare_save().serialization_len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffscreenFrameScenario {
    EmptyProject,
    SmallSchematic,
    MediumSchematic,
    LargeSchematic,
    ValidationPanel,
    Inspector,
    SimulationAnimation,
    PcbWorkspace,
    BreadboardWorkspace,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OffscreenFrameMetrics {
    pub app_update_ms: f64,
    pub top_panel_ms: f64,
    pub left_panel_ms: f64,
    pub right_panel_ms: f64,
    pub bottom_panels_ms: f64,
    pub canvas_ms: f64,
    pub canvas_prepare_ms: f64,
    pub wire_paint_ms: f64,
    pub symbol_paint_ms: f64,
    pub overlay_and_interaction_ms: f64,
    pub tessellation_ms: f64,
    pub total_ms: f64,
    pub shape_count: usize,
    pub primitive_count: usize,
    pub visible_component_count: usize,
    pub visible_wire_segment_count: usize,
}

/// Runs the production `CircuitApp::update_ui` body inside an offscreen egui
/// context, followed by the same tessellation API used by a native viewport.
pub struct OffscreenFrameFixture {
    app: crate::CircuitApp,
    context: egui::Context,
    raw_input: egui::RawInput,
    next_time: f64,
}

impl OffscreenFrameFixture {
    pub fn generate(scenario: OffscreenFrameScenario) -> Self {
        let mut app = crate::CircuitApp::new();
        app.workspace_state.bottom_dock_open = false;
        let schematic_size = match scenario {
            OffscreenFrameScenario::EmptyProject | OffscreenFrameScenario::PcbWorkspace => None,
            OffscreenFrameScenario::SmallSchematic
            | OffscreenFrameScenario::ValidationPanel
            | OffscreenFrameScenario::Inspector
            | OffscreenFrameScenario::SimulationAnimation
            | OffscreenFrameScenario::BreadboardWorkspace => Some(SchematicSize::Small),
            OffscreenFrameScenario::MediumSchematic => Some(SchematicSize::Medium),
            OffscreenFrameScenario::LargeSchematic => Some(SchematicSize::Large),
        };
        if let Some(size) = schematic_size {
            let fixture = SchematicFixture::generate(size);
            app.document.components = fixture.components;
            app.document.wires = fixture.wires;
        }
        match scenario {
            OffscreenFrameScenario::ValidationPanel => {
                app.workspace_state.bottom_dock_open = true;
                app.workspace_state.bottom_dock_tab = crate::ui::bottom_dock::BottomDockTab::Erc;
            }
            OffscreenFrameScenario::Inspector => {
                app.editor.selected = app
                    .document
                    .components
                    .first()
                    .map(|component| crate::app::Selection::Component(component.id));
            }
            OffscreenFrameScenario::SimulationAnimation => {
                app.simulate = true;
                app.simulation_ui.last_simulation_enabled = true;
            }
            OffscreenFrameScenario::PcbWorkspace => {
                app.workspace_state.workspace = crate::ui::app::Workspace::Pcb;
                app.document.board = PcbFixture::generate(PcbSize::Medium).board;
            }
            OffscreenFrameScenario::BreadboardWorkspace => {
                app.workspace_state.workspace = crate::ui::app::Workspace::Breadboard;
                app.breadboard_ui.open = true;
            }
            _ => {}
        }

        let annotations = app.document.annotations.netlist_annotations();
        let connectivity = build_canonical_connectivity_with_annotations(
            &app.document.components,
            &app.document.wires,
            &annotations,
        );
        let revision = app.analysis.revisions.schematic_connectivity;
        app.analysis.cached_netlist = Some((revision, Arc::new(connectivity.netlist.clone())));
        app.analysis.cached_connectivity = Some((revision, Arc::new(connectivity)));

        let raw_input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
            ..Default::default()
        };
        let mut fixture = Self {
            app,
            context: egui::Context::default(),
            raw_input,
            next_time: 0.0,
        };
        let _ = fixture.measure_frame();
        fixture
    }

    pub fn measure_frame(&mut self) -> OffscreenFrameMetrics {
        self.next_time += 1.0 / 60.0;
        let mut input = self.raw_input.clone();
        input.time = Some(self.next_time);
        let total_started = Instant::now();
        let mut app_update_ms = 0.0;
        let output = self.context.run(input, |context| {
            let started = Instant::now();
            self.app.update_ui(context);
            app_update_ms = started.elapsed().as_secs_f64() * 1_000.0;
        });
        let shape_count = output.shapes.len();
        let tessellation_started = Instant::now();
        let primitives = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        let tessellation_ms = tessellation_started.elapsed().as_secs_f64() * 1_000.0;
        OffscreenFrameMetrics {
            app_update_ms,
            top_panel_ms: self.app.performance.top_panel_ms,
            left_panel_ms: self.app.performance.left_panel_ms,
            right_panel_ms: self.app.performance.right_panel_ms,
            bottom_panels_ms: self.app.performance.bottom_panels_ms,
            canvas_ms: self.app.performance.canvas_ms,
            canvas_prepare_ms: self.app.performance.canvas_prepare_ms,
            wire_paint_ms: self.app.performance.wire_paint_ms,
            symbol_paint_ms: self.app.performance.symbol_paint_ms,
            overlay_and_interaction_ms: self.app.performance.overlay_and_interaction_ms,
            tessellation_ms,
            total_ms: total_started.elapsed().as_secs_f64() * 1_000.0,
            shape_count,
            primitive_count: primitives.len(),
            visible_component_count: self.app.performance.rendered_components,
            visible_wire_segment_count: self.app.performance.rendered_wire_segments,
        }
    }

    pub fn checksum(&mut self) -> usize {
        let metrics = self.measure_frame();
        metrics.shape_count
            + metrics.primitive_count
            + metrics.visible_component_count
            + metrics.visible_wire_segment_count
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

    pub fn footprint_hit_checksum(&self, index: usize, indexed: bool) -> u64 {
        let footprint = &self.board.footprints[index.min(self.board.footprints.len() - 1)];
        let point = footprint.position;
        let found = if indexed {
            self.board
                .footprint_candidates(point)
                .into_iter()
                .filter_map(|id| self.board.footprint(id))
                .find(|candidate| {
                    (candidate.position.x - point.x).abs() <= 6.5
                        && (candidate.position.y - point.y).abs() <= 3.5
                })
        } else {
            self.board.footprints.iter().rev().find(|candidate| {
                (candidate.position.x - point.x).abs() <= 6.5
                    && (candidate.position.y - point.y).abs() <= 3.5
            })
        };
        found.map_or(0, |footprint| footprint.id)
    }

    pub fn footprint_miss_checksum(&self, indexed: bool) -> usize {
        let point = Point2::new(-1_000.0, -1_000.0);
        if indexed {
            self.board.footprint_candidates(point).len()
        } else {
            self.board
                .footprints
                .iter()
                .filter(|footprint| {
                    (footprint.position.x - point.x).abs() <= 6.5
                        && (footprint.position.y - point.y).abs() <= 3.5
                })
                .count()
        }
    }

    pub fn pad_hit_dense_checksum(&self) -> usize {
        let footprint = &self.board.footprints[self.board.footprints.len() / 2];
        self.board.pad_candidates(footprint.position).len()
    }

    pub fn track_hit_dense_checksum(&self, indexed: bool) -> u64 {
        let track = &self.board.tracks[self.board.tracks.len() / 2];
        let point = Point2::new(
            (track.start.x + track.end.x) * 0.5,
            (track.start.y + track.end.y) * 0.5,
        );
        let found = if indexed {
            self.board
                .track_candidates(point)
                .into_iter()
                .filter_map(|id| self.board.track(id))
                .find(|candidate| {
                    point_segment_distance(point, candidate.start, candidate.end) <= 0.5
                })
        } else {
            self.board.tracks.iter().rev().find(|candidate| {
                point_segment_distance(point, candidate.start, candidate.end) <= 0.5
            })
        };
        found.map_or(0, |track| track.id)
    }

    pub fn track_miss_checksum(&self, indexed: bool) -> usize {
        let point = Point2::new(-1_000.0, -1_000.0);
        if indexed {
            self.board.track_candidates(point).len()
        } else {
            self.board
                .tracks
                .iter()
                .filter(|track| point_segment_distance(point, track.start, track.end) <= 0.5)
                .count()
        }
    }

    pub fn viewport_checksum(&self, variant: usize) -> usize {
        let (min, max) = match variant {
            0 => (Point2::new(-1_000.0, -1_000.0), Point2::new(-900.0, -900.0)),
            1 => (Point2::new(0.0, 0.0), Point2::new(20.0, 20.0)),
            2 => (Point2::new(0.0, 0.0), Point2::new(160.0, 160.0)),
            _ => (Point2::new(0.0, 0.0), Point2::new(500.0, 500.0)),
        };
        self.board.footprints_in_rect(min, max).len()
            + self.board.track_candidates_in_bounds(min, max).len()
            + self.board.via_candidates_in_bounds(min, max).len()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandHistoryScenario {
    MoveOneComponent,
    MoveHundredComponents,
    RotateComponent,
    EditProperty,
    AddWire,
    AddAndSplitWire,
    RoutePcbTrack,
    AddVia,
    MoveFootprint,
}

pub struct CommandHistoryFixture {
    app: crate::CircuitApp,
    scenario: CommandHistoryScenario,
}

impl CommandHistoryFixture {
    pub fn generate(scenario: CommandHistoryScenario) -> Self {
        let schematic = SchematicFixture::generate(SchematicSize::Large);
        let mut app = crate::CircuitApp::new();
        app.document.components = schematic.components;
        app.document.wires = schematic.wires;
        app.document.next_id = app
            .document
            .components
            .iter()
            .map(|component| component.id)
            .chain(app.document.wires.iter().map(|wire| wire.id))
            .max()
            .unwrap_or(0)
            + 1;
        app.document.board = PcbFixture::generate(PcbSize::Small).board;
        app.analysis.schematic_entity_revision = u64::MAX;
        app.analysis.schematic_spatial_revision = u64::MAX;
        app.analysis.attachment_revision = u64::MAX;
        Self { app, scenario }
    }

    pub fn command_undo_redo_checksum(&mut self) -> usize {
        use crate::commands::EditorCommand;
        use crate::commands::component::ComponentCommand;
        use crate::commands::pcb::PcbCommand;
        use crate::commands::properties::PropertiesCommand;
        use crate::commands::selection::SelectionCommand;
        use crate::commands::wiring::WiringCommand;

        let command = match self.scenario {
            CommandHistoryScenario::MoveOneComponent => {
                EditorCommand::Component(ComponentCommand::Move {
                    component_ids: [1].into_iter().collect(),
                    delta: egui::vec2(20.0, 0.0),
                })
            }
            CommandHistoryScenario::MoveHundredComponents => {
                EditorCommand::Component(ComponentCommand::Move {
                    component_ids: (1..=100).collect(),
                    delta: egui::vec2(20.0, 20.0),
                })
            }
            CommandHistoryScenario::RotateComponent => {
                self.app.editor.selected = Some(crate::app::Selection::Component(1));
                EditorCommand::Selection(SelectionCommand::Rotate)
            }
            CommandHistoryScenario::EditProperty => {
                EditorCommand::Properties(PropertiesCommand::SetComponentValue {
                    component_id: 1,
                    value: "2.2k".to_string(),
                })
            }
            CommandHistoryScenario::AddWire => EditorCommand::Wiring(WiringCommand::Add {
                points: vec![
                    egui::pos2(-2_000.0, -2_000.0),
                    egui::pos2(-1_960.0, -2_000.0),
                ],
            }),
            CommandHistoryScenario::AddAndSplitWire => {
                let first = &self.app.document.wires[0];
                let midpoint = first.points[0].lerp(first.points[1], 0.5);
                EditorCommand::Wiring(WiringCommand::Add {
                    points: vec![midpoint, midpoint + egui::vec2(0.0, 40.0)],
                })
            }
            CommandHistoryScenario::RoutePcbTrack => {
                let id = self
                    .app
                    .document
                    .board
                    .tracks
                    .iter()
                    .map(|track| track.id)
                    .max()
                    .unwrap_or(0)
                    + 1;
                EditorCommand::Pcb(PcbCommand::AddTrack(TrackSegment {
                    id,
                    net_id: 1,
                    layer: BoardLayer::FrontCopper,
                    start: Point2::new(1.0, 1.0),
                    end: Point2::new(8.0, 1.0),
                    width_mm: 0.25,
                }))
            }
            CommandHistoryScenario::AddVia => {
                let id = self
                    .app
                    .document
                    .board
                    .vias
                    .iter()
                    .map(|via| via.id)
                    .max()
                    .unwrap_or(0)
                    + 1;
                EditorCommand::Pcb(PcbCommand::AddVia(Via {
                    id,
                    net_id: 1,
                    position: Point2::new(4.0, 4.0),
                    diameter_mm: 0.6,
                    drill_mm: 0.3,
                }))
            }
            CommandHistoryScenario::MoveFootprint => {
                let footprint = &self.app.document.board.footprints[0];
                EditorCommand::Pcb(PcbCommand::MoveFootprint {
                    footprint_id: footprint.id,
                    position: Point2::new(footprint.position.x + 1.0, footprint.position.y),
                })
            }
        };
        self.app.execute_editor_command(command);
        self.app.undo();
        self.app.redo();
        let checksum = self.app.document.components.len()
            + self.app.document.wires.len()
            + self.app.document.board.tracks.len()
            + self.app.document.board.vias.len();
        self.app.undo();
        checksum
    }
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

    #[test]
    fn realistic_fixtures_build_connectivity_erc_and_mna_without_panics() {
        for kind in [
            RealisticFixtureKind::DenseEsp32I2c,
            RealisticFixtureKind::BranchHeavyPower,
            RealisticFixtureKind::MultiPage,
            RealisticFixtureKind::MixedSimulation,
            RealisticFixtureKind::DenseCrossing,
        ] {
            let fixture = RealisticFixture::generate(kind);
            assert!(fixture.connectivity_checksum() > 0, "{kind:?}");
            let _ = fixture.erc_checksum();
            assert!(fixture.mna_checksum() > 0, "{kind:?}");
        }
        let mixed = RealisticFixture::generate(RealisticFixtureKind::MixedSimulation);
        assert!(mixed.mna_stage_profile().total_ms > 0.0);
    }

    #[test]
    fn real_command_history_scenarios_round_trip_through_undo_and_redo() {
        for scenario in [
            CommandHistoryScenario::MoveOneComponent,
            CommandHistoryScenario::MoveHundredComponents,
            CommandHistoryScenario::RotateComponent,
            CommandHistoryScenario::EditProperty,
            CommandHistoryScenario::AddWire,
            CommandHistoryScenario::AddAndSplitWire,
            CommandHistoryScenario::RoutePcbTrack,
            CommandHistoryScenario::AddVia,
            CommandHistoryScenario::MoveFootprint,
        ] {
            assert!(
                CommandHistoryFixture::generate(scenario).command_undo_redo_checksum() > 0,
                "{scenario:?}"
            );
        }
    }
}
