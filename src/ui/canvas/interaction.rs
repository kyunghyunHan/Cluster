//! Canvas pan, zoom, placement, drag, and wiring interaction boundary.

use crate::model::{CircuitPin, PinRole};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SmartWireTone {
    Compatible,
    Neutral,
    Suspicious,
}

pub(crate) fn assess_pin_pair(
    source: &CircuitPin,
    target: &CircuitPin,
) -> (SmartWireTone, &'static str) {
    use PinRole::*;
    if source.role == NoConnect || target.role == NoConnect {
        return (
            SmartWireTone::Suspicious,
            "No-connect pins should normally remain unwired.",
        );
    }
    if source.role == Ground || target.role == Ground {
        return if source.role == Ground && target.role == Ground {
            (SmartWireTone::Compatible, "Compatible ground connection.")
        } else {
            (
                SmartWireTone::Suspicious,
                "Ground should normally connect to another GND pin.",
            )
        };
    }
    let source_name = source.label.to_ascii_lowercase();
    let target_name = target.label.to_ascii_lowercase();
    if source.role == I2c || target.role == I2c {
        let same_signal = (source_name.contains("sda") && target_name.contains("sda"))
            || (source_name.contains("scl") && target_name.contains("scl"));
        return if same_signal {
            (SmartWireTone::Compatible, "Compatible I²C signal.")
        } else {
            (
                SmartWireTone::Suspicious,
                "Check SDA/SCL: these I²C pin names do not match.",
            )
        };
    }
    if matches!(source.role, Output | PowerOutput) && matches!(target.role, Output | PowerOutput) {
        return (
            SmartWireTone::Suspicious,
            "Two driven outputs can electrically conflict.",
        );
    }
    if matches!(source.role, Digital | Output | Bidirectional) && target.role == Positive {
        return (
            SmartWireTone::Suspicious,
            "GPIO/signal should not drive a module power input.",
        );
    }
    if matches!(source.role, Positive | PowerOutput) && matches!(target.role, Positive | Passive) {
        return (
            SmartWireTone::Compatible,
            "Compatible power connection; verify the voltage rating.",
        );
    }
    if matches!(
        source.role,
        Output | Digital | Bidirectional | OpenCollector
    ) && matches!(
        target.role,
        Input | Digital | Bidirectional | Control | Passive
    ) {
        return (SmartWireTone::Compatible, "Compatible signal connection.");
    }
    (
        SmartWireTone::Neutral,
        "Manual connection; verify pin roles and voltage levels.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;

    fn pin(label: &'static str, role: PinRole) -> CircuitPin {
        CircuitPin {
            label,
            role,
            pos: Pos2::ZERO,
        }
    }

    #[test]
    fn smart_wiring_matches_i2c_names_and_rejects_swaps() {
        assert_eq!(
            assess_pin_pair(&pin("SDA", PinRole::I2c), &pin("SDA", PinRole::I2c)).0,
            SmartWireTone::Compatible
        );
        assert_eq!(
            assess_pin_pair(&pin("SDA", PinRole::I2c), &pin("SCL", PinRole::I2c)).0,
            SmartWireTone::Suspicious
        );
    }

    #[test]
    fn smart_wiring_warns_for_output_conflict() {
        assert_eq!(
            assess_pin_pair(&pin("OUT", PinRole::Output), &pin("OUT", PinRole::Output)).0,
            SmartWireTone::Suspicious
        );
    }
}
