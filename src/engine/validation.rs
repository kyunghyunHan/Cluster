use crate::model::*;
use crate::parse_metric_value;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ErcSeverity {
    Error,
    Warning,
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
    let mut violations = Vec::new();

    for led in netlist
        .pins
        .iter()
        .filter(|pin| pin.component_kind == ComponentKind::Led && pin.pin_name == "A")
    {
        let led_nets = netlist
            .pins
            .iter()
            .filter(|pin| pin.component_id == led.component_id)
            .map(|pin| pin.net_id)
            .collect::<std::collections::HashSet<_>>();
        let has_resistor = netlist.pins.iter().any(|pin| {
            pin.component_kind == ComponentKind::Resistor && led_nets.contains(&pin.net_id)
        });
        if !has_resistor {
            violations.push(ErcViolation {
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

    for net in &netlist.nets {
        let pins = netlist
            .pins
            .iter()
            .filter(|pin| pin.net_id == net.id)
            .collect::<Vec<_>>();
        let has_5v = pins.iter().any(|pin| pin_is_5v_source(pin));
        let has_3v3 = pins.iter().any(|pin| pin_name_is_3v3(&pin.pin_name));
        if has_5v && has_3v3 {
            let target = pins
                .iter()
                .find(|pin| pin_name_is_3v3(&pin.pin_name))
                .copied();
            violations.push(ErcViolation {
                severity: ErcSeverity::Error,
                component_id: target.map(|pin| pin.component_id),
                wire_id: None,
                message: format!("{} connects 5V to a 3.3V rail/pin.", net.name),
            });
        }

        for gpio in pins
            .iter()
            .filter(|pin| pin_is_microcontroller_gpio(pin) && !pin_is_i2c_named(&pin.pin_name))
        {
            if pins
                .iter()
                .any(|pin| pin.component_kind == ComponentKind::DcMotor)
            {
                violations.push(ErcViolation {
                    severity: ErcSeverity::Error,
                    component_id: Some(gpio.component_id),
                    wire_id: None,
                    message: format!(
                        "{} {} is connected directly to a motor. Use a transistor, relay, or driver.",
                        gpio.component_label, gpio.pin_name
                    ),
                });
            }
        }

        let oled_sda = pins.iter().any(|pin| {
            pin.component_kind == ComponentKind::Oled && pin.pin_name.eq_ignore_ascii_case("SDA")
        });
        let oled_scl = pins.iter().any(|pin| {
            pin.component_kind == ComponentKind::Oled && pin.pin_name.eq_ignore_ascii_case("SCL")
        });
        if oled_sda
            && pins
                .iter()
                .any(|pin| pin_is_controller_scl(pin) && !pin_is_controller_sda(pin))
        {
            violations.push(ErcViolation {
                severity: ErcSeverity::Error,
                component_id: pins
                    .iter()
                    .find(|pin| pin.component_kind == ComponentKind::Oled)
                    .map(|pin| pin.component_id),
                wire_id: None,
                message: "OLED SDA is connected to a controller SCL pin.".to_string(),
            });
        }
        if oled_scl
            && pins
                .iter()
                .any(|pin| pin_is_controller_sda(pin) && !pin_is_controller_scl(pin))
        {
            violations.push(ErcViolation {
                severity: ErcSeverity::Error,
                component_id: pins
                    .iter()
                    .find(|pin| pin.component_kind == ComponentKind::Oled)
                    .map(|pin| pin.component_id),
                wire_id: None,
                message: "OLED SCL is connected to a controller SDA pin.".to_string(),
            });
        }
    }

    violations
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(
        component_id: u64,
        label: &str,
        kind: ComponentKind,
        value: &str,
        pin_name: &str,
        net_id: usize,
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
            connected_by_wire: true,
        }
    }

    #[test]
    fn reports_led_without_resistor() {
        let netlist = CircuitNetlist {
            nets: vec![
                Net {
                    id: 0,
                    name: "NET_001".to_string(),
                    connected_pins: Vec::new(),
                },
                Net {
                    id: 1,
                    name: "GND".to_string(),
                    connected_pins: Vec::new(),
                },
            ],
            pins: vec![
                pin(1, "LED1", ComponentKind::Led, "red", "A", 0),
                pin(1, "LED1", ComponentKind::Led, "red", "B", 1),
            ],
            wire_nets: std::collections::HashMap::new(),
            floating_wires: Vec::new(),
        };

        let violations = validate_beginner_rules(&netlist);

        assert!(
            violations
                .iter()
                .any(|violation| violation.message.contains("current limiting resistor"))
        );
    }

    #[test]
    fn reports_5v_on_3v3_net() {
        let netlist = CircuitNetlist {
            nets: vec![Net {
                id: 0,
                name: "NET_001".to_string(),
                connected_pins: Vec::new(),
            }],
            pins: vec![
                pin(1, "ARD1", ComponentKind::ArduinoUno, "", "5V", 0),
                pin(2, "ESP1", ComponentKind::Esp32, "", "3V3", 0),
            ],
            wire_nets: std::collections::HashMap::new(),
            floating_wires: Vec::new(),
        };

        let violations = validate_beginner_rules(&netlist);

        assert!(
            violations
                .iter()
                .any(|violation| violation.severity == ErcSeverity::Error)
        );
    }
}
