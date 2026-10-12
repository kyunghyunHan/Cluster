//! Partial, source-backed device capabilities. Package/header identities and
//! electrical ratings are separate; unknown board variants remain unknown.
#![allow(dead_code)]
use super::{ComponentKind, ElectricalType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProfileCoverage {
    BoardPartial,
    ChipOnly,
    UnspecifiedVariant,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FiveVoltTolerance {
    No,
    ConditionalDigitalInput,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum McuPin {
    Gpio(u8),
    Port { port: char, bit: u8 },
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum InputMaximum {
    SupplyPlus(f32),
    Unknown,
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct BoardProfile {
    pub(crate) name: &'static str,
    pub(crate) chip: Option<&'static str>,
    pub(crate) logic_voltage_v: f32,
    pub(crate) coverage: ProfileCoverage,
    pub(crate) source: &'static str,
}
#[derive(Debug, Clone)]
pub(crate) struct PinCapability {
    pub(crate) board_header_label: String,
    pub(crate) board_header_pin: Option<u8>,
    pub(crate) physical_package_pin: Option<u8>,
    pub(crate) mcu_pin: Option<McuPin>,
    pub(crate) alternate_functions: Vec<&'static str>,
    pub(crate) electrical_type: ElectricalType,
    pub(crate) voltage_domain_v: f32,
    pub(crate) input_maximum: InputMaximum,
    pub(crate) five_volt_tolerance: FiveVoltTolerance,
    pub(crate) absolute_max_current_a: Option<f32>,
    pub(crate) boot_constraint: Option<&'static str>,
    pub(crate) input_only: bool,
    pub(crate) adc_capable: bool,
}

pub(crate) fn board_profile(kind: ComponentKind) -> Option<BoardProfile> {
    use ComponentKind::*;
    let (name, chip, voltage, coverage, source) = match kind {
        Esp32 => (
            "ESP32 DevKit V1",
            Some("ESP32"),
            3.3,
            ProfileCoverage::ChipOnly,
            "https://documentation.espressif.com/esp32_datasheet_en.html",
        ),
        Esp32S3 => (
            "ESP32-S3 module",
            Some("ESP32-S3"),
            3.3,
            ProfileCoverage::ChipOnly,
            "https://documentation.espressif.com/esp32_s3_datasheet_en.pdf",
        ),
        Esp32C3 => (
            "ESP32-C3 module",
            Some("ESP32-C3"),
            3.3,
            ProfileCoverage::ChipOnly,
            "https://docs.espressif.com/projects/esp-hardware-design-guidelines/en/latest/esp32c3/schematic-checklist.html",
        ),
        ArduinoUno => (
            "Arduino UNO R3",
            Some("ATmega328P"),
            5.0,
            ProfileCoverage::BoardPartial,
            "https://docs.arduino.cc/resources/datasheets/A000066-datasheet.pdf",
        ),
        RaspberryPiPico => (
            "Raspberry Pi Pico (RP2040)",
            Some("RP2040"),
            3.3,
            ProfileCoverage::BoardPartial,
            "https://datasheets.raspberrypi.com/pico/pico-datasheet.pdf",
        ),
        Stm32BluePill => (
            "Blue Pill F103C8",
            Some("STM32F103C8"),
            3.3,
            ProfileCoverage::ChipOnly,
            "https://www.st.com/resource/en/datasheet/stm32f103v8.pdf",
        ),
        Stm32Nucleo64 => (
            "Nucleo-64 (variant unspecified)",
            None,
            3.3,
            ProfileCoverage::UnspecifiedVariant,
            "https://www.st.com/resource/en/user_manual/dm00105823-nucleo-f401re-user-manual-stmicroelectronics.pdf",
        ),
        _ => return None,
    };
    Some(BoardProfile {
        name,
        chip,
        logic_voltage_v: voltage,
        coverage,
        source,
    })
}

pub(crate) fn pin_capability(kind: ComponentKind, label: &str) -> Option<PinCapability> {
    let profile = board_profile(kind)?;
    let token = label.split_whitespace().next()?;
    let gpio = token
        .strip_prefix("GPIO")
        .or_else(|| token.strip_prefix("GP"))
        .and_then(|s| s.parse::<u8>().ok());
    let port = label.split_whitespace().find_map(|s| {
        let chars = s.as_bytes();
        if chars.len() >= 3 && chars[0] == b'P' && (b'A'..=b'C').contains(&chars[1]) {
            s[2..].parse::<u8>().ok().map(|bit| McuPin::Port {
                port: chars[1] as char,
                bit,
            })
        } else {
            None
        }
    });
    let mut mcu_pin = gpio.map(McuPin::Gpio).or(port);
    let mut header_pin = None;
    if kind == ComponentKind::RaspberryPiPico
        && let Some(n) = gpio
    {
        header_pin = match n {
            0..=1 => Some(n + 1),
            2..=5 => Some(n + 2),
            6..=9 => Some(n + 3),
            10..=13 => Some(n + 4),
            14..=17 => Some(n + 5),
            18..=21 => Some(n + 6),
            22 => Some(29),
            26 => Some(31),
            27 => Some(32),
            28 => Some(34),
            _ => None,
        };
    }
    if kind == ComponentKind::ArduinoUno {
        mcu_pin = token
            .strip_prefix('D')
            .and_then(|s| s.parse::<u8>().ok())
            .and_then(|n| match n {
                0..=7 => Some(McuPin::Port { port: 'D', bit: n }),
                8..=13 => Some(McuPin::Port {
                    port: 'B',
                    bit: n - 8,
                }),
                _ => None,
            })
            .or_else(|| {
                token
                    .strip_prefix('A')
                    .and_then(|s| s.parse::<u8>().ok())
                    .filter(|n| *n <= 5)
                    .map(|n| McuPin::Port { port: 'C', bit: n })
            });
    }
    let input_only = kind == ComponentKind::Esp32 && gpio.is_some_and(|n| (34..=39).contains(&n))
        || kind == ComponentKind::Esp32S3 && gpio == Some(46);
    let strapping = match kind {
        ComponentKind::Esp32 => gpio.is_some_and(|n| [0, 2, 5, 12, 15].contains(&n)),
        ComponentKind::Esp32S3 => gpio.is_some_and(|n| [0, 3, 45, 46].contains(&n)),
        ComponentKind::Esp32C3 => gpio.is_some_and(|n| [2, 8, 9].contains(&n)),
        _ => false,
    };
    let adc = match (&mcu_pin, kind) {
        (Some(McuPin::Gpio(n)), ComponentKind::Esp32) => [
            0, 2, 4, 12, 13, 14, 15, 25, 26, 27, 32, 33, 34, 35, 36, 37, 38, 39,
        ]
        .contains(n),
        (Some(McuPin::Gpio(n)), ComponentKind::RaspberryPiPico) => (26..=28).contains(n),
        (Some(McuPin::Port { port, bit }), ComponentKind::Stm32BluePill) => {
            (*port == 'A' && *bit <= 7) || (*port == 'B' && *bit <= 1)
        }
        (Some(McuPin::Port { port: 'C', bit }), ComponentKind::ArduinoUno) => *bit <= 5,
        _ => false,
    };
    let five_volt_tolerance = match kind {
        ComponentKind::Esp32
        | ComponentKind::Esp32S3
        | ComponentKind::Esp32C3
        | ComponentKind::RaspberryPiPico => FiveVoltTolerance::No,
        ComponentKind::Stm32BluePill if adc => FiveVoltTolerance::No,
        ComponentKind::Stm32BluePill
            if matches!(
                mcu_pin,
                Some(McuPin::Port {
                    port: 'B',
                    bit: 6 | 7
                }) | Some(McuPin::Port {
                    port: 'A',
                    bit: 9 | 10
                })
            ) =>
        {
            FiveVoltTolerance::ConditionalDigitalInput
        }
        _ => FiveVoltTolerance::Unknown,
    };
    let mut alternate_functions = Vec::new();
    for (name, needle) in [
        ("I2C SDA", "SDA"),
        ("I2C SCL", "SCL"),
        ("SPI MOSI", "MOSI"),
        ("SPI MISO", "MISO"),
        ("SPI clock", "SCK"),
        ("UART TX", "TX"),
        ("UART RX", "RX"),
    ] {
        if label.contains(needle) {
            alternate_functions.push(name);
        }
    }
    if adc {
        alternate_functions.push("ADC");
    }
    Some(PinCapability {
        board_header_label: label.into(),
        board_header_pin: header_pin,
        physical_package_pin: None,
        mcu_pin,
        alternate_functions,
        electrical_type: if input_only {
            ElectricalType::Input
        } else {
            ElectricalType::Bidirectional
        },
        voltage_domain_v: profile.logic_voltage_v,
        input_maximum: InputMaximum::Unknown,
        five_volt_tolerance,
        absolute_max_current_a: None,
        boot_constraint: strapping
            .then_some("Strapping pin: external bias affects boot; consult datasheet."),
        input_only,
        adc_capable: adc,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audit_device_identity_and_constraints_are_separate() {
        let pico = pin_capability(ComponentKind::RaspberryPiPico, "GP26 ADC").unwrap();
        assert_eq!(pico.board_header_pin, Some(31));
        assert_eq!(pico.mcu_pin, Some(McuPin::Gpio(26)));
        assert_eq!(pico.physical_package_pin, None);
        assert!(pico.adc_capable);
        let esp = pin_capability(ComponentKind::Esp32, "GPIO34 ADC").unwrap();
        assert!(esp.input_only);
        assert!(
            pin_capability(ComponentKind::Esp32, "GPIO12")
                .unwrap()
                .boot_constraint
                .is_some()
        );
        assert_eq!(
            pin_capability(ComponentKind::Stm32BluePill, "PA0 ADC")
                .unwrap()
                .five_volt_tolerance,
            FiveVoltTolerance::No
        );
        assert_eq!(
            pin_capability(ComponentKind::Stm32BluePill, "PB6 SCL")
                .unwrap()
                .five_volt_tolerance,
            FiveVoltTolerance::ConditionalDigitalInput
        );
        assert_eq!(
            board_profile(ComponentKind::Stm32Nucleo64).unwrap().chip,
            None
        );
    }
}
