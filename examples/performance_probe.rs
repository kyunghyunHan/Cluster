use cluster::performance::{HistoryFixture, PcbFixture, PcbSize, SchematicFixture, SchematicSize};
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
            fixture.frame_checksum()
        });
        measure(&format!("hit_test_{label}"), samples, || {
            fixture.hit_test_checksum() as usize
        });
        measure(&format!("connectivity_{label}"), samples, || {
            fixture.connectivity_checksum()
        });
        measure(&format!("erc_{label}"), samples, || fixture.erc_checksum());
        measure(&format!("mna_{label}"), samples, || fixture.mna_checksum());
        measure(&format!("save_{label}"), samples, || {
            fixture.serialization_len()
        });
    }

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
}
