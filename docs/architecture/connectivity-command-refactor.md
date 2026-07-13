# Connectivity and command refactor

## Scope

This document records the pre-refactor responsibilities and the staged migration used to make
schematic connectivity authoritative without changing user-visible features or the saved JSON
format. The compatibility boundary remains `SavedCircuit`; runtime connectivity is derived data.

## Responsibilities before the refactor

| Area | Existing responsibility | Dependencies and problems |
| --- | --- | --- |
| `src/app/actions.rs` | IDs/labels, component and wire edits, ERC repairs, Breadboard actions, PCB sync/DRC/export, document save/load/migration, exports, page management, cache access | A 2,391-line `CircuitApp` implementation. Document mutations, UI status, persistence, derived-data caches, and PCB policy are coupled. |
| `src/editor/*` | Undo/redo snapshots; command/selection/wiring placeholders | `commands.rs`, `selection.rs`, and `wiring.rs` contain no command implementation. History calls back into methods implemented in `actions.rs`. |
| `src/model/circuit.rs` | Persistent DTOs and runtime snapshots | Pages use a compatibility tuple and annotations are persisted separately from runtime page state. |
| `src/model/wire.rs` | Wire geometry, typed endpoints, saved endpoint conversion, segment hit testing | Geometry and electrical endpoint identity coexist correctly, but several consumers ignore `WireEndpoint` and infer contact from positions again. |
| `src/model/net.rs` | `CircuitNetlist`, annotations, net/pin/segment projection types | `Net.id` is an untyped `usize`; it is a generated projection rather than the authoritative graph. |
| `src/model/graph.rs` | Explicit node/segment/pin graph and a union-find builder | The builder is not authoritative: `engine/netlist.rs` invokes it only for segment projection and then independently rebuilds nodes, labels, union-find roots, nets, and pin mappings. |
| `src/engine/netlist.rs` | Single/multi-page geometry interpretation, label merging, net generation, diagnostics, and tests | Duplicates graph construction. Pin association partly uses positions, so typed endpoints and consumer results can diverge. |
| `src/engine/validation.rs` | Beginner ERC and DC-result checks | Mostly consumes `CircuitNetlist`, which is good, but it inherits whichever connectivity interpretation produced that projection. |
| `src/engine/simulation.rs` | Public simulation facade | Delegates to `ui/app/energize.rs`; simulation ownership is inverted into UI code. |
| `src/ui/app/energize.rs` | Connectivity reachability and current-flow analysis | Builds another coordinate graph through `CircuitNodes`, `wire_contact_points`, and `connect_wire_contacts`; this can disagree with ERC/netlist. |
| `src/engine/transient.rs` | Narrow RC/PWM transient solver | Calls `build_circuit_netlist` internally, causing an independent rebuild. |
| `src/export/spice.rs` | SPICE text generation | Calls `build_circuit_netlist` internally, causing an independent rebuild. |
| `src/pcb/*` | Board model, schematic footprint sync, ratsnest, DRC, layers/tracks/vias | Consumes `CadNet`; conversion is initiated from `actions.rs`, so agreement with schematic connectivity depends on the conversion path. |

Additional mutation audit:

- Normal editing mutations are concentrated in `actions.rs`, but paste in `ui/app/mod.rs` pushes
  components and wires directly.
- Tests intentionally construct fixtures by mutating public-within-crate vectors; production UI
  must instead dispatch an editor command.
- `same_net_wires` performs a fourth geometry traversal for hover highlighting.
- `connected_pin_positions` performs another geometry interpretation for rendering.

## Target dependency direction

```text
UI intent
  -> EditorCommand
  -> commands::{component,wiring,selection,properties,document,pcb,lessons}
  -> Circuit document mutation + CommandDirtyState
  -> cached CanonicalConnectivity
       -> CircuitNetlist compatibility projection
       -> ERC
       -> DC / transient / current-flow
       -> Breadboard View / code generation
       -> CAD nets / PCB sync
       -> SPICE export
```

No consumer may inspect wire geometry to decide electrical equivalence. Geometry remains available
for drawing, hit testing, editing, and mapping a canonical net back to source wire segments.

## Canonical connectivity build stages

1. **Geometry normalization**: discard degenerate spans, preserve source wire/segment identity, and
   establish deterministic input ordering without mutating saved geometry.
2. **Explicit endpoint resolution**: resolve `WireEndpoint::Pin`, `Junction`, and `FreePoint`; legacy
   free endpoints are migrated only at the load boundary.
3. **Junction resolution**: split at wire endpoints on wire interiors and explicit junction dots;
   crossings without a dot remain separate.
4. **Net-label resolution**: collect normalized names and local/page/global scope.
5. **Multi-page/global merge**: merge page labels only within their declared scope and merge GND
   globally.
6. **Union-find connectivity**: union only normalized segments, resolved junction contacts, and
   label groups.
7. **Canonical net generation**: sort stable member keys before assigning `NetId`, names, pin,
   junction, source-wire, and wire-segment mappings.
8. **Connectivity diagnostics**: record unresolved endpoints, degenerate/floating segments,
   duplicate labels, and invalid annotation references without aborting graph construction.

## Staged implementation plan

Each stage must compile and preserve all prior tests.

1. Introduce the canonical graph API and make `CircuitNetlist` a lossless compatibility projection.
2. Add exact mapping and determinism regression tests for the requested connection cases.
3. Pass one cached graph/projection into simulation, transient, exports, Breadboard, and PCB paths;
   remove their internal connectivity rebuilds.
4. Introduce `EditorCommand` and `CommandDirtyState`, with one dispatcher owning history and cache
   invalidation.
5. Extract existing `CircuitApp` methods into role modules under `src/commands/`, beginning with
   mutations and leaving thin compatibility wrappers where UI call-site churn would be risky.
6. Replace production UI vector mutation (notably paste) with commands.
7. Run format, check, tests, clippy, and release build as proportional final verification.

## Compatibility constraints

- Do not serialize the canonical graph; rebuild it from the existing document fields after load.
- Preserve current schema parsing and legacy endpoint repair.
- Stable IDs are derived deterministically but are not persisted, so old files remain readable.
- Invalid endpoints become diagnostics and electrically floating nodes rather than panics.
- Existing exports and simulations keep their public compatibility entry points for tests and
  callers, while app/runtime paths use the cached canonical result.
