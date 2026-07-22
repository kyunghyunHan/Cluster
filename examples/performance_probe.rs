use cluster::performance::{
    CommandHistoryFixture, CommandHistoryScenario, HistoryFixture, OffscreenFrameFixture,
    OffscreenFrameScenario, PcbFixture, PcbSize, RealisticFixture, RealisticFixtureKind,
    SchematicFixture, SchematicSize,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

struct CountingAllocator;
static CURRENT_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let current = CURRENT_BYTES.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK_BYTES.fetch_max(current, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        CURRENT_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn measure(name: &str, samples: usize, mut operation: impl FnMut() -> usize) {
    let base = CURRENT_BYTES.load(Ordering::Relaxed);
    PEAK_BYTES.store(base, Ordering::Relaxed);
    let mut elapsed = Vec::with_capacity(samples);
    let mut checksum = 0;
    for _ in 0..samples {
        let started = Instant::now();
        checksum ^= black_box(operation());
        elapsed.push(started.elapsed().as_secs_f64() * 1_000.0);
    }
    elapsed.sort_by(f64::total_cmp);
    let percentile = |fraction: f64| {
        let index = ((elapsed.len() - 1) as f64 * fraction).round() as usize;
        elapsed[index]
    };
    let peak = PEAK_BYTES.load(Ordering::Relaxed).saturating_sub(base);
    println!(
        "{name:28} p50={:10.4} ms p95={:10.4} ms max={:10.4} ms peak_heap={} checksum={checksum}",
        percentile(0.50),
        percentile(0.95),
        elapsed[elapsed.len() - 1],
        peak,
    );
}

fn measure_offscreen_frame(name: &str, samples: usize, scenario: OffscreenFrameScenario) {
    let mut fixture = OffscreenFrameFixture::generate(scenario);
    let mut total = Vec::with_capacity(samples);
    let mut update = Vec::with_capacity(samples);
    let mut tessellation = Vec::with_capacity(samples);
    let mut stages = Vec::with_capacity(samples);
    let mut counts = (0, 0, 0, 0);
    for _ in 0..samples {
        let metrics = fixture.measure_frame();
        total.push(metrics.total_ms);
        update.push(metrics.app_update_ms);
        tessellation.push(metrics.tessellation_ms);
        stages.push([
            metrics.top_panel_ms,
            metrics.left_panel_ms,
            metrics.right_panel_ms,
            metrics.bottom_panels_ms,
            metrics.canvas_prepare_ms,
            metrics.wire_paint_ms,
            metrics.symbol_paint_ms,
            metrics.overlay_and_interaction_ms,
        ]);
        counts = (
            metrics.shape_count,
            metrics.primitive_count,
            metrics.visible_component_count,
            metrics.visible_wire_segment_count,
        );
    }
    for values in [&mut total, &mut update, &mut tessellation] {
        values.sort_by(f64::total_cmp);
    }
    let at = |values: &[f64], percentile: f64| {
        values[((values.len() - 1) as f64 * percentile).round() as usize]
    };
    println!(
        "{name:28} total={:.4}/{:.4}/{:.4} ms update_p50={:.4} ms tess_p50={:.4} ms shapes={} primitives={} visible_c={} visible_s={}",
        at(&total, 0.50),
        at(&total, 0.95),
        total[total.len() - 1],
        at(&update, 0.50),
        at(&tessellation, 0.50),
        counts.0,
        counts.1,
        counts.2,
        counts.3,
    );
    let mut stage_p50 = [0.0; 8];
    for index in 0..stage_p50.len() {
        let mut values = stages.iter().map(|row| row[index]).collect::<Vec<_>>();
        values.sort_by(f64::total_cmp);
        stage_p50[index] = at(&values, 0.50);
    }
    println!(
        "{name:28} stages_p50 top={:.3} left={:.3} right={:.3} bottom={:.3} prepare={:.3} wires={:.3} symbols={:.3} overlay/input={:.3} ms",
        stage_p50[0],
        stage_p50[1],
        stage_p50[2],
        stage_p50[3],
        stage_p50[4],
        stage_p50[5],
        stage_p50[6],
        stage_p50[7],
    );
}

fn main() {
    let samples = std::env::var("CLUSTER_PERF_SAMPLES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(9);
    let small = SchematicFixture::generate(SchematicSize::Small);
    let medium = SchematicFixture::generate(SchematicSize::Medium);
    let large = SchematicFixture::generate(SchematicSize::Large);
    for (label, fixture) in [
        ("100c_300s", &small),
        ("500c_2000s", &medium),
        ("1000c_5000s", &large),
    ] {
        measure(&format!("frame_{label}"), samples, || {
            fixture.synthetic_canvas_cpu_checksum()
        });
        measure(&format!("hit_test_{label}"), samples, || {
            fixture.hit_test_checksum() as usize
        });
        measure(&format!("connectivity_{label}"), samples, || {
            fixture.connectivity_checksum()
        });
        let prepared = fixture.prepare_analysis();
        measure(&format!("erc_rules_only_{label}"), samples, || {
            prepared.erc_evaluation_checksum()
        });
        if label == "1000c_5000s" {
            measure("erc_values_only_1000c_5000s", samples, || {
                prepared.erc_values_only_checksum()
            });
            measure("erc_topology_only_1000c_5000s", samples, || {
                prepared.erc_topology_only_checksum()
            });
        }
        measure(&format!("connectivity_plus_erc_{label}"), samples, || {
            fixture.connectivity_plus_erc_checksum()
        });
        measure(&format!("mna_solver_only_{label}"), samples, || {
            prepared.mna_solver_checksum()
        });
        measure(&format!("connectivity_plus_mna_{label}"), samples, || {
            fixture.connectivity_plus_mna_checksum()
        });
        let save = fixture.prepare_save();
        measure(&format!("save_json_{label}"), samples, || {
            save.serialization_len()
        });
    }

    let save = large.prepare_save();
    measure("save_atomic_write_1000c_5000s", samples, || {
        save.atomic_write_len()
    });
    let autosave = large.prepare_autosave();
    measure("autosave_ui_snapshot_1000c_5000s", samples, || {
        autosave.ui_thread_snapshot_len()
    });

    let connectivity = large.connectivity_stage_profile();
    println!(
        "connectivity stages (large): endpoint={:.3} index={:.3} pin_index={:.3} candidates={:.3} exact/normalize={:.3} junction={:.3} contacts={:.3} union={:.3} labels={:.3} nets={:.3} sort={:.3} diagnostics={:.3} total={:.3} ms",
        connectivity.endpoint_extraction_ms,
        connectivity.segment_spatial_index_build_ms,
        connectivity.pin_spatial_index_build_ms,
        connectivity.intersection_candidate_lookup_ms,
        connectivity.exact_intersection_checks_ms,
        connectivity.junction_application_ms,
        connectivity.endpoint_on_segment_contacts_ms,
        connectivity.union_find_ms,
        connectivity.label_merge_ms,
        connectivity.net_construction_ms,
        connectivity.deterministic_sorting_ms,
        connectivity.diagnostics_ms,
        connectivity.total_ms,
    );
    let mixed = RealisticFixture::generate(RealisticFixtureKind::MixedSimulation);
    let mna = mixed.mna_stage_profile();
    println!(
        "MNA stages (mixed): compile={:.3} nodes={:.3} alloc={:.3} stamp={:.3} nonlinear={:.3} solve={:.3} converge={:.3} results={:.3} wire_map={:.3} total={:.3} ms",
        mna.circuit_compilation_ms,
        mna.node_indexing_ms,
        mna.matrix_allocation_ms,
        mna.matrix_stamping_ms,
        mna.nonlinear_iteration_ms,
        mna.factorization_solve_ms,
        mna.convergence_test_ms,
        mna.result_mapping_ms,
        mna.wire_segment_current_mapping_ms,
        mna.total_ms,
    );

    let pcb = PcbFixture::generate(PcbSize::Medium);
    measure("pcb_hit_250_2000_150", samples, || pcb.hit_test_checksum());
    measure("pcb_local_drc", samples, || pcb.local_drc_checksum());
    measure("pcb_full_drc", samples, || pcb.full_drc_checksum());
    measure("pcb_ratsnest", samples, || pcb.ratsnest_checksum());

    let history = HistoryFixture::generate(SchematicSize::Large);
    measure("undo_redo_1000_5000", samples, || {
        history.undo_redo_checksum()
    });
    measure("save_snapshot_1000_5000", samples, || {
        history.snapshot_checksum()
    });
    for (name, scenario) in [
        ("cmd_move_one", CommandHistoryScenario::MoveOneComponent),
        (
            "cmd_move_100",
            CommandHistoryScenario::MoveHundredComponents,
        ),
        ("cmd_rotate", CommandHistoryScenario::RotateComponent),
        ("cmd_property", CommandHistoryScenario::EditProperty),
        ("cmd_add_wire", CommandHistoryScenario::AddWire),
        (
            "cmd_add_split_wire",
            CommandHistoryScenario::AddAndSplitWire,
        ),
        ("cmd_route_track", CommandHistoryScenario::RoutePcbTrack),
        ("cmd_add_via", CommandHistoryScenario::AddVia),
        ("cmd_move_footprint", CommandHistoryScenario::MoveFootprint),
    ] {
        let mut fixture = CommandHistoryFixture::generate(scenario);
        measure(name, samples, || fixture.command_undo_redo_checksum());
    }

    for (name, scenario) in [
        ("egui_empty", OffscreenFrameScenario::EmptyProject),
        ("egui_small", OffscreenFrameScenario::SmallSchematic),
        ("egui_medium", OffscreenFrameScenario::MediumSchematic),
        ("egui_large", OffscreenFrameScenario::LargeSchematic),
        ("egui_validation", OffscreenFrameScenario::ValidationPanel),
        ("egui_inspector", OffscreenFrameScenario::Inspector),
        (
            "egui_simulation",
            OffscreenFrameScenario::SimulationAnimation,
        ),
        ("egui_pcb", OffscreenFrameScenario::PcbWorkspace),
        (
            "egui_breadboard",
            OffscreenFrameScenario::BreadboardWorkspace,
        ),
    ] {
        measure_offscreen_frame(name, samples, scenario);
    }
}
