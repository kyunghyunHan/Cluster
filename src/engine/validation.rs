use crate::model::*;
use crate::parse_metric_value;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ErcSeverity {
    Error,
    Warning,
    #[allow(dead_code)]
    Info,
}

#[derive(Debug, Clone)]
pub(crate) struct ErcViolation {
    pub(crate) severity: ErcSeverity,
    pub(crate) component_id: Option<u64>,
    pub(crate) wire_id: Option<u64>,
    pub(crate) message: String,
}

pub(crate) fn validate_beginner_rules(netlist: &CircuitNetlist) -> Vec<ErcViolation> {
    let mut v = Vec::new();

    check_missing_gnd(netlist, &mut v);
    check_power_gnd_short(netlist, &mut v);
    check_led_without_resistor(netlist, &mut v);
    check_reversed_led(netlist, &mut v);
    check_reversed_diode(netlist, &mut v);
    check_5v_on_3v3(netlist, &mut v);
    check_gpio_direct_loads(netlist, &mut v);
    check_esp_gpio_overvoltage(netlist, &mut v);
    check_power_rail_conflicts(netlist, &mut v);
    check_relay_flyback_diodes(netlist, &mut v);
    check_i2c_pullups(netlist, &mut v);
    check_oled_sda_scl_swap(netlist, &mut v);
    check_missing_values(netlist, &mut v);
    check_floating_pins(netlist, &mut v);
    check_floating_dc_nets(netlist, &mut v);
    check_open_voltage_sources(netlist, &mut v);
    check_symbolic_components(netlist, &mut v);

    v
}

// ─── Individual rule implementations ────────────────────────────────────────

fn check_missing_gnd(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    if netlist.pins.is_empty() {
        return;
    }
    let has_gnd = netlist.nets.iter().any(|net| net.name == "GND")
        || netlist.pins.iter().any(|pin| {
            pin.electrical_type == ElectricalType::Ground
                || pin.component_kind == ComponentKind::Ground
        });
    if !has_gnd {
        v.push(ErcViolation {
            severity: ErcSeverity::Error,
            component_id: None,
            wire_id: None,
            message: "No GND reference found. Add a Ground symbol to your circuit.".to_string(),
        });
    }
}

fn check_power_gnd_short(netlist: &CircuitNetlist, v: &mut Vec<ErcViolation>) {
    for net in &netlist.nets {
        let pins: Vec<&NetlistPin> = netlist.pins.iter().filter(|p| p.net_id == net.id).collect();

        let power_pin = pins.iter().find(|p| pin_is_power_positive(p));
        let gnd_pin = pins.iter().find(|p| {
            p.electrical_type == ElectricalType::Ground || p.component_kind == ComponentKind::Ground
        });

        if let (Some(pwr), Some(gnd)) = (power_pin, gnd_pin) {
            v.push(ErcViolation {
                severity: ErcSeverity::Error,
                component_id: Some(pwr.component_id),
                wire_id: None,
                message: format!(
                    "Power short: {} {} is directly connected to GND ({}).",
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
        if has_5v {
            if let Some(target) = target_3v3 {
                v.push(ErcViolation {
                    severity: ErcSeverity::Error,
                    component_id: Some(target.component_id),
                    wire_id: None,
                    message: format!("{} connects 5V to a 3.3V rail/pin.", net.name),
                });
            }
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
            ) && pin_is_microcontroller_gpio(pin)
        }) {
            v.push(ErcViolation {
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
        if has_5v {
            if let Some(rail) = rail_3v3 {
                v.push(ErcViolation {
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
            let pin = netlist
                .pins
                .iter()
                .find(|pin| {
                    pin.net_id == net_id && pin.pin_name.to_ascii_uppercase().contains(signal)
                })
                .unwrap();
            v.push(ErcViolation {
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
            || electrical_metadata(pin.component_kind).simulation != SimulationSupport::Symbolic
            || matches!(
                pin.component_kind,
                ComponentKind::TextNote | ComponentKind::NetLabel
            )
        {
            continue;
        }
        v.push(ErcViolation {
            severity: ErcSeverity::Info,
            component_id: Some(pin.component_id),
            wire_id: None,
            message: format!(
                "{} {} is symbolic in DC simulation. Connections are checked, but current and voltage behavior are not modeled.",
                component_kind_short(pin.component_kind),
                pin.component_label
            ),
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
            ComponentKind::Battery | ComponentKind::VSource => {
                if pin.component_value.trim().is_empty()
                    || parse_metric_value(&pin.component_value, "v").is_none()
                {
                    seen.insert(pin.component_id);
                    v.push(ErcViolation {
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
            severity: ErcSeverity::Warning,
            component_id: None,
            wire_id: Some(*wire_id),
            message: "Wire is floating (not connected to any component pin).".to_string(),
        });
    }

    for wire_id in &netlist.isolated_wires {
        v.push(ErcViolation {
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
            && matches!(
                pin.electrical_type,
                ElectricalType::Digital | ElectricalType::I2c | ElectricalType::Control
            )
        {
            v.push(ErcViolation {
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
    ) && (pin.pin_name.to_ascii_uppercase().contains("GPIO")
        || pin.pin_name.to_ascii_uppercase().starts_with("GP")
        || pin.pin_name.to_ascii_uppercase().starts_with('D'))
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
    )
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
            floating_wires: Vec::new(),
            isolated_wires: Vec::new(),
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
        assert!(
            v.iter()
                .any(|e| e.message.contains("current limiting resistor"))
        );
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
        assert!(v.iter().any(|e| e.message.contains("GND")));
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
        assert!(
            v.iter()
                .any(|e| e.severity == ErcSeverity::Error && e.message.contains("short"))
        );
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
        assert!(v.iter().any(|e| e.severity == ErcSeverity::Error));
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
        assert!(v.iter().any(|e| e.message.contains("reversed")));
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
        assert!(v.iter().any(|e| e.message.contains("resistance value")));
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
        assert!(v.iter().any(|e| e.message.contains("voltage value")));
    }

    // ── Floating wire ─────────────────────────────────────────────────────

    #[test]
    fn reports_floating_wire() {
        let mut nl = netlist(vec![make_net(0, "NET_001")], vec![]);
        nl.floating_wires.push(42);
        let v = validate_beginner_rules(&nl);
        assert!(v.iter().any(|e| e.wire_id == Some(42)));
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
            floating_wires: Vec::new(),
            isolated_wires: Vec::new(),
        };

        let violations = validate_beginner_rules(&nl);
        assert!(
            violations
                .iter()
                .any(|violation| violation.message.contains("floating DC island"))
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
            floating_wires: Vec::new(),
            isolated_wires: vec![10],
        };

        let violations = validate_beginner_rules(&nl);
        assert!(violations.iter().any(|violation| {
            violation.message.contains("no closed DC load path")
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
        assert!(
            v.iter()
                .any(|e| e.message.contains("SDA") && e.message.contains("SCL"))
        );
    }
}
