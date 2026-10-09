use crate::engine::validation::pin_is_microcontroller_gpio;
use crate::model::{CircuitNetlist, ComponentKind, NetlistPin};

/// Refuse ambiguous firmware instead of silently selecting another MCU or bus.
pub(crate) fn generate_arduino_code_checked(netlist: &CircuitNetlist) -> Result<String, String> {
    let mut controllers: Vec<_> = netlist
        .pins
        .iter()
        .filter(|p| controller_kind(p.component_kind))
        .map(|p| (p.component_id, p.component_kind))
        .collect();
    controllers.sort_by_key(|p| p.0);
    controllers.dedup_by_key(|p| p.0);
    let [(controller_id, controller_kind)] = controllers.as_slice() else {
        return Err(
            "Place exactly one controller on the active page before exporting code.".into(),
        );
    };
    let controller_kind = *controller_kind;
    let controller_id = *controller_id;
    let mut oled_ids: Vec<_> = netlist
        .pins
        .iter()
        .filter(|p| p.component_kind == ComponentKind::Oled)
        .map(|p| p.component_id)
        .collect();
    oled_ids.sort_unstable();
    oled_ids.dedup();
    if oled_ids.len() > 1 {
        return Err("OLED starter code supports one display. Use separate pages or write a multi-display sketch.".into());
    }
    let has_oled = !oled_ids.is_empty();
    let mut i2c_peripherals: Vec<_> = netlist
        .pins
        .iter()
        .filter(|p| {
            matches!(
                p.component_kind,
                ComponentKind::Oled | ComponentKind::Sensor
            )
        })
        .map(|p| p.component_id)
        .collect();
    i2c_peripherals.sort_unstable();
    i2c_peripherals.dedup();
    let has_i2c = !i2c_peripherals.is_empty();
    let mut bus_pins: Option<(&NetlistPin, &NetlistPin)> = None;
    for peripheral_id in i2c_peripherals {
        let module_ground = netlist
            .pins
            .iter()
            .find(|p| p.component_id == peripheral_id && p.pin_name == "GND");
        if !module_ground.is_some_and(|ground| {
            netlist.pins.iter().any(|p| {
                p.component_id == controller_id && p.pin_name == "GND" && p.net_id == ground.net_id
            })
        }) {
            return Err(format!(
                "Connect I2C module #{peripheral_id} and controller to a common GND before exporting."
            ));
        }
        let mut mapped = Vec::new();
        for signal in ["SDA", "SCL"] {
            let Some(pin) = netlist
                .pins
                .iter()
                .find(|p| p.component_id == peripheral_id && p.pin_name == signal)
            else {
                return Err(format!("I2C module #{peripheral_id} is missing {signal}."));
            };
            let candidates: Vec<_> = netlist
                .pins
                .iter()
                .filter(|p| {
                    p.component_id == controller_id && p.net_id == pin.net_id && signal_pin(p)
                })
                .collect();
            let [ctrl] = candidates.as_slice() else {
                return Err(format!(
                    "Wire {} {signal} to exactly one controller signal pin before exporting.",
                    pin.component_label
                ));
            };
            if !supports_output(ctrl)
                || netlist.pins.iter().any(|p| {
                    p.net_id == pin.net_id
                        && matches!(
                            p.electrical_type,
                            crate::model::ElectricalType::Ground
                                | crate::model::ElectricalType::PowerIn
                                | crate::model::ElectricalType::PowerOutput
                        )
                })
            {
                return Err(format!(
                    "{} {signal} is tied to a power rail or an input-only pin. Correct the signal wiring.",
                    pin.component_label
                ));
            }
            if controller_kind == ComponentKind::ArduinoUno && !ctrl.pin_name.ends_with(signal) {
                return Err("UNO I2C must use A4 SDA and A5 SCL.".into());
            }
            mapped.push(*ctrl);
        }
        let pair = (mapped[0], mapped[1]);
        if pair.0.net_id == pair.1.net_id {
            return Err("SDA and SCL are shorted. Separate the two signal nets.".into());
        }
        if bus_pins
            .is_some_and(|bus| bus.0.net_id != pair.0.net_id || bus.1.net_id != pair.1.net_id)
        {
            return Err(
                "Starter code supports one I2C bus. Connect modules to the same SDA/SCL pair."
                    .into(),
            );
        }
        bus_pins = Some(pair);
    }
    let i2c_sda = bus_pins
        .and_then(|p| code_pin_name(p.0))
        .unwrap_or_default();
    let i2c_scl = bus_pins
        .and_then(|p| code_pin_name(p.1))
        .unwrap_or_default();
    if has_i2c && (i2c_sda.is_empty() || i2c_scl.is_empty()) {
        return Err(
            "I2C pins cannot be mapped to this board core. Choose named GPIO/I2C pins.".into(),
        );
    }

    let mut gpio_pins: Vec<_> = netlist
        .pins
        .iter()
        .filter(|p| {
            p.component_id == controller_id
                && p.connected_by_wire
                && signal_pin(p)
                && !bus_pins.is_some_and(|bus| p.net_id == bus.0.net_id || p.net_id == bus.1.net_id)
        })
        .filter_map(|p| code_pin_name(p).map(|g| (p, g)))
        .collect();
    gpio_pins.sort_by(|a, b| a.0.pin_name.cmp(&b.0.pin_name));
    gpio_pins.dedup_by(|a, b| a.0.pin_name == b.0.pin_name);
    let button_gpio = gpio_pins
        .iter()
        .find(|(p, _)| grounded_button(netlist, p) && supports_pullup(p))
        .map(|(_, g)| g.clone());
    let led_gpio = gpio_pins
        .iter()
        .find(|(p, _)| led_output(netlist, p))
        .map(|(_, g)| g.clone());
    let toggles = button_gpio.is_some() && led_gpio.is_some();
    let mut code = String::new();
    code.push_str("// Generated by Cluster\n");
    code.push_str("#include <Arduino.h>\n");
    code.push_str(&format!(
        "// Board: {}. Select the matching Arduino core.\n",
        crate::component_kind_label(controller_kind)
    ));
    if controller_kind == ComponentKind::RaspberryPiPico {
        code.push_str("// Requires the Earle Philhower Arduino-Pico core.\n");
    }
    if has_i2c {
        code.push_str("#include <Wire.h>\n");
    }
    if has_oled {
        code.push_str("#include <Adafruit_GFX.h>\n");
        code.push_str("#include <Adafruit_SSD1306.h>\n\n");
        code.push_str("#define SCREEN_WIDTH 128\n#define SCREEN_HEIGHT 64\n");
        code.push_str("Adafruit_SSD1306 display(SCREEN_WIDTH, SCREEN_HEIGHT, &Wire, -1);\n");
    }
    code.push('\n');

    // Pin constants
    for (pin, gpio) in &gpio_pins {
        code.push_str(&format!(
            "const int PIN_{} = {};\n",
            sanitize_code_ident(&pin.pin_name),
            gpio
        ));
    }

    let mut peripheral_setup = String::new();
    if has_i2c {
        match controller_kind {
            ComponentKind::ArduinoUno => {
                peripheral_setup.push_str("  Wire.begin();  // UNO uses A4 SDA and A5 SCL\n")
            }
            ComponentKind::Esp32 | ComponentKind::Esp32S3 | ComponentKind::Esp32C3 => {
                peripheral_setup.push_str(&format!("  Wire.begin({i2c_sda}, {i2c_scl});\n"));
            }
            _ => peripheral_setup.push_str(&format!(
                "  Wire.setSDA({i2c_sda});\n  Wire.setSCL({i2c_scl});\n  Wire.begin();\n"
            )),
        }
        peripheral_setup.push_str("  Wire.setClock(100000);\n  Serial.println(\"I2C scan (7-bit addresses):\");\n  for (uint8_t address = 1; address < 127; ++address) {\n    Wire.beginTransmission(address);\n    if (Wire.endTransmission() == 0) {\n      Serial.print(\"Found 0x\"); Serial.println(address, HEX);\n    }\n  }\n");
    }
    if has_oled {
        code.push_str("\n// SSD1306 128x64; change OLED_ADDRESS to match the I2C scan.\nconst uint8_t OLED_ADDRESS = 0x3C;\nbool displayReady = false;\n");
        peripheral_setup.push_str("  displayReady = display.begin(SSD1306_SWITCHCAPVCC, OLED_ADDRESS);\n  if (!displayReady) {\n    Serial.println(\"OLED init failed: check power, SDA/SCL, address and libraries.\");\n  } else {\n    display.clearDisplay();\n    display.setTextSize(1);\n    display.setTextColor(SSD1306_WHITE);\n    display.setCursor(0, 0);\n    display.println(\"Cluster ready\");\n    display.display();\n  }\n");
    }

    // Button-toggle pattern: extra state variable
    if toggles && let (Some(btn), Some(led)) = (&button_gpio, &led_gpio) {
        code.push_str(&format!("\nconst int BUTTON_PIN = {btn};\n"));
        code.push_str(&format!("const int LED_PIN    = {led};\n"));
        code.push_str("const unsigned long DEBOUNCE_MS = 50;\n");
        code.push_str("\nbool ledState = false;\n");
        code.push_str("int lastReading = HIGH;\n");
        code.push_str("int stableState = HIGH;\n");
        code.push_str("unsigned long lastDebounceTime = 0;\n");

        code.push_str("\nvoid setup() {\n  Serial.begin(115200);\n");
        code.push_str(&peripheral_setup);
        code.push_str("  pinMode(BUTTON_PIN, INPUT_PULLUP);  // active-low button\n");
        code.push_str("  pinMode(LED_PIN, OUTPUT);\n");
        code.push_str("  digitalWrite(LED_PIN, LOW);\n");
        code.push_str("}\n\nvoid loop() {\n");
        code.push_str("  int reading = digitalRead(BUTTON_PIN);\n");
        code.push_str("  if (reading != lastReading) {\n");
        code.push_str("    lastDebounceTime = millis();\n");
        code.push_str("    lastReading = reading;\n");
        code.push_str("  }\n\n");
        code.push_str(
            "  if ((millis() - lastDebounceTime) > DEBOUNCE_MS && reading != stableState) {\n",
        );
        code.push_str("    stableState = reading;\n");
        code.push_str("    if (stableState == LOW) {  // pressed with INPUT_PULLUP\n");
        code.push_str("      ledState = !ledState;\n");
        code.push_str("      digitalWrite(LED_PIN, ledState ? HIGH : LOW);\n");
        code.push_str("      Serial.println(ledState ? \"LED ON\" : \"LED OFF\");\n");
        code.push_str("    }\n");
        code.push_str("  }\n");
        code.push_str("  delay(1);\n");
        code.push_str("}\n");
        return Ok(code);
    }

    code.push_str("\nvoid setup() {\n  Serial.begin(115200);\n");
    code.push_str(&peripheral_setup);
    for (pin, _) in &gpio_pins {
        let mode = if led_output(netlist, pin) {
            "OUTPUT"
        } else if grounded_button(netlist, pin) && supports_pullup(pin) {
            "INPUT_PULLUP"
        } else {
            "INPUT"
        };
        code.push_str(&format!(
            "  pinMode(PIN_{}, {mode});\n",
            sanitize_code_ident(&pin.pin_name)
        ));
        if mode == "OUTPUT" {
            code.push_str(&format!(
                "  digitalWrite(PIN_{}, LOW);\n",
                sanitize_code_ident(&pin.pin_name)
            ));
        }
    }
    code.push_str("}\n\nvoid loop() {\n");
    let outputs: Vec<_> = gpio_pins
        .iter()
        .filter(|(pin, _)| led_output(netlist, pin))
        .collect();
    if outputs.is_empty() {
        code.push_str("  // Unknown signal directions remain INPUT; configure them for your firmware.\n  delay(1000);\n");
    } else {
        for level in ["HIGH", "LOW"] {
            for (pin, _) in &outputs {
                code.push_str(&format!(
                    "  digitalWrite(PIN_{}, {level});\n",
                    sanitize_code_ident(&pin.pin_name)
                ));
            }
            code.push_str("  delay(500);\n");
        }
    }
    code.push_str("}\n");
    Ok(code)
}

pub(crate) fn digits_from_pin_name(name: &str) -> Option<String> {
    let digits = name
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    (!digits.is_empty()).then_some(digits)
}

fn controller_kind(kind: ComponentKind) -> bool {
    matches!(
        kind,
        ComponentKind::Esp32
            | ComponentKind::Esp32S3
            | ComponentKind::Esp32C3
            | ComponentKind::ArduinoUno
            | ComponentKind::RaspberryPiPico
            | ComponentKind::Stm32BluePill
            | ComponentKind::Stm32Nucleo64
    )
}

fn signal_pin(pin: &NetlistPin) -> bool {
    matches!(
        pin.electrical_type,
        crate::model::ElectricalType::Digital
            | crate::model::ElectricalType::I2c
            | crate::model::ElectricalType::Input
            | crate::model::ElectricalType::Output
            | crate::model::ElectricalType::Bidirectional
    ) && (pin_is_microcontroller_gpio(pin)
        || (controller_kind(pin.component_kind) && code_pin_name(pin).is_some()))
}

fn code_pin_name(pin: &NetlistPin) -> Option<String> {
    let name = pin.pin_name.split_whitespace().next()?;
    match pin.component_kind {
        ComponentKind::ArduinoUno if name.starts_with('A') => Some(name.to_string()),
        ComponentKind::Stm32BluePill => name.starts_with('P').then(|| name.to_string()),
        ComponentKind::Stm32Nucleo64 => pin
            .pin_name
            .split_whitespace()
            .find(|token| token.starts_with('P'))
            .map(str::to_string),
        _ if name.starts_with("GPIO") || name.starts_with("GP") || name.starts_with('D') => {
            digits_from_pin_name(name)
        }
        _ => None,
    }
}

fn supports_output(pin: &NetlistPin) -> bool {
    !(pin.component_kind == ComponentKind::Esp32
        && matches!(
            code_pin_name(pin).as_deref(),
            Some("34" | "35" | "36" | "39")
        ))
}

fn supports_pullup(pin: &NetlistPin) -> bool {
    supports_output(pin)
}

fn ground_net(netlist: &CircuitNetlist, controller_id: u64, id: usize) -> bool {
    netlist.pins.iter().any(|p| {
        p.component_id == controller_id
            && p.net_id == id
            && p.electrical_type == crate::model::ElectricalType::Ground
    })
}

fn grounded_button(netlist: &CircuitNetlist, gpio: &NetlistPin) -> bool {
    let pins: Vec<_> = netlist
        .pins
        .iter()
        .filter(|p| p.net_id == gpio.net_id)
        .collect();
    if pins.len() != 2 {
        return false;
    }
    let Some(button) = pins
        .iter()
        .find(|p| p.component_kind == ComponentKind::PushButton)
    else {
        return false;
    };
    netlist.pins.iter().any(|p| {
        p.component_id == button.component_id
            && p.pin_name != button.pin_name
            && p.net_id != gpio.net_id
            && ground_net(netlist, gpio.component_id, p.net_id)
    })
}

/// Only a dedicated GPIO -> resistor -> LED anode -> ground branch is driven.
/// Mixed nets, reversed LEDs, direct LEDs and input-only pins remain inputs.
fn led_output(netlist: &CircuitNetlist, gpio: &NetlistPin) -> bool {
    if !supports_output(gpio) {
        return false;
    }
    let pins: Vec<_> = netlist
        .pins
        .iter()
        .filter(|p| p.net_id == gpio.net_id)
        .collect();
    if pins.len() != 2 {
        return false;
    }
    let Some(resistor) = pins
        .iter()
        .find(|p| p.component_kind == ComponentKind::Resistor)
    else {
        return false;
    };
    if !crate::engine::parse_metric_value(&resistor.component_value, "ohm")
        .is_some_and(|r| r >= 220.0)
    {
        return false;
    }
    let Some(other) = netlist
        .pins
        .iter()
        .find(|p| p.component_id == resistor.component_id && p.pin_name != resistor.pin_name)
    else {
        return false;
    };
    let branch: Vec<_> = netlist
        .pins
        .iter()
        .filter(|p| p.net_id == other.net_id)
        .collect();
    if branch.len() != 2 {
        return false;
    }
    let Some(led) = branch
        .iter()
        .find(|p| p.component_kind == ComponentKind::Led && p.pin_name == "A")
    else {
        return false;
    };
    netlist.pins.iter().any(|p| {
        p.component_id == led.component_id
            && p.pin_name == "B"
            && p.net_id != gpio.net_id
            && ground_net(netlist, gpio.component_id, p.net_id)
    })
}

pub(crate) fn sanitize_code_ident(name: &str) -> String {
    let ident = name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_ascii_uppercase();
    if ident.is_empty() {
        "GPIO".to_string()
    } else {
        ident
    }
}
