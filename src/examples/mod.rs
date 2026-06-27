use egui::Pos2;

use crate::model::ComponentKind;

impl crate::CircuitApp {
    pub(crate) fn load_led_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(180.0, 390.0));
        let resistor = self.place_component(ComponentKind::Resistor, Pos2::new(360.0, 220.0));
        let led = self.place_component(ComponentKind::Led, Pos2::new(540.0, 220.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(720.0, 400.0));

        self.add_wire_between(battery, "+", resistor, "A");
        self.add_wire_between(resistor, "B", led, "A");
        self.add_wire_between(led, "B", ground, "GND");
        self.add_wire_between(battery, "-", ground, "GND");
        self.place_note(
            Pos2::new(440.0, 110.0),
            "EXPECT: ON\nBattery + -> resistor -> LED -> GND",
        );
        self.status = "Loaded LED current-limiting demo.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_switch_led_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(180.0, 390.0));
        let resistor = self.place_component(ComponentKind::Resistor, Pos2::new(360.0, 220.0));
        let led = self.place_component(ComponentKind::Led, Pos2::new(540.0, 220.0));
        let switch = self.place_component(ComponentKind::Switch, Pos2::new(700.0, 320.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(820.0, 430.0));

        self.add_wire_between(battery, "+", resistor, "A");
        self.add_wire_between(resistor, "B", led, "A");
        self.add_wire_between(led, "B", switch, "A");
        self.add_wire_between(switch, "B", ground, "GND");
        self.add_wire_between(battery, "-", ground, "GND");
        self.place_note(
            Pos2::new(490.0, 120.0),
            "EXPECT: ON when SW1 is closed\nOpen SW1 to break current.",
        );
        self.status =
            "Loaded switch-controlled LED demo. Set SW1 open/closed to compare.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_open_switch_led_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(180.0, 390.0));
        let resistor = self.place_component(ComponentKind::Resistor, Pos2::new(360.0, 220.0));
        let led = self.place_component(ComponentKind::Led, Pos2::new(540.0, 220.0));
        let switch = self.place_component(ComponentKind::Switch, Pos2::new(700.0, 320.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(820.0, 430.0));

        if let Some(component) = self
            .components
            .iter_mut()
            .find(|component| component.id == switch)
        {
            component.value = "open".to_string();
        }
        self.add_wire_between(battery, "+", resistor, "A");
        self.add_wire_between(resistor, "B", led, "A");
        self.add_wire_between(led, "B", switch, "A");
        self.add_wire_between(switch, "B", ground, "GND");
        self.add_wire_between(battery, "-", ground, "GND");
        self.place_note(
            Pos2::new(490.0, 120.0),
            "EXPECT: OFF / Open circuit\nSW1 is open, so current must not pass.",
        );
        self.status = "Loaded open-switch lesson. LED should stay off.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_capacitor_dc_block_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(180.0, 390.0));
        let resistor = self.place_component(ComponentKind::Resistor, Pos2::new(360.0, 220.0));
        let capacitor = self.place_component(ComponentKind::Capacitor, Pos2::new(540.0, 220.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(720.0, 400.0));

        self.add_wire_between(battery, "+", resistor, "A");
        self.add_wire_between(resistor, "B", capacitor, "A");
        self.add_wire_between(capacitor, "B", ground, "GND");
        self.add_wire_between(battery, "-", ground, "GND");
        self.place_note(
            Pos2::new(450.0, 110.0),
            "EXPECT: OFF in DC\nCapacitor blocks steady current.",
        );
        self.status =
            "Loaded DC capacitor block lesson. Simulation should show Open circuit.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_missing_return_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(180.0, 350.0));
        let resistor = self.place_component(ComponentKind::Resistor, Pos2::new(360.0, 220.0));
        let led = self.place_component(ComponentKind::Led, Pos2::new(540.0, 220.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(720.0, 390.0));

        self.add_wire_between(battery, "+", resistor, "A");
        self.add_wire_between(resistor, "B", led, "A");
        self.add_wire_between(battery, "-", ground, "GND");
        self.place_note(
            Pos2::new(450.0, 110.0),
            "EXPECT: OFF / Open circuit\nLED cathode is not returned to GND.",
        );
        self.status =
            "Loaded missing-return lesson. Complete the LED to GND wire to fix it.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_short_circuit_lesson_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(180.0, 340.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(460.0, 340.0));

        self.add_wire_between(battery, "+", ground, "GND");
        self.add_wire_between(battery, "-", ground, "GND");
        self.place_note(
            Pos2::new(330.0, 180.0),
            "EXPECT: ERROR / Short circuit\nBattery + is tied directly to GND.",
        );
        self.status = "Loaded short-circuit lesson. ERC should report an error.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_direct_gpio_motor_warning_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(150.0, 430.0));
        let esp32 = self.place_component(ComponentKind::Esp32, Pos2::new(430.0, 320.0));
        let motor = self.place_component(ComponentKind::DcMotor, Pos2::new(720.0, 260.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(820.0, 430.0));

        self.add_wire_between(battery, "+", esp32, "VIN");
        self.add_wire_between(battery, "-", ground, "GND");
        self.add_wire_between(esp32, "GND", ground, "GND");
        self.add_wire_between(esp32, "GPIO18", motor, "+");
        self.add_wire_between(motor, "-", ground, "GND");
        self.place_note(
            Pos2::new(600.0, 120.0),
            "EXPECT: WARNING\nGPIO should not drive a motor directly.",
        );
        self.status =
            "Loaded direct-GPIO motor warning lesson. Use a motor driver instead.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_parallel_led_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(160.0, 360.0));
        let r1 = self.place_component(ComponentKind::Resistor, Pos2::new(360.0, 220.0));
        let led1 = self.place_component(ComponentKind::Led, Pos2::new(540.0, 220.0));
        let r2 = self.place_component(ComponentKind::Resistor, Pos2::new(360.0, 360.0));
        let led2 = self.place_component(ComponentKind::Led, Pos2::new(540.0, 360.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(740.0, 470.0));

        self.add_wire_between(battery, "+", r1, "A");
        self.add_wire_between(battery, "+", r2, "A");
        self.add_wire_between(r1, "B", led1, "A");
        self.add_wire_between(r2, "B", led2, "A");
        self.add_wire_between(led1, "B", ground, "GND");
        self.add_wire_between(led2, "B", ground, "GND");
        self.add_wire_between(battery, "-", ground, "GND");
        self.place_note(
            Pos2::new(440.0, 120.0),
            "EXPECT: BOTH ON\nEach LED has its own resistor.",
        );
        self.status = "Loaded parallel LEDs demo with one resistor per LED.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_lamp_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(180.0, 360.0));
        let fuse = self.place_component(ComponentKind::Fuse, Pos2::new(360.0, 220.0));
        let lamp = self.place_component(ComponentKind::Lamp, Pos2::new(540.0, 220.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(720.0, 390.0));

        self.add_wire_between(battery, "+", fuse, "A");
        self.add_wire_between(fuse, "B", lamp, "A");
        self.add_wire_between(lamp, "B", ground, "GND");
        self.add_wire_between(battery, "-", ground, "GND");
        self.place_note(
            Pos2::new(430.0, 110.0),
            "EXPECT: ON\nFuse is in series with the lamp.",
        );
        self.status = "Loaded fused lamp demo.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_ohms_law_meter_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(160.0, 390.0));
        let ammeter = self.place_component(ComponentKind::Ammeter, Pos2::new(320.0, 220.0));
        let resistor = self.place_component(ComponentKind::Resistor, Pos2::new(500.0, 220.0));
        let led = self.place_component(ComponentKind::Led, Pos2::new(680.0, 220.0));
        let voltmeter = self.place_component(ComponentKind::Voltmeter, Pos2::new(500.0, 350.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(840.0, 430.0));

        self.add_wire_between(battery, "+", ammeter, "+");
        self.add_wire_between(ammeter, "-", resistor, "A");
        self.add_wire_between(resistor, "B", led, "A");
        self.add_wire_between(led, "B", ground, "GND");
        self.add_wire_between(battery, "-", ground, "GND");
        self.add_wire_between(voltmeter, "+", resistor, "A");
        self.add_wire_between(voltmeter, "-", resistor, "B");
        self.place_note(
            Pos2::new(500.0, 115.0),
            "EXPECT: ON\nAmmeter is series, voltmeter is parallel.",
        );
        self.status = "Loaded Ohm's law meter lesson.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_reversed_led_warning_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(180.0, 360.0));
        let resistor = self.place_component(ComponentKind::Resistor, Pos2::new(360.0, 220.0));
        let led = self.place_component(ComponentKind::Led, Pos2::new(540.0, 220.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(720.0, 390.0));

        self.add_wire_between(battery, "+", resistor, "A");
        self.add_wire_between(resistor, "B", led, "B");
        self.add_wire_between(led, "A", ground, "GND");
        self.add_wire_between(battery, "-", ground, "GND");
        self.place_note(
            Pos2::new(440.0, 110.0),
            "EXPECT: ERROR / OFF\nLED polarity is reversed.",
        );
        self.status =
            "Loaded reversed LED warning demo. ERC should flag LED polarity.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_esp32_oled_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(170.0, 380.0));
        let esp32 = self.place_component(ComponentKind::Esp32, Pos2::new(430.0, 310.0));
        let oled = self.place_component(ComponentKind::Oled, Pos2::new(720.0, 300.0));
        let sda_pullup = self.place_component(ComponentKind::Resistor, Pos2::new(590.0, 150.0));
        let scl_pullup = self.place_component(ComponentKind::Resistor, Pos2::new(690.0, 150.0));
        for id in [sda_pullup, scl_pullup] {
            if let Some(component) = self
                .components
                .iter_mut()
                .find(|component| component.id == id)
            {
                component.value = "4.7k".to_string();
            }
        }

        self.add_wire_between(battery, "+", esp32, "VIN");
        self.add_wire_between(battery, "-", esp32, "GND");
        self.add_wire_between(esp32, "3V3", oled, "VCC");
        self.add_wire_between(esp32, "GND", oled, "GND");
        self.add_wire_between(esp32, "GPIO21", oled, "SDA");
        self.add_wire_between(esp32, "GPIO22", oled, "SCL");
        self.add_wire_between(esp32, "3V3", sda_pullup, "A");
        self.add_wire_between(sda_pullup, "B", esp32, "GPIO21");
        self.add_wire_between(esp32, "3V3", scl_pullup, "A");
        self.add_wire_between(scl_pullup, "B", esp32, "GPIO22");
        self.status = "Loaded ESP32 + OLED I2C demo with 4.7k pull-ups.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_esp32_sensor_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(170.0, 380.0));
        let esp32 = self.place_component(ComponentKind::Esp32, Pos2::new(430.0, 310.0));
        let sensor = self.place_component(ComponentKind::Sensor, Pos2::new(720.0, 300.0));
        let sda_pullup = self.place_component(ComponentKind::Resistor, Pos2::new(590.0, 150.0));
        let scl_pullup = self.place_component(ComponentKind::Resistor, Pos2::new(690.0, 150.0));
        for id in [sda_pullup, scl_pullup] {
            if let Some(component) = self
                .components
                .iter_mut()
                .find(|component| component.id == id)
            {
                component.value = "4.7k".to_string();
            }
        }

        self.add_wire_between(battery, "+", esp32, "VIN");
        self.add_wire_between(battery, "-", esp32, "GND");
        self.add_wire_between(esp32, "3V3", sensor, "VCC");
        self.add_wire_between(esp32, "GND", sensor, "GND");
        self.add_wire_between(esp32, "GPIO21", sensor, "SDA");
        self.add_wire_between(esp32, "GPIO22", sensor, "SCL");
        self.add_wire_between(esp32, "3V3", sda_pullup, "A");
        self.add_wire_between(sda_pullup, "B", esp32, "GPIO21");
        self.add_wire_between(esp32, "3V3", scl_pullup, "A");
        self.add_wire_between(scl_pullup, "B", esp32, "GPIO22");
        self.status = "Loaded ESP32 + I2C sensor demo with 4.7k pull-ups.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_arduino_oled_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(150.0, 430.0));
        let arduino = self.place_component(ComponentKind::ArduinoUno, Pos2::new(430.0, 320.0));
        let oled = self.place_component(ComponentKind::Oled, Pos2::new(760.0, 300.0));
        let sda_pullup = self.place_component(ComponentKind::Resistor, Pos2::new(620.0, 140.0));
        let scl_pullup = self.place_component(ComponentKind::Resistor, Pos2::new(720.0, 140.0));
        for id in [sda_pullup, scl_pullup] {
            if let Some(component) = self
                .components
                .iter_mut()
                .find(|component| component.id == id)
            {
                component.value = "4.7k".to_string();
            }
        }

        self.add_wire_between(battery, "+", arduino, "VIN");
        self.add_wire_between(battery, "-", arduino, "GND");
        self.add_wire_between(arduino, "5V", oled, "VCC");
        self.add_wire_between(arduino, "GND", oled, "GND");
        self.add_wire_between(arduino, "A4 SDA", oled, "SDA");
        self.add_wire_between(arduino, "A5 SCL", oled, "SCL");
        self.add_wire_between(arduino, "5V", sda_pullup, "A");
        self.add_wire_between(sda_pullup, "B", arduino, "A4 SDA");
        self.add_wire_between(arduino, "5V", scl_pullup, "A");
        self.add_wire_between(scl_pullup, "B", arduino, "A5 SCL");
        self.status = "Loaded Arduino UNO + OLED I2C demo with 4.7k pull-ups.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_arduino_led_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(150.0, 430.0));
        let arduino = self.place_component(ComponentKind::ArduinoUno, Pos2::new(430.0, 320.0));
        let resistor = self.place_component(ComponentKind::Resistor, Pos2::new(720.0, 220.0));
        let led = self.place_component(ComponentKind::Led, Pos2::new(860.0, 220.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(920.0, 430.0));

        self.add_wire_between(battery, "+", arduino, "VIN");
        self.add_wire_between(battery, "-", ground, "GND");
        self.add_wire_between(arduino, "GND", ground, "GND");
        self.add_wire_between(arduino, "D13", resistor, "A");
        self.add_wire_between(resistor, "B", led, "A");
        self.add_wire_between(led, "B", ground, "GND");
        self.status = "Loaded Arduino LED output demo.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_motor_driver_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(160.0, 420.0));
        let esp32 = self.place_component(ComponentKind::Esp32, Pos2::new(390.0, 320.0));
        let driver = self.place_component(ComponentKind::MotorDriver, Pos2::new(680.0, 320.0));
        let motor = self.place_component(ComponentKind::DcMotor, Pos2::new(920.0, 300.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(720.0, 500.0));

        self.add_wire_between(battery, "+", esp32, "VIN");
        self.add_wire_between(battery, "+", driver, "VCC");
        self.add_wire_between(battery, "-", ground, "GND");
        self.add_wire_between(esp32, "GND", ground, "GND");
        self.add_wire_between(driver, "GND", ground, "GND");
        self.add_wire_between(esp32, "GPIO18", driver, "IN1");
        self.add_wire_between(esp32, "GPIO19", driver, "IN2");
        self.add_wire_between(esp32, "GPIO5", driver, "EN");
        self.add_wire_between(driver, "OUT1", motor, "+");
        self.add_wire_between(driver, "OUT2", motor, "-");
        self.status = "Loaded ESP32 + motor driver wiring demo.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_motor_relay_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(170.0, 380.0));
        let button = self.place_component(ComponentKind::PushButton, Pos2::new(360.0, 470.0));
        let relay = self.place_component(ComponentKind::Relay, Pos2::new(560.0, 360.0));
        let motor = self.place_component(ComponentKind::DcMotor, Pos2::new(780.0, 280.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(780.0, 500.0));

        self.add_wire_between(battery, "+", relay, "COIL+");
        self.add_wire_between(relay, "COIL-", button, "A");
        self.add_wire_between(button, "B", ground, "GND");
        self.add_wire_between(battery, "-", ground, "GND");
        self.add_wire_between(battery, "+", motor, "+");
        self.add_wire_between(motor, "-", relay, "COM");
        self.add_wire_between(relay, "NO", ground, "GND");
        self.status = "Loaded relay-controlled motor demo.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_transistor_switch_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(180.0, 380.0));
        let resistor = self.place_component(ComponentKind::Resistor, Pos2::new(360.0, 200.0));
        let led = self.place_component(ComponentKind::Led, Pos2::new(520.0, 200.0));
        let npn = self.place_component(ComponentKind::NpnTransistor, Pos2::new(600.0, 360.0));
        let rb = self.place_component(ComponentKind::Resistor, Pos2::new(400.0, 400.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(700.0, 500.0));

        self.add_wire_between(battery, "+", resistor, "A");
        self.add_wire_between(resistor, "B", led, "A");
        self.add_wire_between(led, "B", npn, "C");
        self.add_wire_between(npn, "E", ground, "GND");
        self.add_wire_between(battery, "+", rb, "A");
        self.add_wire_between(rb, "B", npn, "B");
        self.add_wire_between(battery, "-", ground, "GND");
        self.status = "Loaded NPN transistor switch demo.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_voltage_divider_demo(&mut self) {
        self.reset_canvas();
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(200.0, 300.0));
        let r1 = self.place_component(ComponentKind::Resistor, Pos2::new(400.0, 200.0));
        let r2 = self.place_component(ComponentKind::Resistor, Pos2::new(400.0, 380.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(560.0, 480.0));

        self.add_wire_between(battery, "+", r1, "A");
        self.add_wire_between(r1, "B", r2, "A");
        self.add_wire_between(r2, "B", ground, "GND");
        self.add_wire_between(battery, "-", ground, "GND");
        self.status = "Loaded voltage divider demo. Middle node = Vout.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_logic_demo(&mut self) {
        self.reset_canvas();
        let vsrc = self.place_component(ComponentKind::VSource, Pos2::new(160.0, 300.0));
        let not1 = self.place_component(ComponentKind::LogicNot, Pos2::new(360.0, 260.0));
        let not2 = self.place_component(ComponentKind::LogicNot, Pos2::new(500.0, 260.0));
        let led = self.place_component(ComponentKind::Led, Pos2::new(660.0, 260.0));
        let r = self.place_component(ComponentKind::Resistor, Pos2::new(580.0, 260.0));
        let ground = self.place_component(ComponentKind::Ground, Pos2::new(760.0, 360.0));

        self.add_wire_between(vsrc, "+", not1, "IN");
        self.add_wire_between(not1, "OUT", not2, "IN");
        self.add_wire_between(not2, "OUT", r, "A");
        self.add_wire_between(r, "B", led, "A");
        self.add_wire_between(led, "B", ground, "GND");
        self.add_wire_between(vsrc, "-", ground, "GND");
        self.status = "Loaded double-inverter LED demo.".to_string();
        self.pending_fit = true;
    }

    pub(crate) fn load_button_toggle_led_demo(&mut self) {
        self.reset_canvas();
        let esp32 = self.place_component(ComponentKind::Esp32, Pos2::new(420.0, 320.0));
        let button = self.place_component(ComponentKind::PushButton, Pos2::new(180.0, 220.0));
        let r_led = self.place_component(ComponentKind::Resistor, Pos2::new(660.0, 200.0));
        let led = self.place_component(ComponentKind::Led, Pos2::new(780.0, 200.0));
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(180.0, 440.0));
        let gnd = self.place_component(ComponentKind::Ground, Pos2::new(880.0, 340.0));

        self.add_wire_between(battery, "+", esp32, "VIN");
        self.add_wire_between(battery, "-", gnd, "GND");
        self.add_wire_between(esp32, "GND", gnd, "GND");
        self.add_wire_between(esp32, "GPIO23", button, "A");
        self.add_wire_between(button, "B", gnd, "GND");
        self.add_wire_between(esp32, "GPIO18", r_led, "A");
        self.add_wire_between(r_led, "B", led, "A");
        self.add_wire_between(led, "B", gnd, "GND");

        self.simulate = true;
        self.pending_fit = true;
        self.status =
            "Button-Toggle-LED demo loaded. Click the button to toggle the LED path.".to_string();
    }

    pub(crate) fn load_esp32_button_debounce_demo(&mut self) {
        self.reset_canvas();
        let esp32 = self.place_component(ComponentKind::Esp32, Pos2::new(420.0, 320.0));
        let button = self.place_component(ComponentKind::PushButton, Pos2::new(180.0, 220.0));
        let r_led = self.place_component(ComponentKind::Resistor, Pos2::new(660.0, 200.0));
        let led = self.place_component(ComponentKind::Led, Pos2::new(780.0, 200.0));
        let battery = self.place_component(ComponentKind::Battery, Pos2::new(180.0, 440.0));
        let gnd = self.place_component(ComponentKind::Ground, Pos2::new(880.0, 340.0));

        self.place_note(
            Pos2::new(130.0, 90.0),
            "EXPECT: OFF\nGPIO21 INPUT_PULLUP: open button = HIGH, no LED current.",
        );
        self.place_note(
            Pos2::new(575.0, 90.0),
            "Debounce: accept one press only after 50 ms stable LOW.",
        );

        if let (Some(bat_plus), Some(vin)) =
            (self.pin_pos(battery, "+"), self.pin_pos(esp32, "VIN"))
        {
            self.add_wire(vec![
                bat_plus,
                Pos2::new(bat_plus.x, bat_plus.y + 70.0),
                Pos2::new(vin.x + 30.0, bat_plus.y + 70.0),
                Pos2::new(vin.x + 30.0, vin.y),
                vin,
            ]);
        }
        if let (Some(bat_minus), Some(gnd_pin)) =
            (self.pin_pos(battery, "-"), self.pin_pos(gnd, "GND"))
        {
            self.add_wire(vec![
                bat_minus,
                Pos2::new(bat_minus.x, bat_minus.y + 120.0),
                Pos2::new(gnd_pin.x, bat_minus.y + 120.0),
                gnd_pin,
            ]);
        }
        self.add_wire_between(esp32, "GND", gnd, "GND");
        self.add_wire_between(esp32, "GPIO21", button, "A");
        self.add_wire_between(button, "B", gnd, "GND");
        self.add_wire_between(esp32, "GPIO18", r_led, "A");
        self.add_wire_between(r_led, "B", led, "A");
        self.add_wire_between(led, "B", gnd, "GND");

        self.simulate = true;
        self.pending_fit = true;
        self.status =
            "ESP32 debounce lesson loaded. Open button: no LED current. Closed stable press: LED path turns on.".to_string();
    }
}
