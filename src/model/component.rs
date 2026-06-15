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
    DcMna,
    ConnectivityOnly,
    Symbolic,
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
        Resistor => (Some(2), DcMna, "Linear resistor"),
        Capacitor => (Some(2), DcMna, "Open circuit in DC"),
        Inductor => (Some(2), DcMna, "Short circuit in DC"),
        Diode => (Some(2), DcMna, "Piecewise silicon diode"),
        Led => (Some(2), DcMna, "Piecewise LED"),
        ZenerDiode => (Some(2), DcMna, "Zener breakdown approximation"),
        SchottkyDiode => (Some(2), DcMna, "Piecewise Schottky diode"),
        TvsDiode => (Some(2), ConnectivityOnly, "DC clamp approximation"),
        Switch | PushButton | SlideSwitch => (Some(2), DcMna, "Ideal open/closed switch"),
        Ground => (Some(1), DcMna, "0 V reference"),
        VSource | Battery => (Some(2), DcMna, "Ideal DC voltage source"),
        ISource => (Some(2), DcMna, "Ideal DC current source"),
        Lamp => (Some(2), DcMna, "Fixed resistance load"),
        Potentiometer => (Some(3), DcMna, "Fixed wiper approximation"),
        NpnTransistor | PnpTransistor => (Some(3), DcMna, "Linearized BJT"),
        Nmosfet | Pmosfet => (Some(3), DcMna, "Threshold switch MOSFET"),
        VoltageReg => (
            Some(3),
            ConnectivityOnly,
            "Pass-through regulator approximation",
        ),
        Fuse => (Some(2), DcMna, "Low resistance fuse"),
        Relay => (Some(5), ConnectivityOnly, "Coil + ideal contact"),
        DcMotor => (Some(2), ConnectivityOnly, "Fixed resistance motor load"),
        Thermistor | Varistor => (Some(2), DcMna, "Fixed resistance approximation"),
        Phototransistor => (Some(2), ConnectivityOnly, "Fixed resistance approximation"),
        Voltmeter => (Some(2), DcMna, "1 Mohm voltmeter"),
        Ammeter => (Some(2), DcMna, "1 milliohm ammeter"),
        Timer555 => (Some(8), Symbolic, "Symbolic in DC simulation"),
        LogicNot | LogicAnd | LogicOr | LogicNand | LogicNor | LogicXor => {
            (None, Symbolic, "Symbolic logic component")
        }
        Esp32 | Esp32S3 | Esp32C3 | ArduinoUno | RaspberryPiPico => {
            (None, ConnectivityOnly, "Powered module connectivity model")
        }
        Breadboard => (None, Symbolic, "Symbolic breadboard"),
        Servo => (Some(3), Symbolic, "Symbolic PWM servo"),
        Oled | Sensor => (Some(4), Symbolic, "Symbolic I2C module"),
        NetLabel => (Some(2), ConnectivityOnly, "Net alias"),
        Crystal => (Some(2), Symbolic, "Symbolic crystal"),
        Transformer => (Some(4), Symbolic, "Symbolic transformer"),
        Display7Seg => (None, Symbolic, "Symbolic display"),
        VoltageRef => (Some(3), Symbolic, "Symbolic voltage reference"),
        MotorDriver => (None, Symbolic, "Symbolic motor driver"),
        Optocoupler => (Some(4), Symbolic, "Symbolic optocoupler"),
        GenericIc => (None, Symbolic, "Symbolic generic IC"),
        OpAmp => (Some(3), Symbolic, "Symbolic op amp"),
        TextNote => (Some(0), Symbolic, "Annotation only"),
    };

    let (voltage_range, max_current, needs_current_limit, needs_driver) = match kind {
        Led => (Some((-0.3, 3.3)), Some(0.025), true, false),
        Esp32 | Esp32S3 | Esp32C3 => (Some((0.0, 3.6)), Some(0.012), false, false),
        ArduinoUno => (Some((0.0, 5.5)), Some(0.020), false, false),
        RaspberryPiPico => (Some((0.0, 3.6)), Some(0.012), false, false),
        DcMotor | Relay | Servo | Lamp => (None, None, false, true),
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
            SimulationSupport::Symbolic
        );
    }
}
