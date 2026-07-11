use egui::{Pos2, Rect, Vec2};

use super::{CircuitPin, Component, ComponentKind, PinRole};

// ── Component geometry ────────────────────────────────────────────────────────

/// Visual size of a component on the canvas.
///
/// All widths are multiples of 80 (half = 40) so left/right pins land on the
/// 20 px grid when the component centre is placed on the grid.
pub(crate) fn component_size(component: &Component) -> Vec2 {
    let (w, h) = match component.kind {
        ComponentKind::Resistor
        | ComponentKind::Inductor
        | ComponentKind::Diode
        | ComponentKind::ZenerDiode
        | ComponentKind::Fuse
        | ComponentKind::Thermistor
        | ComponentKind::Varistor
        | ComponentKind::SchottkyDiode
        | ComponentKind::TvsDiode => (80.0, 28.0),
        ComponentKind::Potentiometer => (80.0, 44.0),
        ComponentKind::Capacitor | ComponentKind::Crystal => (80.0, 32.0),
        ComponentKind::Battery
        | ComponentKind::Switch
        | ComponentKind::PushButton
        | ComponentKind::SlideSwitch => (80.0, 40.0),
        ComponentKind::Ground => (40.0, 40.0),
        ComponentKind::VSource
        | ComponentKind::ISource
        | ComponentKind::Lamp
        | ComponentKind::Led => (80.0, 60.0),
        ComponentKind::NpnTransistor | ComponentKind::PnpTransistor => (80.0, 68.0),
        ComponentKind::Nmosfet | ComponentKind::Pmosfet => (80.0, 68.0),
        ComponentKind::VoltageReg | ComponentKind::VoltageRef => (80.0, 52.0),
        ComponentKind::LogicNot => (80.0, 44.0),
        ComponentKind::LogicAnd
        | ComponentKind::LogicOr
        | ComponentKind::LogicNand
        | ComponentKind::LogicNor
        | ComponentKind::LogicXor => (80.0, 52.0),
        ComponentKind::OpAmp => (80.0, 68.0),
        ComponentKind::Esp32 | ComponentKind::Esp32S3 => (160.0, 320.0),
        ComponentKind::Esp32C3 => (120.0, 200.0),
        ComponentKind::ArduinoUno => (160.0, 300.0),
        ComponentKind::RaspberryPiPico => (120.0, 180.0),
        ComponentKind::Stm32BluePill => (140.0, 260.0),
        ComponentKind::Stm32Nucleo64 => (180.0, 320.0),
        ComponentKind::Breadboard => (280.0, 160.0),
        ComponentKind::Relay => (80.0, 72.0),
        ComponentKind::DcMotor => (80.0, 64.0),
        ComponentKind::Servo => (80.0, 72.0),
        ComponentKind::Oled => (120.0, 120.0),
        ComponentKind::Sensor => (120.0, 100.0),
        ComponentKind::NetLabel => (80.0, 32.0),
        ComponentKind::Timer555 => (80.0, 100.0),
        ComponentKind::Transformer => (80.0, 64.0),
        ComponentKind::Display7Seg => (80.0, 100.0),
        ComponentKind::MotorDriver => (120.0, 120.0),
        ComponentKind::Phototransistor => (80.0, 68.0),
        ComponentKind::Optocoupler => (80.0, 64.0),
        ComponentKind::GenericIc => (80.0, 80.0),
        ComponentKind::Voltmeter | ComponentKind::Ammeter => (80.0, 60.0),
        ComponentKind::Dht11 | ComponentKind::Dht22 => (80.0, 80.0),
        ComponentKind::HcSr04 => (100.0, 60.0),
        ComponentKind::Buzzer => (60.0, 60.0),
        ComponentKind::NeoPixel => (60.0, 60.0),
        ComponentKind::PirSensor => (80.0, 80.0),
        ComponentKind::Custom => {
            let size = component
                .part_id
                .as_deref()
                .and_then(super::custom_part::custom_part)
                .map(|def| def.size)
                .unwrap_or(Vec2::new(120.0, 80.0));
            (size.x, size.y)
        }
        ComponentKind::TextNote => {
            let lines = component.value.lines().count().max(1) as f32;
            let longest = component
                .value
                .lines()
                .map(|line| line.chars().count())
                .max()
                .unwrap_or(12) as f32;
            (
                (longest * 7.2 + 28.0).clamp(180.0, 320.0),
                28.0 + lines * 18.0,
            )
        }
    };
    Vec2::new(w, h)
}

// ── Rotation ──────────────────────────────────────────────────────────────────

pub(crate) fn rotate_point(point: Pos2, center: Pos2, rotation: i32) -> Pos2 {
    let rot = rotation.rem_euclid(360);
    let translated = point - center;
    match rot {
        90 => Pos2::new(center.x - translated.y, center.y + translated.x),
        180 => Pos2::new(center.x - translated.x, center.y - translated.y),
        270 => Pos2::new(center.x + translated.y, center.y - translated.x),
        _ => point,
    }
}

// ── Pin accessors ─────────────────────────────────────────────────────────────

/// Returns only the world positions of all pins on `component`.
pub(crate) fn component_pins(component: &Component) -> Vec<Pos2> {
    component_pin_defs(component)
        .into_iter()
        .map(|pin| pin.pos)
        .collect()
}

/// Returns the full [`CircuitPin`] descriptors for every pin on `component`,
/// with positions already rotated into world space.
pub(crate) fn component_pin_defs(component: &Component) -> Vec<CircuitPin> {
    let rect = Rect::from_center_size(component.pos, component_size(component));
    let center = rect.center();
    let base = match component.kind {
        ComponentKind::Ground => vec![CircuitPin {
            label: "GND",
            role: PinRole::Ground,
            pos: Pos2::new(center.x, rect.top()),
        }],
        ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource => vec![
            CircuitPin {
                label: "-",
                role: PinRole::Ground,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "+",
                role: PinRole::Positive,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
        ComponentKind::OpAmp => vec![
            CircuitPin {
                label: "-IN",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y - rect.height() * 0.22),
            },
            CircuitPin {
                label: "+IN",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y + rect.height() * 0.22),
            },
            CircuitPin {
                label: "OUT",
                role: PinRole::Output,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
        ComponentKind::Esp32 => module_pin_defs(
            rect,
            &[
                ("3V3", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GPIO2", PinRole::Digital),
                ("GPIO4", PinRole::Digital),
                ("GPIO15", PinRole::Digital),
                ("GPIO23 MOSI", PinRole::Digital),
                ("GPIO22 SCL", PinRole::I2c),
                ("GPIO21 SDA", PinRole::I2c),
                ("GPIO19 MISO", PinRole::Digital),
                ("GPIO18 SCK", PinRole::Digital),
                ("GPIO5 SS", PinRole::Digital),
                ("TX0", PinRole::Digital),
                ("RX0", PinRole::Digital),
                ("GND", PinRole::Ground),
            ],
            &[
                ("VIN", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GPIO34 ADC", PinRole::Digital),
                ("GPIO35 ADC", PinRole::Digital),
                ("GPIO32", PinRole::Digital),
                ("GPIO33", PinRole::Digital),
                ("GPIO25 DAC", PinRole::Digital),
                ("GPIO26 DAC", PinRole::Digital),
                ("GPIO27", PinRole::Digital),
                ("GPIO14", PinRole::Digital),
                ("GPIO13", PinRole::Digital),
                ("GPIO12", PinRole::Digital),
                ("GPIO0", PinRole::Digital),
                ("EN", PinRole::Control),
            ],
        ),
        ComponentKind::Esp32S3 => module_pin_defs(
            rect,
            &[
                ("3V3", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GPIO1", PinRole::Digital),
                ("GPIO2 SDA", PinRole::I2c),
                ("GPIO3 SCL", PinRole::I2c),
                ("GPIO4", PinRole::Digital),
                ("TX0", PinRole::Digital),
                ("RX0", PinRole::Digital),
            ],
            &[
                ("VIN", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GPIO8", PinRole::Digital),
                ("GPIO9", PinRole::Digital),
                ("GPIO10", PinRole::Digital),
                ("GPIO11", PinRole::Digital),
                ("EN", PinRole::Control),
                ("RST", PinRole::Control),
            ],
        ),
        ComponentKind::Esp32C3 => module_pin_defs(
            rect,
            &[
                ("3V3", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GPIO0", PinRole::Digital),
                ("GPIO1 SDA", PinRole::I2c),
                ("GPIO2 SCL", PinRole::I2c),
                ("TX", PinRole::Digital),
                ("RX", PinRole::Digital),
            ],
            &[
                ("5V", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GPIO3", PinRole::Digital),
                ("GPIO4", PinRole::Digital),
                ("GPIO5", PinRole::Digital),
                ("EN", PinRole::Control),
                ("BOOT", PinRole::Control),
            ],
        ),
        ComponentKind::ArduinoUno => module_pin_defs(
            rect,
            &[
                ("VIN", PinRole::Positive),
                ("5V", PinRole::Positive),
                ("3V3", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("A0", PinRole::Digital),
                ("A1", PinRole::Digital),
                ("A2", PinRole::Digital),
                ("A3", PinRole::Digital),
                ("A4 SDA", PinRole::I2c),
                ("A5 SCL", PinRole::I2c),
            ],
            &[
                ("D2", PinRole::Digital),
                ("D3 PWM", PinRole::Digital),
                ("D4", PinRole::Digital),
                ("D5 PWM", PinRole::Digital),
                ("D6 PWM", PinRole::Digital),
                ("D7", PinRole::Digital),
                ("D8", PinRole::Digital),
                ("D9 PWM", PinRole::Digital),
                ("D10", PinRole::Digital),
                ("D11 MOSI", PinRole::Digital),
                ("D12 MISO", PinRole::Digital),
                ("D13 SCK", PinRole::Digital),
                ("TX", PinRole::Digital),
                ("RX", PinRole::Digital),
            ],
        ),
        ComponentKind::RaspberryPiPico => module_pin_defs(
            rect,
            &[
                ("VSYS", PinRole::Positive),
                ("3V3", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GP0 TX", PinRole::Digital),
                ("GP1 RX", PinRole::Digital),
                ("GP4 SDA", PinRole::I2c),
                ("GP5 SCL", PinRole::I2c),
            ],
            &[
                ("VBUS", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("GP14", PinRole::Digital),
                ("GP15", PinRole::Digital),
                ("GP16", PinRole::Digital),
                ("GP17", PinRole::Digital),
                ("RUN", PinRole::Control),
            ],
        ),
        ComponentKind::Stm32BluePill => module_pin_defs(
            rect,
            &[
                ("3V3", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("PA0 ADC", PinRole::Digital),
                ("PA1 ADC", PinRole::Digital),
                ("PA2 TX2", PinRole::Digital),
                ("PA3 RX2", PinRole::Digital),
                ("PA5 SCK", PinRole::Digital),
                ("PA6 MISO", PinRole::Digital),
                ("PA7 MOSI", PinRole::Digital),
                ("PB6 SCL", PinRole::I2c),
                ("PB7 SDA", PinRole::I2c),
            ],
            &[
                ("5V", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("VBAT", PinRole::Positive),
                ("PA9 TX1", PinRole::Digital),
                ("PA10 RX1", PinRole::Digital),
                ("PA13 SWDIO", PinRole::Control),
                ("PA14 SWCLK", PinRole::Control),
                ("PB0 ADC", PinRole::Digital),
                ("PB1 ADC", PinRole::Digital),
                ("BOOT0", PinRole::Control),
                ("NRST", PinRole::Control),
            ],
        ),
        ComponentKind::Stm32Nucleo64 => module_pin_defs(
            rect,
            &[
                ("3V3", PinRole::Positive),
                ("5V", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("A0 PA0 ADC", PinRole::Digital),
                ("A1 PA1 ADC", PinRole::Digital),
                ("D14 PB9 SDA", PinRole::I2c),
                ("D15 PB8 SCL", PinRole::I2c),
                ("D13 PA5 SCK", PinRole::Digital),
                ("D12 PA6 MISO", PinRole::Digital),
                ("D11 PA7 MOSI", PinRole::Digital),
            ],
            &[
                ("VIN", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("D0 PA3 RX", PinRole::Digital),
                ("D1 PA2 TX", PinRole::Digital),
                ("D2 PA10", PinRole::Digital),
                ("D3 PB3", PinRole::Digital),
                ("D4 PB5", PinRole::Digital),
                ("D5 PB4", PinRole::Digital),
                ("D6 PB10", PinRole::Digital),
                ("NRST", PinRole::Control),
            ],
        ),
        ComponentKind::ZenerDiode => vec![
            CircuitPin {
                label: "A",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "K",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
        ComponentKind::NpnTransistor => vec![
            CircuitPin {
                label: "B",
                role: PinRole::Control,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "C",
                role: PinRole::Positive,
                pos: Pos2::new(rect.right(), rect.top() + rect.height() * 0.22),
            },
            CircuitPin {
                label: "E",
                role: PinRole::Ground,
                pos: Pos2::new(rect.right(), rect.bottom() - rect.height() * 0.22),
            },
        ],
        ComponentKind::PnpTransistor => vec![
            CircuitPin {
                label: "B",
                role: PinRole::Control,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "C",
                role: PinRole::Ground,
                pos: Pos2::new(rect.right(), rect.bottom() - rect.height() * 0.22),
            },
            CircuitPin {
                label: "E",
                role: PinRole::Positive,
                pos: Pos2::new(rect.right(), rect.top() + rect.height() * 0.22),
            },
        ],
        ComponentKind::Nmosfet => vec![
            CircuitPin {
                label: "G",
                role: PinRole::Control,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "D",
                role: PinRole::Positive,
                pos: Pos2::new(rect.right(), rect.top() + rect.height() * 0.22),
            },
            CircuitPin {
                label: "S",
                role: PinRole::Ground,
                pos: Pos2::new(rect.right(), rect.bottom() - rect.height() * 0.22),
            },
        ],
        ComponentKind::Pmosfet => vec![
            CircuitPin {
                label: "G",
                role: PinRole::Control,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "D",
                role: PinRole::Ground,
                pos: Pos2::new(rect.right(), rect.bottom() - rect.height() * 0.22),
            },
            CircuitPin {
                label: "S",
                role: PinRole::Positive,
                pos: Pos2::new(rect.right(), rect.top() + rect.height() * 0.22),
            },
        ],
        ComponentKind::Potentiometer => vec![
            CircuitPin {
                label: "A",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "W",
                role: PinRole::Passive,
                pos: Pos2::new(center.x, rect.bottom()),
            },
            CircuitPin {
                label: "B",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
        ComponentKind::VoltageReg => vec![
            CircuitPin {
                label: "IN",
                role: PinRole::Positive,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "GND",
                role: PinRole::Ground,
                pos: Pos2::new(center.x, rect.bottom()),
            },
            CircuitPin {
                label: "OUT",
                role: PinRole::Positive,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
        ComponentKind::Fuse => vec![
            CircuitPin {
                label: "A",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "B",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
        ComponentKind::LogicNot => vec![
            CircuitPin {
                label: "IN",
                role: PinRole::Digital,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "OUT",
                role: PinRole::Output,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
        ComponentKind::LogicAnd
        | ComponentKind::LogicOr
        | ComponentKind::LogicNand
        | ComponentKind::LogicNor
        | ComponentKind::LogicXor => vec![
            CircuitPin {
                label: "A",
                role: PinRole::Digital,
                pos: Pos2::new(rect.left(), center.y - rect.height() * 0.25),
            },
            CircuitPin {
                label: "B",
                role: PinRole::Digital,
                pos: Pos2::new(rect.left(), center.y + rect.height() * 0.25),
            },
            CircuitPin {
                label: "OUT",
                role: PinRole::Output,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
        ComponentKind::Breadboard => breadboard_pin_defs(rect),
        ComponentKind::Relay => vec![
            CircuitPin {
                label: "COIL+",
                role: PinRole::Positive,
                pos: Pos2::new(rect.left(), center.y - rect.height() * 0.25),
            },
            CircuitPin {
                label: "COIL-",
                role: PinRole::Ground,
                pos: Pos2::new(rect.left(), center.y + rect.height() * 0.25),
            },
            CircuitPin {
                label: "COM",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y - rect.height() * 0.28),
            },
            CircuitPin {
                label: "NO",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y),
            },
            CircuitPin {
                label: "NC",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y + rect.height() * 0.28),
            },
        ],
        ComponentKind::DcMotor => vec![
            CircuitPin {
                label: "-",
                role: PinRole::Ground,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "+",
                role: PinRole::Positive,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
        ComponentKind::Servo => vec![
            CircuitPin {
                label: "GND",
                role: PinRole::Ground,
                pos: Pos2::new(rect.left(), center.y - rect.height() * 0.24),
            },
            CircuitPin {
                label: "VCC",
                role: PinRole::Positive,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "SIG",
                role: PinRole::Digital,
                pos: Pos2::new(rect.left(), center.y + rect.height() * 0.24),
            },
        ],
        ComponentKind::Oled => {
            let step = (rect.width() - 16.0) / 3.0;
            vec![
                CircuitPin {
                    label: "GND",
                    role: PinRole::Ground,
                    pos: Pos2::new(rect.left() + 8.0, rect.top()),
                },
                CircuitPin {
                    label: "VCC",
                    role: PinRole::Positive,
                    pos: Pos2::new(rect.left() + 8.0 + step, rect.top()),
                },
                CircuitPin {
                    label: "SCL",
                    role: PinRole::I2c,
                    pos: Pos2::new(rect.left() + 8.0 + step * 2.0, rect.top()),
                },
                CircuitPin {
                    label: "SDA",
                    role: PinRole::I2c,
                    pos: Pos2::new(rect.left() + 8.0 + step * 3.0, rect.top()),
                },
            ]
        }
        ComponentKind::Sensor => vec![
            module_pin(rect, "GND", PinRole::Ground, false, 4, 0),
            module_pin(rect, "VCC", PinRole::Positive, false, 4, 1),
            module_pin(rect, "SCL", PinRole::I2c, false, 4, 2),
            module_pin(rect, "SDA", PinRole::I2c, true, 4, 2),
        ],
        ComponentKind::NetLabel
        | ComponentKind::Crystal
        | ComponentKind::Thermistor
        | ComponentKind::Varistor
        | ComponentKind::SchottkyDiode
        | ComponentKind::TvsDiode => vec![
            CircuitPin {
                label: "A",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "B",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
        ComponentKind::Transformer => vec![
            CircuitPin {
                label: "P1",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y - rect.height() * 0.22),
            },
            CircuitPin {
                label: "P2",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y + rect.height() * 0.22),
            },
            CircuitPin {
                label: "S1",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y - rect.height() * 0.22),
            },
            CircuitPin {
                label: "S2",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y + rect.height() * 0.22),
            },
        ],
        ComponentKind::VoltageRef => vec![
            CircuitPin {
                label: "A",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "K",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y),
            },
            CircuitPin {
                label: "ADJ",
                role: PinRole::Control,
                pos: Pos2::new(center.x, rect.bottom()),
            },
        ],
        ComponentKind::Phototransistor => vec![
            CircuitPin {
                label: "C",
                role: PinRole::Positive,
                pos: Pos2::new(rect.right(), rect.top() + rect.height() * 0.22),
            },
            CircuitPin {
                label: "E",
                role: PinRole::Ground,
                pos: Pos2::new(rect.right(), rect.bottom() - rect.height() * 0.22),
            },
        ],
        ComponentKind::Optocoupler => vec![
            CircuitPin {
                label: "A",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y - rect.height() * 0.22),
            },
            CircuitPin {
                label: "K",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y + rect.height() * 0.22),
            },
            CircuitPin {
                label: "C",
                role: PinRole::Positive,
                pos: Pos2::new(rect.right(), center.y - rect.height() * 0.22),
            },
            CircuitPin {
                label: "E",
                role: PinRole::Ground,
                pos: Pos2::new(rect.right(), center.y + rect.height() * 0.22),
            },
        ],
        ComponentKind::Timer555 => module_pin_defs(
            rect,
            &[
                ("GND", PinRole::Ground),
                ("TR", PinRole::Digital),
                ("Q", PinRole::Output),
                ("RST", PinRole::Control),
            ],
            &[
                ("VCC", PinRole::Positive),
                ("DIS", PinRole::Digital),
                ("THR", PinRole::Digital),
                ("CV", PinRole::Passive),
            ],
        ),
        ComponentKind::Display7Seg => module_pin_defs(
            rect,
            &[
                ("COM", PinRole::Ground),
                ("A", PinRole::Digital),
                ("B", PinRole::Digital),
                ("C", PinRole::Digital),
            ],
            &[
                ("D", PinRole::Digital),
                ("E", PinRole::Digital),
                ("F", PinRole::Digital),
                ("G", PinRole::Digital),
            ],
        ),
        ComponentKind::MotorDriver => module_pin_defs(
            rect,
            &[
                ("VCC", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("IN1", PinRole::Digital),
                ("IN2", PinRole::Digital),
            ],
            &[
                ("OUT1", PinRole::Output),
                ("OUT2", PinRole::Output),
                ("EN", PinRole::Control),
            ],
        ),
        ComponentKind::GenericIc => module_pin_defs(
            rect,
            &[
                ("VCC", PinRole::Positive),
                ("GND", PinRole::Ground),
                ("IN1", PinRole::Digital),
                ("IN2", PinRole::Digital),
            ],
            &[
                ("OUT1", PinRole::Output),
                ("OUT2", PinRole::Output),
                ("CLK", PinRole::Digital),
                ("RST", PinRole::Control),
            ],
        ),
        ComponentKind::Voltmeter | ComponentKind::Ammeter => vec![
            CircuitPin {
                label: "+",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "-",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
        ComponentKind::Dht11 | ComponentKind::Dht22 => vec![
            module_pin(rect, "VCC", PinRole::Positive, false, 3, 0),
            module_pin(rect, "DATA", PinRole::Digital, false, 3, 1),
            module_pin(rect, "GND", PinRole::Ground, false, 3, 2),
        ],
        ComponentKind::HcSr04 => vec![
            module_pin(rect, "VCC", PinRole::Positive, false, 4, 0),
            module_pin(rect, "TRIG", PinRole::Digital, false, 4, 1),
            module_pin(rect, "ECHO", PinRole::Digital, false, 4, 2),
            module_pin(rect, "GND", PinRole::Ground, false, 4, 3),
        ],
        ComponentKind::Buzzer => vec![
            CircuitPin {
                label: "+",
                role: PinRole::Positive,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "-",
                role: PinRole::Ground,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
        ComponentKind::NeoPixel => vec![
            module_pin(rect, "VCC", PinRole::Positive, false, 3, 0),
            module_pin(rect, "DIN", PinRole::Digital, false, 3, 1),
            module_pin(rect, "GND", PinRole::Ground, false, 3, 2),
        ],
        ComponentKind::PirSensor => vec![
            module_pin(rect, "VCC", PinRole::Positive, false, 3, 0),
            module_pin(rect, "OUT", PinRole::Output, false, 3, 1),
            module_pin(rect, "GND", PinRole::Ground, false, 3, 2),
        ],
        ComponentKind::Custom => component
            .part_id
            .as_deref()
            .and_then(super::custom_part::custom_part)
            .map(|def| module_pin_defs(rect, &def.left_pins, &def.right_pins))
            .unwrap_or_default(),
        ComponentKind::TextNote => vec![],
        _ => vec![
            CircuitPin {
                label: "A",
                role: PinRole::Passive,
                pos: Pos2::new(rect.left(), center.y),
            },
            CircuitPin {
                label: "B",
                role: PinRole::Passive,
                pos: Pos2::new(rect.right(), center.y),
            },
        ],
    };
    base.into_iter()
        .map(|pin| CircuitPin {
            pos: rotate_point(pin.pos, center, component.rotation),
            ..pin
        })
        .collect()
}

// ── Module pin layout helpers ─────────────────────────────────────────────────

pub(crate) fn module_pin_defs(
    rect: Rect,
    left: &[(&'static str, PinRole)],
    right: &[(&'static str, PinRole)],
) -> Vec<CircuitPin> {
    let mut pins = Vec::new();
    for (index, (label, role)) in left.iter().copied().enumerate() {
        pins.push(module_pin(rect, label, role, false, left.len(), index));
    }
    for (index, (label, role)) in right.iter().copied().enumerate() {
        pins.push(module_pin(rect, label, role, true, right.len(), index));
    }
    pins
}

pub(crate) fn module_pin(
    rect: Rect,
    label: &'static str,
    role: PinRole,
    right_side: bool,
    count: usize,
    index: usize,
) -> CircuitPin {
    CircuitPin {
        label,
        role,
        pos: Pos2::new(
            if right_side {
                rect.right()
            } else {
                rect.left()
            },
            module_pin_y(rect, count, index),
        ),
    }
}

pub(crate) fn breadboard_pin_defs(rect: Rect) -> Vec<CircuitPin> {
    vec![
        CircuitPin {
            label: "+",
            role: PinRole::Positive,
            pos: Pos2::new(rect.left(), rect.top() + 24.0),
        },
        CircuitPin {
            label: "-",
            role: PinRole::Ground,
            pos: Pos2::new(rect.left(), rect.top() + 44.0),
        },
        CircuitPin {
            label: "+",
            role: PinRole::Positive,
            pos: Pos2::new(rect.right(), rect.top() + 24.0),
        },
        CircuitPin {
            label: "-",
            role: PinRole::Ground,
            pos: Pos2::new(rect.right(), rect.top() + 44.0),
        },
        CircuitPin {
            label: "A",
            role: PinRole::Passive,
            pos: Pos2::new(rect.center().x - 50.0, rect.center().y),
        },
        CircuitPin {
            label: "B",
            role: PinRole::Passive,
            pos: Pos2::new(rect.center().x + 50.0, rect.center().y),
        },
    ]
}

pub(crate) fn module_pin_y(rect: Rect, count: usize, index: usize) -> f32 {
    if count <= 1 {
        return rect.center().y;
    }
    let middle = (count as f32 - 1.0) / 2.0;
    rect.center().y + (index as f32 - middle) * 20.0
}
