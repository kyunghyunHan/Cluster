# Product-grade upgrade audit — 2026-07-20

This report distinguishes implemented behavior from requested future work. It
does not treat a type or an unreachable module as a finished feature.

## 1. Baseline

- Commit: `cd30c3a1ee7f59540e04a7e5aab13d335bc42e97`
  (`fix: preserve connectivity annotations across documents`).
- Branch: `main`, nine commits ahead of `origin/main`; working tree clean.
- `cargo fmt --check`: pass.
- `cargo check --all-targets`: pass, zero warnings.
- `cargo clippy --all-targets -- -D warnings`: pass.
- `cargo test --all-targets`: 211 passed, zero failed/ignored.
- `cargo build --release`: pass.
- Rust source/test/workflow total: 36,589 lines.
- Largest files: `ui/app/mod.rs` 3,452; `ui/app/symbols.rs` 3,118;
  `engine/validation.rs` 2,767; `app/actions.rs` 2,260;
  `ui/app/energize.rs` 1,987; `ui/app/tests.rs` 1,980.

The repository-wide audit found no production `unwrap()`/`expect()` on a
user-controlled parsing or file path. The one production `expect()` is a
checked current-flow invariant. Most occurrences are test assertions.

## 2. Architecture changes in this slice

- `ChangeSet` now names persistence, schematic geometry/connectivity,
  electrical values, simulation topology/parameters, PCB sync/geometry/rules,
  and visual-only domains. The central dispatcher derives cache invalidation,
  autosave eligibility, stale simulation, DRC invalidation, and repaint.
- Added a dedicated PCB workspace separate from the bottom-dock preview.
- Added an explicit ECO report/application model. Existing physical placement,
  rotation, and board side survive updates. Removed symbols become visible
  orphan footprints by default.
- Ratsnest generation now computes remaining copper-connected footprint
  islands and creates a minimal-distance spanning set between islands.
- Added a backend-neutral simulation contract with internal MNA and ngspice
  implementations.
- ngspice uses a per-run directory, configurable executable, timeout,
  cancellation token, captured stderr, and document revision. Fixed shared
  temporary filenames were removed.

Final quality gates:

- `cargo fmt --check`: pass.
- `cargo check --all-targets`: pass, zero warnings.
- `cargo clippy --all-targets -- -D warnings`: pass.
- `cargo test --all-targets`: 217 passed, zero failed/ignored.
- `cargo build --release`: pass.

## 3. New PCB commands

The command boundary now supports single/group footprint move and rotation,
front/back flip, complete multi-segment route plus vias, track add/delete/edit,
via add/delete, board outline replacement, net-class change, and ECO apply.
A route containing several segments and vias is one history item.

## 4. History and direct mutation audit

History entries are entity deltas in a `VecDeque` with a 16 MiB/512-entry
budget. No `Vec::remove(0)` eviction remains. A full `CircuitSnapshot` is still
created transiently before/after generic commands and compatibility
transactions to calculate a delta; it is not stored in history. Board changes
inside that generic delta are still retained as a before/after `Board` pair.
These are the remaining snapshot fallbacks.

Remaining production mutation paths outside narrow command internals:

- page materialization/switch/add/remove and load recovery in `app/actions.rs`;
- demo/lesson fixture construction and ERC auto-fix compound edits;
- legacy auto-place, board-fit, and straight-ratsnest helper methods in
  `app/actions.rs`;
- deserialization, migration, and `Board::apply_eco`, which are documented
  domain/persistence boundaries.

The transitional `Deref<Target = ProjectDocument>` on `CircuitApp` still makes
the ownership boundary convention-based rather than compiler-enforced.

## 5. Connectivity regression result

All canonical connectivity tests pass, including direct wire, crossing with
and without explicit junction, T-junction, endpoint-on-segment, collinear
overlap, pin overflight remaining disconnected, local/page/global labels,
multi-page connectivity, typed endpoints, ordering invariance, and exact
save/load mappings. This slice did not rewrite canonical connectivity.

## 6. PCB UI reachability

`Workspace → PCB` opens the dedicated editor. Reachable operations are
independent pan/zoom/grid, layer visibility, net highlight, footprint
selection/multi-selection/box selection/drag/rotate/flip, 45°/90° manual
routing, via placement/layer transition, track/via selection and deletion,
Escape cancel, Backspace anchor removal, and undo/redo. The bottom dock remains
the ECO, DRC, project, and fabrication command center.

## 7. DRC and ECO

DRC now distinguishes an actual different-net intersection from a clearance
violation and includes locations/object IDs. It additionally checks copper
outside the board, duplicate footprint references, dangling tracks, and
dangling vias. Unrouted warnings use the copper-island ratsnest rather than
assuming that any track on a net routes the whole net.

ECO detects added symbols, removed footprints, changed assignments, renamed
references, and added/removed routed net IDs. Applying ECO is undoable.
Removed footprints are retained as orphans by the current UI policy; the
domain also supports remove/keep-tracks and remove-with-tracks policies.

## 8. Simulation limitations

The internal backend remains an educational operating-point solver with
simplified semiconductor/load models and a narrow one-R/one-C transient
preview. MCU, OLED, and sensor modules remain symbol-only. ngspice operating
point execution is hardened but backend selection and asynchronous result
presentation are not yet connected to the egui workflow; ngspice transient
import is explicitly unsupported.

## 9. Schema and migration

Schematic schema remains v4, board schema remains v1, and custom parts remain
v2 with v1 compatibility. `BoardFootprint.flipped` is a backward-compatible
serde-defaulted board field. No existing file requires migration.

## 10. README/UI claim audit

The old README accurately described the bottom dock but explicitly admitted
that no full PCB editor existed. It is updated to describe the now-reachable
workspace and its remaining pad/mask/zone limitations. Automated tagged
release packaging is still correctly described as unavailable. Comparative
“easier/smarter” statements are product goals, not objectively verified test
results.

## 11. Benchmarks

No reproducible before/after benchmark harness existed at baseline, so this
slice does not invent timing numbers after the fact. The required 100/500/1000
component connectivity/ERC/hit-test/ratsnest/DRC/save/history benchmark suite
remains incomplete. Functional copper-island tests were added, but they are
not presented as performance measurements.

## 12. Remaining limitations

- Pad geometry/net mapping is not yet the routing hit-test source; the first
  editor routes from footprint connection points.
- Track endpoint dragging, interactive board-outline vertices, zones,
  pad-to-pad/hole/silkscreen/mask DRC, and ECO pre-apply policy dialog remain.
- Backend selection, probes, production oscilloscope CSV/cursors, and
  asynchronous ngspice UI remain.
- Typed six-lesson engine, multiple recovery candidates/project locks/recent
  projects/read-only future schema, benchmark harness, and tagged artifacts
  remain.
- Major source files still exceed the preferred size limit.

## 13. Screenshots

Real egui viewport captures were generated from reproducible startup presets:

- `docs/media/cluster-schematic-workflow.png`
- `docs/media/cluster-pcb-workflow.png`
- `docs/media/cluster-diagnostics-workflow.png`
- `docs/media/cluster-simulation-workflow.png`
- `docs/media/cluster-lesson-workflow.png`

For example:

```bash
cargo run -- --demo esp32-oled --workspace pcb \
  --capture docs/media/cluster-pcb-workflow.png
```

The capture path uses egui's viewport screenshot event and then exits. During
this work it also exposed and fixed a pre-existing screenshot deadlock caused
by reading the screenshot image while the input lock was still held.
