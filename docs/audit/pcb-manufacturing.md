# PCB and manufacturing audit

Status: **experimental output**, with independently calculated golden fixtures for a limited two-layer geometry subset. No external parser/viewer or fabrication acceptance test was run. See [verification.json](verification.json) for commands and [final-test.log](final-test.log) for results.

## Confirmed defects and corrected behavior

| Priority | Exact source / function | Cause and reproduction | Correction / regression |
|---|---|---|---|
| P0 | `src/pcb/board.rs::ratsnest_edges`, previous copper connectivity helper | Footprint origins acted as electrical terminals, and pads of one footprint were collapsed. Route to body origins instead of real resistor pads; connect two same-net pads only by membership in one footprint. | `src/pcb/copper.rs::{resolve_pads,pad_islands}` resolves each pad independently and uses real copper overlap. `audit_footprint_origins_are_not_electrical_terminals`, `audit_actual_pad_copper_completes_the_route`, `audit_two_same_net_pads_on_one_footprint_need_copper`. |
| P0 | Previous track connectivity in `src/pcb/board.rs` | Layer and collinear contact handling disagreed with physical copper; tolerance could bridge real gaps. | Same-layer copper joins by segment/capsule overlap; through vias and THT pads bridge layers. `audit_collinear_copper_gap_and_opposite_layers_stay_disconnected` checks gap, opposite layers and subsequent via connection. |
| P1 | `src/app/actions.rs::unrouted_pcb_ratsnest`, `route_pcb_ratsnest` | Any track/via on a net hid all its unrouted edges; route helper used body positions. | Keep pad-island edges until physically connected; use real endpoint pads/common layers. `audit_partial_copper_does_not_hide_unrouted_nets`. |
| P0 | `src/ui/pcb_workspace.rs` manual-route start/finish | Body selection chose a footprint's first net rather than the clicked electrical pad. | Resolve clicked pad/net/layer, snap to its actual center, accept finish at same-net pad on active layer. `audit_route_starts_at_the_selected_pad_on_its_net` directly exercises the route-click handler; no desktop mouse-gesture test was run. |
| P0 | `src/pcb/drc.rs::run_drc` | Bounding rectangle/center checks missed concave cutouts and copper protruding across board edges. | Actual polygon membership, entire segment-to-edge spacing and pad/via extents; malformed/self-crossing outlines checked by `pcb::board::valid_polygon`. `audit_concave_outline_and_copper_width_are_checked`, `audit_pad_extent_outside_board_is_not_hidden_by_footprint_origin`. |
| P0 | `src/pcb/drc.rs::{run_drc,run_drc_with_nets}` | Track-only geometry did not cover manufacturing copper interactions sufficiently. | Shared circle/rectangle/oval distances; track/pad, pad/pad, via/pad, track/via and via/via clearance, net-class spacing, unrouted Error and unknown assignment blocking. Existing short/clearance tests and added physical/outline tests pass. Coverage is still incomplete for holes, masks and silk. |
| P0 | `src/pcb/drc.rs::run_drc_with_nets` | A newly introduced per-footprint/net guard accepted an unmapped pin when another mapped pin shared the net. | Match schematic pin identity as well as footprint/net. `audit_every_schematic_pin_needs_its_own_pad_mapping`; [failing-pad-mapping-guard.log](failing-pad-mapping-guard.log). Discovered and fixed during this audit implementation. |
| P2 | `src/export/gerber.rs::gerber_for_layer` | D10 aperture was repeatedly redefined for different track widths, changing interpretation of prior geometry. | Emit immutable unique aperture definitions before drawing. `audit_track_apertures_are_not_redefined`. |
| P2 | `src/export/gerber.rs::gerber_for_layer` | All pads became circles and vias had no flashed copper. | Preserve C/R/O aperture shape/dimensions and flash via annuli on both copper layers. `audit_gerber_preserves_pad_shape_and_via_copper`. |
| P2 | `src/export/gerber.rs::excellon_drill`, `coord` | Million-scaled drill integers were emitted with an ambiguous implicit Excellon format; i32 Gerber conversion could saturate for larger coordinates. | Explicit decimal millimeter drill coordinates; f64-to-i64 conversion for Gerber grid. `audit_drill_coordinates_have_explicit_decimal_units`. The declared 4.6 Gerber coordinate range still needs a dedicated gate. |
| P2 | `src/app/actions.rs::export_pcb_fabrication_files` | UI success path implied project-folder output although files were written in working directory; unsupported geometry could export silently. | Report actual working-folder location and experimental status, use atomic backup writer, run fresh structural/net-aware DRC and geometry limitations before writing. `audit_unsupported_geometry_blocks_manufacturing`. |

Pre-fix failures are in [failing-manufacturing.log](failing-manufacturing.log), [failing-persistence.log](failing-persistence.log), and [failing-physical-connectivity.log](failing-physical-connectivity.log). The four physical-connectivity tests were appended to an isolated `git archive` of the exact baseline commit and **all four failed there**; the same tests pass in the current source. This separates existing behavior from tests introduced after changes. ECO identity/undo defects and fixes are detailed in [architecture-audit.md](architecture-audit.md).

## Pad mapping and layers

`resolve_pads` derives nets from schematic `CadNet.connected_pins` and each footprint's `symbol_instance_id`; shared library `Pad.net_id` is not used as an instance assignment. Declared resistor A/B map to 1/2; LED A/B map to 2/1, with square cathode pad 1. Other pin names require an exact definition pad-number match. Unsupported mappings do not fall back to the footprint origin and are export errors. This is a bounded mapping set, not a general KiCad symbol-footprint mapper.

`FootprintTransform` is shared by pad lookup, DRC, preview and export. Copper connectivity requires overlapping layer sets; through-hole pads and the current through-via model connect both copper layers. Zones are explicitly rejected at manufacturing export and do not contribute to the connectivity model. Blind/buried vias, non-plated versus plated pad holes and arbitrary pad-local rotations are not represented reliably enough for production export.

## Independent manufacturing fixture

`audit_manufacturing_matches_independent_mirror_rotation_golden_files` compares complete output against checked-in [mirrored-pad-B_Cu.gbr](fixtures/mirrored-pad-B_Cu.gbr) and [mirrored-pad.drl](fixtures/mirrored-pad.drl). Expected coordinates were calculated independently of `FootprintTransform`:

- Back-flipped footprint at (10,20) mm, 90° rotation; local pad (2,1) maps to (9,18) mm.
- A 2×1 mm rectangular pad becomes a 1×2 mm back-copper aperture, with 0.5 mm drill.
- Back-copper track (2,3)→(4,3) mm uses 0.25 mm aperture.
- Via (6,7) mm has 0.8 mm copper and 0.4 mm drill.
- Drill tools retain actual diameters and explicit decimal coordinates. Front/back layer transformation follows the footprint side.

Existing transform tests additionally cover quadrant rotations and back flipping. These fixtures verify serialization of this subset; equality to a reference string does not validate an entire Gerber dialect or all board geometries. [Ucamco's Gerber specification/resources](https://www.ucamco.com/en/gerber) are the format authority; conformance testing against an independent reader remains required.

## Export boundary and outstanding manufacturing work

Export now blocks copper zones, unplaced footprints, non-quadrant rotation, rounded rectangles without a corner radius, unequal-diameter circular pads, structural errors, unmapped pins, unknown copper nets and DRC errors including unrouted connections. `RoundRect` uses an enclosing rectangle conservatively for DRC and is blocked for fabrication. The current output includes copper, outline and drills; it does not provide validated solder mask/paste/silkscreen layers, separate PTH/NPTH files, fabrication tolerances, manufacturer rule sets or transactional publication of the entire file bundle. A partially written bundle remains possible if a later output fails, although status reports failure and each file is individually atomic.

Full DRC is now much slower on the measured large PCB fixture. It runs in the analysis worker for editing, but export's fresh gate may block the UI. See [performance.md](performance.md). Do not call this checkpoint fabrication-ready.
