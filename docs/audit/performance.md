# Measured performance and remaining hot paths

Measured locally on 2026-10-12. Exact baseline: `a998077419fd59d91141ce2b9b1fe48ae0d2d7aa`. Logs: [performance-before.log](performance-before.log) and [performance-after.log](performance-after.log). Both commands exited 0.

## Method and boundaries

One baseline and one final release run, **nine samples per measurement**, using `CLUSTER_PERF_SAMPLES=9 cargo run --release --example performance_probe`. Baseline came from an isolated `git archive` of the exact commit (only cfg(test) physical-connectivity regressions were appended afterward). The same Cargo target directory was used and Cargo recompiled changed source. This is not an interleaved statistical experiment; small differences can be machine/scheduling noise. With nine samples the reported p95 equals the maximum sample.

`examples/performance_probe.rs::measure` reports p50/p95/max and measured peak heap; the allocator instruments allocation volume, not OS RSS/GPU memory. `performance.rs::OffscreenFrameFixture` drives production `CircuitApp::update_ui`, paint generation and tessellation at 1440×900. It does not measure GPU presentation or desktop input latency. The existing 320px narrow empty/nonempty breadboard test also passed; no manual desktop visual inspection was performed.

Synthetic fixtures contain 100/500/1000 components and 300/2000/5000 wire segments. Large synthetic MNA has no ground and measures preparation/failure handling, not a successfully solved 1000-component circuit. Separate mixed solvable fixture checks solver/parameter paths. PCB fixture has 250 footprints, 2000 tracks and 150 vias. Checksums are diagnostic sums, not assertions that two algorithms produced physically identical results.

## Actual before / after results

Values in milliseconds unless stated otherwise. Each pair is p50 / p95.

| Path | Baseline | Final |
|---|---:|---:|
| Canonical connectivity, 1000/5000 | 48.5021 / 49.1315 | 50.6122 / 153.6813 |
| ERC rules only, 1000/5000 | 4.2601 / 4.4870 | 4.4568 / 5.1450 |
| Solvable mixed MNA | 2.0870 / 2.2555 | 2.0758 / 2.1405 |
| Mixed parameter update | 2.1298 / 2.2307 | 2.1093 / 2.1901 |
| Autosave UI snapshot, 1000/5000 | 0.8310 / 0.9940 | 0.8400 / 0.9119 |
| PCB local DRC | 0.0416 / 0.0472 | 0.0413 / 0.0461 |
| **PCB full DRC** | 8.5767 / 16.7440 | 600.8250 / 615.9634 |
| PCB ratsnest | 0.5345 / 0.7058 | 2.0678 / 2.8431 |
| Undo/redo, 1000/5000 | 11.6996 / 12.7953 | 16.3657 / 73.0440 |
| Offscreen egui empty total | 0.3368 / 0.4666 | 0.3377 / 0.4748 |
| Offscreen egui large total | 2.2412 / 2.9958 | 2.3101 / 2.6380 |
| Offscreen egui PCB total | 0.4273 / 0.5463 | 0.4211 / 0.5351 |

Mixed MNA peak heap: 154,728→136,800 bytes. Large connectivity peak heap: 5,993,340 bytes both runs. Ratsnest peak heap: 18,328→107,070 bytes, reflecting explicit pad identities. These observations do not establish a broad speed improvement. Full-DRC p50 regressed **about 70.1×**, because substantially more real copper/pad/layer geometry is now checked. It must not be hidden by the comparatively fast local DRC and frame numbers.

## Architectural observations and correction costs

- `src/engine/worker.rs::execute` continues to reuse `Arc<CanonicalConnectivity>` for value edits. `src/model/net.rs::with_component_properties` clones a property projection once per analysis/property revision; `current_netlist` caches it. Structural ERC now includes electrical revision, so value-only worker edits do more ERC work than the old unsound cache. The rules-only/value-only probe does **not** measure the entire worker cache-refresh overhead.
- `src/app/actions.rs::rebuild_pcb_ratsnest_counts` and analysis state cache computed pad islands for rendering. `ui/pcb_workspace.rs` receives cached edges rather than rebuilding all copper connectivity every paint. Dock projections can still clone edge vectors.
- `src/pcb/drc.rs::run_drc_with_nets` scans track/pad, pad/pad, via/pad, track/via and via/via pairs. `src/pcb/copper.rs::pad_islands` also uses per-net pairwise overlap checks and pad-instance lookups. These are the primary new optimization targets: conservative spatial broad-phase, reused transformed pad shapes, cancellation and measured scaling tests.
- Local editing follows `refresh_local_pcb_analysis` with full worker validation. Fresh fabrication DRC is synchronous, so a large export can pause the UI for the measured full-check duration. Full DRC currently has no fine-grained cancellation; a superseded job can delay later work even though stale results are discarded.
- `src/editor/delta.rs::approximate_board_metadata_cost` includes new nested net bindings; other nested library/zone allocations remain partially estimated. Command benchmark allocation is not a hard 16 MiB history guarantee.

This checkpoint chooses correct copper checks over the previous incomplete result. It does not meet a general large-board DRC responsiveness target. A follow-up should preserve all current regressions while adding spatial broad-phase and cancellation, then rerun multiple interleaved trials. No new network dependencies or rendering framework changes were introduced.
