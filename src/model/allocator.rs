use super::{Component, ComponentKind, Counters, custom_part};

/// Command-local allocator. It is copied from the document before a command
/// and committed only after the command has completed.
#[derive(Debug, Clone)]
pub(crate) struct IdAllocator {
    next_id: u64,
    counters: Counters,
}

impl IdAllocator {
    pub(crate) fn new(next_id: u64, counters: Counters) -> Self {
        Self { next_id, counters }
    }

    pub(crate) fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    pub(crate) fn allocate_label(
        &mut self,
        kind: ComponentKind,
        components: &[Component],
    ) -> String {
        macro_rules! label {
            ($field:ident, $prefix:literal) => {{
                self.counters.$field += 1;
                format!("{}{}", $prefix, self.counters.$field)
            }};
            ($field:ident, $prefix:literal, $offset:expr) => {{
                self.counters.$field += 1;
                format!("{}{}", $prefix, self.counters.$field + $offset)
            }};
        }
        match kind {
            ComponentKind::Resistor => label!(resistor, "R"),
            ComponentKind::Capacitor => label!(capacitor, "C"),
            ComponentKind::Inductor => label!(inductor, "L"),
            ComponentKind::Diode => label!(diode, "D"),
            ComponentKind::Led => label!(led, "LED"),
            ComponentKind::ZenerDiode => label!(zener, "ZD"),
            ComponentKind::NpnTransistor => label!(npn, "Q"),
            ComponentKind::PnpTransistor => label!(pnp, "Q", 100),
            ComponentKind::Nmosfet => label!(mosfet, "M"),
            ComponentKind::Pmosfet => label!(mosfet, "M", 100),
            ComponentKind::Potentiometer => label!(pot, "RV"),
            ComponentKind::VoltageReg => label!(vreg, "U", 50),
            ComponentKind::Fuse => label!(fuse, "F"),
            ComponentKind::LogicNot
            | ComponentKind::LogicAnd
            | ComponentKind::LogicOr
            | ComponentKind::LogicNand
            | ComponentKind::LogicNor
            | ComponentKind::LogicXor => {
                self.counters.logic_gate += 1;
                let prefix = match kind {
                    ComponentKind::LogicNot => "INV",
                    ComponentKind::LogicAnd => "AND",
                    ComponentKind::LogicOr => "OR",
                    ComponentKind::LogicNand => "NAND",
                    ComponentKind::LogicNor => "NOR",
                    ComponentKind::LogicXor => "XOR",
                    _ => unreachable!(),
                };
                format!("{prefix}{}", self.counters.logic_gate)
            }
            ComponentKind::Switch | ComponentKind::PushButton | ComponentKind::SlideSwitch => {
                label!(switch, "SW")
            }
            ComponentKind::Ground => {
                self.counters.ground += 1;
                if self.counters.ground == 1 {
                    "GND".to_string()
                } else {
                    format!("GND{}", self.counters.ground)
                }
            }
            ComponentKind::VSource => label!(vsource, "V"),
            ComponentKind::ISource => label!(isource, "I"),
            ComponentKind::Battery => label!(battery, "BAT"),
            ComponentKind::OpAmp => label!(opamp, "U"),
            ComponentKind::Lamp => label!(lamp, "LA"),
            ComponentKind::Esp32 | ComponentKind::Esp32S3 | ComponentKind::Esp32C3 => {
                label!(esp32, "ESP")
            }
            ComponentKind::ArduinoUno => label!(arduino, "ARD"),
            ComponentKind::RaspberryPiPico => label!(pico, "PICO"),
            ComponentKind::Stm32BluePill | ComponentKind::Stm32Nucleo64 => {
                label!(logic_gate, "STM")
            }
            ComponentKind::Breadboard => label!(breadboard, "BB"),
            ComponentKind::Relay => label!(relay, "K"),
            ComponentKind::DcMotor => label!(motor, "M"),
            ComponentKind::Servo => label!(servo, "SV"),
            ComponentKind::Oled => label!(oled, "OLED"),
            ComponentKind::Sensor => label!(sensor, "SEN"),
            ComponentKind::NetLabel => "NET1".to_string(),
            ComponentKind::Timer555 => label!(logic_gate, "U", 200),
            ComponentKind::Crystal => label!(logic_gate, "X"),
            ComponentKind::Transformer => label!(logic_gate, "T"),
            ComponentKind::Display7Seg => label!(oled, "DS"),
            ComponentKind::Thermistor => label!(resistor, "RT"),
            ComponentKind::Varistor => label!(resistor, "RV"),
            ComponentKind::VoltageRef => label!(vreg, "VR"),
            ComponentKind::MotorDriver => label!(motor, "MD"),
            ComponentKind::SchottkyDiode => label!(diode, "DS"),
            ComponentKind::TvsDiode => label!(diode, "DT"),
            ComponentKind::Phototransistor => label!(npn, "QP"),
            ComponentKind::Optocoupler => label!(logic_gate, "OK"),
            ComponentKind::GenericIc => label!(logic_gate, "IC"),
            ComponentKind::Voltmeter => label!(meter, "VM"),
            ComponentKind::Ammeter => label!(meter, "AM"),
            ComponentKind::TextNote => "NOTE".to_string(),
            ComponentKind::Dht11 | ComponentKind::Dht22 => label!(dht, "DHT"),
            ComponentKind::HcSr04 => label!(hcsr04, "US"),
            ComponentKind::Buzzer => label!(buzzer, "BZ"),
            ComponentKind::NeoPixel => label!(neopixel, "NP"),
            ComponentKind::PirSensor => label!(pir, "PIR"),
            ComponentKind::Custom => self.allocate_custom_label(None, components),
        }
    }

    pub(crate) fn allocate_custom_label(
        &self,
        part_id: Option<&str>,
        components: &[Component],
    ) -> String {
        let prefix = part_id
            .and_then(custom_part)
            .map(|definition| definition.label_prefix)
            .unwrap_or_else(|| "U".to_string());
        let highest = components
            .iter()
            .filter_map(|component| component.label.strip_prefix(&prefix))
            .filter_map(|suffix| suffix.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        format!("{prefix}{}", highest + 1)
    }

    pub(crate) fn commit(self, next_id: &mut u64, counters: &mut Counters) {
        *next_id = self.next_id;
        *counters = self.counters;
    }

    pub(crate) fn reset(&mut self) {
        self.next_id = 1;
        self.counters = Counters::default();
    }
}
