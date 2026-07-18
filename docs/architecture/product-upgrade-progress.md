# Product upgrade progress

Updated: 2026-07-20.

This document records implemented boundaries and deliberately does not describe later product
phases as complete.

## Completed foundation

- Runtime state now has explicit `ProjectDocument`, `EditorState`, `WorkspaceState`, and
  `AnalysisState` owners. The board is persistent document data; PCB CAD and DRC are derived
  analysis data.
- `EditorCommand` receives a restricted `CommandContext` and returns a typed `ChangeSet`. One
  dispatcher owns dirty state, connectivity/ERC/simulation/PCB invalidation, autosave eligibility,
  status feedback, and repaint requests.
- Command history stores entity-level reversible deltas in a `VecDeque` with a 16 MiB/512-entry
  budget. Repeated property/drag commands merge inside a 750 ms window, and pointer movement does
  not create one history entry per frame. A full snapshot exists only transiently while computing
  a compatibility transaction delta; it is not retained in undo/redo.
- Schematic placement, deletion, movement, rotation, properties, wiring, wire control points, and
  tidy operations use commands. PCB primitive commands cover footprint movement/rotation, track
  add/remove, via add/remove, and outline replacement, including board undo/redo.
- Canonical connectivity stages have focused endpoint, spatial-index, intersection, junction,
  geometry, label, union-find, and diagnostics modules while preserving the single cached
  `CanonicalConnectivity` result. Inputs are ID-sorted, so exact pin/junction/segment net mappings
  are independent of component and wire collection order.
- Junction endpoints use a serialization-transparent `JunctionId` runtime type. Legacy numeric
  JSON remains unchanged.
- Static ERC execution is registry-driven through `ErcContext`, `ErcCheck`, stable registry IDs,
  per-rule enablement/severity settings, and structured certainty. Annotation/no-connect and
  ground rules now live in domain rule modules.
- Custom-part schema v2 adds optional tags, voltage range, interfaces, footprint/pad mapping,
  simulation metadata, and documentation. Missing `schema_version` still loads as v1. Duplicate
  pins/pads, missing pad mappings, invalid dimensions, and future versions are rejected.
- Save replacement now keeps three rotated backup generations, syncs the temporary file, renames
  in the target directory, and syncs the directory on Unix.
- Custom-part input rejects symlinks and files over 1 MiB. Poisoned registry locks recover their
  contained data instead of panicking on a user-triggered reload.
- CI separates Linux quality checks from build/test/release checks across Linux, macOS, and
  Windows, and builds documentation.

No schematic, CAD, or board file schema version changed. Custom-part JSON is the only schema
version increment (1 to 2), and v1 files remain accepted.

## Still incomplete

- Several page/demo/ERC-auto-fix and compound PCB operations still begin compatibility
  transactions outside `EditorCommand`; they produce deltas, but should become explicit commands.
- `CircuitApp` keeps a temporary `Deref<ProjectDocument>` compatibility bridge; callers should be
  migrated to explicit owners before it is removed.
- Canonical net generation and most fixtures still live in `engine/netlist.rs`; `net_builder` and
  test-fixture extraction remain.
- ERC rules execute independently through the registry, but most algorithms still live in
  `engine/validation.rs`; more domain files and a richer precomputed context remain.
- Unified ERC/DRC diagnostics UX, editable PCB routing state machine, start/recent-project screen,
  typed guided lessons, probe/scope backend UI, recovery/lock/read-only workflows, property tests,
  large-circuit benchmarks, and release packaging remain future phases.
- There were no UI appearance changes in this slice, so before/after screenshots are not
  applicable.

## Validation

Baseline before this slice is recorded in `product-upgrade-baseline.md`. After the implementation:

- `cargo fmt --check`: pass.
- `cargo clippy --all-targets --all-features -- -D warnings`: pass.
- `cargo test --all-targets`: 207 passed.
- `cargo build --release`: pass.
- `cargo doc --no-deps`: pass.
- `cargo audit`: not run because the `cargo-audit` subcommand is not installed in this
  environment.

## 2026-07-20 PCB workflow increment

- Added the dedicated PCB workspace with independent view state, layer/net
  controls, footprint selection/drag/rotate/flip, manual 45°/90° route state,
  via placement, copper deletion, and command-backed undo/redo.
- Replaced visual footprint-chain ratsnest logic with copper-connected islands.
- Made schematic-to-PCB synchronization an undoable ECO that preserves layout
  and keeps removed components as orphans by default.
- Expanded DRC for different-net shorts, outside copper, duplicate references,
  and dangling tracks/vias.
- Hardened ngspice execution with unique directories, timeout, cancellation,
  stderr, executable configuration, and revision tagging.
- Added reproducible real-window captures under `docs/media/`.

The remaining limitations and current validation results are recorded in
`product-grade-upgrade-2026-07-20.md`.
