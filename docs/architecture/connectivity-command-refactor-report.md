# Connectivity and command refactor report

## Problems before the change

- `model::graph`, `engine::netlist`, DC MNA, AC MNA, current-flow analysis, hover highlighting,
  connected-pin rendering, transient analysis, and SPICE export could each reinterpret wire
  geometry independently.
- A typed `WireEndpoint::Pin` could be ignored in favor of stale coordinates after a move/rotation.
- `CircuitApp` cached only a netlist projection, not the authoritative connectivity result.
- Production UI paths directly edited `components` and `wires`, and dirty/history behavior depended
  on each call site remembering the correct sequence.
- `actions.rs` mixed mutation logic with derived analysis, export, persistence, PCB, and UI status.

## New module structure

- `model/graph.rs`: `CanonicalConnectivity`, `NetId`, exact pin/junction/segment maps, diagnostics.
- `engine/netlist.rs`: the staged canonical builder and compatibility `CircuitNetlist` projection.
- `commands/mod.rs`: `EditorCommand`, dispatcher, and six-field `CommandDirtyState`.
- `commands/component.rs`: place, paste, and move commands.
- `commands/wiring.rs`: add, control-point move, and tidy commands.
- `commands/selection.rs`: delete, rotate, duplicate, align, and distribute commands.
- `commands/properties.rs`: value/label edits and switch state changes.
- `commands/document.rs`: document reset command.
- `commands/pcb.rs` and `commands/lessons.rs`: reserved command boundaries for the next extraction;
  existing PCB/persistence/lesson compatibility methods remain callable while migration continues.

`actions.rs` is 370 lines smaller and its primary interactive mutation bodies have moved to command
modules. Persistence, page lifecycle, lesson setup, and PCB orchestration remain compatibility
services there; moving them is the recommended next mechanical extraction.

## Removed duplicate logic

- Removed the second union-find and net generator formerly compiled in `model::graph.rs`.
- DC and AC MNA now initialize their net maps from `CanonicalConnectivity`.
- Current-flow reachability now joins canonical net members instead of rebuilding wire contacts.
- Hover net highlighting and connected-pin rendering use the cached canonical projection.
- Runtime transient, ERC, PCB/codegen, and SPICE export paths receive the same cached projection.
- Production UI no longer directly mutates schematic component/wire collections.

Compatibility wrappers used by tests or external-in-crate callers may still build a graph when no
cached graph is supplied; runtime `CircuitApp` paths pass the cached graph explicitly.

## Canonical flow

1. Normalize geometry and diagnose degenerate wires.
2. Resolve typed pin endpoints, including stale geometry after component movement/rotation.
3. Resolve T-junctions and explicit junction dots; do not join unmarked crossings.
4. Normalize local/page/global labels.
5. Merge scoped labels and global GND.
6. Run the single union-find connectivity pass.
7. Generate deterministic nets plus exact pin, annotation, source-wire, and raw-segment mappings.
8. Emit non-fatal connectivity diagnostics.

The graph is revision-cached on `CircuitApp`; it is derived data and is intentionally absent from
the saved JSON schema.

## Regression coverage

Coverage now includes:

- direct pin-to-pin typed wire with exact pin and segment mapping;
- crossing without junction and crossing with exact junction mapping;
- T-junction, middle branch, overlapping collinear wires, and multiple branches at one junction;
- component body endpoint and unrelated pin overflight remaining disconnected;
- typed endpoint connectivity after component move and rotation;
- legacy endpoint migration preserving canonical mappings;
- local/page/global labels, same label across pages, and duplicate-label diagnostics;
- floating/isolated wires;
- deterministic exact pin/junction/segment signatures;
- save/load equality for pin, junction, segment, and wire maps;
- ERC/code generation/PCB conversion consuming the same projection;
- command dirty-state completeness, history ownership, and cache invalidation.

All pre-existing tests remain enabled.

## Remaining risks

- `PinRef` identifies a pin by component ID and display name. Components with repeated physical pin
  names are intentionally merged; a future physical-pin UUID/number should remove that ambiguity.
- Runtime page annotations are still represented by compatibility storage fields; a first-class
  `SchematicPage` should own junctions, no-connects, and label scopes.
- `CircuitNetlist` still exposes `usize` IDs for compatibility. A later API cleanup should make the
  `NetId` newtype non-aliasing across documents/revisions.
- Undo for continuous drag uses one snapshot captured at drag start while intermediate command
  updates skip extra snapshots. Pointer-cancel behavior deserves an explicit UI regression test.
- Persistence, multi-page lifecycle, PCB orchestration, and lesson/demo construction are the
  largest responsibilities still resident in `actions.rs`.

## Recommended next refactor targets

1. Extract save/load/page lifecycle and `SavedCircuit` conversion from `actions.rs` into
   `commands/document.rs` plus a storage service.
2. Extract PCB sync/DRC/export orchestration into `commands/pcb.rs`.
3. Move demo/lesson document construction into `commands/lessons.rs` transactions.
4. Move simulation implementation out of `ui/app/energize.rs` into `engine/simulation.rs`; its
   connectivity is canonical now, but ownership is still inverted.
5. Split `engine/validation.rs` by rule family after the graph API has remained stable.
