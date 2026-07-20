use crate::engine::units::parse_metric_value;
use crate::model::*;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErcSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErcCertainty {
    Definite,
    Likely,
    Advisory,
}

impl ErcCertainty {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Definite => "Definite",
            Self::Likely => "Likely",
            Self::Advisory => "Advisory",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ErcViolation {
    pub(crate) rule: ErcRule,
    pub(crate) severity: ErcSeverity,
    pub(crate) component_id: Option<u64>,
    pub(crate) wire_id: Option<u64>,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErcRelated {
    Component(u64),
    Wire(u64),
    ComponentAndWire { component_id: u64, wire_id: u64 },
    Schematic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ErcRuleDetails {
    pub(crate) id: &'static str,
    pub(crate) explanation: &'static str,
    pub(crate) fix_hint: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Stable rule identifiers outlive individual UI integrations.
pub(crate) enum ErcRule {
    General,
    DuplicateReference,
    DuplicateNamedNet,
    NoConnectWired,
    MissingGround,
    PowerShort,
    LedSeriesResistorMissing,
    LedReversed,
    DiodeReversed,
    Rail5vTo3v3,
    GpioDirectLoad,
    GpioOvervoltage5v,
    PowerRailConflict,
    RelayFlybackMissing,
    I2cPullupMissing,
    I2cPullupTooHigh,
    I2cPullupTooLow,
    I2cSdaSclMismatch,
    UartTxRxMismatch,
    SpiSignalMismatch,
    AdcOvervoltage,
    I2cAddressConflict,
    MissingValue,
    FloatingConnectivity,
    FloatingDcIsland,
    OpenVoltageSource,
    SimulationSupportLimited,
    MissingDecouplingCapacitor,
    OutputConflict,
    PowerInputUndriven,
    UnconnectedInput,
    NetLabelRemoteJoin,
    RegulatorVoltageRange,
    ResistorWattage,
    LedCurrentLimit,
    GpioCurrentLimit,
}

impl ErcRule {
    pub(crate) fn details(self) -> ErcRuleDetails {
        match self {
            Self::General => ErcRuleDetails {
                id: "erc.general",
                explanation: "This rule points to a schematic condition that may make the circuit unreliable, unclear, or hard to manufacture.",
                fix_hint: None,
            },
            Self::DuplicateReference => ErcRuleDetails {
                id: "erc.annotation.duplicate_reference",
                explanation: "Duplicate designators make the schematic, BOM, and PCB cross-probing ambiguous.",
                fix_hint: Some(
                    "Rename or re-annotate components so each real part has a unique reference.",
                ),
            },
            Self::DuplicateNamedNet => ErcRuleDetails {
                id: "erc.net.duplicate_named_island",
                explanation: "The same visible net name on disconnected islands is easy to mistake for an intentional connection.",
                fix_hint: Some(
                    "Add a wire/junction if the islands should connect, or rename one label if they are separate.",
                ),
            },
            Self::NoConnectWired => ErcRuleDetails {
                id: "erc.pin.no_connect_wired",
                explanation: "A no-connect marker says the pin is intentionally unused, so wiring that pin contradicts the schematic intent.",
                fix_hint: Some("Remove the wire or remove the no-connect marker."),
            },
            Self::MissingGround => ErcRuleDetails {
                id: "erc.power.ground_missing",
                explanation: "A ground reference gives voltages a common zero point and lets DC analysis decide whether paths are complete.",
                fix_hint: Some("Add a GND symbol and connect the circuit return path to it."),
            },
            Self::PowerShort => ErcRuleDetails {
                id: "erc.power.short",
                explanation: "A supply rail connected directly to ground can draw destructive current and make simulation results meaningless.",
                fix_hint: Some(
                    "Find the highlighted net and place a load, switch, regulator, or correct wiring between power and GND.",
                ),
            },
            Self::LedSeriesResistorMissing => ErcRuleDetails {
                id: "erc.led.series_resistor_missing",
                explanation: "LEDs need current limiting because their voltage-current curve is steep; connecting one directly can damage the LED or the driving pin.",
                fix_hint: Some(
                    "Add a 220-1k ohm resistor in series with the LED. 330 ohm is a safe beginner default.",
                ),
            },
            Self::LedReversed => ErcRuleDetails {
                id: "erc.led.reversed",
                explanation: "An LED only conducts in the forward direction, so a reversed LED will not light in a normal DC path.",
                fix_hint: Some(
                    "Swap the LED terminals so anode goes toward the positive side and cathode toward GND/return.",
                ),
            },
            Self::DiodeReversed => ErcRuleDetails {
                id: "erc.diode.reversed",
                explanation: "A diode blocks current when reversed relative to the intended DC path.",
                fix_hint: Some(
                    "Check diode orientation and swap anode/cathode if it should conduct from power to return.",
                ),
            },
            Self::Rail5vTo3v3 | Self::PowerRailConflict => ErcRuleDetails {
                id: "erc.power.rail_conflict",
                explanation: "Tying 5V and 3.3V rails together can overvoltage low-voltage modules and controller pins.",
                fix_hint: Some(
                    "Separate the rails, use the correct regulator output, or add level shifting for signals crossing voltage domains.",
                ),
            },
            Self::GpioDirectLoad => ErcRuleDetails {
                id: "erc.gpio.direct_load",
                explanation: "Microcontroller GPIO pins can only source or sink a small current. Motors, relays, lamps, and high-current LEDs need a driver stage.",
                fix_hint: Some(
                    "Do not drive motors, relays, lamps, or high-current LEDs directly from GPIO. Use a transistor/MOSFET driver and a separate supply.",
                ),
            },
            Self::GpioOvervoltage5v => ErcRuleDetails {
                id: "erc.gpio.overvoltage_5v",
                explanation: "Most 3.3V controller GPIO pins are not 5V tolerant; overvoltage can permanently damage the input.",
                fix_hint: Some(
                    "Keep ESP32/Pico GPIO at 3.3V. Add a level shifter or resistor divider before the pin.",
                ),
            },
            Self::RelayFlybackMissing => ErcRuleDetails {
                id: "erc.inductive.flyback_missing",
                explanation: "Relay coils and motors produce a voltage spike when switched off. A flyback diode gives that current a safe path.",
                fix_hint: Some(
                    "Place a reverse-biased diode across the relay coil or motor winding to clamp inductive kickback.",
                ),
            },
            Self::I2cPullupMissing => ErcRuleDetails {
                id: "erc.i2c.pullup_missing",
                explanation: "I2C SDA and SCL are open-drain signals, so devices pull the line low and resistors pull it high.",
                fix_hint: Some(
                    "Add a 4.7k pull-up from SDA to logic VCC and another 4.7k pull-up from SCL to logic VCC.",
                ),
            },
            Self::I2cPullupTooHigh => ErcRuleDetails {
                id: "erc.i2c.pullup_too_high",
                explanation: "A pull-up that is too weak makes the I2C rising edge slow, which can cause unreliable communication.",
                fix_hint: Some(
                    "Use a lower pull-up value, typically 2.2k-4.7k for short beginner breadboard buses.",
                ),
            },
            Self::I2cPullupTooLow => ErcRuleDetails {
                id: "erc.i2c.pullup_too_low",
                explanation: "A pull-up that is too strong wastes current and can exceed what devices can safely pull low.",
                fix_hint: Some(
                    "Use a higher pull-up value in the 1k-10k range; 4.7k is a common default.",
                ),
            },
            Self::I2cSdaSclMismatch => ErcRuleDetails {
                id: "erc.i2c.sda_scl_mismatch",
                explanation: "I2C devices use separate data and clock lines. Swapping SDA and SCL prevents the bus from communicating.",
                fix_hint: Some(
                    "Swap the I2C wires so controller SDA goes to module SDA and controller SCL goes to module SCL.",
                ),
            },
            Self::UartTxRxMismatch => ErcRuleDetails {
                id: "erc.uart.tx_rx_mismatch",
                explanation: "UART links are point-to-point signal pairs: one device transmits into the other device's receive pin.",
                fix_hint: Some("Connect TX to RX and RX to TX between the two UART devices."),
            },
            Self::SpiSignalMismatch => ErcRuleDetails {
                id: "erc.spi.signal_mismatch",
                explanation: "SPI roles are not interchangeable on one net; MOSI, MISO, SCK, and chip-select signals each need the matching role.",
                fix_hint: Some(
                    "Separate the SPI signals and connect matching controller/peripheral roles.",
                ),
            },
            Self::AdcOvervoltage => ErcRuleDetails {
                id: "erc.adc.overvoltage",
                explanation: "ADC inputs must stay inside their reference voltage range or readings become invalid and the pin may be damaged.",
                fix_hint: Some(
                    "Scale the signal with a resistor divider or buffer so the ADC pin never exceeds its reference voltage.",
                ),
            },
            Self::I2cAddressConflict => ErcRuleDetails {
                id: "erc.i2c.address_conflict",
                explanation: "Two I2C devices with the same address on one bus will both respond, so the controller cannot address them independently.",
                fix_hint: Some(
                    "Change an address jumper, choose a different module address, or place one device on another I2C bus.",
                ),
            },
            Self::MissingValue => ErcRuleDetails {
                id: "erc.value.invalid_or_missing",
                explanation: "Simulation and exports need parseable electrical values to produce useful results.",
                fix_hint: Some("Enter a value with units, such as 330 ohm, 10k, 5V, or 100nF."),
            },
            Self::FloatingConnectivity | Self::FloatingDcIsland => ErcRuleDetails {
                id: "erc.connectivity.floating",
                explanation: "Floating pins, wires, or DC islands have no defined electrical state, so real hardware behavior is unpredictable.",
                fix_hint: Some(
                    "Connect the item to a driven net, add a pull-up/pull-down, or mark intentionally unused pins as no-connect.",
                ),
            },
            Self::OpenVoltageSource => ErcRuleDetails {
                id: "erc.power.open_source",
                explanation: "A voltage source with no closed load path has voltage but no current flow.",
                fix_hint: Some(
                    "Connect a load path from the source positive terminal back to its return/GND terminal.",
                ),
            },
            Self::SimulationSupportLimited => ErcRuleDetails {
                id: "erc.simulation.support_limited",
                explanation: "This part is checked for connectivity, but the built-in educational solver does not fully model its behavior.",
                fix_hint: Some(
                    "Treat the result as approximate or export to ngspice for detailed analysis.",
                ),
            },
            Self::MissingDecouplingCapacitor => ErcRuleDetails {
                id: "erc.power.decoupling_missing",
                explanation: "Digital ICs and modules draw fast current pulses. A local capacitor keeps the supply stable at the part.",
                fix_hint: Some(
                    "Place a 100 nF ceramic capacitor between VCC/3V3 and GND close to the IC/module power pins.",
                ),
            },
            Self::OutputConflict => ErcRuleDetails {
                id: "erc.logic.output_conflict",
                explanation: "Two actively driven outputs on one net can fight each other and damage parts or produce invalid logic levels.",
                fix_hint: Some(
                    "Keep only one active driver on the net, or add tri-state/open-drain behavior where appropriate.",
                ),
            },
            Self::PowerInputUndriven => ErcRuleDetails {
                id: "erc.power.input_undriven",
                explanation: "A power input pin without a supply means the part is not powered in the schematic.",
                fix_hint: Some(
                    "Connect the pin to a voltage source, regulator output, or intentional power net.",
                ),
            },
            Self::UnconnectedInput => ErcRuleDetails {
                id: "erc.logic.input_unconnected",
                explanation: "Floating digital inputs can randomly read high or low and cause unstable behavior.",
                fix_hint: Some(
                    "Connect the input to a signal, add a pull-up/pull-down resistor, or mark it no-connect if unused.",
                ),
            },
            Self::NetLabelRemoteJoin => ErcRuleDetails {
                id: "erc.net.remote_label_join",
                explanation: "Matching net labels intentionally connect remote schematic locations, which can hide accidental shorts.",
                fix_hint: Some(
                    "Verify the repeated label is intentional, or rename one label to keep the nets separate.",
                ),
            },
            Self::RegulatorVoltageRange => ErcRuleDetails {
                id: "erc.power.regulator_voltage_range",
                explanation: "Regulators need enough input headroom and must stay within their maximum input rating.",
                fix_hint: Some(
                    "Use a suitable LDO/buck regulator, raise Vin if dropout is too low, or verify the regulator rating.",
                ),
            },
            Self::ResistorWattage => ErcRuleDetails {
                id: "erc.resistor.wattage_exceeded",
                explanation: "A resistor dissipating more than its rating can overheat or fail.",
                fix_hint: Some("Use a higher-wattage resistor or reduce the current through it."),
            },
            Self::LedCurrentLimit => ErcRuleDetails {
                id: "erc.led.current_limit_exceeded",
                explanation: "LED current above the rated limit can shorten LED life or damage the driver.",
                fix_hint: Some("Increase the series resistor or lower the supply voltage/current."),
            },
            Self::GpioCurrentLimit => ErcRuleDetails {
                id: "erc.gpio.current_limit_exceeded",
                explanation: "GPIO pins have strict current limits; exceeding them can damage the controller.",
                fix_hint: Some(
                    "Use a transistor, MOSFET, or buffer IC instead of driving the load directly.",
                ),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErcAutoFix {
    AddLedSeriesResistor { component_id: u64 },
    AddI2cPullups { component_id: u64 },
    AddRelayFlybackDiode { component_id: u64 },
    AddGpioDriverNote { component_id: u64 },
    AddLevelShifterNote { component_id: u64 },
}

impl ErcViolation {
    pub(crate) fn details(&self) -> ErcRuleDetails {
        self.rule.details()
    }

    pub(crate) fn rule_id(&self) -> &'static str {
        self.details().id
    }

    pub(crate) fn explanation(&self) -> &'static str {
        self.details().explanation
    }

    pub(crate) fn fix_hint(&self) -> Option<&'static str> {
        self.details().fix_hint
    }

    pub(crate) fn related(&self) -> ErcRelated {
        match (self.component_id, self.wire_id) {
            (Some(component_id), Some(wire_id)) => ErcRelated::ComponentAndWire {
                component_id,
                wire_id,
            },
            (Some(component_id), None) => ErcRelated::Component(component_id),
            (None, Some(wire_id)) => ErcRelated::Wire(wire_id),
            (None, None) => ErcRelated::Schematic,
        }
    }

    pub(crate) fn certainty(&self) -> ErcCertainty {
        match self.rule {
            ErcRule::PowerShort
            | ErcRule::NoConnectWired
            | ErcRule::OutputConflict
            | ErcRule::GpioDirectLoad
            | ErcRule::GpioOvervoltage5v
            | ErcRule::Rail5vTo3v3
            | ErcRule::PowerRailConflict
            | ErcRule::AdcOvervoltage
            | ErcRule::DuplicateReference => ErcCertainty::Definite,
            ErcRule::LedSeriesResistorMissing
            | ErcRule::RelayFlybackMissing
            | ErcRule::I2cPullupMissing
            | ErcRule::LedReversed
            | ErcRule::DiodeReversed
            | ErcRule::MissingDecouplingCapacitor => ErcCertainty::Likely,
            _ => ErcCertainty::Advisory,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn fix_suggestion(&self) -> Option<&'static str> {
        self.fix_hint()
    }

    pub(crate) fn auto_fix(&self) -> Option<ErcAutoFix> {
        let component_id = self.component_id?;
        match self.rule {
            ErcRule::LedSeriesResistorMissing => {
                Some(ErcAutoFix::AddLedSeriesResistor { component_id })
            }
            ErcRule::I2cPullupMissing => Some(ErcAutoFix::AddI2cPullups { component_id }),
            ErcRule::RelayFlybackMissing => Some(ErcAutoFix::AddRelayFlybackDiode { component_id }),
            ErcRule::GpioDirectLoad => Some(ErcAutoFix::AddGpioDriverNote { component_id }),
            ErcRule::GpioOvervoltage5v | ErcRule::Rail5vTo3v3 | ErcRule::PowerRailConflict => {
                Some(ErcAutoFix::AddLevelShifterNote { component_id })
            }
            _ => None,
        }
    }
}

pub(crate) fn validate_beginner_rules(netlist: &CircuitNetlist) -> Vec<ErcViolation> {
    use crate::engine::erc::ErcDependency;
    validate_beginner_rules_for(netlist, &[ErcDependency::Topology, ErcDependency::Values])
}

pub(crate) fn validate_beginner_rules_for(
    netlist: &CircuitNetlist,
    dependencies: &[crate::engine::erc::ErcDependency],
) -> Vec<ErcViolation> {
    use crate::engine::erc::{ErcContext, ErcDependency, ErcRegistry, ErcSettings, FunctionRule};

    let mut registry = ErcRegistry::default();
    registry.register(FunctionRule::new(
        "annotation.duplicate_reference",
        crate::engine::erc::rules::annotation::check_duplicate_references,
    ));
    registry.register(FunctionRule::new(
        "net.duplicate_named",
        crate::engine::erc::rules::annotation::check_duplicate_named_nets,
    ));
    registry.register(FunctionRule::new(
        "pin.no_connect",
        crate::engine::erc::rules::annotation::check_no_connect_pins,
    ));
    registry.register(FunctionRule::new(
        "power.ground",
        crate::engine::erc::rules::power::check_missing_ground,
    ));
    registry.register(FunctionRule::new("power.short", check_power_gnd_short));
    registry.register(FunctionRule::new(
        "led.series_resistor",
        check_led_without_resistor,
    ));
    registry.register(FunctionRule::new("led.polarity", check_reversed_led));
    registry.register(FunctionRule::new("diode.polarity", check_reversed_diode));
    registry.register(FunctionRule::new("power.5v_3v3", check_5v_on_3v3));
    registry.register(FunctionRule::new(
        "gpio.direct_load",
        check_gpio_direct_loads,
    ));
    registry.register(FunctionRule::new(
        "gpio.overvoltage",
        check_esp_gpio_overvoltage,
    ));
    registry.register(FunctionRule::new(
        "power.rail_conflict",
        check_power_rail_conflicts,
    ));
    registry.register(FunctionRule::new(
        "inductive.flyback",
        check_relay_flyback_diodes,
    ));
    registry.register(FunctionRule::new("i2c.pullup", check_i2c_pullups));
    registry.register(
        FunctionRule::new("i2c.pullup_value", check_i2c_pullup_values)
            .with_dependency(ErcDependency::Values),
    );
    registry.register(FunctionRule::new(
        "i2c.signal_mapping",
        check_oled_sda_scl_swap,
    ));
    registry.register(FunctionRule::new(
        "uart.signal_mapping",
        check_uart_tx_rx_swap,
    ));
    registry.register(FunctionRule::new(
        "spi.signal_mapping",
        check_spi_signal_mismatch,
    ));
    registry.register(FunctionRule::new("adc.overvoltage", check_adc_overvoltage));
    registry.register(FunctionRule::new("i2c.address", check_i2c_address_conflict));
    registry.register(
        FunctionRule::new("value.required", check_missing_values)
            .with_dependency(ErcDependency::Values),
    );
    registry.register(FunctionRule::new(
        "connectivity.floating_pin",
        check_floating_pins,
    ));
    registry.register(FunctionRule::new(
        "connectivity.floating_dc",
        check_floating_dc_nets,
    ));
    registry.register(FunctionRule::new(
        "power.open_source",
        check_open_voltage_sources,
    ));
    registry.register(FunctionRule::new(
        "simulation.support",
        check_symbolic_components,
    ));
    registry.register(FunctionRule::new(
        "logic.output_conflict",
        check_output_output_conflict,
    ));
    registry.register(FunctionRule::new(
        "power.input_drive",
        check_power_input_not_driven,
    ));
    registry.register(FunctionRule::new(
        "logic.unconnected_input",
        check_unconnected_input_pins,
    ));
    registry.register(FunctionRule::new(
        "net.remote_label",
        check_net_label_remote_short,
    ));
    registry.register(
        FunctionRule::new("regulator.range", check_regulator_voltage_range)
            .with_dependency(ErcDependency::Values),
    );
    registry.register(FunctionRule::new(
        "power.decoupling",
        check_missing_decoupling_caps,
    ));
    registry.run_dependencies(
        &ErcContext { netlist },
        &ErcSettings::default(),
        dependencies,
    )
}

/// ERC rules that require solved DC operating-point results.
///
/// Pass the same `CircuitNetlist` used for simulation and the `DcResult`
/// from the MNA solver.  Returns additional violations beyond the static rules.
#[allow(dead_code)]
pub(crate) fn validate_dc_rules(
    netlist: &CircuitNetlist,
    dc: &crate::engine::mna::DcResult,
) -> Vec<ErcViolation> {
    let mut v = Vec::new();
    check_resistor_wattage(netlist, dc, &mut v);
    check_led_current_limit(netlist, dc, &mut v);
    check_gpio_current_limit(netlist, dc, &mut v);
    v
}

// ─── Individual rule implementations ────────────────────────────────────────

fn check_power_gnd_short(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        let pins: Vec<&NetlistPin> = netlist.pins.iter().filter(|p| p.net_id == net.id).collect();

        let power_pin = pins.iter().find(|p| pin_is_power_positive(p));
        let gnd_pin = pins.iter().find(|p| {
            p.electrical_type == ElectricalType::Ground || p.component_kind == ComponentKind::Ground
        });

        if let (Some(pwr), Some(gnd)) = (power_pin, gnd_pin) {
            let wire_id = netlist
                .wire_nets
                .iter()
                .find_map(|(wire_id, net_id)| (*net_id == net.id).then_some(*wire_id));
            v.push(ErcViolation {
                rule: ErcRule::PowerShort,
                severity: ErcSeverity::Error,
                component_id: Some(pwr.component_id),
                wire_id,
                message: format!(
                    "Power net conflict: short risk between {} {} and GND ({}).",
                    pwr.component_label, pwr.pin_name, gnd.component_label
                ),
            });
        }
    }
}

fn check_led_without_resistor(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for led in netlist
        .pins
        .iter()
        .filter(|p| p.component_kind == ComponentKind::Led && p.pin_name == "A")
    {
        let led_nets: std::collections::HashSet<usize> = netlist
            .pins
            .iter()
            .filter(|p| p.component_id == led.component_id)
            .map(|p| p.net_id)
            .collect();

        let has_resistor = netlist
            .pins
            .iter()
            .any(|p| p.component_kind == ComponentKind::Resistor && led_nets.contains(&p.net_id));
        if !has_resistor {
            v.push(ErcViolation {
                rule: ErcRule::LedSeriesResistorMissing,
                severity: ErcSeverity::Warning,
                component_id: Some(led.component_id),
                wire_id: None,
                message: format!(
                    "LED {} has no current limiting resistor on either terminal.",
                    led.component_label
                ),
            });
        }
    }
}

fn check_reversed_led(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        if net.name != "GND" {
            continue;
        }
        let pins: Vec<&NetlistPin> = netlist.pins.iter().filter(|p| p.net_id == net.id).collect();
        for pin in &pins {
            if pin.component_kind == ComponentKind::Led && pin.pin_name == "A" {
                v.push(ErcViolation {
                    rule: ErcRule::LedReversed,
                    severity: ErcSeverity::Error,
                    component_id: Some(pin.component_id),
                    wire_id: None,
                    message: format!(
                        "LED {} anode (A) is connected to GND — LED is reversed.",
                        pin.component_label
                    ),
                });
            }
        }
    }
}

fn check_reversed_diode(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    let diode_kinds = [
        ComponentKind::Diode,
        ComponentKind::SchottkyDiode,
        ComponentKind::ZenerDiode,
    ];
    for net in &netlist.nets {
        if net.name != "GND" {
            continue;
        }
        let pins: Vec<&NetlistPin> = netlist.pins.iter().filter(|p| p.net_id == net.id).collect();
        for pin in &pins {
            if diode_kinds.contains(&pin.component_kind) && pin.pin_name == "A" {
                // Anode on GND net — likely reversed if cathode is on a higher potential net.
                let cathode_net = netlist
                    .pins
                    .iter()
                    .find(|p| {
                        p.component_id == pin.component_id
                            && (p.pin_name == "K" || p.pin_name == "B")
                    })
                    .map(|p| p.net_id);
                let cathode_is_power = cathode_net.is_some_and(|nid| {
                    netlist
                        .pins
                        .iter()
                        .any(|p| p.net_id == nid && pin_is_power_positive(p))
                });
                if cathode_is_power {
                    v.push(ErcViolation {
                        rule: ErcRule::DiodeReversed,
                        severity: ErcSeverity::Error,
                        component_id: Some(pin.component_id),
                        wire_id: None,
                        message: format!(
                            "Diode {} appears reversed (anode at GND, cathode at power).",
                            pin.component_label
                        ),
                    });
                }
            }
        }
    }
}

fn check_5v_on_3v3(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        let pins: Vec<&NetlistPin> = netlist.pins.iter().filter(|p| p.net_id == net.id).collect();
        let has_5v = pins.iter().any(|p| pin_is_5v_source(p));
        let target_3v3 = pins.iter().find(|p| pin_name_is_3v3(&p.pin_name));
        if has_5v && let Some(target) = target_3v3 {
            v.push(ErcViolation {
                rule: ErcRule::Rail5vTo3v3,
                severity: ErcSeverity::Error,
                component_id: Some(target.component_id),
                wire_id: None,
                message: format!("{} connects 5V to a 3.3V rail/pin.", net.name),
            });
        }
    }
}

fn check_gpio_direct_loads(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    let direct_loads = [
        ComponentKind::Led,
        ComponentKind::DcMotor,
        ComponentKind::Relay,
        ComponentKind::Servo,
        ComponentKind::Lamp,
    ];
    let mut seen = HashSet::new();
    for net in &netlist.nets {
        let pins = pins_on_net(netlist, net.id);
        let gpios = pins
            .iter()
            .filter(|pin| pin_is_microcontroller_gpio(pin))
            .copied()
            .collect::<Vec<_>>();
        for load in pins.iter().filter(|pin| {
            direct_loads.contains(&pin.component_kind)
                && match pin.component_kind {
                    ComponentKind::Relay => pin.pin_name == "COIL+" || pin.pin_name == "COIL-",
                    ComponentKind::Servo => pin.pin_name == "VCC",
                    _ => true,
                }
        }) {
            if load.component_kind == ComponentKind::Led
                && net_has_component_kind(netlist, net.id, ComponentKind::Resistor)
            {
                continue;
            }
            for gpio in &gpios {
                if !seen.insert((gpio.component_id, load.component_id)) {
                    continue;
                }
                let metadata = electrical_metadata(load.component_kind);
                let fix = if metadata.needs_current_limit {
                    "Add a 220 ohm-1 kohm resistor in series."
                } else {
                    "Use a transistor, MOSFET, relay driver, or motor driver with a separate supply."
                };
                v.push(ErcViolation {
                    rule: ErcRule::GpioDirectLoad,
                    severity: ErcSeverity::Error,
                    component_id: Some(gpio.component_id),
                    wire_id: None,
                    message: format!(
                        "{} {} directly drives {} {}. GPIO current is limited and the load can damage the controller. {}",
                        gpio.component_label,
                        gpio.pin_name,
                        component_kind_short(load.component_kind),
                        load.component_label,
                        fix
                    ),
                });
            }
        }
    }
}

fn check_esp_gpio_overvoltage(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        let pins = pins_on_net(netlist, net.id);
        if !pins.iter().any(|pin| pin_is_5v_source(pin)) {
            continue;
        }
        for gpio in pins.iter().filter(|pin| {
            matches!(
                pin.component_kind,
                ComponentKind::Esp32
                    | ComponentKind::Esp32S3
                    | ComponentKind::Esp32C3
                    | ComponentKind::RaspberryPiPico
                    | ComponentKind::Stm32BluePill
                    | ComponentKind::Stm32Nucleo64
            ) && pin_is_microcontroller_gpio(pin)
        }) {
            v.push(ErcViolation {
                rule: ErcRule::GpioOvervoltage5v,
                severity: ErcSeverity::Error,
                component_id: Some(gpio.component_id),
                wire_id: None,
                message: format!(
                    "{} {} is connected to a 5V net. The GPIO is not 5V tolerant; use a level shifter or resistor divider.",
                    gpio.component_label, gpio.pin_name
                ),
            });
        }
    }
}

fn check_power_rail_conflicts(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        let pins = pins_on_net(netlist, net.id);
        let has_5v = pins
            .iter()
            .any(|pin| pin.pin_name.eq_ignore_ascii_case("5V") || pin_is_5v_source(pin));
        let rail_3v3 = pins.iter().find(|pin| pin_name_is_3v3(&pin.pin_name));
        if has_5v && let Some(rail) = rail_3v3 {
            v.push(ErcViolation {
                    rule: ErcRule::PowerRailConflict,
                    severity: ErcSeverity::Error,
                    component_id: Some(rail.component_id),
                    wire_id: None,
                    message: format!(
                        "{} ties a 3.3V rail to 5V. Separate the rails or add a regulator/level shifter.",
                        net.name
                    ),
                });
        }
    }
}

fn check_relay_flyback_diodes(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    let relay_ids = component_ids_of_kind(netlist, ComponentKind::Relay);
    for relay_id in relay_ids {
        let coil_nets = component_pin_nets(netlist, relay_id, &["COIL+", "COIL-"]);
        let [Some(coil_pos), Some(coil_neg)] = coil_nets.as_slice() else {
            continue;
        };
        let has_diode = netlist.pins.iter().any(|pin| {
            matches!(
                pin.component_kind,
                ComponentKind::Diode
                    | ComponentKind::SchottkyDiode
                    | ComponentKind::ZenerDiode
                    | ComponentKind::TvsDiode
            ) && {
                let diode_nets = component_net_ids(netlist, pin.component_id);
                diode_nets.contains(coil_pos) && diode_nets.contains(coil_neg)
            }
        });
        if !has_diode {
            let label = netlist
                .pins
                .iter()
                .find(|pin| pin.component_id == relay_id)
                .map(|pin| pin.component_label.as_str())
                .unwrap_or("relay");
            v.push(ErcViolation {
                rule: ErcRule::RelayFlybackMissing,
                severity: ErcSeverity::Warning,
                component_id: Some(relay_id),
                wire_id: None,
                message: format!(
                    "Relay {label} has no flyback diode across COIL+ and COIL-. Add a reverse-biased diode to clamp the turn-off voltage spike."
                ),
            });
        }
    }
}

fn check_i2c_pullups(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for signal in ["SDA", "SCL"] {
        let nets = netlist
            .pins
            .iter()
            .filter(|pin| pin.pin_name.to_ascii_uppercase().contains(signal))
            .map(|pin| pin.net_id)
            .collect::<HashSet<_>>();
        for net_id in nets {
            if net_has_pullup(netlist, net_id) {
                continue;
            }
            let Some(pin) = netlist.pins.iter().find(|pin| {
                pin.net_id == net_id && pin.pin_name.to_ascii_uppercase().contains(signal)
            }) else {
                continue;
            };
            v.push(ErcViolation {
                rule: ErcRule::I2cPullupMissing,
                severity: ErcSeverity::Warning,
                component_id: Some(pin.component_id),
                wire_id: None,
                message: format!(
                    "I2C {signal} has no pull-up resistor. Add about 2.2k-10k to the controller logic rail unless the module already includes pull-ups."
                ),
            });
        }
    }
}

fn check_symbolic_components(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    let mut seen = HashSet::new();
    for pin in &netlist.pins {
        if !seen.insert(pin.component_id)
            || matches!(
                pin.component_kind,
                ComponentKind::TextNote | ComponentKind::NetLabel
            )
        {
            continue;
        }
        let meta = electrical_metadata(pin.component_kind);
        // Every message is prefixed with `[<badge>]` using the same label
        // text as the inspector's SimulationSupport pill, so the ERC panel
        // and inspector always agree on the wording for a given support
        // level. `render_violation_row` in the UI strips this prefix back
        // out to render it as a small chip instead of plain text.
        let badge = meta.simulation.label();
        let (severity, msg) = match meta.simulation {
            SimulationSupport::SymbolOnly => (
                ErcSeverity::Info,
                format!(
                    "[{badge}] {} {} is a symbol only — no voltages or currents are computed for this part.",
                    component_kind_short(pin.component_kind),
                    pin.component_label
                ),
            ),
            SimulationSupport::DigitalOnly => (
                ErcSeverity::Info,
                format!(
                    "[{badge}] {} {} is a digital logic element — analogue DC currents are not modelled.",
                    component_kind_short(pin.component_kind),
                    pin.component_label
                ),
            ),
            SimulationSupport::Unsupported => (
                ErcSeverity::Info,
                format!(
                    "[{badge}] {} {} is not simulated. Connectivity and ERC are checked; \
                     use ngspice for voltage/current results.",
                    component_kind_short(pin.component_kind),
                    pin.component_label
                ),
            ),
            SimulationSupport::ApproximateDc => (
                ErcSeverity::Info,
                format!(
                    "[{badge}] {} {} uses an approximate DC model ({}). \
                     Export to ngspice for accurate results.",
                    component_kind_short(pin.component_kind),
                    pin.component_label,
                    meta.model_name
                ),
            ),
            SimulationSupport::ExactDc => continue,
        };
        v.push(ErcViolation {
            rule: ErcRule::SimulationSupportLimited,
            severity,
            component_id: Some(pin.component_id),
            wire_id: None,
            message: msg,
        });
    }
}

fn check_oled_sda_scl_swap(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        let pins: Vec<&NetlistPin> = netlist.pins.iter().filter(|p| p.net_id == net.id).collect();

        let oled_sda = pins.iter().any(|p| {
            p.component_kind == ComponentKind::Oled && p.pin_name.eq_ignore_ascii_case("SDA")
        });
        let oled_scl = pins.iter().any(|p| {
            p.component_kind == ComponentKind::Oled && p.pin_name.eq_ignore_ascii_case("SCL")
        });

        if oled_sda
            && pins
                .iter()
                .any(|p| pin_is_controller_scl(p) && !pin_is_controller_sda(p))
        {
            v.push(ErcViolation {
                rule: ErcRule::I2cSdaSclMismatch,
                severity: ErcSeverity::Error,
                component_id: pins
                    .iter()
                    .find(|p| p.component_kind == ComponentKind::Oled)
                    .map(|p| p.component_id),
                wire_id: None,
                message: "OLED SDA is connected to a controller SCL pin.".to_string(),
            });
        }
        if oled_scl
            && pins
                .iter()
                .any(|p| pin_is_controller_sda(p) && !pin_is_controller_scl(p))
        {
            v.push(ErcViolation {
                rule: ErcRule::I2cSdaSclMismatch,
                severity: ErcSeverity::Error,
                component_id: pins
                    .iter()
                    .find(|p| p.component_kind == ComponentKind::Oled)
                    .map(|p| p.component_id),
                wire_id: None,
                message: "OLED SCL is connected to a controller SDA pin.".to_string(),
            });
        }
    }
}

fn check_uart_tx_rx_swap(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    let mut reported = HashSet::new();
    for net in &netlist.nets {
        let pins = pins_on_net(netlist, net.id);
        let tx_pins = pins
            .iter()
            .filter(|pin| pin_is_uart_tx(pin))
            .copied()
            .collect::<Vec<_>>();
        let rx_pins = pins
            .iter()
            .filter(|pin| pin_is_uart_rx(pin))
            .copied()
            .collect::<Vec<_>>();

        if tx_pins.len() >= 2 {
            let labels = tx_pins
                .iter()
                .map(|pin| format!("{}.{}", pin.component_label, pin.pin_name))
                .collect::<Vec<_>>();
            let Some(first) = tx_pins.first() else {
                continue;
            };
            if reported.insert(("tx", net.id)) {
                v.push(ErcViolation {
                    rule: ErcRule::UartTxRxMismatch,
                    severity: ErcSeverity::Error,
                    component_id: Some(first.component_id),
                    wire_id: first_wire_on_net(netlist, net.id),
                    message: format!(
                        "UART TX/TX mismatch on {}: {} are tied together. Connect TX to the other device RX.",
                        net.name,
                        labels.join(", ")
                    ),
                });
            }
        }

        if rx_pins.len() >= 2 {
            let labels = rx_pins
                .iter()
                .map(|pin| format!("{}.{}", pin.component_label, pin.pin_name))
                .collect::<Vec<_>>();
            let Some(first) = rx_pins.first() else {
                continue;
            };
            if reported.insert(("rx", net.id)) {
                v.push(ErcViolation {
                    rule: ErcRule::UartTxRxMismatch,
                    severity: ErcSeverity::Warning,
                    component_id: Some(first.component_id),
                    wire_id: first_wire_on_net(netlist, net.id),
                    message: format!(
                        "UART RX/RX mismatch on {}: {} are tied together. Connect one device TX to the other device RX.",
                        net.name,
                        labels.join(", ")
                    ),
                });
            }
        }
    }
}

fn check_spi_signal_mismatch(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        let pins = pins_on_net(netlist, net.id);
        let mut roles: HashMap<&'static str, Vec<&NetlistPin>> = HashMap::new();
        for pin in pins {
            if let Some(role) = spi_role(pin) {
                roles.entry(role).or_default().push(pin);
            }
        }
        if roles.len() <= 1 {
            continue;
        }

        let labels = roles
            .iter()
            .flat_map(|(role, pins)| {
                pins.iter()
                    .map(move |pin| format!("{role}:{}.{}", pin.component_label, pin.pin_name))
            })
            .collect::<Vec<_>>();
        let first = roles.values().flatten().next().copied();
        v.push(ErcViolation {
            rule: ErcRule::SpiSignalMismatch,
            severity: ErcSeverity::Error,
            component_id: first.map(|pin| pin.component_id),
            wire_id: first_wire_on_net(netlist, net.id),
            message: format!(
                "SPI signal mismatch on {}: {} share one net. Keep MOSI, MISO, SCK, and CS/SS on separate matching nets.",
                net.name,
                labels.join(", ")
            ),
        });
    }
}

fn check_adc_overvoltage(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        let pins = pins_on_net(netlist, net.id);
        if !pins.iter().any(|pin| pin_is_5v_source(pin)) {
            continue;
        }
        for adc in pins.iter().filter(|pin| pin_is_adc_pin(pin)) {
            v.push(ErcViolation {
                rule: ErcRule::AdcOvervoltage,
                severity: ErcSeverity::Error,
                component_id: Some(adc.component_id),
                wire_id: first_wire_on_net(netlist, net.id),
                message: format!(
                    "{} {} ADC input is connected to a 5V net. Add a divider or level shifter so the ADC stays within its reference voltage.",
                    adc.component_label, adc.pin_name
                ),
            });
        }
    }
}

fn check_missing_decoupling_caps(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    let mut component_ids = netlist
        .pins
        .iter()
        .filter(|pin| {
            matches!(
                pin.component_kind,
                ComponentKind::Esp32
                    | ComponentKind::Esp32S3
                    | ComponentKind::Esp32C3
                    | ComponentKind::ArduinoUno
                    | ComponentKind::RaspberryPiPico
                    | ComponentKind::Stm32BluePill
                    | ComponentKind::Stm32Nucleo64
                    | ComponentKind::GenericIc
                    | ComponentKind::Timer555
            )
        })
        .map(|pin| pin.component_id)
        .collect::<Vec<_>>();
    component_ids.sort_unstable();
    component_ids.dedup();

    for component_id in component_ids {
        let pins = netlist
            .pins
            .iter()
            .filter(|pin| pin.component_id == component_id)
            .collect::<Vec<_>>();
        let Some(first) = pins.first() else { continue };
        let power_nets = pins
            .iter()
            .filter(|pin| {
                pin.electrical_type == ElectricalType::PowerIn
                    || pin_name_is_3v3(&pin.pin_name)
                    || pin.pin_name.eq_ignore_ascii_case("5V")
                    || pin.pin_name.eq_ignore_ascii_case("VCC")
                    || pin.pin_name.eq_ignore_ascii_case("VIN")
            })
            .map(|pin| pin.net_id)
            .collect::<HashSet<_>>();
        let ground_nets = pins
            .iter()
            .filter(|pin| {
                pin.electrical_type == ElectricalType::Ground
                    || pin.pin_name.eq_ignore_ascii_case("GND")
            })
            .map(|pin| pin.net_id)
            .collect::<HashSet<_>>();
        if power_nets.is_empty() || ground_nets.is_empty() {
            continue;
        }

        let has_cap = component_ids_of_kind(netlist, ComponentKind::Capacitor)
            .into_iter()
            .any(|cap_id| {
                let cap_nets = component_net_ids(netlist, cap_id);
                cap_nets.iter().any(|net| power_nets.contains(net))
                    && cap_nets.iter().any(|net| ground_nets.contains(net))
            });
        if !has_cap {
            v.push(ErcViolation {
                rule: ErcRule::MissingDecouplingCapacitor,
                severity: ErcSeverity::Warning,
                component_id: Some(component_id),
                wire_id: None,
                message: format!(
                    "{} {} has no decoupling capacitor between power and GND. Add a 100 nF ceramic capacitor close to the power pins.",
                    component_kind_short(first.component_kind),
                    first.component_label
                ),
            });
        }
    }
}

fn check_missing_values(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    let reported: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut seen = reported;

    for pin in &netlist.pins {
        if seen.contains(&pin.component_id) {
            continue;
        }
        match pin.component_kind {
            ComponentKind::Resistor => {
                if pin.component_value.trim().is_empty()
                    || parse_metric_value(&pin.component_value, "ohm").is_none()
                {
                    seen.insert(pin.component_id);
                    v.push(ErcViolation {
                        rule: ErcRule::MissingValue,
                        severity: ErcSeverity::Warning,
                        component_id: Some(pin.component_id),
                        wire_id: None,
                        message: format!(
                            "Resistor {} has no valid resistance value.",
                            pin.component_label
                        ),
                    });
                }
            }
            ComponentKind::Battery | ComponentKind::VSource
                if pin.component_value.trim().is_empty()
                    || parse_metric_value(&pin.component_value, "v").is_none() =>
            {
                seen.insert(pin.component_id);
                v.push(ErcViolation {
                    rule: ErcRule::MissingValue,
                    severity: ErcSeverity::Warning,
                    component_id: Some(pin.component_id),
                    wire_id: None,
                    message: format!(
                        "{} {} has no valid voltage value.",
                        if pin.component_kind == ComponentKind::Battery {
                            "Battery"
                        } else {
                            "Voltage source"
                        },
                        pin.component_label
                    ),
                });
            }
            _ => {}
        }
    }
}

fn check_floating_pins(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    let connected_component_ids: std::collections::HashSet<u64> = netlist
        .pins
        .iter()
        .filter(|p| p.connected_by_wire)
        .map(|p| p.component_id)
        .collect();

    // Warn about components with no connected pins at all (fully unconnected).
    let mut component_net_counts: std::collections::HashMap<u64, std::collections::HashSet<usize>> =
        std::collections::HashMap::new();
    for pin in &netlist.pins {
        component_net_counts
            .entry(pin.component_id)
            .or_default()
            .insert(pin.net_id);
    }

    // Warn about floating wires (no component connection on either end).
    for wire_id in &netlist.floating_wires {
        v.push(ErcViolation {
            rule: ErcRule::FloatingConnectivity,
            severity: ErcSeverity::Warning,
            component_id: None,
            wire_id: Some(*wire_id),
            message: "Wire is floating (not connected to any component pin).".to_string(),
        });
    }

    for wire_id in &netlist.isolated_wires {
        v.push(ErcViolation {
            rule: ErcRule::FloatingConnectivity,
            severity: ErcSeverity::Warning,
            component_id: None,
            wire_id: Some(*wire_id),
            message: "Wire segment is isolated: it connects to only one component pin.".to_string(),
        });
    }

    let mut reported_components = std::collections::HashSet::new();
    for pin in &netlist.pins {
        if !connected_component_ids.contains(&pin.component_id)
            && !matches!(
                pin.component_kind,
                ComponentKind::Ground | ComponentKind::NetLabel
            )
            && reported_components.insert(pin.component_id)
        {
            v.push(ErcViolation {
                rule: ErcRule::FloatingConnectivity,
                severity: ErcSeverity::Warning,
                component_id: Some(pin.component_id),
                wire_id: None,
                message: format!(
                    "{} {} has no connected wires.",
                    component_kind_short(pin.component_kind),
                    pin.component_label
                ),
            });
        } else if !pin.connected_by_wire
            && !pin.no_connect
            && matches!(
                pin.electrical_type,
                ElectricalType::Digital | ElectricalType::I2c | ElectricalType::Control
            )
        {
            v.push(ErcViolation {
                rule: ErcRule::FloatingConnectivity,
                severity: ErcSeverity::Warning,
                component_id: Some(pin.component_id),
                wire_id: None,
                message: format!(
                    "{} {} input/pin {} is floating.",
                    component_kind_short(pin.component_kind),
                    pin.component_label,
                    pin.pin_name
                ),
            });
        }
    }
}

fn check_floating_dc_nets(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    let ground_nets = netlist
        .pins
        .iter()
        .filter(|pin| {
            pin.electrical_type == ElectricalType::Ground
                || pin.component_kind == ComponentKind::Ground
        })
        .map(|pin| pin.net_id)
        .chain(
            netlist
                .nets
                .iter()
                .filter(|net| net.name.eq_ignore_ascii_case("GND"))
                .map(|net| net.id),
        )
        .collect::<HashSet<_>>();
    if ground_nets.is_empty() {
        return;
    }

    let graph = dc_net_graph(netlist, true);
    let referenced = reachable_net_ids(&graph, ground_nets.iter().copied());
    let wired_nets = netlist.wire_nets.values().copied().collect::<HashSet<_>>();

    for net in &netlist.nets {
        let pins_on_net = netlist
            .pins
            .iter()
            .filter(|pin| pin.net_id == net.id)
            .collect::<Vec<_>>();
        if referenced.contains(&net.id) || !wired_nets.contains(&net.id) || pins_on_net.is_empty() {
            continue;
        }
        let wire_id = netlist
            .wire_nets
            .iter()
            .filter(|(_, net_id)| **net_id == net.id)
            .map(|(wire_id, _)| *wire_id)
            .min();
        v.push(ErcViolation {
            rule: ErcRule::FloatingDcIsland,
            severity: ErcSeverity::Warning,
            component_id: pins_on_net.first().map(|pin| pin.component_id),
            wire_id,
            message: format!(
                "{} is a floating DC island with no conductive path to GND.",
                net.name
            ),
        });
    }
}

fn check_i2c_address_conflict(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    // Map I2C address → list of (component_id, label, kind) sharing that address
    // We identify I2C buses by finding nets that share SDA/SCL pins from the same controller.
    // For simplicity: if two Oled components are connected to the same SDA net, they conflict (0x3C).
    // Also check for same-kind I2C devices sharing any I2C net.
    use std::collections::HashMap as HM;

    // Collect which nets carry I2C signals
    let mut sda_nets: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for pin in &netlist.pins {
        if pin_is_i2c_named(&pin.pin_name) && pin.pin_name.to_uppercase().contains("SDA") {
            sda_nets.insert(pin.net_id);
        }
    }

    // For each I2C SDA net, collect I2C peripheral components
    let mut net_i2c_devices: HM<usize, Vec<(u64, &str, ComponentKind)>> = HM::new();
    for pin in &netlist.pins {
        let is_i2c_peripheral = matches!(
            pin.component_kind,
            ComponentKind::Oled | ComponentKind::Sensor
        ) && sda_nets.contains(&pin.net_id);
        if is_i2c_peripheral {
            net_i2c_devices.entry(pin.net_id).or_default().push((
                pin.component_id,
                pin.component_label.as_str(),
                pin.component_kind,
            ));
        }
    }

    for devices in net_i2c_devices.values() {
        // Deduplicate by component_id
        let mut unique: Vec<(u64, &str, ComponentKind)> = Vec::new();
        for &(id, label, kind) in devices {
            if !unique.iter().any(|(uid, _, _)| *uid == id) {
                unique.push((id, label, kind));
            }
        }
        // Check for multiple OLEDs on the same bus (both default to 0x3C)
        let oled_count = unique
            .iter()
            .filter(|(_, _, k)| *k == ComponentKind::Oled)
            .count();
        if oled_count >= 2 {
            let labels: Vec<&str> = unique
                .iter()
                .filter(|(_, _, k)| *k == ComponentKind::Oled)
                .map(|(_, l, _)| *l)
                .collect();
            v.push(ErcViolation {
                rule: ErcRule::I2cAddressConflict,
                severity: ErcSeverity::Error,
                component_id: unique.iter().find(|(_, _, k)| *k == ComponentKind::Oled).map(|(id, _, _)| *id),
                wire_id: None,
                message: format!(
                    "I2C address conflict: {} all use default address 0x3C. Add an address jumper or use separate buses.",
                    labels.join(", ")
                ),
            });
        }
    }
}

fn check_open_voltage_sources(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    let load_graph = dc_net_graph(netlist, false);
    let mut source_ids = netlist
        .pins
        .iter()
        .filter(|pin| {
            matches!(
                pin.component_kind,
                ComponentKind::Battery | ComponentKind::VSource
            )
        })
        .map(|pin| pin.component_id)
        .collect::<Vec<_>>();
    source_ids.sort_unstable();
    source_ids.dedup();

    for source_id in source_ids {
        let source_pins = netlist
            .pins
            .iter()
            .filter(|pin| pin.component_id == source_id)
            .collect::<Vec<_>>();
        let positive = source_pins
            .iter()
            .find(|pin| pin_is_power_positive(pin))
            .map(|pin| pin.net_id);
        let negative = source_pins
            .iter()
            .find(|pin| pin.electrical_type == ElectricalType::Ground || pin.pin_name == "-")
            .map(|pin| pin.net_id);
        let (Some(positive), Some(negative)) = (positive, negative) else {
            continue;
        };
        if reachable_net_ids(&load_graph, [positive]).contains(&negative) {
            continue;
        }
        let source = source_pins[0];
        let wire_id = netlist
            .wire_nets
            .iter()
            .filter(|(_, net_id)| **net_id == positive)
            .map(|(wire_id, _)| *wire_id)
            .min();
        v.push(ErcViolation {
            rule: ErcRule::OpenVoltageSource,
            severity: ErcSeverity::Warning,
            component_id: Some(source_id),
            wire_id,
            message: format!(
                "{} {} has no closed DC load path; source current is 0 A.",
                component_kind_short(source.component_kind),
                source.component_label
            ),
        });
    }
}

fn dc_net_graph(
    netlist: &CircuitNetlist,
    include_voltage_sources: bool,
) -> HashMap<usize, HashSet<usize>> {
    let mut component_pins: HashMap<u64, Vec<&NetlistPin>> = HashMap::new();
    for pin in &netlist.pins {
        component_pins
            .entry(pin.component_id)
            .or_default()
            .push(pin);
    }

    let mut graph: HashMap<usize, HashSet<usize>> = HashMap::new();
    for pins in component_pins.values() {
        let Some(first) = pins.first() else {
            continue;
        };
        if !component_can_form_dc_path(
            first.component_kind,
            &first.component_value,
            include_voltage_sources,
        ) {
            continue;
        }
        let Some((a, b)) = dc_terminal_nets(pins) else {
            continue;
        };
        if a == b {
            continue;
        }
        graph.entry(a).or_default().insert(b);
        graph.entry(b).or_default().insert(a);
    }
    graph
}

fn component_can_form_dc_path(
    kind: ComponentKind,
    value: &str,
    include_voltage_sources: bool,
) -> bool {
    match kind {
        ComponentKind::Battery | ComponentKind::VSource => include_voltage_sources,
        ComponentKind::Switch | ComponentKind::PushButton | ComponentKind::SlideSwitch => {
            let value = value.to_ascii_lowercase();
            !value.contains("open") && !value.contains("off")
        }
        ComponentKind::Resistor
        | ComponentKind::Inductor
        | ComponentKind::Diode
        | ComponentKind::Led
        | ComponentKind::ZenerDiode
        | ComponentKind::Lamp
        | ComponentKind::Potentiometer
        | ComponentKind::NpnTransistor
        | ComponentKind::PnpTransistor
        | ComponentKind::Nmosfet
        | ComponentKind::Pmosfet
        | ComponentKind::VoltageReg
        | ComponentKind::Fuse
        | ComponentKind::Relay
        | ComponentKind::DcMotor
        | ComponentKind::Transformer
        | ComponentKind::Thermistor
        | ComponentKind::Varistor
        | ComponentKind::SchottkyDiode
        | ComponentKind::TvsDiode
        | ComponentKind::Phototransistor
        | ComponentKind::Ammeter => true,
        _ => false,
    }
}

fn dc_terminal_nets(pins: &[&NetlistPin]) -> Option<(usize, usize)> {
    let by_name = |name: &str| {
        pins.iter()
            .find(|pin| pin.pin_name == name)
            .map(|pin| pin.net_id)
    };
    match pins.first()?.component_kind {
        ComponentKind::NpnTransistor | ComponentKind::PnpTransistor => {
            Some((by_name("C")?, by_name("E")?))
        }
        ComponentKind::Nmosfet | ComponentKind::Pmosfet => Some((by_name("D")?, by_name("S")?)),
        ComponentKind::Relay => Some((by_name("COIL+")?, by_name("COIL-")?)),
        ComponentKind::Battery | ComponentKind::VSource => Some((by_name("+")?, by_name("-")?)),
        _ => Some((pins.first()?.net_id, pins.get(1)?.net_id)),
    }
}

fn reachable_net_ids(
    graph: &HashMap<usize, HashSet<usize>>,
    starts: impl IntoIterator<Item = usize>,
) -> HashSet<usize> {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::new();
    for start in starts {
        if seen.insert(start) {
            queue.push_back(start);
        }
    }
    while let Some(net) = queue.pop_front() {
        if let Some(neighbors) = graph.get(&net) {
            for &neighbor in neighbors {
                if seen.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
    }
    seen
}

fn pins_on_net(netlist: &CircuitNetlist, net_id: usize) -> Vec<&NetlistPin> {
    netlist
        .pins
        .iter()
        .filter(|pin| pin.net_id == net_id)
        .collect()
}

fn net_has_component_kind(netlist: &CircuitNetlist, net_id: usize, kind: ComponentKind) -> bool {
    netlist
        .pins
        .iter()
        .any(|pin| pin.net_id == net_id && pin.component_kind == kind)
}

fn component_ids_of_kind(netlist: &CircuitNetlist, kind: ComponentKind) -> HashSet<u64> {
    netlist
        .pins
        .iter()
        .filter(|pin| pin.component_kind == kind)
        .map(|pin| pin.component_id)
        .collect()
}

fn component_net_ids(netlist: &CircuitNetlist, component_id: u64) -> HashSet<usize> {
    netlist
        .pins
        .iter()
        .filter(|pin| pin.component_id == component_id)
        .map(|pin| pin.net_id)
        .collect()
}

fn component_pin_nets(
    netlist: &CircuitNetlist,
    component_id: u64,
    names: &[&str],
) -> Vec<Option<usize>> {
    names
        .iter()
        .map(|name| {
            netlist
                .pins
                .iter()
                .find(|pin| pin.component_id == component_id && pin.pin_name == *name)
                .map(|pin| pin.net_id)
        })
        .collect()
}

fn net_has_pullup(netlist: &CircuitNetlist, signal_net: usize) -> bool {
    component_ids_of_kind(netlist, ComponentKind::Resistor)
        .into_iter()
        .any(|resistor_id| {
            let nets = component_net_ids(netlist, resistor_id);
            nets.contains(&signal_net)
                && nets.iter().any(|net_id| {
                    *net_id != signal_net
                        && netlist.pins.iter().any(|pin| {
                            pin.net_id == *net_id
                                && (pin.electrical_type == ElectricalType::PowerIn
                                    || pin_name_is_3v3(&pin.pin_name)
                                    || pin.pin_name.eq_ignore_ascii_case("5V"))
                        })
                })
        })
}

fn first_wire_on_net(netlist: &CircuitNetlist, net_id: usize) -> Option<u64> {
    netlist
        .wire_nets
        .iter()
        .filter(|(_, candidate_net)| **candidate_net == net_id)
        .map(|(wire_id, _)| *wire_id)
        .min()
}

fn pin_is_uart_tx(pin: &NetlistPin) -> bool {
    let name = normalized_pin_name(&pin.pin_name);
    name == "TX" || name == "TX0" || name.ends_with("TX") || name.contains("UARTTX")
}

fn pin_is_uart_rx(pin: &NetlistPin) -> bool {
    let name = normalized_pin_name(&pin.pin_name);
    name == "RX" || name == "RX0" || name.ends_with("RX") || name.contains("UARTRX")
}

fn spi_role(pin: &NetlistPin) -> Option<&'static str> {
    let name = normalized_pin_name(&pin.pin_name);
    if name.contains("MOSI") {
        Some("MOSI")
    } else if name.contains("MISO") {
        Some("MISO")
    } else if name.contains("SCK") || name.contains("SCLK") || name.contains("CLK") {
        Some("SCK")
    } else if name == "CS" || name.contains(" CS") || name.contains("SS") || name.ends_with("CS") {
        Some("CS")
    } else {
        None
    }
}

fn pin_is_adc_pin(pin: &NetlistPin) -> bool {
    pin_is_microcontroller(pin) && normalized_pin_name(&pin.pin_name).contains("ADC")
}

fn normalized_pin_name(name: &str) -> String {
    name.to_ascii_uppercase()
        .replace([' ', '_', '-', '/', '.'], "")
}

// ─── New ERC rules ───────────────────────────────────────────────────────────

/// Two driven Output pins on the same net will fight each other; report an error.
fn check_output_output_conflict(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        let drivers: Vec<&NetlistPin> = netlist
            .pins
            .iter()
            .filter(|pin| {
                pin.net_id == net.id
                    && pin.connected_by_wire
                    && pin.electrical_type.is_driver()
                    // Power supply outputs naturally share rails — only warn on logic outputs
                    && !matches!(
                        pin.electrical_type,
                        ElectricalType::PowerOutput | ElectricalType::OpenCollector
                    )
            })
            .collect();

        if drivers.len() >= 2 {
            let names: Vec<String> = drivers
                .iter()
                .map(|p| format!("{}.{}", p.component_label, p.pin_name))
                .collect();
            v.push(ErcViolation {
                rule: ErcRule::OutputConflict,
                severity: ErcSeverity::Error,
                component_id: drivers.first().map(|p| p.component_id),
                wire_id: None,
                message: format!(
                    "Output conflict on {}: {} are all driven outputs on the same net. \
                     Only one driver per net is allowed.",
                    net.name,
                    names.join(", ")
                ),
            });
        }
    }
}

/// A PowerIn pin that shares a net with no PowerOutput means the rail has no supply.
fn check_power_input_not_driven(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        let pins_here: Vec<&NetlistPin> = netlist
            .pins
            .iter()
            .filter(|p| p.net_id == net.id && p.connected_by_wire)
            .collect();

        let has_power_in = pins_here
            .iter()
            .any(|p| p.electrical_type == ElectricalType::PowerIn);
        if !has_power_in {
            continue;
        }

        let has_supply = pins_here.iter().any(|p| {
            p.electrical_type == ElectricalType::PowerOutput
                // Batteries and ideal voltage sources are also valid supplies
                || matches!(
                    p.component_kind,
                    ComponentKind::VSource | ComponentKind::Battery | ComponentKind::VoltageReg
                )
                // A named power net (VCC, 3V3, 5V) that has a ground on the same schematic
                // is acceptable if its name implies a global power rail.
                || p.pin_name.eq_ignore_ascii_case("VCC")
                || pin_name_is_3v3(&p.pin_name)
                || p.pin_name.eq_ignore_ascii_case("5V")
        });

        if !has_supply {
            let consumer = pins_here
                .iter()
                .find(|p| p.electrical_type == ElectricalType::PowerIn);
            if let Some(pin) = consumer {
                v.push(ErcViolation {
                    rule: ErcRule::PowerInputUndriven,
                    severity: ErcSeverity::Warning,
                    component_id: Some(pin.component_id),
                    wire_id: None,
                    message: format!(
                        "Power-in pin {}.{} on net {} has no power source. \
                         Add a voltage source, regulator output, or power-flag net label.",
                        pin.component_label, pin.pin_name, net.name
                    ),
                });
            }
        }
    }
}

/// An Input pin with no wire connection has an undefined logic level — ERC warning.
fn check_unconnected_input_pins(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    let mut seen: HashSet<(u64, String)> = HashSet::new();
    for pin in &netlist.pins {
        if pin.electrical_type != ElectricalType::Input {
            continue;
        }
        if pin.no_connect || pin.connected_by_wire {
            continue;
        }
        let key = (pin.component_id, pin.pin_name.clone());
        if !seen.insert(key) {
            continue;
        }
        v.push(ErcViolation {
            rule: ErcRule::UnconnectedInput,
            severity: ErcSeverity::Warning,
            component_id: Some(pin.component_id),
            wire_id: None,
            message: format!(
                "Input pin {}.{} is unconnected. Floating inputs can cause undefined \
                 behavior. Connect to a signal, pull-up, or pull-down resistor, or add \
                 a No-Connect marker.",
                pin.component_label, pin.pin_name
            ),
        });
    }
}

// ─── New ERC rules added in refactor ─────────────────────────────────────────

/// I2C pull-up resistors exist but their values may be wrong.
/// Too high (> 10 kΩ): bus too slow / won't reach VOH.
/// Too low (< 1 kΩ at 3.3 V / < 1.5 kΩ at 5 V): excessive current draw.
fn check_i2c_pullup_values(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for signal in ["SDA", "SCL"] {
        let nets: HashSet<usize> = netlist
            .pins
            .iter()
            .filter(|pin| pin.pin_name.to_ascii_uppercase().contains(signal))
            .map(|pin| pin.net_id)
            .collect();
        for net_id in nets {
            for resistor_id in component_ids_of_kind(netlist, ComponentKind::Resistor) {
                let r_nets = component_net_ids(netlist, resistor_id);
                if !r_nets.contains(&net_id) {
                    continue;
                }
                let on_power = r_nets.iter().any(|nid| {
                    *nid != net_id
                        && netlist.pins.iter().any(|p| {
                            p.net_id == *nid
                                && (pin_name_is_3v3(&p.pin_name)
                                    || p.pin_name.eq_ignore_ascii_case("5V")
                                    || p.electrical_type == ElectricalType::PowerOutput)
                        })
                });
                if !on_power {
                    continue;
                }
                let Some(pin) = netlist.pins.iter().find(|p| p.component_id == resistor_id) else {
                    continue;
                };
                let ohms = parse_metric_value(&pin.component_value, "ohm");
                if let Some(r) = ohms {
                    if r > 10_000.0 {
                        v.push(ErcViolation {
                            rule: ErcRule::I2cPullupTooHigh,
                            severity: ErcSeverity::Warning,
                            component_id: Some(resistor_id),
                            wire_id: None,
                            message: format!(
                                "I2C {signal} pull-up {} ({}) is too high (> 10 kΩ). \
                                 Use 2.2 kΩ–4.7 kΩ for reliable bus operation.",
                                pin.component_label, pin.component_value
                            ),
                        });
                    } else if r < 1_000.0 {
                        v.push(ErcViolation {
                            rule: ErcRule::I2cPullupTooLow,
                            severity: ErcSeverity::Warning,
                            component_id: Some(resistor_id),
                            wire_id: None,
                            message: format!(
                                "I2C {signal} pull-up {} ({}) is too low (< 1 kΩ). \
                                 Excessive current through the pull-up; use 1 kΩ–10 kΩ.",
                                pin.component_label, pin.component_value
                            ),
                        });
                    }
                }
            }
        }
    }
}

/// Net labels with the same name on disconnected islands look like short circuits
/// to a reader but are intentional; report as Info so the designer is aware.
/// (Unintentional duplicates are caught by `check_duplicate_named_nets` as Error.)
fn check_net_label_remote_short(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    let mut seen_labels: HashMap<String, usize> = HashMap::new();
    for net in &netlist.nets {
        if net.name.starts_with("NET_") || net.name.eq_ignore_ascii_case("GND") {
            continue;
        }
        let key = net.name.to_ascii_uppercase();
        let label_count = netlist
            .pins
            .iter()
            .filter(|p| p.component_kind == ComponentKind::NetLabel && p.net_id == net.id)
            .count();
        if label_count >= 1 {
            *seen_labels.entry(key).or_default() += 1;
        }
    }
    for (label, count) in &seen_labels {
        if *count >= 2 {
            v.push(ErcViolation {
                rule: ErcRule::NetLabelRemoteJoin,
                severity: ErcSeverity::Info,
                component_id: None,
                wire_id: None,
                message: format!(
                    "Net label '{label}' connects {count} remote schematic locations. \
                     Verify this is intentional — an accidental duplicate creates a hidden short."
                ),
            });
        }
    }
}

/// Voltage regulator: check dropout (Vin must exceed Vout by ≥ 1.5 V) and
/// that the output is within the module voltage range if connected to a 3.3V MCU.
fn check_regulator_voltage_range(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for reg_id in component_ids_of_kind(netlist, ComponentKind::VoltageReg) {
        let reg_pin = netlist.pins.iter().find(|p| p.component_id == reg_id);
        let Some(reg_pin) = reg_pin else { continue };
        let reg_value = &reg_pin.component_value;
        let Some(vout) = parse_metric_value(reg_value, "v") else {
            continue;
        };

        // Find Vin net
        let vin_net = netlist
            .pins
            .iter()
            .find(|p| p.component_id == reg_id && p.pin_name == "IN")
            .map(|p| p.net_id);
        let Some(vin_net) = vin_net else { continue };

        // Look for a voltage source on the Vin net to determine its voltage
        for src_pin in netlist.pins.iter().filter(|p| {
            p.net_id == vin_net
                && matches!(
                    p.component_kind,
                    ComponentKind::VSource | ComponentKind::Battery
                )
                && (p.pin_name == "+" || p.electrical_type == ElectricalType::PowerOutput)
        }) {
            let Some(vin) = parse_metric_value(&src_pin.component_value, "v") else {
                continue;
            };
            let dropout = vin - vout;
            if dropout < 1.5 {
                v.push(ErcViolation {
                    rule: ErcRule::RegulatorVoltageRange,
                    severity: ErcSeverity::Warning,
                    component_id: Some(reg_id),
                    wire_id: None,
                    message: format!(
                        "Voltage regulator {} ({} output): Vin={:.1}V, dropout={:.2}V. \
                         Linear regulators typically need ≥ 1.5 V headroom. \
                         Consider a lower-dropout (LDO) regulator or raise Vin.",
                        reg_pin.component_label, reg_value, vin, dropout
                    ),
                });
            }
            if vin > 36.0 {
                v.push(ErcViolation {
                    rule: ErcRule::RegulatorVoltageRange,
                    severity: ErcSeverity::Warning,
                    component_id: Some(reg_id),
                    wire_id: None,
                    message: format!(
                        "Voltage regulator {} has Vin={:.1}V. \
                         Verify the regulator's maximum input voltage rating.",
                        reg_pin.component_label, vin
                    ),
                });
            }
        }
    }
}

/// Resistor dissipates more than its rated wattage (default 0.25 W).
/// Requires DC operating-point results.
fn check_resistor_wattage(
    netlist: &CircuitNetlist,
    dc: &crate::engine::mna::DcResult,
    v: &mut Vec<ErcViolation>,
) {
    for pin in netlist
        .pins
        .iter()
        .filter(|p| p.component_kind == ComponentKind::Resistor)
    {
        let Some(&power) = dc.component_power.get(&pin.component_id) else {
            continue;
        };
        // Default rating 0.25 W; parse from value string if it includes "W" annotation
        let rated = parse_metric_value(&pin.component_value, "w")
            .filter(|&r| r > 0.0)
            .unwrap_or(0.25);
        if power > rated as f64 * 0.9 {
            let severity = if power > rated as f64 * 1.1 {
                ErcSeverity::Error
            } else {
                ErcSeverity::Warning
            };
            v.push(ErcViolation {
                rule: ErcRule::ResistorWattage,
                severity,
                component_id: Some(pin.component_id),
                wire_id: None,
                message: format!(
                    "Resistor {} dissipates {:.0} mW (rated {:.0} mW). \
                     Use a higher-wattage resistor or reduce the current.",
                    pin.component_label,
                    power * 1000.0,
                    rated * 1000.0,
                ),
            });
        }
    }
}

/// LED forward current exceeds the 25 mA absolute maximum.
/// Requires DC operating-point results.
fn check_led_current_limit(
    netlist: &CircuitNetlist,
    dc: &crate::engine::mna::DcResult,
    v: &mut Vec<ErcViolation>,
) {
    let mut seen: HashSet<u64> = HashSet::new();
    for pin in netlist
        .pins
        .iter()
        .filter(|p| p.component_kind == ComponentKind::Led)
    {
        if !seen.insert(pin.component_id) {
            continue;
        }
        let Some(&current) = dc.branch_current.get(&pin.component_id) else {
            continue;
        };
        let i_abs = current.abs();
        if i_abs > 0.030 {
            v.push(ErcViolation {
                rule: ErcRule::LedCurrentLimit,
                severity: ErcSeverity::Error,
                component_id: Some(pin.component_id),
                wire_id: None,
                message: format!(
                    "LED {} forward current {:.1} mA exceeds 30 mA absolute maximum. \
                     Increase the series resistor value.",
                    pin.component_label,
                    i_abs * 1000.0,
                ),
            });
        } else if i_abs > 0.025 {
            v.push(ErcViolation {
                rule: ErcRule::LedCurrentLimit,
                severity: ErcSeverity::Warning,
                component_id: Some(pin.component_id),
                wire_id: None,
                message: format!(
                    "LED {} forward current {:.1} mA is near the 25 mA rated maximum. \
                     Consider increasing the series resistor.",
                    pin.component_label,
                    i_abs * 1000.0,
                ),
            });
        }
    }
}

/// MCU GPIO pin current exceeds the per-pin maximum (typically 12 mA for 3.3 V MCUs).
/// Requires DC operating-point results.
fn check_gpio_current_limit(
    netlist: &CircuitNetlist,
    dc: &crate::engine::mna::DcResult,
    v: &mut Vec<ErcViolation>,
) {
    for net in &netlist.nets {
        let pins = pins_on_net(netlist, net.id);
        let gpio_pins: Vec<&NetlistPin> = pins
            .iter()
            .filter(|p| pin_is_microcontroller_gpio(p))
            .copied()
            .collect();
        if gpio_pins.is_empty() {
            continue;
        }

        // For each component on this net, check if its current exceeds GPIO max
        let mut seen_comp: HashSet<u64> = HashSet::new();
        for comp_pin in &pins {
            if !seen_comp.insert(comp_pin.component_id) {
                continue;
            }
            let Some(&current) = dc.branch_current.get(&comp_pin.component_id) else {
                continue;
            };
            let i_abs = current.abs();
            let meta = electrical_metadata(comp_pin.component_kind);
            let max_i_gpio = 0.012_f64;

            if i_abs > max_i_gpio {
                for gpio in &gpio_pins {
                    let meta_gpio = electrical_metadata(gpio.component_kind);
                    let limit = meta_gpio.max_current.unwrap_or(0.012) as f64;
                    if i_abs > limit {
                        v.push(ErcViolation {
                            rule: ErcRule::GpioCurrentLimit,
                            severity: ErcSeverity::Error,
                            component_id: Some(gpio.component_id),
                            wire_id: None,
                            message: format!(
                                "{} {} GPIO pin {} drives {:.1} mA (limit ~{:.0} mA). \
                                 Use a driver transistor or buffer IC.",
                                component_kind_short(gpio.component_kind),
                                gpio.component_label,
                                gpio.pin_name,
                                i_abs * 1000.0,
                                limit * 1000.0,
                            ),
                        });
                    }
                }
            }
            let _ = meta;
        }
    }
}

// ─── Helper predicates ───────────────────────────────────────────────────────

fn pin_is_power_positive(pin: &NetlistPin) -> bool {
    matches!(
        pin.component_kind,
        ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource
    ) && (pin.electrical_type == ElectricalType::PowerIn
        || pin.pin_name == "+"
        || pin.pin_name.eq_ignore_ascii_case("POS"))
}

fn pin_is_5v_source(pin: &NetlistPin) -> bool {
    pin.pin_name.eq_ignore_ascii_case("5V")
        || (matches!(
            pin.component_kind,
            ComponentKind::VSource | ComponentKind::Battery
        ) && parse_metric_value(&pin.component_value, "v").is_some_and(|v| v > 3.6))
}

fn pin_name_is_3v3(name: &str) -> bool {
    let compact = name.to_ascii_uppercase().replace(['.', ' '], "");
    compact.contains("3V3") || compact.contains("3.3V")
}

pub(crate) fn pin_is_microcontroller_gpio(pin: &NetlistPin) -> bool {
    matches!(
        pin.component_kind,
        ComponentKind::Esp32
            | ComponentKind::Esp32S3
            | ComponentKind::Esp32C3
            | ComponentKind::ArduinoUno
            | ComponentKind::RaspberryPiPico
            | ComponentKind::Stm32BluePill
            | ComponentKind::Stm32Nucleo64
    ) && (pin.pin_name.to_ascii_uppercase().contains("GPIO")
        || pin.pin_name.to_ascii_uppercase().starts_with("GP")
        || pin.pin_name.to_ascii_uppercase().starts_with('D')
        || pin_name_is_stm32_port(&pin.pin_name))
}

pub(crate) fn pin_is_i2c_named(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("sda") || lower.contains("scl")
}

pub(crate) fn pin_is_controller_sda(pin: &NetlistPin) -> bool {
    pin_is_microcontroller(pin) && pin.pin_name.to_ascii_lowercase().contains("sda")
}

pub(crate) fn pin_is_controller_scl(pin: &NetlistPin) -> bool {
    pin_is_microcontroller(pin) && pin.pin_name.to_ascii_lowercase().contains("scl")
}

fn pin_is_microcontroller(pin: &NetlistPin) -> bool {
    matches!(
        pin.component_kind,
        ComponentKind::Esp32
            | ComponentKind::Esp32S3
            | ComponentKind::Esp32C3
            | ComponentKind::ArduinoUno
            | ComponentKind::RaspberryPiPico
            | ComponentKind::Stm32BluePill
            | ComponentKind::Stm32Nucleo64
    )
}

fn pin_name_is_stm32_port(name: &str) -> bool {
    let compact = normalized_pin_name(name);
    ["PA", "PB", "PC", "PD", "PE", "PF", "PG"]
        .iter()
        .any(|prefix| {
            compact.match_indices(prefix).any(|(index, _)| {
                compact[index + prefix.len()..]
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_digit())
            })
        })
}

fn component_kind_short(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Resistor => "Resistor",
        ComponentKind::Capacitor => "Capacitor",
        ComponentKind::Led => "LED",
        ComponentKind::Diode => "Diode",
        ComponentKind::Battery => "Battery",
        ComponentKind::VSource => "Voltage source",
        ComponentKind::ISource => "Current source",
        ComponentKind::Esp32 | ComponentKind::Esp32S3 | ComponentKind::Esp32C3 => "ESP32",
        ComponentKind::ArduinoUno => "Arduino",
        ComponentKind::RaspberryPiPico => "Pico",
        ComponentKind::Stm32BluePill | ComponentKind::Stm32Nucleo64 => "STM32",
        ComponentKind::Oled => "OLED",
        _ => "Component",
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pin(
        component_id: u64,
        label: &str,
        kind: ComponentKind,
        value: &str,
        pin_name: &str,
        net_id: usize,
        connected_by_wire: bool,
    ) -> NetlistPin {
        NetlistPin {
            component_id,
            component_label: label.to_string(),
            component_kind: kind,
            component_value: value.to_string(),
            pin_name: pin_name.to_string(),
            electrical_type: ElectricalType::Passive,
            position: egui::Pos2::ZERO,
            net_id,
            connected_by_wire,
            no_connect: false,
        }
    }

    fn make_net(id: usize, name: &str) -> Net {
        Net {
            id,
            name: name.to_string(),
            connected_pins: Vec::new(),
        }
    }

    fn netlist(nets: Vec<Net>, pins: Vec<NetlistPin>) -> CircuitNetlist {
        CircuitNetlist {
            nets,
            pins,
            wire_nets: Default::default(),
            wire_segments: Vec::new(),
            floating_wires: Vec::new(),
            isolated_wires: Vec::new(),
            explicit_junctions: Vec::new(),
            no_connects: Vec::new(),
        }
    }

    // ── LED without resistor ──────────────────────────────────────────────

    #[test]
    fn reports_led_without_resistor() {
        let nl = netlist(
            vec![make_net(0, "NET_001"), make_net(1, "GND")],
            vec![
                make_pin(1, "LED1", ComponentKind::Led, "red", "A", 0, true),
                make_pin(1, "LED1", ComponentKind::Led, "red", "K", 1, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(v.iter().any(|e| e.rule == ErcRule::LedSeriesResistorMissing
            && e.message.contains("current limiting resistor")));
    }

    // ── LED with resistor → no warning ───────────────────────────────────

    #[test]
    fn no_warning_when_led_has_resistor() {
        let nl = netlist(
            vec![make_net(0, "NET_001"), make_net(1, "GND")],
            vec![
                make_pin(1, "LED1", ComponentKind::Led, "red", "A", 0, true),
                make_pin(1, "LED1", ComponentKind::Led, "red", "K", 1, true),
                make_pin(2, "R1", ComponentKind::Resistor, "330", "A", 0, true),
                make_pin(2, "R1", ComponentKind::Resistor, "330", "B", 2, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(
            !v.iter()
                .any(|e| e.message.contains("current limiting resistor"))
        );
    }

    // ── Missing GND ───────────────────────────────────────────────────────

    #[test]
    fn reports_missing_gnd() {
        let nl = netlist(
            vec![make_net(0, "NET_001")],
            vec![
                make_pin(1, "R1", ComponentKind::Resistor, "1k", "A", 0, true),
                make_pin(1, "R1", ComponentKind::Resistor, "1k", "B", 0, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(
            v.iter()
                .any(|e| e.rule == ErcRule::MissingGround && e.message.contains("GND"))
        );
    }

    // ── Power-GND short ───────────────────────────────────────────────────

    #[test]
    fn reports_power_gnd_short() {
        let nl = netlist(
            vec![make_net(0, "GND")],
            vec![
                make_pin(1, "BAT1", ComponentKind::Battery, "9V", "+", 0, true),
                make_pin(2, "GND1", ComponentKind::Ground, "0V", "GND", 0, true),
            ],
        );
        // Mark battery + as PowerIn
        let mut nl2 = nl;
        nl2.pins[0].electrical_type = ElectricalType::PowerIn;
        nl2.pins[1].electrical_type = ElectricalType::Ground;
        let v = validate_beginner_rules(&nl2);
        assert!(v.iter().any(|e| e.rule == ErcRule::PowerShort
            && e.severity == ErcSeverity::Error
            && e.message.contains("short")));
    }

    // ── 5V on 3.3V pin ───────────────────────────────────────────────────

    #[test]
    fn reports_5v_on_3v3() {
        let nl = netlist(
            vec![make_net(0, "NET_001")],
            vec![
                make_pin(1, "ARD1", ComponentKind::ArduinoUno, "", "5V", 0, true),
                make_pin(2, "ESP1", ComponentKind::Esp32, "", "3V3", 0, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(
            v.iter()
                .any(|e| e.rule == ErcRule::Rail5vTo3v3 && e.severity == ErcSeverity::Error)
        );
    }

    #[test]
    fn reports_5v_on_stm32_gpio_and_adc() {
        let nl = netlist(
            vec![make_net(0, "NET_5V")],
            vec![
                make_pin(1, "ARD1", ComponentKind::ArduinoUno, "", "5V", 0, true),
                make_pin(
                    2,
                    "STM1",
                    ComponentKind::Stm32BluePill,
                    "",
                    "PB7 SDA",
                    0,
                    true,
                ),
                make_pin(
                    2,
                    "STM1",
                    ComponentKind::Stm32BluePill,
                    "",
                    "PA0 ADC",
                    0,
                    true,
                ),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(v.iter().any(|e| e.rule == ErcRule::GpioOvervoltage5v
            && e.message.contains("PB7 SDA")
            && e.message.contains("5V net")));
        assert!(v.iter().any(|e| e.rule == ErcRule::AdcOvervoltage
            && e.message.contains("PA0 ADC")
            && e.message.contains("ADC input")));
    }

    // ── Reversed LED ─────────────────────────────────────────────────────

    #[test]
    fn reports_reversed_led() {
        let nl = netlist(
            vec![make_net(0, "GND"), make_net(1, "VCC")],
            vec![
                // Anode (A) on GND net
                make_pin(1, "LED1", ComponentKind::Led, "red", "A", 0, true),
                make_pin(1, "LED1", ComponentKind::Led, "red", "K", 1, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(
            v.iter()
                .any(|e| e.rule == ErcRule::LedReversed && e.message.contains("reversed"))
        );
    }

    // ── Resistor without value ────────────────────────────────────────────

    #[test]
    fn reports_resistor_without_value() {
        let nl = netlist(
            vec![make_net(0, "GND"), make_net(1, "NET_001")],
            vec![
                make_pin(1, "R1", ComponentKind::Resistor, "", "A", 0, true),
                make_pin(1, "R1", ComponentKind::Resistor, "", "B", 1, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(
            v.iter()
                .any(|e| e.rule == ErcRule::MissingValue && e.message.contains("resistance value"))
        );
    }

    // ── Battery without voltage ───────────────────────────────────────────

    #[test]
    fn reports_battery_without_voltage() {
        let nl = netlist(
            vec![make_net(0, "GND"), make_net(1, "VCC")],
            vec![
                make_pin(1, "BAT1", ComponentKind::Battery, "", "+", 1, true),
                make_pin(1, "BAT1", ComponentKind::Battery, "", "-", 0, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(
            v.iter()
                .any(|e| e.rule == ErcRule::MissingValue && e.message.contains("voltage value"))
        );
    }

    // ── Floating wire ─────────────────────────────────────────────────────

    #[test]
    fn reports_floating_wire() {
        let mut nl = netlist(vec![make_net(0, "NET_001")], vec![]);
        nl.floating_wires.push(42);
        let v = validate_beginner_rules(&nl);
        assert!(
            v.iter()
                .any(|e| e.rule == ErcRule::FloatingConnectivity && e.wire_id == Some(42))
        );
    }

    #[test]
    fn reports_floating_dc_island() {
        let mut gnd = make_pin(1, "GND1", ComponentKind::Ground, "0V", "GND", 0, true);
        gnd.electrical_type = ElectricalType::Ground;
        let nl = CircuitNetlist {
            nets: vec![make_net(0, "GND"), make_net(1, "FLOAT")],
            pins: vec![
                gnd,
                make_pin(2, "R1", ComponentKind::Resistor, "1k", "A", 1, true),
            ],
            wire_nets: [(10, 0), (11, 1)].into_iter().collect(),
            wire_segments: Vec::new(),
            floating_wires: Vec::new(),
            isolated_wires: Vec::new(),
            explicit_junctions: Vec::new(),
            no_connects: Vec::new(),
        };

        let violations = validate_beginner_rules(&nl);
        assert!(
            violations
                .iter()
                .any(|violation| violation.rule == ErcRule::FloatingDcIsland
                    && violation.message.contains("floating DC island"))
        );
    }

    #[test]
    fn reports_voltage_source_without_closed_load_path() {
        let mut positive = make_pin(1, "BAT1", ComponentKind::Battery, "5V", "+", 1, true);
        positive.electrical_type = ElectricalType::PowerIn;
        let mut negative = make_pin(1, "BAT1", ComponentKind::Battery, "5V", "-", 0, false);
        negative.electrical_type = ElectricalType::Ground;
        let nl = CircuitNetlist {
            nets: vec![make_net(0, "GND"), make_net(1, "VCC")],
            pins: vec![positive, negative],
            wire_nets: [(10, 1)].into_iter().collect(),
            wire_segments: Vec::new(),
            floating_wires: Vec::new(),
            isolated_wires: vec![10],
            explicit_junctions: Vec::new(),
            no_connects: Vec::new(),
        };

        let violations = validate_beginner_rules(&nl);
        assert!(violations.iter().any(|violation| {
            violation.rule == ErcRule::OpenVoltageSource
                && violation.message.contains("no closed DC load path")
                && violation.message.contains("0 A")
        }));
    }

    // ── OLED SDA/SCL swap ────────────────────────────────────────────────

    #[test]
    fn reports_oled_sda_scl_swap() {
        let nl = netlist(
            vec![make_net(0, "NET_001")],
            vec![
                make_pin(1, "OLED1", ComponentKind::Oled, "0.96 I2C", "SDA", 0, true),
                make_pin(2, "ESP1", ComponentKind::Esp32, "", "GPIO22_SCL", 0, true),
            ],
        );
        let v = validate_beginner_rules(&nl);
        assert!(v.iter().any(|e| e.rule == ErcRule::I2cSdaSclMismatch
            && e.message.contains("SDA")
            && e.message.contains("SCL")));
    }

    #[test]
    fn reports_uart_tx_tx_swap() {
        let nl = netlist(
            vec![make_net(0, "UART_TX_BAD")],
            vec![
                make_pin(1, "ESP1", ComponentKind::Esp32, "", "TX0", 0, true),
                make_pin(
                    2,
                    "PICO1",
                    ComponentKind::RaspberryPiPico,
                    "",
                    "GP0 TX",
                    0,
                    true,
                ),
            ],
        );

        let violations = validate_beginner_rules(&nl);

        assert!(violations.iter().any(|violation| {
            violation.rule == ErcRule::UartTxRxMismatch
                && violation.severity == ErcSeverity::Error
                && violation.message.contains("UART TX/TX mismatch")
                && violation.component_id.is_some()
        }));
    }

    #[test]
    fn reports_spi_mosi_miso_mismatch() {
        let nl = netlist(
            vec![make_net(0, "SPI_BAD")],
            vec![
                make_pin(1, "ESP1", ComponentKind::Esp32, "", "GPIO23 MOSI", 0, true),
                make_pin(
                    2,
                    "ARD1",
                    ComponentKind::ArduinoUno,
                    "",
                    "D12 MISO",
                    0,
                    true,
                ),
            ],
        );

        let violations = validate_beginner_rules(&nl);

        assert!(violations.iter().any(|violation| {
            violation.rule == ErcRule::SpiSignalMismatch
                && violation.severity == ErcSeverity::Error
                && violation.message.contains("SPI signal mismatch")
                && violation.component_id.is_some()
        }));
    }

    #[test]
    fn reports_adc_overvoltage_from_5v_source() {
        let nl = netlist(
            vec![make_net(0, "ADC_BAD")],
            vec![
                make_pin(1, "BAT1", ComponentKind::Battery, "5V", "+", 0, true),
                make_pin(2, "ESP1", ComponentKind::Esp32, "", "GPIO34 ADC", 0, true),
            ],
        );

        let violations = validate_beginner_rules(&nl);

        assert!(violations.iter().any(|violation| {
            violation.rule == ErcRule::AdcOvervoltage
                && violation.severity == ErcSeverity::Error
                && violation.message.contains("ADC input")
                && violation.component_id == Some(2)
        }));
    }

    #[test]
    fn reports_missing_mcu_decoupling_capacitor() {
        let nl = netlist(
            vec![make_net(0, "3V3"), make_net(1, "GND")],
            vec![
                make_pin(1, "ESP1", ComponentKind::Esp32, "", "3V3", 0, true),
                make_pin(1, "ESP1", ComponentKind::Esp32, "", "GND", 1, true),
            ],
        );

        let violations = validate_beginner_rules(&nl);

        assert!(violations.iter().any(|violation| {
            violation.rule == ErcRule::MissingDecouplingCapacitor
                && violation.severity == ErcSeverity::Warning
                && violation.message.contains("decoupling capacitor")
                && violation.component_id == Some(1)
        }));
    }

    #[test]
    fn common_erc_violations_provide_repair_suggestions_and_auto_fixes() {
        let led = ErcViolation {
            rule: ErcRule::LedSeriesResistorMissing,
            severity: ErcSeverity::Warning,
            component_id: Some(7),
            wire_id: None,
            message: "LED LED1 has no current limiting resistor on either terminal.".to_string(),
        };
        assert!(
            led.fix_suggestion()
                .is_some_and(|suggestion| suggestion.contains("330 ohm"))
        );
        assert_eq!(
            led.auto_fix(),
            Some(ErcAutoFix::AddLedSeriesResistor { component_id: 7 })
        );
        assert_eq!(led.rule_id(), "erc.led.series_resistor_missing");
        assert_eq!(led.related(), ErcRelated::Component(7));
        assert!(led.explanation().contains("current limiting"));

        let i2c = ErcViolation {
            rule: ErcRule::I2cPullupMissing,
            severity: ErcSeverity::Warning,
            component_id: Some(9),
            wire_id: None,
            message: "I2C SDA has no pull-up resistor.".to_string(),
        };
        assert!(
            i2c.fix_suggestion()
                .is_some_and(|suggestion| suggestion.contains("4.7k"))
        );
        assert_eq!(
            i2c.auto_fix(),
            Some(ErcAutoFix::AddI2cPullups { component_id: 9 })
        );
        assert_eq!(i2c.rule_id(), "erc.i2c.pullup_missing");
        assert!(i2c.explanation().contains("open-drain"));
    }

    #[test]
    fn erc_metadata_exposes_rule_id_fix_hint_and_related_target() {
        let adc = ErcViolation {
            rule: ErcRule::AdcOvervoltage,
            severity: ErcSeverity::Error,
            component_id: Some(12),
            wire_id: Some(44),
            message: "ESP1 GPIO34 ADC input is connected to a 5V net.".to_string(),
        };

        assert_eq!(adc.rule_id(), "erc.adc.overvoltage");
        assert!(
            adc.fix_hint()
                .is_some_and(|hint| hint.contains("resistor divider"))
        );
        assert_eq!(
            adc.related(),
            ErcRelated::ComponentAndWire {
                component_id: 12,
                wire_id: 44,
            }
        );

        let generic = ErcViolation {
            rule: ErcRule::General,
            severity: ErcSeverity::Info,
            component_id: None,
            wire_id: None,
            message: "Custom validation note.".to_string(),
        };

        assert_eq!(generic.rule_id(), "erc.general");
        assert_eq!(generic.related(), ErcRelated::Schematic);
        assert!(generic.fix_hint().is_none());
    }

    #[test]
    fn reports_duplicate_component_reference_designators() {
        let nl = netlist(
            vec![make_net(0, "NET_001"), make_net(1, "NET_002")],
            vec![
                make_pin(1, "R1", ComponentKind::Resistor, "1k", "A", 0, false),
                make_pin(1, "R1", ComponentKind::Resistor, "1k", "B", 1, false),
                make_pin(2, "R1", ComponentKind::Resistor, "2k", "A", 0, false),
                make_pin(2, "R1", ComponentKind::Resistor, "2k", "B", 1, false),
            ],
        );

        let violations = validate_beginner_rules(&nl);

        assert!(violations.iter().any(|violation| {
            violation.rule == ErcRule::DuplicateReference
                && violation.severity == ErcSeverity::Error
                && violation.message.contains("Duplicate reference R1")
        }));
    }

    #[test]
    fn reports_duplicate_named_net_islands() {
        let nl = netlist(
            vec![make_net(0, "VCC"), make_net(1, "VCC")],
            vec![
                make_pin(1, "R1", ComponentKind::Resistor, "1k", "A", 0, true),
                make_pin(2, "R2", ComponentKind::Resistor, "1k", "A", 1, true),
            ],
        );

        let violations = validate_beginner_rules(&nl);

        assert!(violations.iter().any(|violation| {
            violation.rule == ErcRule::DuplicateNamedNet
                && violation.severity == ErcSeverity::Error
                && violation.message.contains("Duplicate net name VCC")
        }));
    }

    #[test]
    fn reports_connected_no_connect_pin() {
        let mut pin = make_pin(1, "U1", ComponentKind::GenericIc, "", "NC", 0, true);
        pin.no_connect = true;
        let nl = netlist(vec![make_net(0, "NET_001")], vec![pin]);

        let violations = validate_beginner_rules(&nl);

        assert!(violations.iter().any(|violation| {
            violation.rule == ErcRule::NoConnectWired
                && violation.severity == ErcSeverity::Error
                && violation.message.contains("marked no-connect")
        }));
    }
}
