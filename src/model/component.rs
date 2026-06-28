use egui::Pos2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum ComponentKind {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    Led,
    ZenerDiode,
    Switch,
    PushButton,
    SlideSwitch,
    Ground,
    VSource,
    ISource,
    Battery,
    OpAmp,
    Lamp,
    Potentiometer,
    NpnTransistor,
    PnpTransistor,
    Nmosfet,
    Pmosfet,
    VoltageReg,
    Fuse,
    LogicNot,
    LogicAnd,
    LogicOr,
    LogicNand,
    LogicNor,
    LogicXor,
    Esp32,
    Esp32S3,
    Esp32C3,
    ArduinoUno,
    RaspberryPiPico,
    Stm32BluePill,
    Stm32Nucleo64,
    Breadboard,
    Relay,
    DcMotor,
    Servo,
    Oled,
    Sensor,
    NetLabel,
    Timer555,
    Crystal,
    Transformer,
    Display7Seg,
    Thermistor,
    Varistor,
    VoltageRef,
    MotorDriver,
    SchottkyDiode,
    TvsDiode,
    Phototransistor,
    Optocoupler,
    GenericIc,
    Voltmeter,
    Ammeter,
    TextNote,
    Dht11,
    Dht22,
    HcSr04,
    Buzzer,
    NeoPixel,
    PirSensor,
}

#[derive(Debug, Clone)]
pub(crate) struct Component {
    pub(crate) id: u64,
    pub(crate) kind: ComponentKind,
    pub(crate) pos: Pos2,
    pub(crate) rotation: i32,
    pub(crate) label: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SimulationSupport {
    /// Full DC operating-point via Modified Nodal Analysis.
    ExactDc,
    /// Simplified DC model (piecewise linear, companion model).
    /// Values are educational approximations — not SPICE sign-off accuracy.
    ApproximateDc,
    /// Digital logic only; analogue DC currents are not modelled.
    DigitalOnly,
    /// Schematic symbol only; no electrical behaviour is modelled at all.
    SymbolOnly,
    /// Connectivity and ERC are checked, but voltages/currents are not computed.
    Unsupported,
}

impl SimulationSupport {
    pub(crate) fn label(self) -> &'static str {
        match self {
            SimulationSupport::ExactDc => "Exact DC (MNA)",
            SimulationSupport::ApproximateDc => "Approximate DC",
            SimulationSupport::DigitalOnly => "Digital / logic only",
            SimulationSupport::SymbolOnly => "Symbol only",
            SimulationSupport::Unsupported => "Not simulated",
        }
    }

    pub(crate) fn warning(self) -> Option<&'static str> {
        match self {
            SimulationSupport::ExactDc => None,
            SimulationSupport::ApproximateDc => Some(
                "Values are educational approximations, not SPICE sign-off accuracy. \
                 Export to ngspice for precise results.",
            ),
            SimulationSupport::DigitalOnly => Some(
                "This part is treated as a digital element. \
                 Analogue DC currents and voltages are not modelled.",
            ),
            SimulationSupport::SymbolOnly => Some(
                "This part is a schematic symbol only. \
                 No electrical behaviour is modelled. Do not trust wiring state as a simulation result.",
            ),
            SimulationSupport::Unsupported => Some(
                "Cluster checks connectivity and ERC for this part but does not \
                 compute voltages or currents. Use ngspice for simulation.",
            ),
        }
    }

    /// True if this simulation support level implies the component is approximate
    /// or unsupported — show a warning in the inspector.
    pub(crate) fn needs_inspector_warning(self) -> bool {
        !matches!(self, SimulationSupport::ExactDc)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ElectricalMetadata {
    pub(crate) pin_count: Option<usize>,
    pub(crate) voltage_range: Option<(f32, f32)>,
    pub(crate) max_current: Option<f32>,
    pub(crate) needs_current_limit: bool,
    pub(crate) needs_driver: bool,
    pub(crate) simulation: SimulationSupport,
    pub(crate) model_name: &'static str,
}

pub(crate) fn electrical_metadata(kind: ComponentKind) -> ElectricalMetadata {
    use ComponentKind::*;
    use SimulationSupport::*;

    let (pin_count, simulation, model_name) = match kind {
        Resistor => (Some(2), ExactDc, "Linear resistor"),
        Capacitor => (Some(2), ExactDc, "Open circuit in DC"),
        Inductor => (Some(2), ExactDc, "Short circuit in DC"),
        Diode => (Some(2), ApproximateDc, "Piecewise silicon diode"),
        Led => (Some(2), ApproximateDc, "Piecewise LED"),
        ZenerDiode => (Some(2), ApproximateDc, "Zener breakdown approximation"),
        SchottkyDiode => (Some(2), ApproximateDc, "Piecewise Schottky diode"),
        TvsDiode => (Some(2), ApproximateDc, "DC clamp approximation"),
        Switch | PushButton | SlideSwitch => (Some(2), ExactDc, "Ideal open/closed switch"),
        Ground => (Some(1), ExactDc, "0 V reference"),
        VSource | Battery => (Some(2), ExactDc, "Ideal DC voltage source"),
        ISource => (Some(2), ExactDc, "Ideal DC current source"),
        Lamp => (
            Some(2),
            ApproximateDc,
            "Fixed resistance load — approximate",
        ),
        Potentiometer => (Some(3), ExactDc, "Fixed wiper approximation"),
        NpnTransistor | PnpTransistor => (Some(3), ApproximateDc, "Linearized BJT companion model"),
        Nmosfet | Pmosfet => (Some(3), ApproximateDc, "Threshold-switch MOSFET model"),
        VoltageReg => (
            Some(3),
            ApproximateDc,
            "Pass-through regulator — approximate; no regulation modelled",
        ),
        Fuse => (Some(2), ExactDc, "Low resistance fuse"),
        Relay => (Some(5), ApproximateDc, "Coil resistance + ideal contact"),
        DcMotor => (
            Some(2),
            ApproximateDc,
            "Fixed winding resistance — no back-EMF",
        ),
        Thermistor | Varistor => (Some(2), ApproximateDc, "Fixed resistance approximation"),
        Phototransistor => (Some(2), ApproximateDc, "Fixed resistance approximation"),
        Voltmeter => (Some(2), ExactDc, "1 MΩ voltmeter probe"),
        Ammeter => (Some(2), ExactDc, "1 mΩ ammeter shunt"),
        Timer555 => (Some(8), SymbolOnly, "Symbolic — not modelled in DC"),
        LogicNot | LogicAnd | LogicOr | LogicNand | LogicNor | LogicXor => (
            None,
            DigitalOnly,
            "Digital logic gate — no DC current model",
        ),
        Esp32 | Esp32S3 | Esp32C3 | ArduinoUno | RaspberryPiPico | Stm32BluePill
        | Stm32Nucleo64 => (None, Unsupported, "MCU — connectivity and ERC only"),
        Breadboard => (None, SymbolOnly, "Symbolic breadboard"),
        Servo => (Some(3), SymbolOnly, "Symbolic PWM servo"),
        Oled | Sensor => (
            Some(4),
            Unsupported,
            "I²C module — connectivity and ERC only",
        ),
        NetLabel => (Some(2), Unsupported, "Net alias"),
        Crystal => (Some(2), SymbolOnly, "Symbolic crystal oscillator"),
        Transformer => (Some(4), SymbolOnly, "Symbolic transformer"),
        Display7Seg => (None, SymbolOnly, "Symbolic 7-segment display"),
        VoltageRef => (Some(3), ApproximateDc, "Approximate voltage reference stub"),
        MotorDriver => (None, Unsupported, "Motor driver — ERC only"),
        Optocoupler => (Some(4), SymbolOnly, "Symbolic optocoupler"),
        GenericIc => (None, SymbolOnly, "Symbolic generic IC"),
        OpAmp => (
            Some(3),
            SymbolOnly,
            "Symbolic op-amp — SPICE needed for accuracy",
        ),
        TextNote => (Some(0), SymbolOnly, "Annotation only"),
        Dht11 => (Some(3), Unsupported, "1-Wire digital sensor — ERC only"),
        Dht22 => (Some(3), Unsupported, "1-Wire digital sensor — ERC only"),
        HcSr04 => (Some(4), Unsupported, "Ultrasonic sensor — ERC only"),
        Buzzer => (
            Some(2),
            ApproximateDc,
            "Piezo buzzer — resistive load approximation",
        ),
        NeoPixel => (Some(3), Unsupported, "WS2812 LED — ERC only"),
        PirSensor => (Some(3), Unsupported, "PIR sensor — ERC only"),
    };

    let (voltage_range, max_current, needs_current_limit, needs_driver) = match kind {
        Led => (Some((-0.3, 3.3)), Some(0.025), true, false),
        Esp32 | Esp32S3 | Esp32C3 => (Some((0.0, 3.6)), Some(0.012), false, false),
        ArduinoUno => (Some((0.0, 5.5)), Some(0.020), false, false),
        RaspberryPiPico | Stm32BluePill | Stm32Nucleo64 => {
            (Some((0.0, 3.6)), Some(0.020), false, false)
        }
        DcMotor | Relay | Servo | Lamp => (None, None, false, true),
        Dht11 | Dht22 => (Some((3.0, 5.5)), Some(0.002), false, false),
        HcSr04 => (Some((4.5, 5.5)), None, false, false),
        Buzzer => (Some((3.0, 5.5)), Some(0.030), false, false),
        NeoPixel => (Some((4.5, 5.5)), Some(0.060), false, false),
        PirSensor => (Some((4.5, 12.0)), None, false, false),
        _ => (None, None, false, false),
    };

    ElectricalMetadata {
        pin_count,
        voltage_range,
        max_current,
        needs_current_limit,
        needs_driver,
        simulation,
        model_name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_marks_protection_driver_and_symbolic_models() {
        assert!(electrical_metadata(ComponentKind::Led).needs_current_limit);
        assert!(electrical_metadata(ComponentKind::DcMotor).needs_driver);
        assert_eq!(
            electrical_metadata(ComponentKind::Timer555).simulation,
            SimulationSupport::SymbolOnly
        );
        assert_eq!(
            electrical_metadata(ComponentKind::Led).simulation,
            SimulationSupport::ApproximateDc
        );
        assert!(
            electrical_metadata(ComponentKind::Led)
                .simulation
                .warning()
                .unwrap()
                .contains("approximations")
        );
    }
}
