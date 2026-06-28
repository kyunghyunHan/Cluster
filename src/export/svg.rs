#![allow(dead_code)]
use crate::model::{Component, ComponentKind, Wire};
use egui::{Pos2, Rect};

/// Build an SVG schematic document from the given circuit.
///
/// Each component is rendered with a proper IEC/ANSI schematic symbol rather
/// than a generic rectangle.  Wires, net labels, ref-des, and values are
/// preserved.  Energized state colors the strokes amber.
pub(crate) fn circuit_to_svg(
    components: &[Component],
    wires: &[Wire],
    energized_components: &std::collections::HashSet<u64>,
    energized_wires: &std::collections::HashSet<u64>,
) -> String {
    let bounds = circuit_bounds(components, wires)
        .unwrap_or_else(|| Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(960.0, 640.0)));
    let margin = 50.0;
    let min_x = bounds.left() - margin;
    let min_y = bounds.top() - margin;
    let width = (bounds.width() + margin * 2.0).max(480.0);
    let height = (bounds.height() + margin * 2.0).max(320.0);

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
         viewBox=\"{min_x:.1} {min_y:.1} {width:.1} {height:.1}\" \
         width=\"{width:.1}\" height=\"{height:.1}\">\n"
    ));
    // Dark background
    svg.push_str(&format!(
        "<rect x=\"{min_x:.1}\" y=\"{min_y:.1}\" \
         width=\"{width:.1}\" height=\"{height:.1}\" fill=\"#101216\"/>\n"
    ));

    // ── Defs: arrow markers ──────────────────────────────────────────────────
    svg.push_str(
        "<defs>\
         <marker id=\"ah\" markerWidth=\"6\" markerHeight=\"4\" refX=\"5\" refY=\"2\" \
           orient=\"auto\"><polygon points=\"0 0, 6 2, 0 4\" fill=\"#dee2e8\"/></marker>\
         </defs>\n",
    );

    svg.push_str("<g fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n");

    // ── Wires ────────────────────────────────────────────────────────────────
    for wire in wires {
        if wire.points.len() < 2 {
            continue;
        }
        let color = if energized_wires.contains(&wire.id) {
            "#ffaa37"
        } else {
            "#69b2ff"
        };
        let pts: String = wire
            .points
            .iter()
            .map(|p| format!("{:.1},{:.1}", p.x, p.y))
            .collect::<Vec<_>>()
            .join(" ");
        svg.push_str(&format!(
            "<polyline points=\"{pts}\" stroke=\"{color}\" stroke-width=\"2.4\"/>\n"
        ));
    }

    // ── Junction dots ────────────────────────────────────────────────────────
    // (junction rendering is handled by main.rs for the canvas; in SVG we skip explicit
    //  junctions since they are topology-only markers — the wire geometry already shows them)

    // ── Components ───────────────────────────────────────────────────────────
    for component in components {
        let energized = energized_components.contains(&component.id);
        let stroke_color = if energized { "#ffb950" } else { "#dee2e8" };
        let sw = if energized { 2.4_f32 } else { 2.0_f32 };
        let cx = component.pos.x;
        let cy = component.pos.y;

        // Dispatch to per-symbol drawing function
        svg_component(&mut svg, component, cx, cy, stroke_color, sw, energized);

        // Ref-des label (below component)
        if !matches!(
            component.kind,
            ComponentKind::TextNote | ComponentKind::NetLabel
        ) {
            svg.push_str(&format!(
                "<text x=\"{cx:.1}\" y=\"{:.1}\" fill=\"#e1e4e8\" \
                 font-family=\"Arial,sans-serif\" font-size=\"11\" \
                 text-anchor=\"middle\">{}</text>\n",
                cy + symbol_label_y_offset(component.kind),
                escape_xml(&component.label)
            ));
        }

        // Value label (above component)
        if !component.value.trim().is_empty()
            && !matches!(
                component.kind,
                ComponentKind::TextNote | ComponentKind::Ground | ComponentKind::NetLabel
            )
        {
            svg.push_str(&format!(
                "<text x=\"{cx:.1}\" y=\"{:.1}\" fill=\"#9aa4ae\" \
                 font-family=\"Arial,sans-serif\" font-size=\"10\" \
                 text-anchor=\"middle\">{}</text>\n",
                cy - symbol_value_y_offset(component.kind),
                escape_xml(&component.value)
            ));
        }
    }

    svg.push_str("</g>\n</svg>\n");
    svg
}

// ─── Per-component SVG symbol renderer ──────────────────────────────────────

fn svg_component(
    out: &mut String,
    c: &Component,
    cx: f32,
    cy: f32,
    color: &str,
    sw: f32,
    energized: bool,
) {
    let s = format!("stroke=\"{color}\" stroke-width=\"{sw:.1}\"");
    match c.kind {
        ComponentKind::Resistor | ComponentKind::Thermistor | ComponentKind::Varistor => {
            svg_resistor(out, cx, cy, &s, c.kind);
        }
        ComponentKind::Capacitor => svg_capacitor(out, cx, cy, &s),
        ComponentKind::Inductor => svg_inductor(out, cx, cy, &s),
        ComponentKind::Diode | ComponentKind::SchottkyDiode | ComponentKind::TvsDiode => {
            svg_diode(out, cx, cy, &s, false);
        }
        ComponentKind::ZenerDiode => svg_zener(out, cx, cy, &s),
        ComponentKind::Led => svg_led(out, cx, cy, &s, energized),
        ComponentKind::NpnTransistor | ComponentKind::Phototransistor => {
            svg_bjt(out, cx, cy, &s, true)
        }
        ComponentKind::PnpTransistor => svg_bjt(out, cx, cy, &s, false),
        ComponentKind::Nmosfet | ComponentKind::Pmosfet => svg_mosfet(out, cx, cy, &s),
        ComponentKind::Ground => svg_ground(out, cx, cy, &s),
        ComponentKind::VSource => svg_vsource(out, cx, cy, &s),
        ComponentKind::ISource => svg_isource(out, cx, cy, &s),
        ComponentKind::Battery => svg_battery(out, cx, cy, &s),
        ComponentKind::Switch | ComponentKind::SlideSwitch => {
            let closed = !c.value.to_ascii_lowercase().contains("open");
            svg_switch(out, cx, cy, &s, closed);
        }
        ComponentKind::PushButton => {
            let closed = c.value.to_ascii_lowercase().contains("closed");
            svg_push_button(out, cx, cy, &s, closed);
        }
        ComponentKind::Lamp => svg_lamp(out, cx, cy, &s),
        ComponentKind::Fuse => svg_fuse(out, cx, cy, &s),
        ComponentKind::Potentiometer => svg_potentiometer(out, cx, cy, &s),
        ComponentKind::VoltageReg | ComponentKind::VoltageRef => {
            svg_ic_box(out, cx, cy, &s, color, "REG")
        }
        ComponentKind::OpAmp => svg_opamp(out, cx, cy, &s),
        ComponentKind::NetLabel => svg_net_label(out, cx, cy, &s, color, &c.value),
        ComponentKind::TextNote => svg_text_note(out, cx, cy, color, &c.value),
        ComponentKind::Crystal => svg_crystal(out, cx, cy, &s),
        ComponentKind::Relay => svg_ic_box(out, cx, cy, &s, color, "K"),
        ComponentKind::DcMotor => svg_dc_motor(out, cx, cy, &s),
        ComponentKind::Voltmeter => svg_meter(out, cx, cy, &s, color, "V"),
        ComponentKind::Ammeter => svg_meter(out, cx, cy, &s, color, "A"),
        // Large module blocks
        ComponentKind::Esp32 => svg_module_box(out, cx, cy, &s, color, "ESP32", 80.0, 160.0),
        ComponentKind::Esp32S3 => svg_module_box(out, cx, cy, &s, color, "ESP32-S3", 80.0, 160.0),
        ComponentKind::Esp32C3 => svg_module_box(out, cx, cy, &s, color, "ESP32-C3", 60.0, 100.0),
        ComponentKind::ArduinoUno => {
            svg_module_box(out, cx, cy, &s, color, "Arduino\nUno", 80.0, 150.0)
        }
        ComponentKind::RaspberryPiPico => {
            svg_module_box(out, cx, cy, &s, color, "Pico", 60.0, 90.0)
        }
        ComponentKind::Stm32BluePill => {
            svg_module_box(out, cx, cy, &s, color, "STM32\nBlue Pill", 70.0, 130.0)
        }
        ComponentKind::Stm32Nucleo64 => {
            svg_module_box(out, cx, cy, &s, color, "STM32\nNucleo", 90.0, 160.0)
        }
        ComponentKind::Oled => svg_module_box(out, cx, cy, &s, color, "OLED", 50.0, 50.0),
        ComponentKind::Sensor => svg_module_box(out, cx, cy, &s, color, "SEN", 50.0, 50.0),
        ComponentKind::Dht11 => svg_module_box(out, cx, cy, &s, color, "DHT11", 40.0, 40.0),
        ComponentKind::Dht22 => svg_module_box(out, cx, cy, &s, color, "DHT22", 40.0, 40.0),
        ComponentKind::HcSr04 => svg_module_box(out, cx, cy, &s, color, "HC-SR04", 50.0, 30.0),
        ComponentKind::NeoPixel => svg_module_box(out, cx, cy, &s, color, "NP", 25.0, 25.0),
        ComponentKind::PirSensor => svg_module_box(out, cx, cy, &s, color, "PIR", 40.0, 40.0),
        ComponentKind::Buzzer => svg_buzzer(out, cx, cy, &s),
        ComponentKind::Servo => svg_module_box(out, cx, cy, &s, color, "SV", 35.0, 35.0),
        ComponentKind::Breadboard => {
            svg_module_box(out, cx, cy, &s, color, "Breadboard", 120.0, 70.0)
        }
        ComponentKind::LogicNot => svg_logic_not(out, cx, cy, &s),
        ComponentKind::LogicAnd => svg_logic_and(out, cx, cy, &s, false),
        ComponentKind::LogicNand => svg_logic_and(out, cx, cy, &s, true),
        ComponentKind::LogicOr | ComponentKind::LogicXor => svg_logic_or(out, cx, cy, &s, false),
        ComponentKind::LogicNor => svg_logic_or(out, cx, cy, &s, true),
        ComponentKind::Timer555 => svg_ic_box(out, cx, cy, &s, color, "555"),
        ComponentKind::Transformer => svg_transformer(out, cx, cy, &s),
        ComponentKind::Display7Seg => svg_module_box(out, cx, cy, &s, color, "7SEG", 35.0, 50.0),
        ComponentKind::MotorDriver => svg_module_box(out, cx, cy, &s, color, "MD", 55.0, 55.0),
        ComponentKind::Optocoupler => svg_ic_box(out, cx, cy, &s, color, "OC"),
        ComponentKind::GenericIc => svg_ic_box(out, cx, cy, &s, color, "IC"),
    }
}

// ── Schematic symbol primitives ──────────────────────────────────────────────

/// Resistor: two leads with a rectangular body (IEEE-style).
fn svg_resistor(out: &mut String, cx: f32, cy: f32, s: &str, kind: ComponentKind) {
    let hw = 16.0_f32;
    let hh = 7.0_f32;
    // leads
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx - 40.0,
        cx - hw
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx + hw,
        cx + 40.0
    ));
    // body
    out.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" {s}/>\n",
        cx - hw,
        cy - hh,
        hw * 2.0,
        hh * 2.0
    ));
    // Thermistor marker
    if kind == ComponentKind::Thermistor {
        out.push_str(&format!(
            "<text x=\"{cx:.1}\" y=\"{:.1}\" fill=\"#9aa4ae\" \
             font-family=\"Arial\" font-size=\"9\" text-anchor=\"middle\">NTC</text>\n",
            cy + 4.0
        ));
    }
}

/// Capacitor: two parallel plates with leads.
fn svg_capacitor(out: &mut String, cx: f32, cy: f32, s: &str) {
    // leads
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx - 40.0,
        cx - 5.0
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx + 5.0,
        cx + 40.0
    ));
    // plates
    let ph = 14.0_f32;
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
        cx - 5.0,
        cy - ph,
        cx - 5.0,
        cy + ph
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
        cx + 5.0,
        cy - ph,
        cx + 5.0,
        cy + ph
    ));
}

/// Inductor: series of bumps approximated by arcs.
fn svg_inductor(out: &mut String, cx: f32, cy: f32, s: &str) {
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx - 40.0,
        cx - 22.0
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx + 22.0,
        cx + 40.0
    ));
    // Three arcs
    for i in 0..3 {
        let bx = cx - 15.0 + i as f32 * 15.0;
        out.push_str(&format!(
            "<path d=\"M {:.1} {cy:.1} a 7.5 7.5 0 0 1 15 0\" {s}/>\n",
            bx
        ));
    }
}

/// Diode: triangle pointing right with a bar.
fn svg_diode(out: &mut String, cx: f32, cy: f32, s: &str, filled: bool) {
    let fill = if filled { "#ffb950" } else { "none" };
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx - 40.0,
        cx - 12.0
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx + 12.0,
        cx + 40.0
    ));
    out.push_str(&format!(
        "<polygon points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" fill=\"{fill}\" {s}/>\n",
        cx - 12.0,
        cy - 12.0,
        cx - 12.0,
        cy + 12.0,
        cx + 12.0,
        cy
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
        cx + 12.0,
        cy - 12.0,
        cx + 12.0,
        cy + 12.0
    ));
}

/// Zener: diode with bent cathode bar.
fn svg_zener(out: &mut String, cx: f32, cy: f32, s: &str) {
    svg_diode(out, cx, cy, s, false);
    // Override the cathode bar with zener bends
    out.push_str(&format!(
        "<polyline points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" {s}/>\n",
        cx + 8.0,
        cy - 16.0,
        cx + 12.0,
        cy - 12.0,
        cx + 12.0,
        cy + 12.0,
        cx + 16.0,
        cy + 16.0
    ));
}

/// LED: diode with two arrow rays.
fn svg_led(out: &mut String, cx: f32, cy: f32, s: &str, energized: bool) {
    svg_diode(out, cx, cy, s, energized);
    let ray_color = if energized { "#ffe030" } else { "#9aa4ae" };
    let rs = format!("stroke=\"{ray_color}\" stroke-width=\"1.6\" marker-end=\"url(#ah)\"");
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {rs}/>\n",
        cx + 2.0,
        cy - 16.0,
        cx + 16.0,
        cy - 28.0
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {rs}/>\n",
        cx + 8.0,
        cy - 12.0,
        cx + 22.0,
        cy - 24.0
    ));
}

/// BJT: NPN or PNP symbol.
fn svg_bjt(out: &mut String, cx: f32, cy: f32, s: &str, npn: bool) {
    // Base lead
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx - 40.0,
        cx - 12.0
    ));
    // Vertical bar
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
        cx - 12.0,
        cy - 22.0,
        cx - 12.0,
        cy + 22.0
    ));
    // Emitter and collector leads with arrows
    let (ey, cy2) = if npn {
        (cy + 18.0, cy - 18.0)
    } else {
        (cy - 18.0, cy + 18.0)
    };
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
        cx - 12.0,
        cy,
        cx + 8.0,
        ey
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
        cx + 8.0,
        ey,
        cx + 40.0,
        cy + (if npn { 30.0 } else { -30.0 })
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
        cx - 12.0,
        cy,
        cx + 8.0,
        cy2
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
        cx + 8.0,
        cy2,
        cx + 40.0,
        cy - (if npn { 30.0 } else { -30.0 })
    ));
}

/// MOSFET symbol (simplified E/S/G).
fn svg_mosfet(out: &mut String, cx: f32, cy: f32, s: &str) {
    // Gate lead
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx - 40.0,
        cx - 14.0
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
        cx - 14.0,
        cy - 18.0,
        cx - 14.0,
        cy + 18.0
    ));
    // Body line
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
        cx - 8.0,
        cy - 18.0,
        cx - 8.0,
        cy + 18.0
    ));
    // D and S dashes + leads
    for (dy, label_y) in [(-14.0_f32, -20.0_f32), (14.0, 20.0)] {
        out.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
            cx - 8.0,
            cy + dy,
            cx + 8.0,
            cy + dy
        ));
        out.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
            cx + 8.0,
            cy + dy,
            cx + 40.0,
            cy + label_y
        ));
    }
}

/// Ground: three horizontal bars narrowing downward.
fn svg_ground(out: &mut String, cx: f32, cy: f32, s: &str) {
    // Lead
    out.push_str(&format!(
        "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cy - 20.0
    ));
    // Three bars
    for (i, hw) in [(0, 18.0_f32), (1, 12.0), (2, 6.0)] {
        let y = cy + i as f32 * 8.0;
        out.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{y:.1}\" x2=\"{:.1}\" y2=\"{y:.1}\" {s}/>\n",
            cx - hw,
            cx + hw
        ));
    }
}

/// Voltage source: circle with +/-.
fn svg_vsource(out: &mut String, cx: f32, cy: f32, s: &str) {
    let r = 18.0_f32;
    out.push_str(&format!(
        "<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{r:.1}\" {s}/>\n"
    ));
    // leads
    out.push_str(&format!(
        "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{:.1}\" {s}/>\n",
        cy - 40.0,
        cy - r
    ));
    out.push_str(&format!(
        "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{:.1}\" {s}/>\n",
        cy + r,
        cy + 40.0
    ));
    // + and - symbols
    out.push_str(&format!(
        "<text x=\"{cx:.1}\" y=\"{:.1}\" fill=\"#dee2e8\" \
         font-family=\"Arial\" font-size=\"12\" text-anchor=\"middle\">+</text>\n",
        cy - 4.0
    ));
    out.push_str(&format!(
        "<text x=\"{cx:.1}\" y=\"{:.1}\" fill=\"#dee2e8\" \
         font-family=\"Arial\" font-size=\"14\" text-anchor=\"middle\">−</text>\n",
        cy + 14.0
    ));
}

/// Current source: circle with arrow.
fn svg_isource(out: &mut String, cx: f32, cy: f32, s: &str) {
    let r = 18.0_f32;
    out.push_str(&format!(
        "<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{r:.1}\" {s}/>\n"
    ));
    out.push_str(&format!(
        "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{:.1}\" {s}/>\n",
        cy - 40.0,
        cy - r
    ));
    out.push_str(&format!(
        "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{:.1}\" {s}/>\n",
        cy + r,
        cy + 40.0
    ));
    // Arrow inside circle pointing up
    out.push_str(&format!(
        "<polygon points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" fill=\"#dee2e8\"/>\n",
        cx,
        cy - 10.0,
        cx - 5.0,
        cy + 4.0,
        cx + 5.0,
        cy + 4.0
    ));
    out.push_str(&format!(
        "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{:.1}\" {s}/>\n",
        cy + 4.0,
        cy + 12.0
    ));
}

/// Battery: alternating long/short plates.
fn svg_battery(out: &mut String, cx: f32, cy: f32, s: &str) {
    // leads
    out.push_str(&format!(
        "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{:.1}\" {s}/>\n",
        cy - 40.0,
        cy - 10.0
    ));
    out.push_str(&format!(
        "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{:.1}\" {s}/>\n",
        cy + 10.0,
        cy + 40.0
    ));
    // Two cells: long plate (−) and short plate (+)
    for (i, long) in [(0, false), (1, true)] {
        let y = cy - 10.0 + i as f32 * 20.0;
        let hw = if long { 14.0_f32 } else { 8.0_f32 };
        out.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{y:.1}\" x2=\"{:.1}\" y2=\"{y:.1}\" {s}/>\n",
            cx - hw,
            cx + hw
        ));
    }
    // + label
    out.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"#9aa4ae\" \
         font-family=\"Arial\" font-size=\"10\" text-anchor=\"start\">+</text>\n",
        cx + 16.0,
        cy - 6.0
    ));
}

/// Switch: lead gap with a lever line.
fn svg_switch(out: &mut String, cx: f32, cy: f32, s: &str, closed: bool) {
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx - 40.0,
        cx - 16.0
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx + 16.0,
        cx + 40.0
    ));
    // Contact dots
    out.push_str(&format!(
        "<circle cx=\"{:.1}\" cy=\"{cy:.1}\" r=\"3\" {s} fill=\"none\"/>\n",
        cx - 16.0
    ));
    out.push_str(&format!(
        "<circle cx=\"{:.1}\" cy=\"{cy:.1}\" r=\"3\" {s} fill=\"none\"/>\n",
        cx + 16.0
    ));
    // Lever
    let (lever_x2, lever_y2) = if closed {
        (cx + 16.0, cy)
    } else {
        (cx + 14.0, cy - 16.0)
    };
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{lever_x2:.1}\" y2=\"{lever_y2:.1}\" {s}/>\n",
        cx - 16.0
    ));
}

/// Push button.
fn svg_push_button(out: &mut String, cx: f32, cy: f32, s: &str, closed: bool) {
    svg_switch(out, cx, cy, s, closed);
    // Button cap above the switch
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
        cx - 12.0,
        cy - 20.0,
        cx + 12.0,
        cy - 20.0
    ));
    out.push_str(&format!(
        "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cy - 20.0
    ));
}

/// Lamp (light bulb inside a circle).
fn svg_lamp(out: &mut String, cx: f32, cy: f32, s: &str) {
    let r = 16.0_f32;
    out.push_str(&format!(
        "<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{r:.1}\" {s}/>\n"
    ));
    // X inside
    let d = r * 0.65;
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
        cx - d,
        cy - d,
        cx + d,
        cy + d
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
        cx + d,
        cy - d,
        cx - d,
        cy + d
    ));
    out.push_str(&format!(
        "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{:.1}\" {s}/>\n",
        cy - 40.0,
        cy - r
    ));
    out.push_str(&format!(
        "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{:.1}\" {s}/>\n",
        cy + r,
        cy + 40.0
    ));
}

/// Fuse: rectangle with a line through it.
fn svg_fuse(out: &mut String, cx: f32, cy: f32, s: &str) {
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx - 40.0,
        cx - 16.0
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx + 16.0,
        cx + 40.0
    ));
    out.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"32\" height=\"12\" {s}/>\n",
        cx - 16.0,
        cy - 6.0
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx - 16.0,
        cx + 16.0
    ));
}

/// Potentiometer: resistor + wiper arrow.
fn svg_potentiometer(out: &mut String, cx: f32, cy: f32, s: &str) {
    svg_resistor(out, cx, cy, s, ComponentKind::Resistor);
    // Wiper arrow from above
    out.push_str(&format!(
        "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{:.1}\" {s}/>\n",
        cy - 30.0,
        cy - 7.0
    ));
    out.push_str(&format!(
        "<polygon points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" fill=\"#dee2e8\"/>\n",
        cx,
        cy - 7.0,
        cx - 5.0,
        cy - 17.0,
        cx + 5.0,
        cy - 17.0
    ));
}

/// Op-amp: triangle.
fn svg_opamp(out: &mut String, cx: f32, cy: f32, s: &str) {
    let h = 28.0_f32;
    let w = 36.0_f32;
    out.push_str(&format!(
        "<polygon points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{cy:.1}\" {s}/>\n",
        cx - w,
        cy - h,
        cx - w,
        cy + h,
        cx + w
    ));
    // + input
    out.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"#dee2e8\" \
         font-family=\"Arial\" font-size=\"10\">+</text>\n",
        cx - w + 4.0,
        cy + 4.0
    ));
    // − input
    out.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"#dee2e8\" \
         font-family=\"Arial\" font-size=\"11\">−</text>\n",
        cx - w + 4.0,
        cy - h + 14.0
    ));
    // Leads
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
        cx - 40.0,
        cy - h * 0.6,
        cx - w,
        cy - h * 0.6
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
        cx - 40.0,
        cy + h * 0.6,
        cx - w,
        cy + h * 0.6
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx + w,
        cx + 40.0
    ));
}

/// Net label: flag shape with the label text.
fn svg_net_label(out: &mut String, cx: f32, cy: f32, s: &str, color: &str, label: &str) {
    let hw = 20.0_f32 + label.len() as f32 * 4.0;
    out.push_str(&format!(
        "<polygon points=\"{:.1},{cy:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {cx:.1},{cy:.1}\" \
         fill=\"#1a2030\" {s}/>\n",
        cx,
        cx + hw,
        cy,
        cx + hw,
        cy - 12.0,
        cx,
        cy - 12.0
    ));
    out.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{color}\" \
         font-family=\"monospace\" font-size=\"11\" text-anchor=\"middle\">{}</text>\n",
        cx + hw * 0.5,
        cy - 2.0,
        escape_xml(label)
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{cx:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx - 20.0
    ));
}

/// Text note: plain text box.
fn svg_text_note(out: &mut String, cx: f32, cy: f32, color: &str, text: &str) {
    for (i, line) in text.lines().enumerate() {
        out.push_str(&format!(
            "<text x=\"{cx:.1}\" y=\"{:.1}\" fill=\"{color}\" \
             font-family=\"Arial,sans-serif\" font-size=\"11\" \
             text-anchor=\"middle\">{}</text>\n",
            cy + i as f32 * 15.0,
            escape_xml(line)
        ));
    }
}

/// Crystal: two plates with capacitor-like symbol.
fn svg_crystal(out: &mut String, cx: f32, cy: f32, s: &str) {
    // Leads
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx - 40.0,
        cx - 14.0
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx + 14.0,
        cx + 40.0
    ));
    // Crystal body rect
    out.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"12\" height=\"22\" {s}/>\n",
        cx - 6.0,
        cy - 11.0
    ));
    // Outer plates
    for xoff in [-14.0_f32, 14.0] {
        out.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
            cx + xoff,
            cy - 14.0,
            cx + xoff,
            cy + 14.0
        ));
    }
}

/// DC motor: circle with M.
fn svg_dc_motor(out: &mut String, cx: f32, cy: f32, s: &str) {
    let r = 18.0_f32;
    out.push_str(&format!(
        "<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{r:.1}\" {s}/>\n"
    ));
    out.push_str(&format!(
        "<text x=\"{cx:.1}\" y=\"{:.1}\" fill=\"#dee2e8\" \
         font-family=\"Arial\" font-size=\"14\" font-weight=\"bold\" text-anchor=\"middle\">M</text>\n",
        cy + 5.0
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx - 40.0,
        cx - r
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx + r,
        cx + 40.0
    ));
}

/// Buzzer: circle with Z.
fn svg_buzzer(out: &mut String, cx: f32, cy: f32, s: &str) {
    let r = 16.0_f32;
    out.push_str(&format!(
        "<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{r:.1}\" {s}/>\n"
    ));
    out.push_str(&format!(
        "<text x=\"{cx:.1}\" y=\"{:.1}\" fill=\"#dee2e8\" \
         font-family=\"Arial\" font-size=\"12\" text-anchor=\"middle\">BZ</text>\n",
        cy + 4.0
    ));
    out.push_str(&format!(
        "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{:.1}\" {s}/>\n",
        cy - 40.0,
        cy - r
    ));
    out.push_str(&format!(
        "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{:.1}\" {s}/>\n",
        cy + r,
        cy + 40.0
    ));
}

/// Voltmeter / Ammeter: circle with label.
fn svg_meter(out: &mut String, cx: f32, cy: f32, s: &str, color: &str, letter: &str) {
    let r = 18.0_f32;
    out.push_str(&format!(
        "<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{r:.1}\" {s}/>\n"
    ));
    out.push_str(&format!(
        "<text x=\"{cx:.1}\" y=\"{:.1}\" fill=\"{color}\" \
         font-family=\"Arial\" font-size=\"16\" font-weight=\"bold\" text-anchor=\"middle\">{letter}</text>\n",
        cy + 6.0
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx - 40.0,
        cx - r
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx + r,
        cx + 40.0
    ));
}

/// Logic NOT gate: triangle + bubble.
fn svg_logic_not(out: &mut String, cx: f32, cy: f32, s: &str) {
    let hw = 16.0_f32;
    let hh = 14.0_f32;
    out.push_str(&format!(
        "<polygon points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{cy:.1}\" {s}/>\n",
        cx - hw,
        cy - hh,
        cx - hw,
        cy + hh,
        cx + hw - 4.0
    ));
    out.push_str(&format!(
        "<circle cx=\"{:.1}\" cy=\"{cy:.1}\" r=\"4\" {s}/>\n",
        cx + hw
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx - 40.0,
        cx - hw
    ));
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx + hw + 4.0,
        cx + 40.0
    ));
}

/// AND / NAND gate.
fn svg_logic_and(out: &mut String, cx: f32, cy: f32, s: &str, nand: bool) {
    let hw = 18.0_f32;
    let hh = 16.0_f32;
    let rx = cx + (if nand { hw - 4.0 } else { hw });
    out.push_str(&format!(
        "<path d=\"M {:.1} {:.1} L {:.1} {:.1} A {hh:.1} {hh:.1} 0 0 1 {:.1} {:.1} Z\" {s}/>\n",
        cx - hw,
        cy - hh,
        cx,
        cy - hh,
        cx,
        cy + hh
    ));
    if nand {
        out.push_str(&format!(
            "<circle cx=\"{rx:.1}\" cy=\"{cy:.1}\" r=\"4\" {s}/>\n"
        ));
    }
    for dy in [-hh * 0.5, hh * 0.5] {
        out.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
            cx - 40.0,
            cy + dy,
            cx - hw,
            cy + dy
        ));
    }
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx + hw + if nand { 8.0 } else { 0.0 },
        cx + 40.0
    ));
}

/// OR / NOR gate.
fn svg_logic_or(out: &mut String, cx: f32, cy: f32, s: &str, nor: bool) {
    let hw = 18.0_f32;
    let hh = 16.0_f32;
    out.push_str(&format!(
        "<path d=\"M {:.1} {:.1} Q {:.1} {cy:.1} {:.1} {:.1} Q {:.1} {cy:.1} {:.1} {:.1} Z\" {s}/>\n",
        cx - hw,
        cy - hh,
        cx + hw,
        cx + hw,
        cy,
        cx + hw,
        cx - hw,
        cy + hh
    ));
    if nor {
        let rx = cx + hw + 4.0;
        out.push_str(&format!(
            "<circle cx=\"{rx:.1}\" cy=\"{cy:.1}\" r=\"4\" {s}/>\n"
        ));
    }
    for dy in [-hh * 0.5, hh * 0.5] {
        out.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {s}/>\n",
            cx - 40.0,
            cy + dy,
            cx - hw + 6.0,
            cy + dy
        ));
    }
    out.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{:.1}\" y2=\"{cy:.1}\" {s}/>\n",
        cx + hw + if nor { 8.0 } else { 0.0 },
        cx + 40.0
    ));
}

/// Generic IC box with label.
fn svg_ic_box(out: &mut String, cx: f32, cy: f32, s: &str, color: &str, label: &str) {
    let hw = 28.0_f32;
    let hh = 22.0_f32;
    out.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
         rx=\"3\" fill=\"#181e26\" {s}/>\n",
        cx - hw,
        cy - hh,
        hw * 2.0,
        hh * 2.0
    ));
    out.push_str(&format!(
        "<text x=\"{cx:.1}\" y=\"{:.1}\" fill=\"{color}\" \
         font-family=\"monospace\" font-size=\"11\" text-anchor=\"middle\">{}</text>\n",
        cy + 4.0,
        escape_xml(label)
    ));
}

/// Large module box (ESP32, Arduino, etc.).
fn svg_module_box(
    out: &mut String,
    cx: f32,
    cy: f32,
    s: &str,
    color: &str,
    label: &str,
    hw: f32,
    hh: f32,
) {
    out.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
         rx=\"6\" fill=\"#181e26\" {s}/>\n",
        cx - hw,
        cy - hh,
        hw * 2.0,
        hh * 2.0
    ));
    for (i, line) in label.lines().enumerate() {
        out.push_str(&format!(
            "<text x=\"{cx:.1}\" y=\"{:.1}\" fill=\"{color}\" \
             font-family=\"monospace\" font-size=\"12\" text-anchor=\"middle\">{}</text>\n",
            cy - (label.lines().count() as f32 - 1.0) * 8.0 + i as f32 * 16.0,
            escape_xml(line)
        ));
    }
}

/// Transformer: two coils side by side.
fn svg_transformer(out: &mut String, cx: f32, cy: f32, s: &str) {
    // Primary coil (left bumps)
    for i in 0..3 {
        let bx = cx - 22.0 + i as f32 * 10.0;
        out.push_str(&format!(
            "<path d=\"M {bx:.1} {cy:.1} a 5 5 0 0 1 10 0\" {s}/>\n"
        ));
    }
    // Secondary coil (right bumps)
    for i in 0..3 {
        let bx = cx + 2.0 + i as f32 * 10.0;
        out.push_str(&format!(
            "<path d=\"M {bx:.1} {cy:.1} a 5 5 0 0 0 10 0\" {s}/>\n"
        ));
    }
    // Center line separator
    out.push_str(&format!(
        "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{:.1}\" {s}/>\n",
        cy - 18.0,
        cy + 18.0
    ));
    // Leads
    for (xoff, xend) in [(-22.0_f32, -40.0_f32), (32.0, 40.0)] {
        out.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{cy:.1}\" x2=\"{xend:.1}\" y2=\"{cy:.1}\" {s}/>\n",
            cx + xoff
        ));
    }
}

// ─── Geometry helpers ────────────────────────────────────────────────────────

fn symbol_label_y_offset(kind: ComponentKind) -> f32 {
    match kind {
        ComponentKind::Ground => 40.0,
        ComponentKind::Esp32 | ComponentKind::Esp32S3 => 175.0,
        ComponentKind::ArduinoUno => 165.0,
        ComponentKind::Esp32C3 | ComponentKind::RaspberryPiPico => 115.0,
        ComponentKind::Breadboard => 85.0,
        ComponentKind::Oled | ComponentKind::Sensor => 65.0,
        _ => 30.0,
    }
}

fn symbol_value_y_offset(kind: ComponentKind) -> f32 {
    match kind {
        ComponentKind::Esp32 | ComponentKind::Esp32S3 => 175.0,
        ComponentKind::ArduinoUno => 165.0,
        ComponentKind::Esp32C3 | ComponentKind::RaspberryPiPico => 115.0,
        ComponentKind::Breadboard => 85.0,
        ComponentKind::Oled | ComponentKind::Sensor => 65.0,
        _ => 32.0,
    }
}

fn circuit_bounds(components: &[Component], wires: &[Wire]) -> Option<Rect> {
    let mut rect: Option<Rect> = None;
    let expand = |r: &mut Option<Rect>, pos: Pos2| {
        *r = Some(match r {
            None => Rect::from_center_size(pos, egui::Vec2::splat(4.0)),
            Some(existing) => existing.union(Rect::from_center_size(pos, egui::Vec2::splat(4.0))),
        });
    };
    for c in components {
        expand(&mut rect, c.pos);
    }
    for w in wires {
        for &p in &w.points {
            expand(&mut rect, p);
        }
    }
    rect
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
