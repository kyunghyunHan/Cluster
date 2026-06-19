#![allow(dead_code)]
use crate::model::{Component, ComponentKind, Wire};
use egui::{Pos2, Rect};

/// Build an SVG document from the given circuit components and wires.
/// Returns the complete SVG string, ready to write to a .svg file.
pub(crate) fn circuit_to_svg(
    components: &[Component],
    wires: &[Wire],
    energized_components: &std::collections::HashSet<u64>,
    energized_wires: &std::collections::HashSet<u64>,
) -> String {
    let bounds = circuit_bounds(components, wires)
        .unwrap_or_else(|| Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(960.0, 640.0)));
    let margin = 40.0;
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
    svg.push_str(&format!(
        "<rect x=\"{min_x:.1}\" y=\"{min_y:.1}\" \
         width=\"{width:.1}\" height=\"{height:.1}\" fill=\"#101216\"/>\n"
    ));
    svg.push_str("<g fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n");

    // Wires
    for wire in wires {
        if wire.points.len() < 2 {
            continue;
        }
        let color = if energized_wires.contains(&wire.id) {
            "#ffaa37"
        } else {
            "#69b2ff"
        };
        let pts = wire
            .points
            .iter()
            .map(|p| format!("{:.1},{:.1}", p.x, p.y))
            .collect::<Vec<_>>()
            .join(" ");
        svg.push_str(&format!(
            "<polyline points=\"{pts}\" stroke=\"{color}\" stroke-width=\"2.4\"/>\n"
        ));
    }

    // Components
    for component in components {
        let rect = component_rect(component);
        let energized = energized_components.contains(&component.id);
        let stroke = if energized { "#ffb950" } else { "#dee2e8" };
        let fill = if is_module(component.kind) {
            if energized { "#3e2e16" } else { "#181e26" }
        } else {
            "none"
        };
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
             rx=\"4\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"2\"/>\n",
            rect.left(),
            rect.top(),
            rect.width(),
            rect.height(),
        ));
        // Kind label inside
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{stroke}\" \
             font-family=\"Arial,sans-serif\" font-size=\"12\" text-anchor=\"middle\">{}</text>\n",
            rect.center().x,
            rect.center().y + 4.0,
            escape_xml(component_kind_label(component.kind))
        ));
        // Ref label below
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"#e1e4e8\" \
             font-family=\"Arial,sans-serif\" font-size=\"11\" text-anchor=\"middle\">{}</text>\n",
            rect.center().x,
            rect.bottom() + 15.0,
            escape_xml(&component.label)
        ));
        // Value label above
        if !component.value.trim().is_empty() {
            svg.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"#9aa4ae\" \
                 font-family=\"Arial,sans-serif\" font-size=\"10\" text-anchor=\"middle\">{}</text>\n",
                rect.center().x,
                rect.top() - 7.0,
                escape_xml(&component.value)
            ));
        }
        // Pin dots
        for pin_pos in component_pin_positions(component) {
            svg.push_str(&format!(
                "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"3.2\" \
                 fill=\"#facd5f\" stroke=\"#281f14\" stroke-width=\"1\"/>\n",
                pin_pos.x, pin_pos.y
            ));
        }
    }

    svg.push_str("</g>\n</svg>\n");
    svg
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

fn component_rect(c: &Component) -> Rect {
    let (hw, hh) = if is_module(c.kind) {
        (60.0_f32, 80.0_f32)
    } else {
        (20.0_f32, 14.0_f32)
    };
    Rect::from_center_size(c.pos, egui::Vec2::new(hw * 2.0, hh * 2.0))
}

fn component_pin_positions(c: &Component) -> Vec<Pos2> {
    // Delegate to the crate-root pin definitions so we don't duplicate the pin table.
    // component_pin_defs is pub(crate) in main.rs; call it from here.
    crate::component_pin_defs(c)
        .into_iter()
        .map(|p| p.pos)
        .collect()
}

fn is_module(kind: ComponentKind) -> bool {
    matches!(
        kind,
        ComponentKind::Esp32
            | ComponentKind::Esp32S3
            | ComponentKind::Esp32C3
            | ComponentKind::ArduinoUno
            | ComponentKind::RaspberryPiPico
            | ComponentKind::Breadboard
            | ComponentKind::OpAmp
            | ComponentKind::Timer555
            | ComponentKind::Oled
            | ComponentKind::GenericIc
            | ComponentKind::Voltmeter
            | ComponentKind::Ammeter
    )
}

fn component_kind_label(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Resistor => "R",
        ComponentKind::Capacitor => "C",
        ComponentKind::Inductor => "L",
        ComponentKind::Diode => "D",
        ComponentKind::Led => "LED",
        ComponentKind::ZenerDiode => "Z",
        ComponentKind::SchottkyDiode => "D(S)",
        ComponentKind::TvsDiode => "TVS",
        ComponentKind::Switch => "SW",
        ComponentKind::PushButton => "BTN",
        ComponentKind::SlideSwitch => "SW",
        ComponentKind::Ground => "GND",
        ComponentKind::VSource => "V",
        ComponentKind::ISource => "I",
        ComponentKind::Battery => "BAT",
        ComponentKind::OpAmp => "U(OA)",
        ComponentKind::Lamp => "LA",
        ComponentKind::Potentiometer => "RV",
        ComponentKind::NpnTransistor => "Q(NPN)",
        ComponentKind::PnpTransistor => "Q(PNP)",
        ComponentKind::Nmosfet => "M(N)",
        ComponentKind::Pmosfet => "M(P)",
        ComponentKind::VoltageReg => "VReg",
        ComponentKind::Fuse => "F",
        ComponentKind::LogicNot => "NOT",
        ComponentKind::LogicAnd => "AND",
        ComponentKind::LogicOr => "OR",
        ComponentKind::LogicNand => "NAND",
        ComponentKind::LogicNor => "NOR",
        ComponentKind::LogicXor => "XOR",
        ComponentKind::Esp32 => "ESP32",
        ComponentKind::Esp32S3 => "ESP32-S3",
        ComponentKind::Esp32C3 => "ESP32-C3",
        ComponentKind::ArduinoUno => "Arduino",
        ComponentKind::RaspberryPiPico => "Pico",
        ComponentKind::Breadboard => "BB",
        ComponentKind::Relay => "K",
        ComponentKind::DcMotor => "M",
        ComponentKind::Servo => "SV",
        ComponentKind::Oled => "OLED",
        ComponentKind::Sensor => "SEN",
        ComponentKind::NetLabel => "NET",
        ComponentKind::Timer555 => "555",
        ComponentKind::Crystal => "XTAL",
        ComponentKind::Transformer => "TX",
        ComponentKind::Display7Seg => "7SEG",
        ComponentKind::Thermistor => "RT",
        ComponentKind::Varistor => "VDR",
        ComponentKind::VoltageRef => "VRef",
        ComponentKind::MotorDriver => "MD",
        ComponentKind::Phototransistor => "QP",
        ComponentKind::Optocoupler => "OC",
        ComponentKind::GenericIc => "IC",
        ComponentKind::Voltmeter => "VM",
        ComponentKind::Ammeter => "AM",
        ComponentKind::TextNote => "NOTE",
        ComponentKind::Dht11 => "DHT11",
        ComponentKind::Dht22 => "DHT22",
        ComponentKind::HcSr04 => "US",
        ComponentKind::Buzzer => "BZ",
        ComponentKind::NeoPixel => "NP",
        ComponentKind::PirSensor => "PIR",
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
