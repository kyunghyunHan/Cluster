use cluster::performance::{
    CommandHistoryFixture, CommandHistoryScenario, HistoryFixture, OffscreenFrameFixture,
    OffscreenFrameScenario, PcbFixture, PcbSize, RealisticFixture, RealisticFixtureKind,
    SchematicFixture, SchematicSize,
};
use criterion::{Criterion, criterion_group};
use std::hint::black_box;

fn schematic_benchmarks(c: &mut Criterion) {
    let small = SchematicFixture::generate(SchematicSize::Small);
    let medium = SchematicFixture::generate(SchematicSize::Medium);
    let large = SchematicFixture::generate(SchematicSize::Large);
    let prepared_small = small.prepare_analysis();
    let prepared_medium = medium.prepare_analysis();
    let prepared_large = large.prepare_analysis();
    let save_small = small.prepare_save();
    let save_large = large.prepare_save();
    let autosave_large = large.prepare_autosave();

    for (name, fixture) in [
        ("connectivity_small", &small),
        ("connectivity_medium", &medium),
        ("connectivity_large", &large),
    ] {
        c.bench_function(name, |b| {
            b.iter(|| black_box(fixture).connectivity_checksum())
        });
    }
    for (name, fixture) in [
        ("connectivity_plus_erc_small", &small),
        ("connectivity_plus_erc_medium", &medium),
        ("connectivity_plus_erc_large", &large),
    ] {
        c.bench_function(name, |b| {
            b.iter(|| black_box(fixture).connectivity_plus_erc_checksum())
        });
    }
    for (name, prepared) in [
        ("erc_rules_only_small", &prepared_small),
        ("erc_rules_only_medium", &prepared_medium),
        ("erc_rules_only_large", &prepared_large),
    ] {
        c.bench_function(name, |b| {
            b.iter(|| black_box(prepared).erc_evaluation_checksum())
        });
    }
    c.bench_function("erc_values_only_large", |b| {
        b.iter(|| black_box(&prepared_large).erc_values_only_checksum())
    });
    c.bench_function("erc_topology_only_large", |b| {
        b.iter(|| black_box(&prepared_large).erc_topology_only_checksum())
    });
    c.bench_function("component_hit_first", |b| {
        b.iter(|| black_box(&large).component_hit_checksum(0))
    });
    c.bench_function("component_hit_middle", |b| {
        b.iter(|| black_box(&large).component_hit_checksum(large_component_middle()))
    });
    c.bench_function("component_hit_last", |b| {
        b.iter(|| black_box(&large).component_hit_checksum(1_249))
    });
    c.bench_function("component_miss", |b| {
        b.iter(|| black_box(&large).component_miss_checksum())
    });
    c.bench_function("pin_hit_dense", |b| {
        b.iter(|| black_box(&large).pin_hit_dense_checksum())
    });
    c.bench_function("wire_hit_dense", |b| {
        b.iter(|| black_box(&large).wire_hit_dense_checksum())
    });
    c.bench_function("wire_miss", |b| {
        b.iter(|| black_box(&large).wire_miss_checksum())
    });
    for (name, viewport) in [
        (
            "viewport_empty",
            egui::Rect::from_min_max(
                egui::pos2(-10_000.0, -10_000.0),
                egui::pos2(-9_000.0, -9_000.0),
            ),
        ),
        (
            "viewport_sparse",
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(120.0, 120.0)),
        ),
        (
            "viewport_dense",
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(640.0, 480.0)),
        ),
        (
            "viewport_full",
            egui::Rect::from_min_max(egui::pos2(-500.0, -500.0), egui::pos2(5_000.0, 5_000.0)),
        ),
    ] {
        c.bench_function(name, |b| {
            b.iter(|| black_box(&large).indexed_viewport_checksum(viewport))
        });
    }
    c.bench_function("brute_force_hit_test", |b| {
        b.iter(|| black_box(&large).component_hit_checksum(large_component_middle()))
    });
    c.bench_function("indexed_hit_test", |b| {
        b.iter(|| black_box(&large).indexed_hit_test_checksum())
    });
    c.bench_function("connectivity_plus_mna_small", |b| {
        b.iter(|| black_box(&small).connectivity_plus_mna_checksum())
    });
    c.bench_function("connectivity_plus_mna_medium", |b| {
        b.iter(|| black_box(&medium).connectivity_plus_mna_checksum())
    });
    c.bench_function("mna_solver_only_small", |b| {
        b.iter(|| black_box(&prepared_small).mna_solver_checksum())
    });
    c.bench_function("mna_solver_only_medium", |b| {
        b.iter(|| black_box(&prepared_medium).mna_solver_checksum())
    });
    c.bench_function("flow_path_generation", |b| {
        b.iter(|| black_box(&large).flow_path_checksum())
    });
    c.bench_function("flow_animation_update", |b| {
        let mut phase = 0.0_f32;
        b.iter(|| {
            phase += 0.016;
            black_box(&large).flow_animation_checksum(black_box(phase))
        })
    });
    for (name, fixture) in [
        ("synthetic_canvas_cpu_small", &small),
        ("synthetic_canvas_cpu_medium", &medium),
        ("synthetic_canvas_cpu_large", &large),
    ] {
        c.bench_function(name, |b| {
            b.iter(|| black_box(fixture).synthetic_canvas_cpu_checksum())
        });
    }
    for (name, scenario) in [
        (
            "egui_frame_empty_project",
            OffscreenFrameScenario::EmptyProject,
        ),
        (
            "egui_frame_small_schematic",
            OffscreenFrameScenario::SmallSchematic,
        ),
        (
            "egui_frame_medium_schematic",
            OffscreenFrameScenario::MediumSchematic,
        ),
        (
            "egui_frame_large_schematic",
            OffscreenFrameScenario::LargeSchematic,
        ),
        (
            "egui_frame_validation_panel",
            OffscreenFrameScenario::ValidationPanel,
        ),
        ("egui_frame_inspector", OffscreenFrameScenario::Inspector),
        (
            "egui_frame_simulation_animation",
            OffscreenFrameScenario::SimulationAnimation,
        ),
        (
            "egui_frame_pcb_workspace",
            OffscreenFrameScenario::PcbWorkspace,
        ),
        (
            "egui_frame_breadboard_workspace",
            OffscreenFrameScenario::BreadboardWorkspace,
        ),
    ] {
        let mut frame = OffscreenFrameFixture::generate(scenario);
        c.bench_function(name, |b| b.iter(|| black_box(frame.checksum())));
    }
    c.bench_function("save_small", |b| {
        b.iter(|| black_box(&save_small).serialization_len())
    });
    c.bench_function("save_large", |b| {
        b.iter(|| black_box(&save_large).serialization_len())
    });
    c.bench_function("save_atomic_write_large", |b| {
        b.iter(|| black_box(&save_large).atomic_write_len())
    });
    c.bench_function("autosave_ui_snapshot_large", |b| {
        b.iter(|| black_box(&autosave_large).ui_thread_snapshot_len())
    });
}

const fn large_component_middle() -> usize {
    625
}

fn pcb_benchmarks(c: &mut Criterion) {
    let small = PcbFixture::generate(PcbSize::Small);
    let medium = PcbFixture::generate(PcbSize::Medium);
    c.bench_function("pcb_hit_test", |b| {
        b.iter(|| black_box(&medium).hit_test_checksum())
    });
    for (name, index) in [
        ("pcb_component_hit_first", 0),
        ("pcb_component_hit_middle", 125),
        ("pcb_component_hit_last", 249),
    ] {
        c.bench_function(name, |b| {
            b.iter(|| black_box(&medium).footprint_hit_checksum(index, true))
        });
    }
    c.bench_function("pcb_component_miss", |b| {
        b.iter(|| black_box(&medium).footprint_miss_checksum(true))
    });
    c.bench_function("pcb_pin_hit_dense", |b| {
        b.iter(|| black_box(&medium).pad_hit_dense_checksum())
    });
    c.bench_function("pcb_wire_hit_dense", |b| {
        b.iter(|| black_box(&medium).track_hit_dense_checksum(true))
    });
    c.bench_function("pcb_wire_miss", |b| {
        b.iter(|| black_box(&medium).track_miss_checksum(true))
    });
    for (name, variant) in [
        ("pcb_viewport_empty", 0),
        ("pcb_viewport_sparse", 1),
        ("pcb_viewport_dense", 2),
        ("pcb_viewport_full", 3),
    ] {
        c.bench_function(name, |b| {
            b.iter(|| black_box(&medium).viewport_checksum(variant))
        });
    }
    c.bench_function("pcb_brute_force_hit_test", |b| {
        b.iter(|| black_box(&medium).track_hit_dense_checksum(false))
    });
    c.bench_function("pcb_indexed_hit_test", |b| {
        b.iter(|| black_box(&medium).track_hit_dense_checksum(true))
    });
    c.bench_function("pcb_ratsnest", |b| {
        b.iter(|| black_box(&medium).ratsnest_checksum())
    });
    c.bench_function("pcb_local_drc", |b| {
        b.iter(|| black_box(&small).local_drc_checksum())
    });
    c.bench_function("pcb_full_drc", |b| {
        b.iter(|| black_box(&medium).full_drc_checksum())
    });
}

fn history_benchmarks(c: &mut Criterion) {
    let fixture = HistoryFixture::generate(SchematicSize::Large);
    c.bench_function("history_push_snapshot", |b| {
        b.iter(|| black_box(&fixture).snapshot_checksum())
    });
    c.bench_function("history_push_delta", |b| {
        b.iter(|| black_box(&fixture).delta_checksum())
    });
    c.bench_function("undo_redo_large", |b| {
        b.iter(|| black_box(&fixture).undo_redo_checksum())
    });
    for (name, scenario) in [
        (
            "command_move_one_undo_redo",
            CommandHistoryScenario::MoveOneComponent,
        ),
        (
            "command_move_100_undo_redo",
            CommandHistoryScenario::MoveHundredComponents,
        ),
        (
            "command_rotate_undo_redo",
            CommandHistoryScenario::RotateComponent,
        ),
        (
            "command_property_undo_redo",
            CommandHistoryScenario::EditProperty,
        ),
        (
            "command_add_wire_undo_redo",
            CommandHistoryScenario::AddWire,
        ),
        (
            "command_add_split_wire_undo_redo",
            CommandHistoryScenario::AddAndSplitWire,
        ),
        (
            "command_route_track_undo_redo",
            CommandHistoryScenario::RoutePcbTrack,
        ),
        ("command_add_via_undo_redo", CommandHistoryScenario::AddVia),
        (
            "command_move_footprint_undo_redo",
            CommandHistoryScenario::MoveFootprint,
        ),
    ] {
        let mut command = CommandHistoryFixture::generate(scenario);
        c.bench_function(name, |b| {
            b.iter(|| black_box(command.command_undo_redo_checksum()))
        });
    }
}

fn realistic_benchmarks(c: &mut Criterion) {
    for (name, kind) in [
        ("dense_esp32_i2c", RealisticFixtureKind::DenseEsp32I2c),
        ("branch_heavy_power", RealisticFixtureKind::BranchHeavyPower),
        ("multi_page_10", RealisticFixtureKind::MultiPage),
        ("mixed_simulation", RealisticFixtureKind::MixedSimulation),
        ("dense_crossing", RealisticFixtureKind::DenseCrossing),
    ] {
        let fixture = RealisticFixture::generate(kind);
        c.bench_function(&format!("realistic_connectivity_{name}"), |b| {
            b.iter(|| black_box(&fixture).connectivity_checksum())
        });
        c.bench_function(&format!("realistic_erc_{name}"), |b| {
            b.iter(|| black_box(&fixture).erc_checksum())
        });
        c.bench_function(&format!("realistic_mna_{name}"), |b| {
            b.iter(|| black_box(&fixture).mna_checksum())
        });
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(20);
    targets = schematic_benchmarks, pcb_benchmarks, history_benchmarks, realistic_benchmarks
);

// `cargo test --all-targets` also executes non-harness benches. Skip the
// expensive Criterion driver in that mode; `cargo bench --bench performance`
// remains the explicit measurement command.
fn main() {
    if cfg!(debug_assertions) || std::env::args().any(|arg| arg == "--test") {
        return;
    }
    benches();
}
