# Main branch upgrade baseline

Baseline date: 2026-07-16 (Australia/Melbourne).

This is a fresh measurement of the current `main` working tree before the
editor/PCB upgrade. The branch was clean and two local commits ahead of
`origin/main`:

- `f96519b refactor: establish document command and analysis boundaries`
- `cc60940 feat: version parts and harden atomic saves`

No result below is inferred from an earlier report.

## Required command results

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Passed. |
| `cargo check --all-targets` | Passed with 97 `float_literal_f32_fallback` future-compatibility warnings. |
| `cargo clippy --all-targets -- -D warnings` | Failed with 98 errors: the same 97 future-compatibility warnings plus one `clippy::collapsible_match` in `src/engine/validation.rs`. |
| `cargo test --all-targets` | Passed: 198 tests; emitted the same 97 warnings. |
| `cargo build --release` | Passed; emitted the same 97 warnings. |

The warnings and Clippy failure are recorded before repair. They are not
suppressed as a way of making the baseline green.

## Repository size and concentration

The Rust source tree contains 34,845 lines. The largest files are:

| File | Lines |
| --- | ---: |
| `src/ui/app/mod.rs` | 3,440 |
| `src/ui/app/symbols.rs` | 3,110 |
| `src/engine/validation.rs` | 2,829 |
| `src/app/actions.rs` | 2,162 |
| `src/ui/app/energize.rs` | 1,987 |
| `src/ui/app/tests.rs` | 1,845 |
| `src/engine/mna/mod.rs` | 1,397 |
| `src/ui/app/util.rs` | 1,310 |
| `src/engine/netlist.rs` | 1,212 |
| `src/export/svg.rs` | 1,137 |
| `src/engine/mna/dc.rs` | 1,034 |

These concentrations identify the first extraction targets; line count alone
is not treated as proof that a module is incorrectly designed.

## State ownership at baseline

| State | Current owner | Boundary defect |
| --- | --- | --- |
| Persistent schematic and PCB | `model::project::ProjectDocument` | The type exists, but fields remain crate-visible and `CircuitApp` exposes them through transitional `Deref` access. |
| Editor state | `ui::app::EditorState` | Tool/selection/drag/clipboard/history are grouped, but command handlers still receive unrestricted `&mut CircuitApp`. |
| Derived analysis | `ui::app::AnalysisState` | Caches are grouped, but invalidation uses coarse booleans and compatibility helpers. |
| Workspace/view state | `ui::app::WorkspaceState` plus flat `CircuitApp` fields | Several dialog, status, simulation and PCB preview fields remain on the application root. |
| Command history | `HistoryState` | Entries are complete `CircuitSnapshot` clones in `Vec`; eviction uses `Vec::remove(0)`, and continuous merge skips snapshots rather than merging reversible deltas. |

`EditorCommand::apply(self, &mut CircuitApp)` is the principal ownership leak:
every command can currently mutate UI state, caches and persistence data, not
only the document fields it owns.

## Persistent mutation audit

Searches covered writes to component, wire, page, junction and PCB collections
outside model/storage/test code. There are 51 syntactic candidates, including
derived caches and builders. Confirmed user-document mutation paths are:

- schematic component and wire helpers in `src/app/actions.rs`, invoked by
  commands but still implemented on the unrestricted application object;
- PCB footprint/track/via/outline edits and route helpers in
  `src/app/actions.rs`;
- page add/remove and current-page materialization in `src/app/actions.rs`;
- `pcb::board::Board::update_from_schematic`, called from UI orchestration;
- compatibility conversion and endpoint migration during load, which are
  intentionally allowed at the persistence boundary.

UI rendering code does not currently contain a confirmed direct write to the
schematic `components` or `wires` collections. This is useful progress, but it
is not enforceable while document fields and the `Deref` compatibility layer
remain crate-visible.

## Panic and placeholder audit

- The tree contains 263 explicit `.unwrap()`/`.expect()` calls. Most occur in
  tests. Product-reachable exceptions include poisoned custom-part registry
  mutex handling and one checked current-flow invariant.
- `#[allow(dead_code)]` appears throughout command, simulation, storage,
  export, CAD and PCB modules. Several annotations protect UI-inaccessible PCB
  command variants, so they are treated as evidence of incomplete workflows,
  not harmless cleanup.
- `LessonCommand::Noop` is a real no-op command placeholder.
- A Breadboard status string contains the word `TODO`; it is user-facing copy,
  not executable placeholder behavior.

## Connectivity, ERC and PCB baseline

- `CanonicalConnectivity` is the shared result type and the connectivity
  module names geometry, label, union-find and diagnostics stages.
- The builder remains concentrated in `engine/netlist.rs`; stage boundaries
  are not yet independently cacheable and no spatial index protects the hot
  geometry paths.
- ERC remains a 2,829-line module. A registry exists, but rule isolation and
  UI-independent diagnostics are incomplete.
- PCB has persisted two-layer primitives and data-level command variants, but
  the reachable UI is still a bottom-dock preview. Move/rotate/route/via and
  layer-aware editing are not a dedicated workspace.
- Ratsnest generation chains footprint identifiers per net. It does not yet
  compute remaining copper islands, so partial routing can report misleading
  airwires.

## Compatibility baseline

- Schematic save format: schema 4, including typed optional wire endpoints and
  legacy endpoint migration.
- CAD project, board and library catalog: schema 1.
- Custom-part format: schema 2 with compatible loading of schema 1.
- Persistence uses schema DTOs; runtime ownership and command-history changes
  must not alter those DTOs without an explicit migration.

## Immediate execution order

1. Repair the recorded compiler/Clippy failures and restore a warning-free
   baseline.
2. Restrict command mutation through `CommandContext` and replace coarse dirty
   booleans with typed invalidation.
3. Replace snapshot history with memory-bounded reversible deltas.
4. Remove remaining persistent mutation paths outside commands and migration.
5. Build the dedicated PCB workspace and route state machine on that boundary.

