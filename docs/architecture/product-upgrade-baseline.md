# Product upgrade baseline

Baseline date: 2026-07-14 (Australia/Melbourne).

The working tree already contained the preceding canonical-connectivity and command-boundary
refactor. Those changes are preserved and treated as the starting point for this upgrade.

## Source ownership before this upgrade

| Concern | Current owner | Boundary issue |
| --- | --- | --- |
| Persistent document | Flat fields on `ui::app::CircuitApp`, DTOs in `model::circuit`, PCB board in `PcbUiState` | Persistent data and runtime/UI state share one application object. |
| Editor state | `CircuitApp`, `app::state`, `HistoryState`, `editor::history` | Tool, selection, clipboard, drag and history are not one owned boundary. |
| Workspace UI | `UiState`, `CanvasState`, palette/inspector/simulation/breadboard/PCB UI structs | Several view-only fields remain flat on `CircuitApp`. |
| Connectivity | `model::graph::CanonicalConnectivity`, built in `engine::netlist` | Canonical result exists, but the 1,300+ line builder still mixes every stage and tests. |
| ERC | `engine::validation` plus UI-oriented ERC code in `ui::app::energize` | 2,700+ line monolith; rules are functions rather than registered checks. |
| Simulation | facade in `engine::simulation`, implementations in MNA modules and `ui::app::energize` | Engine ownership is inverted into UI code. |
| PCB | `pcb::*`, `PcbUiState`, orchestration in `app::actions` | Board persistence/edit state and preview UI are coupled. |
| Persistence | `storage::save`, `storage::autosave`, and duplicate conversion/load orchestration in `app::actions` | Backup write is not yet a fully synced atomic replace/recovery system. |
| Undo/redo | snapshot stacks in `HistoryState` and `editor::history`; command dispatcher in `commands` | Commands report dirty state but do not yet own reversible deltas or merge completed drags. |
| Cache invalidation | `CommandDirtyState`, `DirtyFlags`, `editor::history`, and cache accessors in `app::actions` | Manual invalidation paths still coexist with dispatcher invalidation. |

## Serializable schemas inspected

- Schematic JSON: schema 4 (`SavedCircuit`, `SavedPage`, typed optional wire endpoints).
- CAD project: schema 1.
- Board: schema 1.
- Custom parts: schema 1 with rejection of unsupported future versions.
- Library catalog: schema 1.

No schema change is made merely by introducing runtime architecture types. Compatibility parsing
and legacy endpoint migration remain the persistence boundary.

## Required command baseline

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Passed. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Failed: one `clippy::type_complexity` in the canonical connectivity test signature at `engine/netlist.rs`. This was introduced by the preceding refactor and is fixed immediately after recording this baseline with a named test-only type alias. |
| `cargo test --all-targets` | Passed: 195 tests. |
| `cargo build --release` | Passed. |

The clippy failure is recorded rather than suppressed or omitted.
