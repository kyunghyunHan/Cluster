# Commercial completion audit — 2026-07-23

This is the evidence record for the commercial-completion plan. It describes
what is reachable and tested at the audited commit; a type or scaffold alone is
not counted as complete.

## Baseline identity and protocol

- Upstream baseline: `84b4f237e64250fe9971da13f38ea98490763baf`
  (`fix: keep schematic indexes and pcb transforms consistent`). Local `main`
  and `origin/main` matched before changes.
- Platform: macOS Darwin 25.3.0, Apple M1 (4 performance + 4 efficiency
  cores), 16 GiB RAM, arm64.
- Toolchain: `rustc 1.97.0 (2d8144b78 2026-07-07)`, Cargo 1.97.0.
- Power/background state: AC was connected, although the battery tool reported
  78% and discharging. Load averages were approximately 1.98/2.34/3.46;
  macOS thermal state was unavailable. This is a development-machine baseline,
  not a thermally isolated laboratory result.
- Command: `CLUSTER_PERF_SAMPLES=21 cargo run --release --example performance_probe`.
- Three independent invocations used the same release binary with no source
  change between runs. Each table cell is the median of the three run-level
  p50, p95, max, or peak-incremental-heap values. Individual samples were not
  pooled. Raw run outputs were retained as `/tmp/cluster-perf-final{1,2,3}.txt`
  during the audit.

ERC and MNA now have explicit timing boundaries. `erc_rules_only_*` reuses a
prepared canonical connectivity result; `mna_solver_only_*` does the same.
The completion slice additionally reports `erc_values_only_*` and
`erc_topology_only_*`, so dependency-filtered work is measurable rather than
inferred from the full rule set.
`connectivity_plus_erc_*` and `connectivity_plus_mna_*` rebuild connectivity.
Separately measured rows must not be arithmetically added because they run in
different sample loops and are affected differently by caches and system load.

## Measured baseline

Times are milliseconds and heap values are bytes.

| Synthetic workload | p50 | p95 | max | Peak heap |
| --- | ---: | ---: | ---: | ---: |
| Connectivity, 100 / 300 | 3.3956 | 5.2952 | 6.9823 | 415,886 |
| ERC rules only, 100 / 300 | 0.6622 | 0.6897 | 0.8260 | 85,164 |
| Connectivity + ERC, 100 / 300 | 3.1589 | 3.2680 | 3.2941 | 415,886 |
| MNA solver only, 100 / 300 | 0.4803 | 0.5063 | 0.5317 | 46,449 |
| Connectivity + MNA, 100 / 300 | 2.9660 | 3.0994 | 3.1858 | 415,886 |
| Connectivity, 500 / 2,000 | 18.2896 | 18.7317 | 18.9142 | 2,690,979 |
| ERC rules only, 500 / 2,000 | 12.2497 | 12.3908 | 12.4264 | 591,284 |
| Connectivity + ERC, 500 / 2,000 | 30.7302 | 31.6380 | 32.9358 | 2,690,979 |
| MNA solver only, 500 / 2,000 | 16.5850 | 16.8789 | 16.9400 | 366,320 |
| Connectivity + MNA, 500 / 2,000 | 34.8763 | 35.5773 | 35.8897 | 2,690,979 |
| Connectivity, 1,000 / 5,000 | 48.2897 | 48.7855 | 49.7068 | 5,993,436 |
| ERC rules only, 1,000 / 5,000 | 92.2542 | 97.8833 | 115.8097 | 1,181,668 |
| Connectivity + ERC, 1,000 / 5,000 | 138.6502 | 143.2533 | 148.2399 | 5,993,436 |
| MNA solver only, 1,000 / 5,000 | 95.1147 | 96.3391 | 97.2700 | 723,088 |
| Connectivity + MNA, 1,000 / 5,000 | 144.0003 | 146.6168 | 150.3731 | 5,993,436 |
| JSON serialization, 1,000 / 5,000 | 3.7347 | 3.8537 | 3.9012 | 3,143,839 |
| Atomic write + sync/backup, 1,000 / 5,000 | 10.9559 | 12.8948 | 13.0268 | 168 |
| Autosave UI-thread DTO, 1,000 / 5,000 | 1.1102 | 1.3535 | 1.3660 | 1,381,652 |

| Command/PCB workload | p50 | p95 | max | Peak heap |
| --- | ---: | ---: | ---: | ---: |
| Move one component + undo/redo | 0.4688 | 0.4889 | 4.2949 | 2,699,399 |
| Move 100 components + undo/redo | 1.2641 | 1.3507 | 5.1594 | 2,700,515 |
| Rotate component + undo/redo | 0.4683 | 0.4994 | 4.2629 | 2,699,355 |
| Property edit + undo/redo | 0.0615 | 0.0700 | 3.9992 | 2,699,359 |
| Add/split wire + undo/redo | 10.3123 | 10.6827 | 13.8198 | 9,828,754 |
| PCB route + undo/redo | 0.3000 | 0.3120 | 4.2325 | 2,699,355 |
| PCB via + undo/redo | 0.2934 | 0.3490 | 4.2670 | 2,699,355 |
| PCB footprint move + undo/redo | 0.3051 | 0.3406 | 4.2523 | 2,699,355 |
| PCB indexed hit, 250 / 2,000 / 150 | <0.0001 | 0.0001 | 0.0001 | 168 |
| PCB local DRC | 0.0424 | 0.0556 | 0.0582 | 4,176 |
| PCB full DRC | 7.0801 | 7.2097 | 7.2743 | 681,102 |
| PCB ratsnest | 0.4767 | 0.5073 | 0.5213 | 18,424 |
| Large undo + redo | 11.2176 | 11.4238 | 11.5387 | 5,886,294 |
| Large save snapshot clone | 0.3588 | 0.3664 | 0.3741 | 1,382,275 |

| Production offscreen state | Total p50 / p95 / max | Update p50 | Tessellate p50 |
| --- | ---: | ---: | ---: |
| Empty | 0.3349 / 0.3580 / 0.4546 | 0.2437 | 0.0854 |
| Small schematic | 0.8493 / 1.0112 / 1.0554 | 0.6360 | 0.1977 |
| Medium schematic | 1.7432 / 1.9489 / 1.9740 | 1.1772 | 0.4892 |
| Large schematic | 2.1988 / 2.3862 / 2.5122 | 1.4430 | 0.7210 |
| Validation open | 1.3725 / 1.5357 / 2.4020 | 1.1822 | 0.1850 |
| Inspector selection | 0.8872 / 1.0727 / 1.3085 | 0.6687 | 0.2098 |
| Simulation animation | 0.8589 / 1.0347 / 1.0892 | 0.6492 | 0.2024 |
| PCB workspace | 0.4125 / 0.5017 / 0.5318 | 0.3311 | 0.0773 |
| Breadboard workspace | 0.8800 / 1.1229 / 1.1472 | 0.6562 | 0.2109 |

The measured bottlenecks are not frame painting. At large scale they are MNA
solver-only p95 (96.34 ms), ERC rule evaluation p95 (97.88 ms), canonical
connectivity p95 (48.79 ms), and add/split-wire command p95 (10.68 ms with a
9.83 MiB peak allocation). These establish the optimization order for M2–M4.
Atomic write is intentionally slower than the UI budget because it includes
file and directory sync plus backup rotation and executes on the worker. The UI
thread autosave DTO work is isolated and remains below its 3 ms target.

## Completion-slice results

After the baseline, the standard schematic command and drag paths were changed
from whole-document snapshots to scoped entity deltas. The ERC rule evaluator
was also changed from repeated `nets × all pins` scans to one pin-to-net index
per affected rule. The following values are medians of three new independent
21-sample release probes (`/tmp/cluster-perf-after{1,2,3}.txt`). Times are
milliseconds.

| Large analysis workload | Before p50 / p95 | After p50 / p95 / max | Peak heap after |
| --- | ---: | ---: | ---: |
| Connectivity | 48.2897 / 48.7855 | 49.1252 / 49.8498 / 49.9859 | 5,993,436 |
| ERC rules only | 92.2542 / 97.8833 | 4.2804 / 4.3620 / 4.3631 | 1,181,668 |
| ERC values only | not isolated | 0.3819 / 0.4122 / 0.4209 | 1,790 |
| ERC topology only | not isolated | 3.9153 / 4.0192 / 4.0331 | 1,181,668 |
| Connectivity + ERC | 138.6502 / 143.2533 | 53.3970 / 54.0169 / 54.4929 | 5,993,436 |
| MNA solver only | 95.1147 / 96.3391 | 96.0559 / 97.8217 / 98.9065 | 723,088 |
| Connectivity + MNA | 144.0003 / 146.6168 | 145.4240 / 146.6680 / 147.0070 | 5,993,436 |

ERC output checksums remained identical (`6002` full, `6001` topology), and
the beginner ERC test suite remained green. The 95% rules-only reduction is
therefore an execution-plan change, not a reduced rule set. MNA did not improve
because compiled-topology/parameter reuse is not implemented yet.

| Command + undo/redo/undo | Before p50 / p95 | After p50 / p95 / max | Heap before → after |
| --- | ---: | ---: | ---: |
| Move one | 0.4688 / 0.4889 | 0.4695 / 0.5076 / 4.4700 | 2,699,399 → 2,699,399 |
| Move 100 | 1.2641 / 1.3507 | 1.2507 / 1.3881 / 5.2919 | 2,700,515 → 2,700,515 |
| Add wire | not isolated | 0.7337 / 0.7729 / 4.3664 | 9,828,754 combined → 3,456,885 |
| Add and split wire | 10.3123 / 10.6827 | 0.8222 / 0.8900 / 4.6067 | 9,828,754 → 3,457,453 |

The final large offscreen production frame was
`2.1812/2.5437/2.8828 ms` p50/p95/max. Atomic write p95 was 13.2098 ms and ran
on the worker; UI-thread autosave DTO p95 was 1.3058 ms.

### Acceptance check

| Criterion | Result | Status |
| --- | ---: | --- |
| Actual large egui frame p95 < 12 ms | 2.3862 ms | Pass |
| Move one component p95 < 2 ms | 0.4889 ms | Pass |
| Move 100 components p95 < 5 ms | 1.3507 ms | Pass |
| Wire add < 3 ms and split < 5 ms | add 0.7729; split 0.8900 ms | Pass |
| Dense pin query p95 < 0.1 ms | Criterion estimate about 0.0001 ms | Pass |
| Full schematic viewport p95 < 1 ms | Criterion estimate about 0.81 ms | Pass |
| Medium full connectivity p95 < 20 ms | 18.7317 ms | Pass |
| Large full connectivity p95 < 50 ms | 48.7855 ms | Pass |
| Value-only ERC p95 < 3 ms | 0.4122 ms | Pass |
| Large full ERC rules-only p95 < 75 ms | 4.3620 ms | Pass |
| Large aggregate analysis ideally < 100 ms | ERC 54.0169; MNA 146.6680 ms | ERC pass; MNA fail |
| PCB local DRC p95 < 5 ms | 0.0556 ms | Pass |
| PCB full DRC p95 < 20 ms | 7.2097 ms | Pass |
| Autosave UI-thread work p95 < 3 ms | 1.3535 ms | Pass |

Pan/zoom/select input, single local connectivity, local-net topology ERC, and
MNA parameter reuse still need isolated measurements.
No inferred pass is assigned to them.

## Repository-wide pattern audit

The audited Rust surface is 47,321 lines in `src`, `benches`, and `examples`.
Raw matches are triage inputs, not automatic defects: TODO/FIXME/HACK/XXX 1;
`todo!`/`unimplemented!` 0; `panic!` 0; `unwrap` 246; `expect` 34; `clone` 226;
`DocumentDelta::between` 5; direct `.snapshot()` 8; `rebuild` 46; `retain` 23;
`.iter().find` 104; `.position` 6; thread-builder creation 2; repaint requests 7.

Classification:

- Most `unwrap`/`expect` matches are test assertions or solver-internal checked
  invariants. User-controlled parse/file paths use `Result`; these still need a
  production-only query before the release panic audit is signed off.
- Standard editor commands and schematic drag transactions now use scoped
  entity deltas. Snapshot/delta fallback remains only in generic compatibility
  history used by page/load/annotation and compound auto-fix paths, plus tests
  and the explicit snapshot benchmark.
- Full rebuilds are valid at load/import/document replacement boundaries. Full
  schematic rebuilds during generic command/history fallback and full board
  rebuilds during broad delta/ECO application require separate removal or an
  explicit complexity justification.
- The PCB immutable lookup and DRC-candidate linear fallbacks were silent
  behavior changes on stale indexes. They were removed in this slice. Mutable
  board APIs still perform an explicit consistency check and rebuild.
- A single bounded worker serializes schematic analysis, full DRC, and autosave.
  Thread startup/disconnect failures are surfaced to UI status. Queue saturation
  retains the latest autosave in a cancellation-aware pending slot and retries
  when capacity becomes available.

## Reachable call paths

### Editor command

`CircuitApp::execute_editor_command` → revision-gated derived indexes → typed
delta capture (or remaining snapshot fallback) → `EditorCommand::apply` with a
restricted `CommandContext` → `DocumentDelta` history entry →
`dispatch_changes` → revisions/cache/dirty/autosave/repaint → optional local PCB
analysis. Continuous drag uses `execute_continuous_editor_command`, previews
geometry without advancing heavy-analysis revisions, and commits one history
transaction on release.

### Analysis

`dispatch_changes` marks dependency-specific revisions → UI submits a bounded,
revision-tagged `AnalysisJob` → worker builds canonical connectivity → MNA and
simulation → topology/value/dynamic ERC → UI polls results and discards stale
revision keys. Full PCB DRC and autosave share the same bounded worker queue.

### Save/load/export

Save materializes current pages → `SavedCircuit` schema v4 → pretty JSON →
same-directory temporary file, sync, rename, and three backup generations.
Autosave serializes/writes in the worker. Load parses/migrates/repairs → restores
the document → rebuilds derived indexes → runs structured document/derived/PCB
invariant validation → reports repair and invariant counts in status. SVG,
PNG, SPICE, netlist, Arduino, BOM, Gerber, Excellon, and CPL are reachable from
toolbar or PCB dock actions; fabrication export is DRC-gated.

### PCB

Schematic canonical netlist → CAD projection → ECO report/application → board
entity/spatial indexes → interactive PCB commands → local DRC during edits or
full DRC worker → DRC-gated Gerber/Excellon/BOM/CPL export. Pad placement,
spatial lookup, Gerber, drill, and CPL share the footprint transform.

## Cache dependency audit

| Cache/result | Key dependencies | Invalidated by | Must not be invalidated by |
| --- | --- | --- | --- |
| Canonical connectivity/netlist | schematic connectivity revision | component/wire/junction/label topology | selection, pan/zoom, PCB-only edits, value-only edits |
| MNA/simulation | connectivity + topology + parameter/electrical revisions | topology/model/value/switch changes | visual-only and PCB-only edits |
| ERC topology | topology revision | topology and annotations | value-only and visual edits |
| ERC values/dynamic | value/simulation revisions | values, model state, simulation result | pan/zoom/selection |
| Schematic entity/attachment/spatial indexes | schematic geometry revision | component/wire geometry, annotations as applicable | electrical values, PCB edits |
| PCB entity/spatial index | board topology/geometry | footprints/tracks/vias/outline | schematic visual edits |
| PCB DRC | board topology/geometry/rules + CAD nets | copper, footprint, outline, rules, ECO | schematic selection/pan |
| Flow paint cache | connectivity/simulation + geometry | current result or visible path geometry | unrelated panel state |

## Test and benchmark inventory

There are 240 passing tests after this slice. The largest groups are
UI/app integration (76), MNA (30), canonical netlist (26), beginner ERC (22),
current flow/custom parts (10 each), PCB board (8), and command dispatch (8).
Criterion defines 42 benchmark registrations plus parameterized rows. The
release probe covers synthetic and production offscreen frames, connectivity,
isolated/aggregate ERC and MNA, PCB hit/DRC/ratsnest, history, real commands,
serialization, stage profiles, and peak incremental heap.

New correctness coverage includes structured malformed-document and malformed-
board validator tests, plus a deterministic 240-operation sequence spanning
place/move/rotate/delete/wire/undo/redo. After every operation it compares the
entity index with linear source-of-truth order and validates attachment and
spatial indexes.

## Milestone acceptance status

- M0: accepted for this platform. Repository/call-path/cache/test audit,
  three-run protocol, separated ERC/MNA timing, command/PCB/frame/heap,
  serialization, atomic-write, and autosave UI-thread baselines are complete.
- M1: partially complete. Structured document and board validators, load-boundary
  diagnostics, silent PCB fallback removal, selection/history/drag debug
  invariants, worker startup/queue failure handling, malformed-state tests, and
  the deterministic command sequence are implemented. Broader randomized
  geometry/connectivity and PCB/ECO differential tests remain.
- M2: partially complete. Standard commands and component/wire drag use scoped
  deltas, and isolated wire add/split meet their latency targets. Generic
  compatibility history for page/load/annotation/compound repair still uses a
  full snapshot and must be converted before M2 acceptance.
- M3: partially complete. Prepared rules-only/value-only/topology-only timing,
  topology ERC caching, and indexed pin grouping are implemented. Full canonical
  connectivity remains the reference path; local connectivity rebuild, local-net
  issue-key merge, and differential incremental tests remain.
- M4–M9: not accepted. Existing reachable functionality is recorded in README
  and prior audits, but the completion-plan criteria have not been re-proven.
  In particular snapshot fallbacks, incremental connectivity/ERC/MNA reuse,
  manufacturing-complete zones/DRC/export goldens, editable breadboard lessons,
  project locking/recovery UX, accessibility evidence, installers/signing, and
  release-candidate evidence remain open.

This status is intentionally conservative: no milestone is marked complete
because a scaffold exists or because a neighboring feature works.

## Validation executed for this slice

- `cargo fmt --all -- --check`: pass.
- `cargo check --all-targets --all-features`: pass.
- `cargo clippy --all-targets --all-features -- -D warnings`: pass.
- `cargo test --all-targets --all-features`: 240 passed, 0 failed/ignored.
- `cargo build --release --all-features`: pass.
- `cargo bench --bench performance`: pass; all Criterion groups completed,
  including the new ERC dependency and isolated add-wire rows. The large ERC
  estimate improved by about 96%. A 2.9% (`3.88 ms` absolute) regression was
  reported for the small realistic mixed-simulation ERC fixture, while dense
  crossing was within Criterion's noise threshold; this is retained as a
  follow-up for sharing one prepared rule context across rules.
- Three independent 21-sample release probes plus one post-clippy working-tree
  probe: pass. The post-clippy probe reported large ERC p95 `4.3109 ms`, add
  `0.7638 ms`, split `0.8562 ms`, and large egui `2.6944 ms`.
- `cargo audit` and `cargo deny check` were attempted but their Cargo
  subcommands are not installed. Linux and Windows were not executable in this
  macOS workspace. No release tag, signed installer, or distributable artifact
  was made.
