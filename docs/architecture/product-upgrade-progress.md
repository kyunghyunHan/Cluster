# Product upgrade progress

Date: 2026-07-14.

This document records implemented boundaries and deliberately does not describe later product
phases as complete.

## Completed foundation

- Runtime state now has explicit `ProjectDocument`, `EditorState`, `WorkspaceState`, and
  `AnalysisState` owners. The board is persistent document data; PCB CAD and DRC are derived
  analysis data.
- `EditorCommand` returns a typed `ChangeSet`. One dispatcher owns dirty state, connectivity/ERC/
  simulation/PCB invalidation, autosave eligibility, and repaint requests.
- Command history stores human-readable descriptions and merge keys. Repeated property/drag-like
  commands merge only inside a 750 ms window. Snapshot fallback remains for complex legacy
  operations.
- Schematic placement, deletion, movement, rotation, properties, wiring, wire control points, and
  tidy operations use commands. PCB primitive commands cover footprint movement/rotation, track
  add/remove, via add/remove, and outline replacement, including board undo/redo.
- Canonical connectivity stages have focused `geometry`, `labels`, `union_find`, and `diagnostics`
  modules while preserving the single cached `CanonicalConnectivity` result.
- Static ERC execution is registry-driven through `ErcContext`, `ErcCheck`, and stable registry
  IDs. Existing rule algorithms and result ordering are preserved.
- Custom-part schema v2 adds optional tags, voltage range, interfaces, footprint/pad mapping,
  simulation metadata, and documentation. Missing `schema_version` still loads as v1. Duplicate
  pins/pads, missing pad mappings, invalid dimensions, and future versions are rejected.
- Save replacement now keeps three rotated backup generations, syncs the temporary file, renames
  in the target directory, and syncs the directory on Unix.

No schematic, CAD, or board file schema version changed. Custom-part JSON is the only schema
version increment (1 to 2), and v1 files remain accepted.

## Still incomplete

- Several page/demo/ERC-auto-fix and compound PCB operations still use the documented snapshot
  fallback and must be converted to explicit transaction commands.
- `CircuitApp` keeps a temporary `Deref<ProjectDocument>` compatibility bridge; callers should be
  migrated to explicit owners before it is removed.
- Connectivity endpoint, junction, canonicalization, query, and test code still need further
  physical extraction from `engine/netlist.rs`.
- ERC rules execute independently through the registry, but individual algorithms still live in
  `engine/validation.rs`; domain files and a richer precomputed context remain.
- Unified ERC/DRC diagnostics UX, editable PCB routing state machine, start/recent-project screen,
  typed guided lessons, probe/scope backend UI, recovery/lock/read-only workflows, property tests,
  large-circuit benchmarks, and release packaging remain future phases.
- There were no UI appearance changes in this slice, so before/after screenshots are not
  applicable.

## Validation

Baseline before this slice is recorded in `product-upgrade-baseline.md`. After the implementation:

- `cargo fmt --check`: pass.
- `cargo clippy --all-targets --all-features -- -D warnings`: pass.
- `cargo test --all-targets`: 198 passed.
- `cargo build --release`: pass.
