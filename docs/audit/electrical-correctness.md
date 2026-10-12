# Electrical correctness evidence

Baseline and verification are in [architecture-audit.md](architecture-audit.md). These fixes establish specific regression cases; Cluster remains an educational solver.

## Reproduced defects and corrections

| Priority | Exact implementation | Failure mechanism / reproduction | Correction and test |
|---|---|---|---|
| P0 | `src/engine/mna/dc.rs::solve_dc_profiled_internal`; reference selection also in `ac.rs::solve_ac_with_connectivity` | Every source negative terminal was treated as ground. Two series sources with grounded final return produced wrong total voltage. | Explicit ground is authoritative; without one, choose only the lowest-ID source return as educational reference. `audit_series_sources_preserve_floating_negative_terminal`. |
| P0 | `src/engine/mna/matrix.rs::Mna::stamp_r` | Zero/subnano resistance returned without stamping, silently modeling an open. | Stamp with 1 nΩ floor and use the same floor in decoded current. Invalid negative/nonfinite stamps cannot succeed. `audit_zero_resistance_is_not_silently_open`. The floor is a regularization, not an exact ideal-short model. |
| P1 | `src/engine/mna/matrix.rs::Mna::solve_with_cancellation` | Absolute pivot threshold rejected tiny but well-scaled admittance; elimination skipped tiny coefficients irrespective of scale. | Row/column equilibration, relative pivot threshold, exact-zero skipping, finite checks and `NumericalFailure`. `audit_tiny_admittance_is_not_a_singular_matrix`, `audit_nonfinite_matrix_never_returns_success`. This is not a condition-number certificate. |
| P1 | `src/engine/mna/matrix.rs::validate_voltage_sources` | Pair-only checks missed multi-source cycles. Three sources enclosing a loop failed without correct diagnosis. | Traverse source constraints with node potentials; reject inconsistent cycles and separately identify redundant constraints. `audit_three_source_loops_are_classified`. |
| P0 | `src/engine/mna/ac.rs::solve_ac_with_connectivity` | A complex AC MNA voltage source did not receive independent real and imaginary branch constraints. | Double source branch unknowns and stamp both constraints; reject invalid frequency/nonfinite solutions. `audit_ac_rc_lowpass_matches_complex_transfer_function`, checked against analytic complex RC transfer. |
| P0 | `src/engine/transient.rs::solve_transient_with_netlist` | Any resistor touching a capacitor could generate a charging curve without a source-return loop. Extra storage was ignored. | Require exactly one resistor/capacitor/source, actual series charging path and return; unsupported extra storage/components return no RC approximation. `audit_rc_requires_source_return_and_rejects_extra_storage`. |
| P0 | `src/engine/erc/rules/beginner.rs::pin_is_5v_source` | 5V source minus counted as a high-voltage driver. Connect its return to an ESP GPIO. | Only positive source terminal supplies this voltage. `audit_source_negative_terminal_is_not_a_high_voltage_driver`. |
| P0 | `src/app/actions.rs::current_netlist`, `current_simulation`; `src/engine/worker.rs::execute` | Property edits reused stale pin values and cached structural rules; 3.3V→5V suppressed new GPIO error. | Separate property projection from immutable connectivity, include electrical revision in structural ERC cache, refresh worker/backend consumers. Two cache tests assert fresh values/errors AND identical canonical Arc. |

The original failures are preserved in [failing-electrical.log](failing-electrical.log), [failing-transient-hardware.log](failing-transient-hardware.log), and [failing-property-cache.log](failing-property-cache.log). Relevant integration cases include open source/current zero, open switch, reversed/forward LED, NMOS gate low/high, DC-open capacitor, floating nodes and missing ground. These tests passed in the final full run. Source function naming for the DC implementation is the internal profiled solve path, reached by `solve_dc_detailed_with_connectivity`.

## Device capability coverage

`src/model/device.rs::{board_profile,pin_capability}` introduces separate physical package pin, board header label/number, MCU GPIO/port, alternate functions, electrical type, voltage domain, maximum input relation, current rating and boot constraint fields. Package, input-maximum and current-rating fields remain explicitly unknown in the present profile data. The supply-relative maximum enum is a schema foundation, not yet a populated rating. Inspector Model displays profile coverage and source. `audit_device_identity_and_constraints_are_separate` checks identity distinctions, Pico mapping and STM32 conditional tolerance.

| Existing supported board family | Verified subset incorporated | Deliberate gaps |
|---|---|---|
| ESP32 DevKit V1 | Chip GPIO identity, input-only GPIO34–39, boot strap subset, 3.3V domain | Clone board/package/header mapping not verified; chip-only profile. |
| ESP32-S3 / C3 modules | Chip GPIO and documented boot strap subsets | Exact module/header/flash variant unresolved; chip-only. |
| Arduino UNO R3 | ATmega328P identity, 5V domain, I2C A4/A5 alias information | No full package/header table or verified total current budget. |
| Raspberry Pi Pico RP2040 | 3.3V domain, GP26/27/28 ADC identities and header mappings | Partial alternate functions and header coverage; no full current budget. |
| STM32 Blue Pill | STM32F103 subset, analog inputs, known FT digital inputs as conditional | Clone schematic/package variants unknown; conditional FT is not a 5V guarantee for analog/unpowered operation. |
| STM32 Nucleo64 | Explicit unspecified variant profile | Generic symbol does not identify exact MCU. Maximum ratings and package mappings cannot be asserted. |

Manufacturer sources consulted (facts are scoped to their device/board, not all clone variants):

- [Espressif ESP32 datasheet](https://documentation.espressif.com/esp32_datasheet_en.html), [ESP32-S3 datasheet](https://documentation.espressif.com/esp32_s3_datasheet_en.pdf), [ESP32-C3 hardware checklist](https://docs.espressif.com/projects/esp-hardware-design-guidelines/en/latest/esp32c3/schematic-checklist.html): GPIO constraints and boot strapping.
- [Arduino UNO R3 datasheet](https://docs.arduino.cc/resources/datasheets/A000066-datasheet.pdf): MCU and board voltage/interface identity.
- [Pico datasheet](https://datasheets.raspberrypi.com/pico/pico-datasheet.pdf), [RP2040 datasheet](https://datasheets.raspberrypi.org/rp2040/rp2040-datasheet.pdf), §5.5.3.1 Table 622: RP2040 input absolute maximum is relative to IOVDD; do not use it as a normal operating recommendation.
- [ST STM32F103 datasheet](https://www.st.com/resource/en/datasheet/stm32f103v8.pdf), pin tables and FT conditions: distinguish analog/non-FT from conditionally tolerant digital inputs.
- [ST UM1724 Nucleo user manual](https://www.st.com/resource/en/user_manual/dm00105823-nucleo-f401re-user-manual-stmicroelectronics.pdf): multiple MCU variants justify an unspecified generic profile.

This satisfies the model foundation and verified subsets, not full verified profiles for every board. Existing general ERC rules still contain heuristic metadata in `model/component.rs::electrical_metadata`; they have not all been migrated to capabilities. GPIO current, thermal, total-bank current, pull-up availability, analog limits and unpowered injection require further work. No hardware or firmware compilation was performed.

## Numerical/model limits

`dc.rs` uses piecewise diode forward drop/leakage and threshold MOS ON/OFF resistances with bounded state iteration. Existing tests verify those approximations, not Shockley behavior, MOS operating regions, temperature or SPICE agreement. AC uses simplified component impedances rather than a complete small-signal linearization at the DC operating point. Transient is a tightly gated single-RC analytic helper, with fixed sampling and simplified PWM input. Dense elimination remains cubic; extreme mixed-scale systems may remain ill-conditioned. An error/unsupported result is preferable to inventing an accurate waveform. See [remaining-risks.md](remaining-risks.md) for follow-up scope.
