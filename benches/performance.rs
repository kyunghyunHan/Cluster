use cluster::performance::{HistoryFixture, PcbFixture, PcbSize, SchematicFixture, SchematicSize};
use criterion::{Criterion, criterion_group};
use std::hint::black_box;

fn schematic_benchmarks(c: &mut Criterion) {
    let small = SchematicFixture::generate(SchematicSize::Small);
    let medium = SchematicFixture::generate(SchematicSize::Medium);
    let large = SchematicFixture::generate(SchematicSize::Large);

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
        ("erc_small", &small),
        ("erc_medium", &medium),
        ("erc_large", &large),
    ] {
        c.bench_function(name, |b| b.iter(|| black_box(fixture).erc_checksum()));
    }
    c.bench_function("schematic_hit_test_small", |b| {
        b.iter(|| black_box(&small).hit_test_checksum())
    });
    c.bench_function("schematic_hit_test_large", |b| {
        b.iter(|| black_box(&large).hit_test_checksum())
    });
    c.bench_function("viewport_query_small", |b| {
        b.iter(|| black_box(&small).viewport_query_checksum())
    });
    c.bench_function("viewport_query_large", |b| {
        b.iter(|| black_box(&large).viewport_query_checksum())
    });
    c.bench_function("mna_small", |b| b.iter(|| black_box(&small).mna_checksum()));
    c.bench_function("mna_medium", |b| {
        b.iter(|| black_box(&medium).mna_checksum())
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
        ("frame_small", &small),
        ("frame_medium", &medium),
        ("frame_large", &large),
    ] {
        c.bench_function(name, |b| b.iter(|| black_box(fixture).frame_checksum()));
    }
    c.bench_function("save_small", |b| {
        b.iter(|| black_box(&small).serialization_len())
    });
    c.bench_function("save_large", |b| {
        b.iter(|| black_box(&large).serialization_len())
    });
}

fn pcb_benchmarks(c: &mut Criterion) {
    let small = PcbFixture::generate(PcbSize::Small);
    let medium = PcbFixture::generate(PcbSize::Medium);
    c.bench_function("pcb_hit_test", |b| {
        b.iter(|| black_box(&medium).hit_test_checksum())
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
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(20);
    targets = schematic_benchmarks, pcb_benchmarks, history_benchmarks
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
