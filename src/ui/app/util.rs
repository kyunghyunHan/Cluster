use super::*;

#[derive(Default)]
pub(crate) struct UnionFind {
    pub(crate) parent: Vec<usize>,
}

impl UnionFind {
    pub(crate) fn ensure(&mut self, index: usize) {
        while self.parent.len() <= index {
            self.parent.push(self.parent.len());
        }
    }

    pub(crate) fn find(&mut self, index: usize) -> usize {
        self.ensure(index);
        if self.parent[index] != index {
            self.parent[index] = self.find(self.parent[index]);
        }
        self.parent[index]
    }

    pub(crate) fn union(&mut self, a: usize, b: usize) {
        let a = self.find(a);
        let b = self.find(b);
        if a != b {
            self.parent[b] = a;
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn circuit_to_spice_netlist(components: &[Component], wires: &[Wire]) -> String {
    let mut nodes = CircuitNodes::default();
    let mut nets = UnionFind::default();

    for wire in wires {
        for point in &wire.points {
            let node = nodes.node_for(*point);
            nets.ensure(node);
        }
        for segment in wire.points.windows(2) {
            let a = nodes.node_for(segment[0]);
            let b = nodes.node_for(segment[1]);
            nets.union(a, b);
        }
    }

    for component in components {
        for pin in component_pin_defs(component) {
            let node = nodes.node_for(pin.pos);
            nets.ensure(node);
        }
    }
    for contact in wire_contact_points(components, wires) {
        let contact_node = nodes.node_for(contact);
        nets.ensure(contact_node);
        for wire in wires {
            for segment in wire.points.windows(2) {
                if point_touches_wire_segment(contact, segment[0], segment[1]) {
                    let a = nodes.node_for(segment[0]);
                    let b = nodes.node_for(segment[1]);
                    nets.ensure(a);
                    nets.ensure(b);
                    nets.union(contact_node, a);
                    nets.union(contact_node, b);
                }
            }
        }
    }

    let mut label_nodes: HashMap<String, Vec<usize>> = HashMap::new();
    for component in components {
        if component.kind != ComponentKind::NetLabel {
            continue;
        }
        let label = component.value.trim().to_ascii_lowercase();
        if label.is_empty() {
            continue;
        }
        for pin in component_pin_defs(component) {
            label_nodes
                .entry(label.clone())
                .or_default()
                .push(nodes.node_for(pin.pos));
        }
    }
    for nodes_with_label in label_nodes.values() {
        for pair in nodes_with_label.windows(2) {
            nets.union(pair[0], pair[1]);
        }
    }

    let mut ground_roots = HashSet::new();
    for component in components {
        if component.kind != ComponentKind::Ground {
            continue;
        }
        for pin in component_pin_defs(component) {
            let node = nodes.node_for(pin.pos);
            ground_roots.insert(nets.find(node));
        }
    }

    let mut named_roots = HashMap::new();
    for index in 0..nodes.positions.len() {
        let root = nets.find(index);
        if ground_roots.contains(&root) {
            named_roots.insert(root, "0".to_string());
        }
    }
    for component in components {
        if component.kind != ComponentKind::NetLabel {
            continue;
        }
        let Some(name) = spice_node_name(&component.value) else {
            continue;
        };
        for pin in component_pin_defs(component) {
            let root = nets.find(nodes.node_for(pin.pos));
            if !ground_roots.contains(&root) {
                named_roots.entry(root).or_insert_with(|| name.clone());
            }
        }
    }

    let mut roots = (0..nodes.positions.len())
        .map(|index| nets.find(index))
        .collect::<Vec<_>>();
    roots.sort_unstable();
    roots.dedup();

    let mut next_net = 1;
    for root in roots {
        named_roots.entry(root).or_insert_with(|| {
            let name = format!("N{next_net:03}");
            next_net += 1;
            name
        });
    }

    let mut net_name = |pos: Pos2| {
        let node = nodes.node_for(pos);
        let root = nets.find(node);
        named_roots
            .entry(root)
            .or_insert_with(|| {
                let name = format!("N{next_net:03}");
                next_net += 1;
                name
            })
            .clone()
    };

    let mut used_names = HashSet::new();
    let mut lines = Vec::new();
    let mut skipped = Vec::new();
    let mut uses_diode_model = false;
    let mut uses_led_model = false;
    let mut uses_zener_model = false;
    let mut uses_npn_model = false;
    let mut uses_pnp_model = false;
    let mut uses_nmos_model = false;
    let mut uses_pmos_model = false;

    for component in components {
        let pins = component_pin_defs(component);
        let line = match component.kind {
            ComponentKind::Resistor
            | ComponentKind::Capacitor
            | ComponentKind::Inductor
            | ComponentKind::Diode
            | ComponentKind::ZenerDiode
            | ComponentKind::Led
            | ComponentKind::VSource
            | ComponentKind::ISource
            | ComponentKind::Battery
            | ComponentKind::Fuse
            | ComponentKind::Potentiometer => {
                let Some((a, b)) = spice_two_pin_nets(component, &pins, &mut net_name) else {
                    skipped.push(format!("* skipped {}: missing pins", component.label));
                    continue;
                };
                match component.kind {
                    ComponentKind::Resistor | ComponentKind::Potentiometer => Some(format!(
                        "{} {a} {b} {}",
                        unique_spice_name("R", &component.label, component.id, &mut used_names),
                        spice_value(component, "1k")
                    )),
                    ComponentKind::Fuse => Some(format!(
                        "{} {a} {b} 0.1",
                        unique_spice_name("R", &component.label, component.id, &mut used_names),
                    )),
                    ComponentKind::Capacitor => Some(format!(
                        "{} {a} {b} {}",
                        unique_spice_name("C", &component.label, component.id, &mut used_names),
                        spice_value(component, "100n")
                    )),
                    ComponentKind::Inductor => Some(format!(
                        "{} {a} {b} {}",
                        unique_spice_name("L", &component.label, component.id, &mut used_names),
                        spice_value(component, "10u")
                    )),
                    ComponentKind::VSource | ComponentKind::Battery => Some(format!(
                        "{} {a} {b} DC {}",
                        unique_spice_name("V", &component.label, component.id, &mut used_names),
                        spice_value(component, "5")
                    )),
                    ComponentKind::ISource => Some(format!(
                        "{} {a} {b} DC {}",
                        unique_spice_name("I", &component.label, component.id, &mut used_names),
                        spice_value(component, "1m")
                    )),
                    ComponentKind::Diode => {
                        uses_diode_model = true;
                        Some(format!(
                            "{} {a} {b} DGEN",
                            unique_spice_name("D", &component.label, component.id, &mut used_names)
                        ))
                    }
                    ComponentKind::ZenerDiode => {
                        uses_zener_model = true;
                        Some(format!(
                            "{} {a} {b} DZEN",
                            unique_spice_name("D", &component.label, component.id, &mut used_names)
                        ))
                    }
                    ComponentKind::Led => {
                        uses_led_model = true;
                        Some(format!(
                            "{} {a} {b} LEDGEN",
                            unique_spice_name("D", &component.label, component.id, &mut used_names)
                        ))
                    }
                    _ => None,
                }
            }
            ComponentKind::NpnTransistor => {
                let result = (|| -> Option<String> {
                    let c_pin = pins.iter().find(|p| p.label == "C")?;
                    let b_pin = pins.iter().find(|p| p.label == "B")?;
                    let e_pin = pins.iter().find(|p| p.label == "E")?;
                    let c_net = net_name(c_pin.pos);
                    let b_net = net_name(b_pin.pos);
                    let e_net = net_name(e_pin.pos);
                    Some(format!(
                        "{} {c_net} {b_net} {e_net} NBJT",
                        unique_spice_name("Q", &component.label, component.id, &mut used_names)
                    ))
                })();
                uses_npn_model = result.is_some();
                result
            }
            ComponentKind::PnpTransistor => {
                let result = (|| -> Option<String> {
                    let c_pin = pins.iter().find(|p| p.label == "C")?;
                    let b_pin = pins.iter().find(|p| p.label == "B")?;
                    let e_pin = pins.iter().find(|p| p.label == "E")?;
                    let c_net = net_name(c_pin.pos);
                    let b_net = net_name(b_pin.pos);
                    let e_net = net_name(e_pin.pos);
                    Some(format!(
                        "{} {c_net} {b_net} {e_net} PBJT",
                        unique_spice_name("Q", &component.label, component.id, &mut used_names)
                    ))
                })();
                uses_pnp_model = result.is_some();
                result
            }
            ComponentKind::Nmosfet => {
                let result = (|| -> Option<String> {
                    let d_pin = pins.iter().find(|p| p.label == "D")?;
                    let g_pin = pins.iter().find(|p| p.label == "G")?;
                    let s_pin = pins.iter().find(|p| p.label == "S")?;
                    let d_net = net_name(d_pin.pos);
                    let g_net = net_name(g_pin.pos);
                    let s_net = net_name(s_pin.pos);
                    Some(format!(
                        "{} {d_net} {g_net} {s_net} {s_net} NMOS",
                        unique_spice_name("M", &component.label, component.id, &mut used_names)
                    ))
                })();
                uses_nmos_model = result.is_some();
                result
            }
            ComponentKind::Pmosfet => {
                let result = (|| -> Option<String> {
                    let d_pin = pins.iter().find(|p| p.label == "D")?;
                    let g_pin = pins.iter().find(|p| p.label == "G")?;
                    let s_pin = pins.iter().find(|p| p.label == "S")?;
                    let d_net = net_name(d_pin.pos);
                    let g_net = net_name(g_pin.pos);
                    let s_net = net_name(s_pin.pos);
                    Some(format!(
                        "{} {d_net} {g_net} {s_net} {s_net} PMOS",
                        unique_spice_name("M", &component.label, component.id, &mut used_names)
                    ))
                })();
                uses_pmos_model = result.is_some();
                result
            }
            ComponentKind::VoltageReg => (|| -> Option<String> {
                let in_pin = pins.iter().find(|p| p.label == "IN")?;
                let out_pin = pins.iter().find(|p| p.label == "OUT")?;
                let gnd_pin = pins.iter().find(|p| p.label == "GND")?;
                let in_net = net_name(in_pin.pos);
                let out_net = net_name(out_pin.pos);
                let gnd_net = net_name(gnd_pin.pos);
                Some(format!(
                    "* {} LM7805: IN={in_net} OUT={out_net} GND={gnd_net} (5V fixed)",
                    component.label
                ))
            })(),
            ComponentKind::LogicNot
            | ComponentKind::LogicAnd
            | ComponentKind::LogicOr
            | ComponentKind::LogicNand
            | ComponentKind::LogicNor
            | ComponentKind::LogicXor => {
                let kind_str = component_kind_label(component.kind);
                let nets: Vec<String> = pins.iter().map(|p| net_name(p.pos)).collect();
                let net_str = nets.join(" ");
                Some(format!("* {} {} [{}]", component.label, kind_str, net_str))
            }
            ComponentKind::Voltmeter => {
                let Some((a, b)) = spice_two_pin_nets(component, &pins, &mut net_name) else {
                    skipped.push(format!("* skipped {}: missing pins", component.label));
                    continue;
                };
                // Voltmeter = 1 GΩ resistor (ideal high impedance)
                Some(format!(
                    "{} {a} {b} 1G",
                    unique_spice_name("R", &component.label, component.id, &mut used_names)
                ))
            }
            ComponentKind::Ammeter => {
                let Some((a, b)) = spice_two_pin_nets(component, &pins, &mut net_name) else {
                    skipped.push(format!("* skipped {}: missing pins", component.label));
                    continue;
                };
                // Ammeter = 0 V source (ideal current sense)
                Some(format!(
                    "{} {a} {b} DC 0",
                    unique_spice_name("V", &component.label, component.id, &mut used_names)
                ))
            }
            ComponentKind::Ground => None,
            _ => {
                skipped.push(format!(
                    "* skipped {}: {} has no SPICE primitive yet",
                    component.label,
                    component_kind_label(component.kind)
                ));
                None
            }
        };
        if let Some(line) = line {
            lines.push(line);
        }
    }

    let mut output = String::new();
    output.push_str("* Cluster SPICE netlist\n");
    output.push_str("* Generated from the schematic connectivity graph.\n");
    if lines.is_empty() {
        output.push_str("* No supported SPICE primitives in this schematic.\n");
    } else {
        for line in lines {
            output.push_str(&line);
            output.push('\n');
        }
    }
    if uses_diode_model {
        output.push_str(".model DGEN D(Is=2n Rs=0.6 N=1.8)\n");
    }
    if uses_zener_model {
        output.push_str(".model DZEN D(Is=1e-14 Rs=0.5 N=1.0 BV=5.1 IBV=10m)\n");
    }
    if uses_led_model {
        output.push_str(".model LEDGEN D(Is=10n Rs=4 N=2.0 Eg=2.0)\n");
    }
    if uses_npn_model {
        output.push_str(".model NBJT NPN(Is=1e-14 Bf=200 Br=2 Cje=10p Cjc=5p)\n");
    }
    if uses_pnp_model {
        output.push_str(".model PBJT PNP(Is=1e-14 Bf=200 Br=2 Cje=10p Cjc=5p)\n");
    }
    if uses_nmos_model {
        output.push_str(".model NMOS NMOS(Level=1 Vto=2 Kp=200u W=10u L=1u)\n");
    }
    if uses_pmos_model {
        output.push_str(".model PMOS PMOS(Level=1 Vto=-2 Kp=80u W=20u L=1u)\n");
    }
    for line in skipped {
        output.push_str(&line);
        output.push('\n');
    }
    output.push_str(".op\n.end\n");
    output
}

pub(crate) fn circuit_to_netlist_text(netlist: &CircuitNetlist) -> String {
    let mut out = String::new();
    out.push_str("# Cluster netlist\n");
    out.push_str("# Format: Component.Pin -> NET_NAME\n\n");
    for net in &netlist.nets {
        out.push_str(&format!("{}:\n", net.name));
        let mut pins = netlist
            .pins
            .iter()
            .filter(|pin| pin.net_id == net.id)
            .collect::<Vec<_>>();
        pins.sort_by(|a, b| {
            a.component_label
                .cmp(&b.component_label)
                .then_with(|| a.pin_name.cmp(&b.pin_name))
        });
        if pins.is_empty() {
            out.push_str("  (no connected pins)\n");
        } else {
            for pin in pins {
                out.push_str(&format!(
                    "  {}.{} [{:?}] @ ({:.0}, {:.0})\n",
                    pin.component_label,
                    pin.pin_name,
                    pin.electrical_type,
                    pin.position.x,
                    pin.position.y
                ));
            }
        }
        out.push('\n');
    }
    if !netlist.floating_wires.is_empty() {
        out.push_str("Floating wires:\n");
        for wire_id in &netlist.floating_wires {
            let net_name = netlist
                .wire_nets
                .get(wire_id)
                .and_then(|id| netlist.nets.iter().find(|net| net.id == *id))
                .map(|net| net.name.as_str())
                .unwrap_or("UNKNOWN");
            out.push_str(&format!("  Wire {wire_id} -> {net_name}\n"));
        }
    }
    out
}

pub(crate) fn generate_arduino_code(netlist: &CircuitNetlist) -> String {
    let has_oled = netlist
        .pins
        .iter()
        .any(|p| p.component_kind == ComponentKind::Oled);
    let has_button = netlist
        .pins
        .iter()
        .any(|p| p.component_kind == ComponentKind::PushButton);
    let has_led = netlist
        .pins
        .iter()
        .any(|p| p.component_kind == ComponentKind::Led);
    let controller_kind = netlist.pins.iter().find_map(|pin| {
        matches!(
            pin.component_kind,
            ComponentKind::Esp32
                | ComponentKind::Esp32S3
                | ComponentKind::Esp32C3
                | ComponentKind::ArduinoUno
                | ComponentKind::RaspberryPiPico
        )
        .then_some(pin.component_kind)
    });

    let mut i2c_sda = "21".to_string();
    let mut i2c_scl = "22".to_string();

    // GPIO nets: map pin_name → (connected_to_button, connected_to_led)
    let mut button_gpio: Option<String> = None;
    let mut led_gpio: Option<String> = None;

    for net in &netlist.nets {
        let pins: Vec<&NetlistPin> = netlist.pins.iter().filter(|p| p.net_id == net.id).collect();

        if pins
            .iter()
            .any(|p| p.component_kind == ComponentKind::Oled && p.pin_name == "SDA")
            && let Some(ctrl) = pins.iter().find(|p| pin_is_controller_sda(p))
        {
            i2c_sda = digits_from_pin_name(&ctrl.pin_name).unwrap_or_else(|| i2c_sda.clone());
        }
        if pins
            .iter()
            .any(|p| p.component_kind == ComponentKind::Oled && p.pin_name == "SCL")
            && let Some(ctrl) = pins.iter().find(|p| pin_is_controller_scl(p))
        {
            i2c_scl = digits_from_pin_name(&ctrl.pin_name).unwrap_or_else(|| i2c_scl.clone());
        }

        // Detect which GPIO is connected to the button and which to the LED
        let has_button_on_net = pins
            .iter()
            .any(|p| p.component_kind == ComponentKind::PushButton);
        let has_led_on_net = pins.iter().any(|p| p.component_kind == ComponentKind::Led);
        if let Some(gpio_pin) = pins.iter().find(|p| pin_is_microcontroller_gpio(p))
            && let Some(gpio_num) = digits_from_pin_name(&gpio_pin.pin_name)
        {
            if has_button_on_net && button_gpio.is_none() {
                button_gpio = Some(gpio_num.clone());
            }
            if (has_led_on_net || net_drives_led_through_series_part(netlist, net.id))
                && led_gpio.is_none()
            {
                led_gpio = Some(gpio_num);
            }
        }
    }

    let mut gpio_pins: Vec<(String, String)> = netlist
        .pins
        .iter()
        .filter(|p| {
            p.connected_by_wire && pin_is_microcontroller_gpio(p) && !pin_is_i2c_named(&p.pin_name)
        })
        .filter_map(|p| digits_from_pin_name(&p.pin_name).map(|g| (p.pin_name.clone(), g)))
        .collect();
    gpio_pins.sort();
    gpio_pins.dedup();

    let mut code = String::new();
    code.push_str("// Generated by Cluster\n");
    code.push_str("#include <Arduino.h>\n");
    if has_oled {
        code.push_str("#include <Wire.h>\n");
        code.push_str("#include <Adafruit_GFX.h>\n");
        code.push_str("#include <Adafruit_SSD1306.h>\n\n");
        code.push_str("#define SCREEN_WIDTH 128\n#define SCREEN_HEIGHT 64\n");
        code.push_str("Adafruit_SSD1306 display(SCREEN_WIDTH, SCREEN_HEIGHT, &Wire, -1);\n");
    }
    code.push('\n');

    // Pin constants
    for (name, gpio) in &gpio_pins {
        code.push_str(&format!(
            "const int PIN_{} = {};\n",
            sanitize_code_ident(name),
            gpio
        ));
    }

    // Button-toggle pattern: extra state variable
    if has_button
        && has_led
        && let (Some(btn), Some(led)) = (&button_gpio, &led_gpio)
    {
        code.push_str(&format!("\nconst int BUTTON_PIN = {btn};\n"));
        code.push_str(&format!("const int LED_PIN    = {led};\n"));
        code.push_str("const unsigned long DEBOUNCE_MS = 50;\n");
        code.push_str("\nbool ledState = false;\n");
        code.push_str("int lastReading = HIGH;\n");
        code.push_str("int stableState = HIGH;\n");
        code.push_str("unsigned long lastDebounceTime = 0;\n");

        code.push_str("\nvoid setup() {\n  Serial.begin(115200);\n");
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
        return code;
    }

    // OLED setup
    code.push_str("\nvoid setup() {\n  Serial.begin(115200);\n");
    if has_oled {
        if controller_kind == Some(ComponentKind::ArduinoUno) {
            code.push_str("  Wire.begin();  // UNO uses A4 SDA and A5 SCL\n");
        } else {
            code.push_str(&format!("  Wire.begin({i2c_sda}, {i2c_scl});\n"));
        }
        code.push_str("  if (!display.begin(SSD1306_SWITCHCAPVCC, 0x3C)) {\n");
        code.push_str(
            "    Serial.println(\"OLED init failed\");\n    while (true) delay(100);\n  }\n",
        );
        code.push_str("  display.clearDisplay();\n  display.setTextSize(1);\n  display.setTextColor(SSD1306_WHITE);\n  display.setCursor(0, 0);\n  display.println(\"Cluster ready\");\n  display.display();\n");
    }
    for (name, _) in &gpio_pins {
        code.push_str(&format!(
            "  pinMode(PIN_{}, OUTPUT);\n",
            sanitize_code_ident(name)
        ));
    }
    code.push_str("}\n\nvoid loop() {\n");
    if gpio_pins.is_empty() {
        code.push_str("  delay(1000);\n");
    } else {
        for (name, _) in &gpio_pins {
            let id = sanitize_code_ident(name);
            code.push_str(&format!("  digitalWrite(PIN_{id}, HIGH);\n"));
        }
        code.push_str("  delay(500);\n");
        for (name, _) in &gpio_pins {
            let id = sanitize_code_ident(name);
            code.push_str(&format!("  digitalWrite(PIN_{id}, LOW);\n"));
        }
        code.push_str("  delay(500);\n");
    }
    code.push_str("}\n");
    code
}

pub(crate) fn digits_from_pin_name(name: &str) -> Option<String> {
    let digits = name
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    (!digits.is_empty()).then_some(digits)
}

pub(crate) fn net_drives_led_through_series_part(netlist: &CircuitNetlist, net_id: usize) -> bool {
    let series_parts = netlist
        .pins
        .iter()
        .filter(|pin| pin.net_id == net_id)
        .filter(|pin| {
            matches!(
                pin.component_kind,
                ComponentKind::Resistor | ComponentKind::Ammeter | ComponentKind::Fuse
            )
        });

    for part_pin in series_parts {
        let output_net_ids = netlist
            .pins
            .iter()
            .filter(|pin| {
                pin.component_id == part_pin.component_id
                    && pin.pin_name != part_pin.pin_name
                    && pin.net_id != net_id
            })
            .map(|pin| pin.net_id);

        for output_net_id in output_net_ids {
            if netlist
                .pins
                .iter()
                .any(|pin| pin.net_id == output_net_id && pin.component_kind == ComponentKind::Led)
            {
                return true;
            }
        }
    }

    false
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

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn spice_two_pin_nets(
    component: &Component,
    pins: &[CircuitPin],
    net_name: &mut impl FnMut(Pos2) -> String,
) -> Option<(String, String)> {
    match component.kind {
        ComponentKind::VSource
        | ComponentKind::Battery
        | ComponentKind::ISource
        | ComponentKind::DcMotor => {
            let positive = pins.iter().find(|pin| pin.role == PinRole::Positive)?;
            let negative = pins.iter().find(|pin| pin.role == PinRole::Ground)?;
            Some((net_name(positive.pos), net_name(negative.pos)))
        }
        _ => {
            let a = pins.first()?;
            let b = pins.get(1)?;
            Some((net_name(a.pos), net_name(b.pos)))
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn unique_spice_name(
    prefix: &str,
    label: &str,
    id: u64,
    used: &mut HashSet<String>,
) -> String {
    let mut name = label
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    if name.is_empty() {
        name = format!("{prefix}{id}");
    }
    if !name
        .chars()
        .next()
        .is_some_and(|ch| ch.eq_ignore_ascii_case(&prefix.chars().next().unwrap_or('X')))
    {
        name = format!("{prefix}{name}");
    }
    if used.insert(name.clone()) {
        return name;
    }
    let with_id = format!("{name}_{id}");
    used.insert(with_id.clone());
    with_id
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn spice_value(component: &Component, fallback: &str) -> String {
    let normalized = component.value.trim().replace(' ', "");
    if normalized.is_empty() {
        return fallback.to_string();
    }
    let lower = normalized.to_lowercase();
    let stripped = match component.kind {
        ComponentKind::Resistor => lower.strip_suffix("ohm").unwrap_or(&normalized),
        ComponentKind::Capacitor => lower.strip_suffix('f').unwrap_or(&normalized),
        ComponentKind::Inductor => lower.strip_suffix('h').unwrap_or(&normalized),
        ComponentKind::VSource | ComponentKind::Battery => lower
            .strip_suffix("volts")
            .or_else(|| lower.strip_suffix("volt"))
            .or_else(|| lower.strip_suffix('v'))
            .unwrap_or(&normalized),
        ComponentKind::ISource => lower
            .strip_suffix("amps")
            .or_else(|| lower.strip_suffix("amp"))
            .or_else(|| lower.strip_suffix('a'))
            .unwrap_or(&normalized),
        _ => &normalized,
    };
    if stripped.trim().is_empty() {
        fallback.to_string()
    } else {
        stripped.to_string()
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn spice_node_name(value: &str) -> Option<String> {
    let mut name = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    while name.contains("__") {
        name = name.replace("__", "_");
    }
    name = name.trim_matches('_').to_string();
    if name.is_empty() {
        return None;
    }
    if name.starts_with(|ch: char| ch.is_ascii_digit()) {
        name.insert_str(0, "N_");
    }
    Some(name)
}

pub(crate) fn circuit_to_svg(components: &[Component], wires: &[Wire]) -> String {
    let bounds = circuit_bounds(components, wires)
        .unwrap_or_else(|| Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(960.0, 640.0)));
    let margin = 40.0;
    let min_x = bounds.left() - margin;
    let min_y = bounds.top() - margin;
    let width = (bounds.width() + margin * 2.0).max(480.0);
    let height = (bounds.height() + margin * 2.0).max(320.0);
    let simulation = analyze_circuit(components, wires);

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{:.1} {:.1} {:.1} {:.1}" width="{:.1}" height="{:.1}">
<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="#101216"/>
<g fill="none" stroke-linecap="round" stroke-linejoin="round">
"##,
        min_x, min_y, width, height, width, height, min_x, min_y, width, height
    ));

    for wire in wires {
        if wire.points.len() < 2 {
            continue;
        }
        let color = if simulation.energized_wires.contains(&wire.id) {
            "#ffaa37"
        } else {
            "#69b2ff"
        };
        let points = wire
            .points
            .iter()
            .map(|p| format!("{:.1},{:.1}", p.x, p.y))
            .collect::<Vec<_>>()
            .join(" ");
        svg.push_str(&format!(
            r##"<polyline points="{}" stroke="{}" stroke-width="2.4"/>"##,
            points, color
        ));
        svg.push('\n');
    }

    for component in components {
        let rect = component_bounds(component);
        let energized = simulation.energized_components.contains(&component.id);
        let stroke = if energized { "#ffb950" } else { "#dee2e8" };
        let fill = if component_is_module(component) {
            if energized { "#3e2e16" } else { "#181e26" }
        } else {
            "none"
        };
        svg.push_str(&format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="4" fill="{}" stroke="{}" stroke-width="2"/>"##,
            rect.left(),
            rect.top(),
            rect.width(),
            rect.height(),
            fill,
            stroke
        ));
        svg.push('\n');
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" fill="{}" font-family="Arial, sans-serif" font-size="12" text-anchor="middle">{}</text>"##,
            rect.center().x,
            rect.center().y - 2.0,
            stroke,
            escape_xml(component_kind_label(component.kind))
        ));
        svg.push('\n');
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" fill="#e1e4e8" font-family="Arial, sans-serif" font-size="11" text-anchor="middle">{}</text>"##,
            rect.center().x,
            rect.bottom() + 15.0,
            escape_xml(&component.label)
        ));
        svg.push('\n');
        if !component.value.trim().is_empty() {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" fill="#9aa4ae" font-family="Arial, sans-serif" font-size="10" text-anchor="middle">{}</text>"##,
                rect.center().x,
                rect.top() - 7.0,
                escape_xml(&component.value)
            ));
            svg.push('\n');
        }
        for pin in component_pins(component) {
            svg.push_str(&format!(
                r##"<circle cx="{:.1}" cy="{:.1}" r="3.2" fill="#facd5f" stroke="#281f14" stroke-width="1"/>"##,
                pin.x, pin.y
            ));
            svg.push('\n');
        }
    }

    svg.push_str("</g>\n</svg>\n");
    svg
}

#[allow(clippy::type_complexity)] // Accepts the legacy schema-v4 page representation.
pub(crate) fn circuit_to_bom_csv(
    pages: &[(String, Vec<Component>, Vec<Wire>, u64, Counters)],
) -> String {
    let mut rows = pages
        .iter()
        .flat_map(|(page_name, components, _, _, _)| {
            components
                .iter()
                .filter(|component| component.kind != ComponentKind::Ground)
                .map(move |component| {
                    (
                        page_name.as_str(),
                        component.label.as_str(),
                        component_kind_label(component.kind),
                        component.value.as_str(),
                    )
                })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        a.0.cmp(b.0)
            .then_with(|| naturalish_label_key(a.1).cmp(&naturalish_label_key(b.1)))
            .then_with(|| a.1.cmp(b.1))
    });

    let mut lines = vec!["Page,Label,Kind,Value".to_string()];
    for (page, label, kind, value) in rows {
        lines.push(format!(
            "{},{},{},{}",
            csv_cell(page),
            csv_cell(label),
            csv_cell(kind),
            csv_cell(value)
        ));
    }
    lines.join("\n") + "\n"
}

pub(crate) fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

pub(crate) fn naturalish_label_key(label: &str) -> (String, u32) {
    let split_at = label
        .char_indices()
        .find(|(_, ch)| ch.is_ascii_digit())
        .map(|(idx, _)| idx)
        .unwrap_or(label.len());
    let prefix = label[..split_at].to_ascii_uppercase();
    let number = label[split_at..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse::<u32>()
        .unwrap_or(u32::MAX);
    (prefix, number)
}

pub(crate) fn circuit_bounds(components: &[Component], wires: &[Wire]) -> Option<Rect> {
    let mut min = Pos2::new(f32::INFINITY, f32::INFINITY);
    let mut max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut has_content = false;

    for component in components {
        let rect = component_bounds(component);
        min.x = min.x.min(rect.left());
        min.y = min.y.min(rect.top());
        max.x = max.x.max(rect.right());
        max.y = max.y.max(rect.bottom());
        has_content = true;
    }

    for wire in wires {
        for point in &wire.points {
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
            has_content = true;
        }
    }

    has_content.then(|| Rect::from_min_max(min, max))
}

pub(crate) fn component_kind_label(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Resistor => "Resistor",
        ComponentKind::Capacitor => "Capacitor",
        ComponentKind::Inductor => "Inductor",
        ComponentKind::Diode => "Diode",
        ComponentKind::Led => "LED",
        ComponentKind::ZenerDiode => "Zener",
        ComponentKind::Switch => "Switch",
        ComponentKind::PushButton => "Push Button",
        ComponentKind::SlideSwitch => "Slide Switch",
        ComponentKind::Ground => "Ground",
        ComponentKind::VSource => "V Source",
        ComponentKind::ISource => "I Source",
        ComponentKind::Battery => "Battery",
        ComponentKind::OpAmp => "Op Amp",
        ComponentKind::Lamp => "Lamp",
        ComponentKind::Potentiometer => "Potentiometer",
        ComponentKind::NpnTransistor => "NPN BJT",
        ComponentKind::PnpTransistor => "PNP BJT",
        ComponentKind::Nmosfet => "N-MOSFET",
        ComponentKind::Pmosfet => "P-MOSFET",
        ComponentKind::VoltageReg => "Voltage Reg",
        ComponentKind::Fuse => "Fuse",
        ComponentKind::LogicNot => "NOT Gate",
        ComponentKind::LogicAnd => "AND Gate",
        ComponentKind::LogicOr => "OR Gate",
        ComponentKind::LogicNand => "NAND Gate",
        ComponentKind::LogicNor => "NOR Gate",
        ComponentKind::LogicXor => "XOR Gate",
        ComponentKind::Esp32 => "ESP32 WROOM",
        ComponentKind::Esp32S3 => "ESP32-S3",
        ComponentKind::Esp32C3 => "ESP32-C3",
        ComponentKind::ArduinoUno => "Arduino UNO",
        ComponentKind::RaspberryPiPico => "Pi Pico",
        ComponentKind::Stm32BluePill => "STM32 Blue Pill",
        ComponentKind::Stm32Nucleo64 => "STM32 Nucleo-64",
        ComponentKind::Breadboard => "Breadboard",
        ComponentKind::Relay => "Relay",
        ComponentKind::DcMotor => "DC Motor",
        ComponentKind::Servo => "Servo",
        ComponentKind::Oled => "OLED I2C",
        ComponentKind::Sensor => "Sensor",
        ComponentKind::NetLabel => "Net Label",
        ComponentKind::Timer555 => "555 Timer",
        ComponentKind::Crystal => "Crystal",
        ComponentKind::Transformer => "Transformer",
        ComponentKind::Display7Seg => "7-Seg Display",
        ComponentKind::Thermistor => "Thermistor",
        ComponentKind::Varistor => "Varistor",
        ComponentKind::VoltageRef => "Voltage Ref",
        ComponentKind::MotorDriver => "Motor Driver",
        ComponentKind::SchottkyDiode => "Schottky",
        ComponentKind::TvsDiode => "TVS Diode",
        ComponentKind::Phototransistor => "Phototransistor",
        ComponentKind::Optocoupler => "Optocoupler",
        ComponentKind::GenericIc => "Generic IC",
        ComponentKind::Voltmeter => "Voltmeter",
        ComponentKind::Ammeter => "Ammeter",
        ComponentKind::TextNote => "Text Note",
        ComponentKind::Dht11 => "DHT11",
        ComponentKind::Dht22 => "DHT22",
        ComponentKind::HcSr04 => "HC-SR04",
        ComponentKind::Buzzer => "Buzzer",
        ComponentKind::NeoPixel => "NeoPixel",
        ComponentKind::PirSensor => "PIR Sensor",
        ComponentKind::Custom => "Custom Part",
    }
}

pub(crate) fn component_is_module(component: &Component) -> bool {
    matches!(
        component.kind,
        ComponentKind::Esp32
            | ComponentKind::Esp32S3
            | ComponentKind::Esp32C3
            | ComponentKind::ArduinoUno
            | ComponentKind::RaspberryPiPico
            | ComponentKind::Oled
            | ComponentKind::Sensor
            | ComponentKind::Timer555
            | ComponentKind::Display7Seg
            | ComponentKind::MotorDriver
            | ComponentKind::Optocoupler
            | ComponentKind::GenericIc
            | ComponentKind::Dht11
            | ComponentKind::Dht22
            | ComponentKind::HcSr04
            | ComponentKind::NeoPixel
            | ComponentKind::PirSensor
            | ComponentKind::Custom
    )
}

pub(crate) fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ─── New sensor drawing functions ────────────────────────────────────────────

pub(crate) fn draw_sensor_module(
    painter: &egui::Painter,
    rect: Rect,
    stroke: Stroke,
    energized: bool,
    label: &str,
    accent: Color32,
) {
    let center = rect.center();
    let body_fill = if energized {
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 80)
    } else {
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 35)
    };
    painter.rect_filled(rect, 6.0, body_fill);
    painter.rect_stroke(rect, 6.0, stroke, StrokeKind::Middle);
    painter.text(
        center,
        Align2::CENTER_CENTER,
        label,
        egui::FontId::monospace(10.0),
        if energized {
            Color32::from_rgb(255, 230, 150)
        } else {
            Color32::from_rgb(200, 215, 230)
        },
    );
}

pub(crate) fn draw_hcsr04(painter: &egui::Painter, rect: Rect, stroke: Stroke, energized: bool) {
    let center = rect.center();
    let fill = if energized {
        Color32::from_rgba_unmultiplied(80, 180, 255, 70)
    } else {
        Color32::from_rgba_unmultiplied(60, 120, 200, 35)
    };
    painter.rect_filled(rect, 4.0, fill);
    painter.rect_stroke(rect, 4.0, stroke, StrokeKind::Middle);
    // Two transducer circles
    let r = rect.height() * 0.28;
    let left_cx = rect.center().x - rect.width() * 0.22;
    let right_cx = rect.center().x + rect.width() * 0.22;
    painter.circle_stroke(Pos2::new(left_cx, center.y), r, stroke);
    painter.circle_stroke(Pos2::new(right_cx, center.y), r, stroke);
    painter.text(
        Pos2::new(center.x, rect.bottom() - 8.0),
        Align2::CENTER_CENTER,
        "HC-SR04",
        egui::FontId::monospace(8.0),
        if energized {
            Color32::from_rgb(160, 220, 255)
        } else {
            Color32::from_rgb(150, 170, 200)
        },
    );
}

pub(crate) fn draw_buzzer(
    painter: &egui::Painter,
    rect: Rect,
    _rotation: i32,
    stroke: Stroke,
    energized: bool,
) {
    let center = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    let fill = if energized {
        Color32::from_rgba_unmultiplied(255, 200, 50, 80)
    } else {
        Color32::from_rgba_unmultiplied(180, 160, 50, 35)
    };
    painter.circle_filled(center, r, fill);
    painter.circle_stroke(center, r, stroke);
    // Sound wave arcs
    let wave_col = if energized {
        stroke.color
    } else {
        Color32::from_rgb(130, 140, 150)
    };
    for i in 1..=2u32 {
        let arc_r = r + i as f32 * 6.0;
        painter.circle_stroke(center, arc_r, Stroke::new(stroke.width * 0.6, wave_col));
    }
    // Plus/minus
    painter.text(
        Pos2::new(center.x, center.y),
        Align2::CENTER_CENTER,
        "BZ",
        egui::FontId::monospace(9.0),
        if energized {
            Color32::from_rgb(255, 230, 100)
        } else {
            Color32::from_rgb(180, 190, 200)
        },
    );
    // Terminal lines left/right
    painter.line_segment(
        [
            Pos2::new(rect.left(), center.y),
            Pos2::new(center.x - r, center.y),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(rect.right(), center.y),
            Pos2::new(center.x + r, center.y),
        ],
        stroke,
    );
}

pub(crate) fn draw_neopixel(painter: &egui::Painter, rect: Rect, stroke: Stroke, energized: bool) {
    let center = rect.center();
    let inner_fill = if energized {
        Color32::from_rgb(255, 80, 200)
    } else {
        Color32::from_rgba_unmultiplied(80, 50, 100, 60)
    };
    painter.rect_filled(rect, 8.0, Color32::from_rgba_unmultiplied(30, 20, 50, 120));
    painter.rect_stroke(rect, 8.0, stroke, StrokeKind::Middle);
    // Inner LED square
    let inner = Rect::from_center_size(center, Vec2::splat(rect.width().min(rect.height()) * 0.45));
    painter.rect_filled(inner, 4.0, inner_fill);
    painter.text(
        Pos2::new(center.x, rect.bottom() - 7.0),
        Align2::CENTER_CENTER,
        "NP",
        egui::FontId::monospace(8.0),
        if energized {
            Color32::from_rgb(255, 180, 255)
        } else {
            Color32::from_rgb(160, 140, 180)
        },
    );
}

/// Minimal dependency-free PNG encoder (RGBA8).
/// Uses zlib's `deflate` via the `miniz_oxide` crate which is a transitive
/// dependency of eframe, so no new dependency is required.
pub(crate) fn write_png(
    path: &str,
    width: usize,
    height: usize,
    rgba: &[u8],
) -> std::io::Result<()> {
    use std::io::Write;

    fn adler32(data: &[u8]) -> u32 {
        let (mut s1, mut s2) = (1u32, 0u32);
        for &b in data {
            s1 = (s1 + b as u32) % 65521;
            s2 = (s2 + s1) % 65521;
        }
        (s2 << 16) | s1
    }
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }
    fn write_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
        let len = data.len() as u32;
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(tag);
        out.extend_from_slice(data);
        let mut crc_data = Vec::with_capacity(4 + data.len());
        crc_data.extend_from_slice(tag);
        crc_data.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_data).to_be_bytes());
    }

    // Build raw scanlines with filter byte 0 (None)
    let mut raw: Vec<u8> = Vec::with_capacity(height * (1 + width * 4));
    for y in 0..height {
        raw.push(0); // filter type None
        raw.extend_from_slice(&rgba[y * width * 4..(y + 1) * width * 4]);
    }

    // Store uncompressed via zlib non-compressed block (no extra deps)
    let mut zlib: Vec<u8> = Vec::new();
    zlib.push(0x78); // CMF: deflate, window=32KB
    zlib.push(0x01); // FLG: no dict, check bits
    // Non-compressed deflate blocks (BFINAL=1, BTYPE=00)
    let mut pos = 0usize;
    while pos < raw.len() {
        let block_len = (raw.len() - pos).min(65535) as u16;
        let last = (pos + block_len as usize) >= raw.len();
        zlib.push(last as u8); // BFINAL + BTYPE=00
        zlib.extend_from_slice(&block_len.to_le_bytes());
        zlib.extend_from_slice(&(!block_len).to_le_bytes());
        zlib.extend_from_slice(&raw[pos..pos + block_len as usize]);
        pos += block_len as usize;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out: Vec<u8> = Vec::new();
    // PNG signature
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    // IHDR
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // colour type RGBA
    ihdr.extend_from_slice(&[0, 0, 0]); // compression, filter, interlace
    write_chunk(&mut out, b"IHDR", &ihdr);
    // IDAT
    write_chunk(&mut out, b"IDAT", &zlib);
    // IEND
    write_chunk(&mut out, b"IEND", &[]);

    let mut f = std::fs::File::create(path)?;
    f.write_all(&out)?;
    Ok(())
}
