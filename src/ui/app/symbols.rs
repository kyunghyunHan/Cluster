use super::*;

#[allow(clippy::too_many_arguments)] // Hot-path painter avoids per-frame option allocation.
pub(crate) fn draw_component(
    painter: &egui::Painter,
    component: &Component,
    selected: bool,
    show_pins: bool,
    energized: bool,
    connected_pins: &[(i32, i32)],
    view: CanvasView,
    dc_voltage: Option<f64>,
    dc_current: Option<f64>,
    show_dc_overlay: bool,
) {
    let stroke = if selected {
        Stroke::new(2.2_f32, Color32::from_rgb(90, 235, 170))
    } else if energized {
        Stroke::new(2.8_f32, Color32::from_rgb(255, 185, 80))
    } else {
        Stroke::new(2.0_f32, Color32::from_rgb(222, 226, 232))
    };
    let screen_center = view.to_screen(component.pos);
    let size = component_size(component) * view.zoom;
    let rect = Rect::from_center_size(screen_center, size);
    // Effective bounds (swapped for 90/270 rotation)
    let rot = ((component.rotation % 360) + 360) % 360;
    let bounds_size = if rot == 90 || rot == 270 {
        Vec2::new(size.y, size.x)
    } else {
        size
    };
    let bounds = Rect::from_center_size(screen_center, bounds_size);

    if selected {
        let sel_rect = bounds.expand(8.0);
        // Faint fill
        painter.rect_filled(
            sel_rect,
            4.0,
            Color32::from_rgba_unmultiplied(60, 230, 160, 10),
        );
        // Subtle outer rect
        painter.rect_stroke(
            sel_rect,
            4.0,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(80, 200, 140, 80)),
            StrokeKind::Outside,
        );
        // Corner L-brackets for crisp selection feel
        let col = Color32::from_rgb(70, 220, 150);
        let cs = Stroke::new(2.0_f32, col);
        let cr = sel_rect.width().min(sel_rect.height()) * 0.25;
        let corners = [
            sel_rect.left_top(),
            sel_rect.right_top(),
            sel_rect.left_bottom(),
            sel_rect.right_bottom(),
        ];
        let dx = [Vec2::X, -Vec2::X, Vec2::X, -Vec2::X];
        let dy = [Vec2::Y, Vec2::Y, -Vec2::Y, -Vec2::Y];
        for (i, &c) in corners.iter().enumerate() {
            painter.line_segment([c, c + dx[i] * cr], cs);
            painter.line_segment([c, c + dy[i] * cr], cs);
        }
    }

    match component.kind {
        ComponentKind::Resistor => {
            let ohms = parse_metric_value(&component.value, "ohm").unwrap_or(1000.0) as f64;
            draw_resistor_with_bands(painter, rect, component.rotation, stroke, ohms);
        }
        ComponentKind::Capacitor => draw_capacitor(painter, rect, component.rotation, stroke),
        ComponentKind::Inductor => draw_inductor(painter, rect, component.rotation, stroke),
        ComponentKind::Diode => draw_diode(painter, rect, component.rotation, stroke, false),
        ComponentKind::ZenerDiode => draw_zener(painter, rect, component.rotation, stroke),
        ComponentKind::Led => {
            if energized {
                let (outer, inner) = led_glow_colors(&component.value);
                painter.circle_filled(screen_center, rect.width().max(rect.height()) * 0.7, outer);
                painter.circle_filled(screen_center, rect.width().max(rect.height()) * 0.4, inner);
            }
            draw_led(painter, rect, component.rotation, stroke);
        }
        ComponentKind::NpnTransistor => {
            draw_npn(painter, rect, component.rotation, stroke, energized)
        }
        ComponentKind::PnpTransistor => {
            draw_pnp(painter, rect, component.rotation, stroke, energized)
        }
        ComponentKind::Nmosfet => {
            draw_nmosfet(painter, rect, component.rotation, stroke, energized)
        }
        ComponentKind::Pmosfet => {
            draw_pmosfet(painter, rect, component.rotation, stroke, energized)
        }
        ComponentKind::Potentiometer => {
            draw_potentiometer(painter, rect, component.rotation, stroke)
        }
        ComponentKind::VoltageReg => {
            draw_voltage_reg(painter, rect, component.rotation, stroke, energized)
        }
        ComponentKind::Fuse => draw_fuse(painter, rect, component.rotation, stroke),
        ComponentKind::LogicNot => draw_logic_not(painter, rect, component.rotation, stroke),
        ComponentKind::LogicAnd => draw_logic_and(painter, rect, component.rotation, stroke, false),
        ComponentKind::LogicOr => draw_logic_or(painter, rect, component.rotation, stroke, false),
        ComponentKind::LogicNand => draw_logic_and(painter, rect, component.rotation, stroke, true),
        ComponentKind::LogicNor => draw_logic_or(painter, rect, component.rotation, stroke, true),
        ComponentKind::LogicXor => draw_logic_xor(painter, rect, component.rotation, stroke, false),
        ComponentKind::Switch | ComponentKind::SlideSwitch => {
            let closed = !component.value.to_lowercase().contains("open");
            draw_switch(painter, rect, component.rotation, stroke, closed)
        }
        ComponentKind::PushButton => {
            let closed = !component.value.to_lowercase().contains("open");
            draw_push_button(painter, rect, component.rotation, stroke, closed);
        }
        ComponentKind::Ground => draw_ground(painter, rect, component.rotation, stroke),
        ComponentKind::VSource => draw_vsource(painter, rect, component.rotation, stroke),
        ComponentKind::ISource => draw_isource(painter, rect, component.rotation, stroke),
        ComponentKind::Battery => draw_battery(painter, rect, component.rotation, stroke),
        ComponentKind::OpAmp => draw_opamp(painter, rect, component.rotation, stroke),
        ComponentKind::Lamp => draw_lamp(painter, rect, component.rotation, stroke),
        ComponentKind::Esp32 => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "ESP32",
            &[
                "3V3",
                "GND",
                "GPIO2",
                "GPIO4",
                "GPIO15",
                "GPIO23 MOSI",
                "GPIO22 SCL",
                "GPIO21 SDA",
                "GPIO19 MISO",
                "GPIO18 SCK",
                "GPIO5 SS",
                "TX0",
                "RX0",
                "GND",
            ],
            &[
                "VIN",
                "GND",
                "GPIO34 ADC",
                "GPIO35 ADC",
                "GPIO32",
                "GPIO33",
                "GPIO25 DAC",
                "GPIO26 DAC",
                "GPIO27",
                "GPIO14",
                "GPIO13",
                "GPIO12",
                "GPIO0",
                "EN",
            ],
        ),
        ComponentKind::Esp32S3 => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "ESP32-S3",
            &[
                "3V3",
                "GND",
                "GPIO1",
                "GPIO2 SDA",
                "GPIO3 SCL",
                "GPIO4",
                "TX0",
                "RX0",
            ],
            &[
                "VIN", "GND", "GPIO8", "GPIO9", "GPIO10", "GPIO11", "EN", "RST",
            ],
        ),
        ComponentKind::Esp32C3 => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "ESP32-C3",
            &["3V3", "GND", "GPIO0", "GPIO1 SDA", "GPIO2 SCL", "TX", "RX"],
            &["5V", "GND", "GPIO3", "GPIO4", "GPIO5", "EN", "BOOT"],
        ),
        ComponentKind::ArduinoUno => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "ARDUINO UNO",
            &[
                "VIN", "5V", "3V3", "GND", "A0", "A1", "A2", "A3", "A4 SDA", "A5 SCL",
            ],
            &[
                "D2", "D3 PWM", "D4", "D5 PWM", "D6 PWM", "D7", "D8", "D9 PWM", "D10", "D11 MOSI",
                "D12 MISO", "D13 SCK", "TX", "RX",
            ],
        ),
        ComponentKind::RaspberryPiPico => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "PI PICO",
            &[
                "VSYS", "3V3", "GND", "GP0 TX", "GP1 RX", "GP4 SDA", "GP5 SCL",
            ],
            &["VBUS", "GND", "GP14", "GP15", "GP16", "GP17", "RUN"],
        ),
        ComponentKind::Stm32BluePill => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "STM32 BLUE PILL",
            &[
                "3V3", "GND", "PA0 ADC", "PA1 ADC", "PA2 TX2", "PA3 RX2", "PA5 SCK", "PA6 MISO",
                "PA7 MOSI", "PB6 SCL", "PB7 SDA",
            ],
            &[
                "5V",
                "GND",
                "VBAT",
                "PA9 TX1",
                "PA10 RX1",
                "PA13 SWDIO",
                "PA14 SWCLK",
                "PB0 ADC",
                "PB1 ADC",
                "BOOT0",
                "NRST",
            ],
        ),
        ComponentKind::Stm32Nucleo64 => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "STM32 NUCLEO",
            &[
                "3V3",
                "5V",
                "GND",
                "A0 PA0 ADC",
                "A1 PA1 ADC",
                "D14 PB9 SDA",
                "D15 PB8 SCL",
                "D13 PA5 SCK",
                "D12 PA6 MISO",
                "D11 PA7 MOSI",
            ],
            &[
                "VIN",
                "GND",
                "D0 PA3 RX",
                "D1 PA2 TX",
                "D2 PA10",
                "D3 PB3",
                "D4 PB5",
                "D5 PB4",
                "D6 PB10",
                "NRST",
            ],
        ),
        ComponentKind::Breadboard => draw_breadboard(painter, rect, stroke),
        ComponentKind::Relay => draw_relay(painter, rect, component.rotation, stroke),
        ComponentKind::DcMotor => draw_dc_motor(painter, rect, component.rotation, stroke),
        ComponentKind::Servo => draw_servo(painter, rect, stroke, energized),
        ComponentKind::Oled => draw_oled(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
        ),
        ComponentKind::Sensor => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "SENSOR",
            &["GND", "VCC", "SCL"],
            &["SDA"],
        ),
        ComponentKind::NetLabel => draw_net_label(painter, component, rect, stroke, energized),
        ComponentKind::Timer555 => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "555",
            &["GND", "TR", "Q", "R"],
            &["VCC", "DIS", "THR", "CV"],
        ),
        ComponentKind::Crystal => draw_crystal(painter, rect, component.rotation, stroke),
        ComponentKind::Transformer => draw_transformer(painter, rect, component.rotation, stroke),
        ComponentKind::Display7Seg => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "7-SEG",
            &["COM", "A", "B", "C"],
            &["D", "E", "F", "G"],
        ),
        ComponentKind::Thermistor => draw_thermistor(painter, rect, component.rotation, stroke),
        ComponentKind::Varistor => draw_varistor(painter, rect, component.rotation, stroke),
        ComponentKind::SchottkyDiode => {
            draw_diode(painter, rect, component.rotation, stroke, false);
            // Schottky mark: small horizontal stroke at cathode
            let center = rect.center();
            let r_h = rect.height() * 0.22;
            let s = view.scale_f(1.0);
            let _ = (center, r_h, s);
        }
        ComponentKind::TvsDiode => draw_diode(painter, rect, component.rotation, stroke, true),
        ComponentKind::VoltageRef => {
            draw_ic_box(painter, rect, component.rotation, stroke, energized, "VREF")
        }
        ComponentKind::MotorDriver => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "H-BRIDGE",
            &["VCC", "GND", "IN1", "IN2"],
            &["OUT1", "OUT2", "EN"],
        ),
        ComponentKind::Phototransistor => {
            draw_npn(painter, rect, component.rotation, stroke, energized)
        }
        ComponentKind::Optocoupler => {
            draw_ic_box(painter, rect, component.rotation, stroke, energized, "OPTO")
        }
        ComponentKind::GenericIc => {
            draw_ic_box(painter, rect, component.rotation, stroke, energized, "IC")
        }
        ComponentKind::Voltmeter => {
            draw_meter(painter, rect, component.rotation, stroke, "V", energized);
            if show_dc_overlay && let Some(v) = dc_voltage {
                let center = view.to_screen(component.pos);
                let r = (rect.width().min(rect.height()) * 0.44 + 12.0) * view.zoom;
                painter.text(
                    center + Vec2::new(0.0, r),
                    Align2::CENTER_TOP,
                    mna::format_voltage(v),
                    egui::FontId::proportional(10.5),
                    Color32::from_rgb(100, 240, 170),
                );
            }
        }
        ComponentKind::TextNote => {
            // Bordered text box with the note text (stored in `value`)
            let text_fill = Color32::from_rgba_unmultiplied(30, 36, 46, 220);
            let border = if selected {
                Stroke::new(1.5_f32, Color32::from_rgb(90, 200, 140))
            } else {
                Stroke::new(1.0_f32, Color32::from_rgb(90, 110, 140))
            };
            painter.rect_filled(rect, 4.0, text_fill);
            painter.rect_stroke(rect, 4.0, border, egui::StrokeKind::Outside);
            let font = egui::FontId::proportional(12.0 * view.zoom.sqrt());
            let line_h = 16.0 * view.zoom.sqrt();
            let lines = component.value.lines().collect::<Vec<_>>();
            let total_h = line_h * lines.len().max(1) as f32;
            let mut y = rect.center().y - total_h * 0.5 + line_h * 0.5;
            for line in lines {
                painter.text(
                    Pos2::new(rect.center().x, y),
                    Align2::CENTER_CENTER,
                    line,
                    font.clone(),
                    Color32::from_rgb(210, 220, 230),
                );
                y += line_h;
            }
        }
        ComponentKind::Ammeter => {
            draw_meter(painter, rect, component.rotation, stroke, "A", energized);
            if show_dc_overlay && let Some(i) = dc_current {
                let center = view.to_screen(component.pos);
                let r = (rect.width().min(rect.height()) * 0.44 + 12.0) * view.zoom;
                painter.text(
                    center + Vec2::new(0.0, r),
                    Align2::CENTER_TOP,
                    mna::format_current(i),
                    egui::FontId::proportional(10.5),
                    Color32::from_rgb(100, 210, 255),
                );
            }
        }
        ComponentKind::Dht11 => draw_sensor_module(
            painter,
            rect,
            stroke,
            energized,
            "DHT11",
            Color32::from_rgb(30, 100, 180),
        ),
        ComponentKind::Dht22 => draw_sensor_module(
            painter,
            rect,
            stroke,
            energized,
            "DHT22",
            Color32::from_rgb(20, 140, 80),
        ),
        ComponentKind::HcSr04 => draw_hcsr04(painter, rect, stroke, energized),
        ComponentKind::Buzzer => draw_buzzer(painter, rect, component.rotation, stroke, energized),
        ComponentKind::NeoPixel => draw_neopixel(painter, rect, stroke, energized),
        ComponentKind::PirSensor => draw_sensor_module(
            painter,
            rect,
            stroke,
            energized,
            "PIR",
            Color32::from_rgb(160, 80, 30),
        ),
        ComponentKind::Custom => {
            let def = component
                .part_id
                .as_deref()
                .and_then(crate::model::custom_part::custom_part);
            let title = def
                .as_ref()
                .map(|def| def.chip_label.clone())
                .unwrap_or_else(|| "PART?".to_string());
            let left: Vec<&str> = def
                .as_ref()
                .map(|def| def.left_pins.iter().map(|(name, _)| *name).collect())
                .unwrap_or_default();
            let right: Vec<&str> = def
                .as_ref()
                .map(|def| def.right_pins.iter().map(|(name, _)| *name).collect())
                .unwrap_or_default();
            draw_module(
                painter,
                component,
                rect,
                stroke,
                energized,
                component.rotation,
                &title,
                &left,
                &right,
            );
        }
    }

    if show_pins {
        for pin in component_pin_defs(component) {
            let spos = view.to_screen(pin.pos);
            let key = (pin.pos.x.round() as i32, pin.pos.y.round() as i32);
            let is_connected = connected_pins.contains(&key);
            if is_connected {
                painter.circle_filled(spos, 3.0, Color32::from_rgb(250, 205, 95));
                painter.circle_stroke(
                    spos,
                    4.0,
                    Stroke::new(1.0_f32, Color32::from_rgb(40, 35, 20)),
                );
            } else {
                // Unconnected pin: hollow circle with small cross
                painter.circle_stroke(
                    spos,
                    4.5,
                    Stroke::new(1.5_f32, Color32::from_rgb(220, 80, 60)),
                );
                let d = 3.0;
                painter.line_segment(
                    [spos - Vec2::new(d, 0.0), spos + Vec2::new(d, 0.0)],
                    Stroke::new(1.0_f32, Color32::from_rgb(220, 80, 60)),
                );
                painter.line_segment(
                    [spos - Vec2::new(0.0, d), spos + Vec2::new(0.0, d)],
                    Stroke::new(1.0_f32, Color32::from_rgb(220, 80, 60)),
                );
            }
            if should_draw_pin_label(component.kind, &pin) {
                let screen_pin = CircuitPin {
                    pos: spos,
                    label: pin.label,
                    role: pin.role,
                };
                draw_pin_label(painter, screen_center, &screen_pin);
            }
        }
    }

    painter.text(
        bounds.center_bottom() + Vec2::new(0.0, 6.0),
        Align2::CENTER_TOP,
        &component.label,
        egui::FontId::proportional(12.0),
        if energized {
            Color32::from_rgb(255, 210, 130)
        } else {
            Color32::from_rgb(225, 228, 232)
        },
    );
    if let Some(val_label) = canvas_value_label(component) {
        let vpos = bounds.center_top() - Vec2::new(0.0, 7.0);
        let font = egui::FontId::proportional(11.0);
        let text_w = val_label.len() as f32 * 5.8 + 6.0;
        let pill = Rect::from_center_size(vpos, Vec2::new(text_w, 14.0));
        painter.rect_filled(pill, 3.5, Color32::from_rgba_unmultiplied(12, 16, 24, 185));
        painter.text(
            vpos,
            Align2::CENTER_CENTER,
            &val_label,
            font,
            Color32::from_rgb(160, 200, 240),
        );
    }

    // ── DC Simulation overlay badge ──────────────────────────────────────
    if show_dc_overlay {
        let mut lines: Vec<String> = Vec::new();
        if let Some(v) = dc_voltage
            && v.abs() > 1e-9
        {
            lines.push(mna::format_voltage(v));
        }
        if let Some(i) = dc_current
            && i.abs() > 1e-12
        {
            lines.push(mna::format_current(i));
        }
        if !lines.is_empty() {
            let text = lines.join(" / ");
            let badge_pos = bounds.left_top() + Vec2::new(-4.0, -4.0);
            let font = egui::FontId::proportional(9.5);
            // Dark background pill
            let text_w = text.len() as f32 * 5.4 + 6.0;
            let bg =
                Rect::from_min_size(badge_pos - Vec2::new(text_w, 14.0), Vec2::new(text_w, 13.0));
            painter.rect_filled(bg, 3.0, Color32::from_rgba_unmultiplied(15, 18, 24, 210));
            painter.rect_stroke(
                bg,
                3.0,
                Stroke::new(
                    0.8_f32,
                    if energized {
                        Color32::from_rgb(200, 140, 30)
                    } else {
                        Color32::from_rgb(60, 70, 85)
                    },
                ),
                StrokeKind::Outside,
            );
            painter.text(
                bg.center(),
                Align2::CENTER_CENTER,
                &text,
                font,
                if energized {
                    Color32::from_rgb(255, 220, 100)
                } else {
                    Color32::from_rgb(140, 200, 255)
                },
            );
        }
    }
}

/// Draw a single wire polyline.
///
/// `dc_current` **must** be `None` when the wire is at a T-junction or
/// multi-branch net — passing a value there would display a physically
/// incorrect net-wide current.  Callers are responsible for supplying
/// `Some(I)` only when `DcResult::wire_current_known` contains the wire ID.
///
/// Voltage colour is always shown when `dc_voltage` is `Some`.  Current
/// arrows and thickness scaling are suppressed when `dc_current` is `None`.
#[allow(clippy::too_many_arguments)] // Hot-path painter receives precomputed render state.
pub(crate) fn draw_wire(
    painter: &egui::Painter,
    wire: &Wire,
    selected: bool,
    energized: bool,
    fault_highlight: bool,
    show_flow: bool,
    flow_phase: f32,
    net_highlighted: bool,
    dc_voltage: Option<f64>,
    dc_current: Option<f64>,
    dc_vmax: f64,
    dc_current_max: f64,
    show_voltage_labels: bool,
    open_wire: bool,
    view: CanvasView,
) {
    // Wire thickness scales with branch current only when it is well-defined.
    // Never scale by a net-wide average — that would be physically misleading.
    let wire_current = dc_current
        .map(|current| {
            if dc_current_max > 1e-12 {
                (current.abs() / dc_current_max).clamp(0.0, 1.0)
            } else {
                0.0
            }
        })
        .unwrap_or(0.0);
    let base_w = if wire_current > 0.0 {
        2.8 + wire_current as f32 * 2.0
    } else if energized {
        2.8
    } else {
        2.0
    };

    let stroke = if selected {
        Stroke::new(3.5_f32, Color32::from_rgb(90, 235, 170))
    } else if fault_highlight {
        Stroke::new(4.2_f32, Color32::from_rgb(255, 72, 58))
    } else if open_wire {
        Stroke::new(2.2_f32, Color32::from_rgb(225, 155, 65))
    } else if let Some(v) = dc_voltage {
        let col = mna::voltage_color(v, dc_vmax);
        Stroke::new(base_w, col)
    } else if energized {
        Stroke::new(base_w, Color32::from_rgb(255, 170, 55))
    } else if net_highlighted {
        Stroke::new(2.8_f32, Color32::from_rgb(140, 210, 255))
    } else {
        Stroke::new(2.0_f32, Color32::from_rgb(105, 178, 255))
    };

    let mut screen_points: Vec<Pos2> = wire.points.iter().map(|&p| view.to_screen(p)).collect();
    if dc_current.is_some_and(|current| current < 0.0) {
        screen_points.reverse();
    }
    for segment in screen_points.windows(2) {
        if open_wire && !selected {
            draw_dashed_segment(painter, segment[0], segment[1], stroke, 8.0, 5.0);
        } else {
            painter.line_segment([segment[0], segment[1]], stroke);
        }
    }

    if fault_highlight {
        draw_short_fault_markers(painter, &screen_points);
    }

    // Voltage/current label overlay
    if show_voltage_labels && let Some(mid) = midpoint_of_polyline(&screen_points) {
        let mut labels = Vec::new();
        if let Some(v) = dc_voltage {
            labels.push(mna::format_voltage(v));
        }
        if let Some(current) = dc_current.filter(|current| current.abs() > 1e-12) {
            labels.push(mna::format_current(current.abs()));
        }
        if !labels.is_empty() {
            let label = labels.join(" / ");
            let col = dc_voltage
                .map(|voltage| mna::voltage_color(voltage, dc_vmax))
                .unwrap_or(Color32::from_rgb(100, 210, 255));
            // background pill
            let font = egui::FontId::proportional(10.0);
            let galley = painter.layout_no_wrap(label, font.clone(), col);
            let text_rect = Rect::from_center_size(
                mid + Vec2::new(0.0, -12.0),
                galley.size() + Vec2::new(6.0, 3.0),
            );
            painter.rect_filled(
                text_rect,
                3.0,
                Color32::from_rgba_unmultiplied(20, 22, 28, 200),
            );
            painter.text(
                mid + Vec2::new(0.0, -12.0),
                Align2::CENTER_CENTER,
                galley.text(),
                font,
                col,
            );
        }
    }

    if show_flow {
        draw_flow_pulses(painter, &screen_points, flow_phase, stroke.width);
        draw_flow_markers(painter, &screen_points, flow_phase);
    }
}

pub(crate) fn draw_dashed_segment(
    painter: &egui::Painter,
    start: Pos2,
    end: Pos2,
    stroke: Stroke,
    dash: f32,
    gap: f32,
) {
    let length = start.distance(end);
    if length <= 0.1 {
        return;
    }
    let direction = (end - start) / length;
    let mut offset = 0.0;
    while offset < length {
        let dash_end = (offset + dash).min(length);
        painter.line_segment(
            [start + direction * offset, start + direction * dash_end],
            stroke,
        );
        offset += dash + gap;
    }
}

pub(crate) fn midpoint_of_polyline(pts: &[Pos2]) -> Option<Pos2> {
    if pts.is_empty() {
        return None;
    }
    if pts.len() == 1 {
        return Some(pts[0]);
    }
    let total = polyline_length(pts);
    point_on_polyline(pts, total * 0.5)
}

pub(crate) fn draw_flow_markers(painter: &egui::Painter, points: &[Pos2], flow_phase: f32) {
    let total = polyline_length(points);
    if total <= 1.0 {
        return;
    }

    let spacing = 42.0;
    let arrow_size = 8.5;
    let mut distance = flow_phase.rem_euclid(spacing);
    if total < spacing {
        distance = flow_phase.rem_euclid(total.max(1.0));
    }

    while distance < total {
        let Some(pos) = point_on_polyline(points, distance) else {
            distance += spacing;
            continue;
        };
        // Direction of the wire at this point (tangent)
        let look_ahead = point_on_polyline(points, (distance + 3.0).min(total - 0.1));
        let dir = match look_ahead {
            Some(next) if pos.distance(next) > 0.01 => (next - pos).normalized(),
            _ => Vec2::new(1.0, 0.0),
        };
        let perp = Vec2::new(-dir.y, dir.x);

        // Arrow head (filled triangle pointing in flow direction)
        let tip = pos + dir * arrow_size;
        let left = pos - dir * arrow_size * 0.3 + perp * arrow_size * 0.45;
        let right = pos - dir * arrow_size * 0.3 - perp * arrow_size * 0.45;

        painter.circle_filled(
            pos - dir * arrow_size * 0.2,
            arrow_size * 0.72,
            Color32::from_rgba_unmultiplied(255, 205, 50, 48),
        );
        painter.add(egui::Shape::convex_polygon(
            vec![tip, left, right],
            Color32::from_rgb(255, 245, 120),
            Stroke::new(1.4_f32, Color32::from_rgb(90, 55, 0)),
        ));

        // Bright dot at tail for glow effect
        painter.circle_filled(
            pos - dir * arrow_size * 0.35,
            2.8,
            Color32::from_rgb(255, 190, 45),
        );

        distance += spacing;
    }
}

pub(crate) fn draw_short_fault_markers(painter: &egui::Painter, points: &[Pos2]) {
    let total = polyline_length(points);
    if total <= 1.0 {
        return;
    }

    let spacing = 70.0;
    let marker_count = (total / spacing).ceil().max(1.0) as usize;
    let stroke = Stroke::new(2.0_f32, Color32::from_rgb(255, 220, 210));
    for idx in 0..marker_count {
        let distance = if marker_count == 1 {
            total * 0.5
        } else {
            (idx as f32 + 0.5) * total / marker_count as f32
        };
        let Some(pos) = point_on_polyline(points, distance) else {
            continue;
        };
        let r = 5.0;
        painter.circle_filled(pos, 7.0, Color32::from_rgba_unmultiplied(120, 0, 0, 120));
        painter.line_segment([pos + Vec2::new(-r, -r), pos + Vec2::new(r, r)], stroke);
        painter.line_segment([pos + Vec2::new(-r, r), pos + Vec2::new(r, -r)], stroke);
    }
}

pub(crate) fn draw_flow_pulses(
    painter: &egui::Painter,
    points: &[Pos2],
    flow_phase: f32,
    wire_width: f32,
) {
    let total = polyline_length(points);
    if total <= 1.0 {
        return;
    }

    let spacing = 42.0;
    let pulse_len = 18.0_f32.min(total.max(1.0));
    let mut distance = flow_phase.rem_euclid(spacing) - pulse_len;
    if total < spacing {
        distance = flow_phase.rem_euclid(total.max(1.0)) - pulse_len;
    }

    while distance < total {
        let start = distance.max(0.0);
        let end = (distance + pulse_len).min(total);
        if end > start {
            draw_polyline_range(
                painter,
                points,
                start,
                end,
                Stroke::new(
                    (wire_width + 3.2).max(5.2),
                    Color32::from_rgba_unmultiplied(255, 220, 70, 135),
                ),
            );
            draw_polyline_range(
                painter,
                points,
                start,
                end,
                Stroke::new(
                    (wire_width + 0.8).max(2.8),
                    Color32::from_rgb(255, 252, 170),
                ),
            );
        }
        distance += spacing;
    }
}

pub(crate) fn draw_polyline_range(
    painter: &egui::Painter,
    points: &[Pos2],
    start_distance: f32,
    end_distance: f32,
    stroke: Stroke,
) {
    if end_distance <= start_distance {
        return;
    }

    let mut traveled = 0.0;
    for segment in points.windows(2) {
        let a = segment[0];
        let b = segment[1];
        let length = a.distance(b);
        if length <= 0.1 {
            continue;
        }

        let segment_start = traveled;
        let segment_end = traveled + length;
        if segment_end >= start_distance && segment_start <= end_distance {
            let local_start = ((start_distance - segment_start) / length).clamp(0.0, 1.0);
            let local_end = ((end_distance - segment_start) / length).clamp(0.0, 1.0);
            if local_end > local_start {
                let p0 = a + (b - a) * local_start;
                let p1 = a + (b - a) * local_end;
                painter.line_segment([p0, p1], stroke);
            }
        }
        traveled = segment_end;
        if traveled > end_distance {
            break;
        }
    }
}

pub(crate) fn polyline_length(points: &[Pos2]) -> f32 {
    points
        .windows(2)
        .map(|segment| segment[0].distance(segment[1]))
        .sum()
}

pub(crate) fn point_on_polyline(points: &[Pos2], mut distance: f32) -> Option<Pos2> {
    for segment in points.windows(2) {
        let a = segment[0];
        let b = segment[1];
        let length = a.distance(b);
        if length <= 0.1 {
            continue;
        }
        if distance <= length {
            let t = distance / length;
            return Some(a + (b - a) * t);
        }
        distance -= length;
    }
    points.last().copied()
}

pub(crate) fn should_draw_pin_label(kind: ComponentKind, pin: &CircuitPin) -> bool {
    if matches!(kind, ComponentKind::Battery | ComponentKind::Ground) {
        return false;
    }
    matches!(pin.role, PinRole::Positive | PinRole::Ground)
}

pub(crate) fn draw_pin_label(painter: &egui::Painter, component_center: Pos2, pin: &CircuitPin) {
    let outward = pin.pos - component_center;
    let horizontal = outward.x.abs() >= outward.y.abs();
    let offset = if horizontal {
        Vec2::new(if outward.x >= 0.0 { 10.0 } else { -10.0 }, -10.0)
    } else {
        Vec2::new(10.0, if outward.y >= 0.0 { 10.0 } else { -10.0 })
    };
    let align = if horizontal && outward.x < 0.0 {
        Align2::RIGHT_CENTER
    } else if horizontal {
        Align2::LEFT_CENTER
    } else if outward.y < 0.0 {
        Align2::LEFT_BOTTOM
    } else {
        Align2::LEFT_TOP
    };
    let color = match pin.role {
        PinRole::Positive => Color32::from_rgb(255, 210, 120),
        PinRole::Ground => Color32::from_rgb(170, 210, 255),
        _ => Color32::from_rgb(220, 225, 230),
    };
    painter.text(
        pin.pos + offset,
        align,
        pin.label,
        egui::FontId::proportional(12.0),
        color,
    );
}

pub(crate) fn draw_wire_preview(painter: &egui::Painter, points: &[Pos2]) {
    let stroke = Stroke::new(1.8_f32, Color32::from_rgb(130, 200, 255));
    for segment in points.windows(2) {
        painter.line_segment([segment[0], segment[1]], stroke);
    }
    for point in points {
        painter.circle_filled(*point, 3.0, Color32::from_rgb(130, 200, 255));
    }
}

// component_pin_defs, rotate_point, component_pins, component_size and helpers
// are in model/pin_defs.rs; re-exported at crate root above.

pub(crate) fn draw_zener(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let anode = Pos2::new(center.x - rect.width() * 0.18, center.y);
    let cathode = Pos2::new(center.x + rect.width() * 0.2, center.y);
    let tri_top = Pos2::new(
        center.x - rect.width() * 0.18,
        center.y - rect.height() * 0.42,
    );
    let tri_bottom = Pos2::new(
        center.x - rect.width() * 0.18,
        center.y + rect.height() * 0.42,
    );
    let cathode_top = Pos2::new(cathode.x, center.y - rect.height() * 0.42);
    let cathode_bottom = Pos2::new(cathode.x, center.y + rect.height() * 0.42);
    let zbar_top = Pos2::new(
        cathode.x + rect.width() * 0.06,
        center.y - rect.height() * 0.42,
    );
    let zbar_bottom = Pos2::new(
        cathode.x - rect.width() * 0.06,
        center.y + rect.height() * 0.42,
    );

    let pts = [
        left,
        right,
        anode,
        cathode,
        tri_top,
        tri_bottom,
        cathode_top,
        cathode_bottom,
        zbar_top,
        zbar_bottom,
    ];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([r[0], r[2]], stroke);
    painter.line_segment([r[3], r[1]], stroke);
    painter.line_segment([r[4], r[5]], stroke);
    painter.line_segment([r[5], r[3]], stroke);
    painter.line_segment([r[3], r[4]], stroke);
    painter.line_segment([r[8], r[6]], stroke);
    painter.line_segment([r[6], r[7]], stroke);
    painter.line_segment([r[7], r[9]], stroke);
}

pub(crate) fn draw_npn(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    energized: bool,
) {
    let center = rect.center();
    let circle_r = rect.width().min(rect.height()) * 0.46;
    let base_x = rect.left() + rect.width() * 0.18;
    let body_fill = if energized {
        Color32::from_rgba_unmultiplied(255, 185, 80, 18)
    } else {
        Color32::TRANSPARENT
    };
    painter.circle_filled(center, circle_r, body_fill);
    painter.circle_stroke(center, circle_r, stroke);

    let base_in = Pos2::new(base_x, center.y);
    let base_out = Pos2::new(rect.left(), center.y);
    let ce_x = center.x + circle_r * 0.22;
    let c_top = Pos2::new(ce_x, center.y - rect.height() * 0.28);
    let c_pin = Pos2::new(rect.right(), rect.top() + rect.height() * 0.22);
    let e_bottom = Pos2::new(ce_x, center.y + rect.height() * 0.28);
    let e_pin = Pos2::new(rect.right(), rect.bottom() - rect.height() * 0.22);
    let bar_top = Pos2::new(base_x, center.y - rect.height() * 0.32);
    let bar_bot = Pos2::new(base_x, center.y + rect.height() * 0.32);

    let pts = [
        base_in, base_out, c_top, c_pin, e_bottom, e_pin, bar_top, bar_bot,
    ];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([r[1], r[0]], stroke);
    painter.line_segment([r[6], r[7]], stroke);
    painter.line_segment([r[0], r[2]], stroke);
    painter.line_segment([r[2], r[3]], stroke);
    painter.line_segment([r[0], r[4]], stroke);
    painter.line_segment([r[4], r[5]], stroke);
    // Emitter arrow
    let dir = (r[5] - r[4]).normalized();
    let perp = Vec2::new(-dir.y, dir.x);
    let arr = r[4] + dir * 8.0;
    painter.line_segment([arr, arr - dir * 7.0 + perp * 4.0], stroke);
    painter.line_segment([arr, arr - dir * 7.0 - perp * 4.0], stroke);
}

pub(crate) fn draw_pnp(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    energized: bool,
) {
    let center = rect.center();
    let circle_r = rect.width().min(rect.height()) * 0.46;
    let base_x = rect.left() + rect.width() * 0.18;
    let body_fill = if energized {
        Color32::from_rgba_unmultiplied(255, 185, 80, 18)
    } else {
        Color32::TRANSPARENT
    };
    painter.circle_filled(center, circle_r, body_fill);
    painter.circle_stroke(center, circle_r, stroke);

    let base_out = Pos2::new(rect.left(), center.y);
    let ce_x = center.x + circle_r * 0.22;
    let e_top = Pos2::new(ce_x, center.y - rect.height() * 0.28);
    let e_pin = Pos2::new(rect.right(), rect.top() + rect.height() * 0.22);
    let c_bottom = Pos2::new(ce_x, center.y + rect.height() * 0.28);
    let c_pin = Pos2::new(rect.right(), rect.bottom() - rect.height() * 0.22);
    let bar_top = Pos2::new(base_x, center.y - rect.height() * 0.32);
    let bar_bot = Pos2::new(base_x, center.y + rect.height() * 0.32);
    let base_in = Pos2::new(base_x, center.y);

    let pts = [
        base_in, base_out, e_top, e_pin, c_bottom, c_pin, bar_top, bar_bot,
    ];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([r[1], r[0]], stroke);
    painter.line_segment([r[6], r[7]], stroke);
    painter.line_segment([r[0], r[2]], stroke);
    painter.line_segment([r[2], r[3]], stroke);
    painter.line_segment([r[0], r[4]], stroke);
    painter.line_segment([r[4], r[5]], stroke);
    // Emitter arrow (inward for PNP)
    let dir = (r[2] - r[3]).normalized();
    let perp = Vec2::new(-dir.y, dir.x);
    let arr = r[2] - dir * 2.0;
    painter.line_segment([arr, arr - dir * 7.0 + perp * 4.0], stroke);
    painter.line_segment([arr, arr - dir * 7.0 - perp * 4.0], stroke);
}

pub(crate) fn draw_nmosfet(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    energized: bool,
) {
    let center = rect.center();
    let circle_r = rect.width().min(rect.height()) * 0.44;
    let body_fill = if energized {
        Color32::from_rgba_unmultiplied(255, 185, 80, 18)
    } else {
        Color32::TRANSPARENT
    };
    painter.circle_filled(center, circle_r, body_fill);
    painter.circle_stroke(center, circle_r, stroke);

    let gate_out = Pos2::new(rect.left(), center.y);
    let gate_in = Pos2::new(center.x - circle_r * 0.55, center.y);
    let gate_bar_top = Pos2::new(center.x - circle_r * 0.55, center.y - rect.height() * 0.30);
    let gate_bar_bot = Pos2::new(center.x - circle_r * 0.55, center.y + rect.height() * 0.30);
    let chan_top = Pos2::new(center.x - circle_r * 0.20, center.y - rect.height() * 0.30);
    let chan_bot = Pos2::new(center.x - circle_r * 0.20, center.y + rect.height() * 0.30);
    let d_inner = Pos2::new(center.x + circle_r * 0.18, center.y - rect.height() * 0.28);
    let d_pin = Pos2::new(rect.right(), rect.top() + rect.height() * 0.22);
    let s_inner = Pos2::new(center.x + circle_r * 0.18, center.y + rect.height() * 0.28);
    let s_pin = Pos2::new(rect.right(), rect.bottom() - rect.height() * 0.22);

    let pts = [
        gate_out,
        gate_in,
        gate_bar_top,
        gate_bar_bot,
        chan_top,
        chan_bot,
        d_inner,
        d_pin,
        s_inner,
        s_pin,
    ];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([r[0], r[1]], stroke);
    painter.line_segment([r[2], r[3]], Stroke::new(stroke.width * 2.0, stroke.color));
    painter.line_segment([r[4], r[5]], stroke);
    painter.line_segment([r[6], r[7]], stroke);
    painter.line_segment([r[8], r[9]], stroke);
    // Arrow (N-type points inward)
    let mid = r[4].lerp(r[5], 0.5);
    let dir = (r[8] - r[6]).normalized();
    painter.line_segment([r[1], mid], stroke);
    let arr = mid + dir * 2.0;
    let perp = Vec2::new(-dir.y, dir.x);
    painter.line_segment([arr, arr - dir * 6.0 + perp * 3.5], stroke);
    painter.line_segment([arr, arr - dir * 6.0 - perp * 3.5], stroke);
}

pub(crate) fn draw_pmosfet(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    energized: bool,
) {
    // Same as NMOSFET but arrow points out, bubble on gate
    draw_nmosfet(painter, rect, rotation, stroke, energized);
    // Draw bubble on gate
    let center = rect.center();
    let bubble_center_nat = Pos2::new(rect.left() + rect.width() * 0.14, center.y);
    let bubble = rotate_point(bubble_center_nat, center, rotation);
    painter.circle_stroke(bubble, 4.5, stroke);
}

pub(crate) fn draw_potentiometer(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let mut points = Vec::new();
    let zig_count = 6;
    let step = rect.width() / (zig_count as f32 + 1.0);
    points.push(left);
    for i in 1..=zig_count {
        let x = rect.left() + step * i as f32;
        let y = if i % 2 == 0 {
            center.y - rect.height() * 0.28
        } else {
            center.y + rect.height() * 0.28
        };
        points.push(Pos2::new(x, y));
    }
    points.push(right);

    let rotated: Vec<Pos2> = points
        .into_iter()
        .map(|p| rotate_point(p, center, rotation))
        .collect();
    for seg in rotated.windows(2) {
        painter.line_segment([seg[0], seg[1]], stroke);
    }

    // Wiper arrow
    let wiper_start_nat = Pos2::new(center.x, center.y - rect.height() * 0.55);
    let wiper_tip_nat = Pos2::new(center.x, center.y);
    let wiper_pin_nat = Pos2::new(center.x, rect.bottom());
    let ws = rotate_point(wiper_start_nat, center, rotation);
    let wt = rotate_point(wiper_tip_nat, center, rotation);
    let wp = rotate_point(wiper_pin_nat, center, rotation);
    painter.line_segment([ws, wt], stroke);
    painter.line_segment([wt, wp], stroke);
    let dir = (wt - ws).normalized();
    let perp = Vec2::new(-dir.y, dir.x);
    painter.line_segment([wt, wt - dir * 6.0 + perp * 3.5], stroke);
    painter.line_segment([wt, wt - dir * 6.0 - perp * 3.5], stroke);
}

pub(crate) fn draw_voltage_reg(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    energized: bool,
) {
    let center = rect.center();
    let body_fill = if energized {
        Color32::from_rgb(38, 52, 28)
    } else {
        Color32::from_rgb(22, 28, 36)
    };
    let box_rect =
        Rect::from_center_size(center, Vec2::new(rect.width() * 0.72, rect.height() * 0.72));
    painter.rect_filled(box_rect, 4.0, body_fill);
    painter.rect_stroke(box_rect, 4.0, stroke, StrokeKind::Outside);
    painter.text(
        center,
        Align2::CENTER_CENTER,
        "REG",
        egui::FontId::proportional(11.0),
        stroke.color,
    );

    let pts = [
        Pos2::new(rect.left(), center.y),
        Pos2::new(box_rect.left(), center.y),
        Pos2::new(center.x, rect.bottom()),
        Pos2::new(center.x, box_rect.bottom()),
        Pos2::new(rect.right(), center.y),
        Pos2::new(box_rect.right(), center.y),
    ];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();
    painter.line_segment([r[0], r[1]], stroke);
    painter.line_segment([r[2], r[3]], stroke);
    painter.line_segment([r[4], r[5]], stroke);
}

pub(crate) fn draw_fuse(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let box_w = rect.width() * 0.44;
    let box_h = rect.height() * 0.56;
    let bl = Pos2::new(center.x - box_w * 0.5, center.y - box_h * 0.5);
    let br = Pos2::new(center.x + box_w * 0.5, center.y - box_h * 0.5);
    let tl = Pos2::new(center.x - box_w * 0.5, center.y + box_h * 0.5);
    let tr = Pos2::new(center.x + box_w * 0.5, center.y + box_h * 0.5);

    let pts = [
        left,
        right,
        bl,
        br,
        tl,
        tr,
        Pos2::new(center.x - box_w * 0.5, center.y),
        Pos2::new(center.x + box_w * 0.5, center.y),
    ];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([r[0], r[6]], stroke);
    painter.line_segment([r[7], r[1]], stroke);
    painter.rect_stroke(
        Rect::from_points(&[r[2], r[3], r[4], r[5]]),
        2.0,
        stroke,
        StrokeKind::Outside,
    );
    // Fuse element line
    painter.line_segment([r[6], r[7]], Stroke::new(stroke.width * 0.8, stroke.color));
}

pub(crate) fn draw_logic_not(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let apex = Pos2::new(rect.right() - 7.0, center.y);
    let tl = Pos2::new(rect.left() + 4.0, rect.top() + 4.0);
    let bl = Pos2::new(rect.left() + 4.0, rect.bottom() - 4.0);
    let in_pin = Pos2::new(rect.left(), center.y);
    let out_pin = Pos2::new(rect.right(), center.y);
    let bubble_center = Pos2::new(rect.right() - 4.5, center.y);

    let pts = [apex, tl, bl, in_pin, out_pin, bubble_center];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([r[1], r[0]], stroke);
    painter.line_segment([r[0], r[2]], stroke);
    painter.line_segment([r[2], r[1]], stroke);
    painter.line_segment([r[3], r[1].lerp(r[2], 0.5)], stroke);
    painter.circle_stroke(r[5], 4.5, stroke);
}

pub(crate) fn draw_logic_and(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    bubble: bool,
) {
    let center = rect.center();
    let x0 = rect.left() + 4.0;
    let x1 = rect.center().x + 2.0;
    let right_x = if bubble {
        rect.right() - 8.0
    } else {
        rect.right()
    };
    let top_y = rect.top() + 4.0;
    let bot_y = rect.bottom() - 4.0;

    let in_a = Pos2::new(rect.left(), center.y - rect.height() * 0.25);
    let in_b = Pos2::new(rect.left(), center.y + rect.height() * 0.25);
    let out = Pos2::new(rect.right(), center.y);
    let tl = Pos2::new(x0, top_y);
    let bl = Pos2::new(x0, bot_y);
    let tr = Pos2::new(x1, top_y);
    let br = Pos2::new(x1, bot_y);
    let arc_right = Pos2::new(right_x, center.y);

    let pts = [in_a, in_b, out, tl, bl, tr, br, arc_right];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([r[3], r[4]], stroke);
    painter.line_segment([r[3], r[5]], stroke);
    painter.line_segment([r[4], r[6]], stroke);
    // Approximated semicircle on right side
    let steps = 12;
    let mut prev = r[5];
    for i in 1..=steps {
        let a = std::f32::consts::PI * 0.5 - std::f32::consts::PI * i as f32 / steps as f32;
        let nat_p = Pos2::new(
            x1 + (bot_y - top_y) * 0.5 * a.cos(),
            center.y - (bot_y - top_y) * 0.5 * a.sin(),
        );
        let p = rotate_point(nat_p, center, rotation);
        painter.line_segment([prev, p], stroke);
        prev = p;
    }
    painter.line_segment(
        [
            r[0],
            rotate_point(
                Pos2::new(x0, center.y - rect.height() * 0.25),
                center,
                rotation,
            ),
        ],
        stroke,
    );
    painter.line_segment(
        [
            r[1],
            rotate_point(
                Pos2::new(x0, center.y + rect.height() * 0.25),
                center,
                rotation,
            ),
        ],
        stroke,
    );
    painter.line_segment([r[7], r[2]], stroke);
    if bubble {
        let bc = rotate_point(Pos2::new(rect.right() - 4.5, center.y), center, rotation);
        painter.circle_stroke(bc, 4.5, stroke);
    }
}

pub(crate) fn draw_logic_or(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    bubble: bool,
) {
    let center = rect.center();
    let x0 = rect.left() + 4.0;
    let right_x = if bubble {
        rect.right() - 8.0
    } else {
        rect.right()
    };
    let top_y = rect.top() + 4.0;
    let bot_y = rect.bottom() - 4.0;

    let in_a = Pos2::new(rect.left(), center.y - rect.height() * 0.25);
    let in_b = Pos2::new(rect.left(), center.y + rect.height() * 0.25);
    let out = Pos2::new(rect.right(), center.y);
    let tl = Pos2::new(x0, top_y);
    let bl = Pos2::new(x0, bot_y);

    let pts = [in_a, in_b, out, tl, bl];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    // Back curve
    let hw = (bot_y - top_y) * 0.5;
    let steps = 16;
    let mut prev_bot = r[4];
    let mut prev_top = r[3];
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let fa = std::f32::consts::PI * 0.5 - std::f32::consts::PI * t;
        let nat_f = Pos2::new(x0 + hw * fa.cos() + hw, center.y - hw * fa.sin());
        let f = rotate_point(nat_f, center, rotation);
        if i == steps {
            painter.line_segment([prev_top, f], stroke);
            painter.line_segment([prev_bot, f], stroke);
        } else {
            painter.line_segment([prev_top, f], stroke);
            prev_top = f;
            let ba = std::f32::consts::PI * t - std::f32::consts::PI * 0.5;
            let nat_b = Pos2::new(x0 + hw * 0.22 * ba.cos(), center.y - hw * ba.sin());
            let b = rotate_point(nat_b, center, rotation);
            painter.line_segment([prev_bot, b], stroke);
            prev_bot = b;
        }
    }
    painter.line_segment(
        [
            r[0],
            rotate_point(
                Pos2::new(x0, center.y - rect.height() * 0.25),
                center,
                rotation,
            ),
        ],
        stroke,
    );
    painter.line_segment(
        [
            r[1],
            rotate_point(
                Pos2::new(x0, center.y + rect.height() * 0.25),
                center,
                rotation,
            ),
        ],
        stroke,
    );

    let end_pt = rotate_point(Pos2::new(right_x, center.y), center, rotation);
    painter.line_segment([end_pt, r[2]], stroke);
    if bubble {
        let bc = rotate_point(Pos2::new(rect.right() - 4.5, center.y), center, rotation);
        painter.circle_stroke(bc, 4.5, stroke);
    }
}

pub(crate) fn draw_logic_xor(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    bubble: bool,
) {
    draw_logic_or(painter, rect, rotation, stroke, bubble);
    let center = rect.center();
    let hw = (rect.bottom() - 4.0 - (rect.top() + 4.0)) * 0.5;
    let x0 = rect.left();
    let steps = 10;
    let bot_y = rect.bottom() - 4.0;
    let mut prev = rotate_point(Pos2::new(x0, bot_y), center, rotation);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let ba = std::f32::consts::PI * t - std::f32::consts::PI * 0.5;
        let nat_b = Pos2::new(x0 - 4.0 + hw * 0.22 * ba.cos(), center.y - hw * ba.sin());
        let b = rotate_point(nat_b, center, rotation);
        painter.line_segment([prev, b], stroke);
        prev = b;
    }
}

// ─── New commercial component drawing functions ──────────────────────────────

pub(crate) fn draw_net_label(
    painter: &egui::Painter,
    component: &Component,
    rect: Rect,
    stroke: Stroke,
    energized: bool,
) {
    let center = rect.center();
    let w = rect.width() * 0.5;
    let h = rect.height() * 0.4;
    // Arrow-flag shape pointing right
    let pts = vec![
        Pos2::new(rect.left(), center.y - h),
        Pos2::new(rect.right() - rect.width() * 0.15, center.y - h),
        Pos2::new(rect.right(), center.y),
        Pos2::new(rect.right() - rect.width() * 0.15, center.y + h),
        Pos2::new(rect.left(), center.y + h),
    ];
    let fill = if energized {
        Color32::from_rgba_unmultiplied(255, 200, 80, 50)
    } else {
        Color32::from_rgba_unmultiplied(80, 160, 255, 35)
    };
    painter.add(egui::Shape::convex_polygon(pts.clone(), fill, stroke));
    // Net name label
    let text_col = if energized {
        Color32::from_rgb(255, 210, 100)
    } else {
        Color32::from_rgb(160, 200, 255)
    };
    painter.text(
        Pos2::new(rect.left() + w * 0.55, center.y),
        Align2::CENTER_CENTER,
        &component.value,
        egui::FontId::monospace(11.0),
        text_col,
    );
}

pub(crate) fn draw_crystal(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let hw = rect.width() * 0.5;
    let hh = rect.height() * 0.45;
    let plate_gap = hw * 0.28;
    let plate_h = hh * 0.55;

    let (p1, p2, p3, p4, plates) = if rotation == 90 || rotation == 270 {
        let a = Pos2::new(center.x, rect.top());
        let b = Pos2::new(center.x, center.y - plate_gap);
        let c = Pos2::new(center.x, center.y + plate_gap);
        let d = Pos2::new(center.x, rect.bottom());
        let ps = vec![
            [
                Pos2::new(center.x - plate_h, center.y - plate_gap),
                Pos2::new(center.x + plate_h, center.y - plate_gap),
            ],
            [
                Pos2::new(center.x - plate_h, center.y + plate_gap),
                Pos2::new(center.x + plate_h, center.y + plate_gap),
            ],
        ];
        (a, b, c, d, ps)
    } else {
        let a = Pos2::new(rect.left(), center.y);
        let b = Pos2::new(center.x - plate_gap, center.y);
        let c = Pos2::new(center.x + plate_gap, center.y);
        let d = Pos2::new(rect.right(), center.y);
        let ps = vec![
            [
                Pos2::new(center.x - plate_gap, center.y - plate_h),
                Pos2::new(center.x - plate_gap, center.y + plate_h),
            ],
            [
                Pos2::new(center.x + plate_gap, center.y - plate_h),
                Pos2::new(center.x + plate_gap, center.y + plate_h),
            ],
        ];
        (a, b, c, d, ps)
    };

    painter.line_segment([p1, p2], stroke);
    painter.line_segment([p3, p4], stroke);
    for plate in plates {
        painter.line_segment(plate, Stroke::new(stroke.width + 1.0, stroke.color));
    }
    // Body box between plates
    let body = if rotation == 90 || rotation == 270 {
        Rect::from_center_size(center, Vec2::new(plate_h * 2.0, plate_gap * 2.0))
    } else {
        Rect::from_center_size(center, Vec2::new(plate_gap * 2.0, plate_h * 2.0))
    };
    painter.rect_stroke(body, 2.0, stroke, StrokeKind::Middle);
}

pub(crate) fn draw_transformer(
    painter: &egui::Painter,
    rect: Rect,
    _rotation: i32,
    stroke: Stroke,
) {
    let center = rect.center();
    let hw = rect.width() * 0.46;
    let hh = rect.height() * 0.38;
    // Primary coil (left side)
    let num_loops = 4;
    for i in 0..num_loops {
        painter.circle_stroke(
            Pos2::new(rect.left() + hw * 0.22 + i as f32 * (hw * 0.22), center.y),
            hh * 0.28,
            stroke,
        );
    }
    // Secondary coil (right side)
    for i in 0..num_loops {
        painter.circle_stroke(
            Pos2::new(rect.right() - hw * 0.22 - i as f32 * (hw * 0.22), center.y),
            hh * 0.28,
            stroke,
        );
    }
    // Core line
    let core_x = center.x;
    painter.line_segment(
        [
            Pos2::new(core_x - 2.0, center.y - hh),
            Pos2::new(core_x - 2.0, center.y + hh),
        ],
        Stroke::new(2.0_f32, stroke.color),
    );
    painter.line_segment(
        [
            Pos2::new(core_x + 2.0, center.y - hh),
            Pos2::new(core_x + 2.0, center.y + hh),
        ],
        Stroke::new(2.0_f32, stroke.color),
    );
    // Lead wires
    painter.line_segment(
        [
            Pos2::new(rect.left(), center.y - hh * 0.6),
            Pos2::new(rect.left() + hw * 0.22 - hh * 0.28, center.y - hh * 0.6),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(rect.left(), center.y + hh * 0.6),
            Pos2::new(rect.left() + hw * 0.22 - hh * 0.28, center.y + hh * 0.6),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(rect.right(), center.y - hh * 0.6),
            Pos2::new(rect.right() - hw * 0.22 + hh * 0.28, center.y - hh * 0.6),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(rect.right(), center.y + hh * 0.6),
            Pos2::new(rect.right() - hw * 0.22 + hh * 0.28, center.y + hh * 0.6),
        ],
        stroke,
    );
}

pub(crate) fn draw_thermistor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    // Draw as a resistor with a diagonal arrow through it (NTC symbol)
    draw_resistor(painter, rect, rotation, stroke);
    let center = rect.center();
    let hw = rect.width() * 0.32;
    let hh = rect.height() * 0.55;
    // Diagonal temperature arrow
    let arr_start = Pos2::new(center.x - hw * 0.6, center.y + hh * 0.8);
    let arr_end = Pos2::new(center.x + hw * 0.6, center.y - hh * 0.8);
    painter.line_segment(
        [arr_start, arr_end],
        Stroke::new(1.5_f32, Color32::from_rgb(255, 160, 80)),
    );
    // Arrowhead
    painter.line_segment(
        [arr_end, Pos2::new(arr_end.x - 5.0, arr_end.y + 2.0)],
        Stroke::new(1.5_f32, Color32::from_rgb(255, 160, 80)),
    );
    painter.line_segment(
        [arr_end, Pos2::new(arr_end.x - 2.0, arr_end.y + 5.0)],
        Stroke::new(1.5_f32, Color32::from_rgb(255, 160, 80)),
    );
    painter.text(
        Pos2::new(center.x + hw * 0.7, center.y - hh * 0.9),
        Align2::LEFT_BOTTOM,
        "T",
        egui::FontId::proportional(9.0),
        Color32::from_rgb(255, 160, 80),
    );
}

pub(crate) fn draw_varistor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    draw_resistor(painter, rect, rotation, stroke);
    let center = rect.center();
    // "V" label inside
    painter.text(
        center,
        Align2::CENTER_CENTER,
        "V",
        egui::FontId::proportional(10.0),
        stroke.color,
    );
}

pub(crate) fn draw_ic_box(
    painter: &egui::Painter,
    rect: Rect,
    _rotation: i32,
    stroke: Stroke,
    energized: bool,
    label: &str,
) {
    let fill = if energized {
        Color32::from_rgba_unmultiplied(60, 120, 80, 80)
    } else {
        Color32::from_rgba_unmultiplied(38, 44, 54, 200)
    };
    painter.rect_filled(rect, 4.0, fill);
    painter.rect_stroke(rect, 4.0, stroke, StrokeKind::Middle);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        egui::FontId::monospace(10.0),
        stroke.color,
    );
}

// ─────────────────────────────────────────────────────────────────────────────

#[allow(clippy::needless_range_loop)] // Pairwise crossing detection uses stable indices.
pub(crate) fn draw_junctions(painter: &egui::Painter, wires: &[Wire], view: CanvasView) {
    let mut junction_keys: HashSet<(i32, i32)> = HashSet::new();
    let mut junctions: Vec<Pos2> = Vec::new();

    // Pass 1: shared vertices + collect unique endpoint keys in one scan
    let mut counts: HashMap<(i32, i32), (Pos2, u32)> = HashMap::with_capacity(wires.len() * 3);
    let mut endpoint_keys: HashSet<(i32, i32)> = HashSet::with_capacity(wires.len() * 2);
    for wire in wires {
        let n = wire.points.len();
        for (idx, &point) in wire.points.iter().enumerate() {
            let key = (point.x.round() as i32, point.y.round() as i32);
            let entry = counts.entry(key).or_insert((point, 0));
            entry.1 += 1;
            if idx == 0 || idx + 1 == n {
                endpoint_keys.insert(key);
            }
        }
    }
    for (&key, &(pos, count)) in &counts {
        if count > 1 && junction_keys.insert(key) {
            junctions.push(pos);
        }
    }

    // Pass 2: T-intersections — flatten all segments for cache-friendly scan
    let segments: Vec<(Pos2, Pos2)> = wires
        .iter()
        .flat_map(|w| w.points.windows(2).map(|s| (s[0], s[1])))
        .collect();

    for &ep_key in &endpoint_keys {
        if junction_keys.contains(&ep_key) {
            continue;
        }
        let ep = counts[&ep_key].0;
        'seg: for &(sa, sb) in &segments {
            if ep.distance(sa) > 1.5
                && ep.distance(sb) > 1.5
                && distance_to_segment(ep, sa, sb) < 1.5
            {
                if junction_keys.insert(ep_key) {
                    junctions.push(ep);
                }
                break 'seg;
            }
        }
    }

    for pos in &junctions {
        let sp = view.to_screen(*pos);
        let r = view.scale_f(5.0).clamp(3.5, 7.0);
        painter.circle_filled(sp, r, Color32::from_rgb(105, 178, 255));
        painter.circle_stroke(
            sp,
            r + 1.5,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(105, 178, 255, 80)),
        );
    }

    // Draw hop arcs at non-connecting crossings (two wires cross but are NOT in junction list)
    // Flat segment list with AABB min/max pre-computed to skip non-overlapping pairs quickly
    let seg_data: Vec<(Pos2, Pos2, f32, f32, f32, f32)> = segments
        .iter()
        .map(|&(a, b)| (a, b, a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y)))
        .collect();

    let n_seg = seg_data.len();
    for i in 0..n_seg {
        let (a0, a1, ax0, ax1, ay0, ay1) = seg_data[i];
        for j in (i + 1)..n_seg {
            let (b0, b1, bx0, bx1, by0, by1) = seg_data[j];
            // AABB overlap test — eliminates ~90% of pairs before the heavier intersection math
            if ax1 < bx0 || bx1 < ax0 || ay1 < by0 || by1 < ay0 {
                continue;
            }
            if let Some(cross) = segment_intersection(a0, a1, b0, b1) {
                let key = (cross.x.round() as i32, cross.y.round() as i32);
                if junction_keys.contains(&key) {
                    continue;
                }
                let sp = view.to_screen(cross);
                let hop_r = view.scale_f(5.5).clamp(3.0, 8.0);
                let dir = (b1 - b0).normalized();
                let perp = Vec2::new(-dir.y, dir.x);
                let ha0 = sp - dir * hop_r;
                let ha1 = sp + dir * hop_r;
                let ctrl = sp + perp * hop_r * 1.2;
                let p0 = ha0.lerp(ctrl, 0.5);
                let p2 = ha1.lerp(ctrl, 0.5);
                let bg = Color32::from_rgb(18, 22, 28);
                painter.line_segment([ha0, ha1], Stroke::new(5.0_f32, bg));
                let hop_stroke = Stroke::new(2.0_f32, Color32::from_rgb(105, 178, 255));
                painter.line_segment([ha0, p0], hop_stroke);
                painter.line_segment([p0, ctrl], hop_stroke);
                painter.line_segment([ctrl, p2], hop_stroke);
                painter.line_segment([p2, ha1], hop_stroke);
            }
        }
    }
}

pub(crate) fn segment_intersection(a0: Pos2, a1: Pos2, b0: Pos2, b1: Pos2) -> Option<Pos2> {
    let da = a1 - a0;
    let db = b1 - b0;
    let denom = da.x * db.y - da.y * db.x;
    if denom.abs() < 1e-6 {
        return None;
    } // parallel
    let t = ((b0.x - a0.x) * db.y - (b0.y - a0.y) * db.x) / denom;
    let u = ((b0.x - a0.x) * da.y - (b0.y - a0.y) * da.x) / denom;
    if t > 0.01 && t < 0.99 && u > 0.01 && u < 0.99 {
        Some(a0 + da * t)
    } else {
        None
    }
}

pub(crate) fn draw_meter(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    symbol: &str,
    energized: bool,
) {
    let center = rect.center();
    let r = rect.width().min(rect.height()) * 0.44;
    let body_fill = if energized {
        Color32::from_rgba_unmultiplied(60, 120, 80, 30)
    } else {
        Color32::from_rgba_unmultiplied(28, 36, 46, 180)
    };
    painter.circle_filled(center, r, body_fill);
    painter.circle_stroke(center, r, stroke);

    let text_col = if energized {
        Color32::from_rgb(100, 255, 170)
    } else {
        stroke.color
    };
    painter.text(
        center,
        Align2::CENTER_CENTER,
        symbol,
        egui::FontId::proportional(r * 0.85),
        text_col,
    );

    // Terminal leads
    let left_nat = Pos2::new(rect.left(), center.y);
    let left_inner_nat = Pos2::new(center.x - r, center.y);
    let right_inner_nat = Pos2::new(center.x + r, center.y);
    let right_nat = Pos2::new(rect.right(), center.y);
    let points = [left_nat, left_inner_nat, right_inner_nat, right_nat];
    let rp: Vec<Pos2> = points
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();
    painter.line_segment([rp[0], rp[1]], stroke);
    painter.line_segment([rp[2], rp[3]], stroke);

    // Polarity marks: + on left lead, − on right lead
    let plus_pos = rotate_point(
        Pos2::new(rect.left() + (rect.width() * 0.5 - r) * 0.5, center.y - 8.0),
        center,
        rotation,
    );
    let minus_pos = rotate_point(
        Pos2::new(
            rect.right() - (rect.width() * 0.5 - r) * 0.5,
            center.y - 8.0,
        ),
        center,
        rotation,
    );
    let pol_col = Color32::from_rgb(180, 190, 200);
    painter.text(
        plus_pos,
        Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(9.0),
        pol_col,
    );
    painter.text(
        minus_pos,
        Align2::CENTER_CENTER,
        "−",
        egui::FontId::proportional(9.0),
        pol_col,
    );
}

pub(crate) fn resistor_band_color(digit: u8) -> Color32 {
    match digit {
        0 => Color32::from_rgb(20, 20, 20),
        1 => Color32::from_rgb(139, 69, 19),
        2 => Color32::from_rgb(220, 40, 40),
        3 => Color32::from_rgb(255, 140, 0),
        4 => Color32::from_rgb(255, 220, 0),
        5 => Color32::from_rgb(60, 180, 60),
        6 => Color32::from_rgb(50, 80, 220),
        7 => Color32::from_rgb(160, 32, 240),
        8 => Color32::from_rgb(170, 170, 170),
        _ => Color32::WHITE,
    }
}

pub(crate) fn resistor_value_to_bands(ohms: f64) -> [u8; 4] {
    // Returns [band1, band2, multiplier_exp, tolerance=5%=7]
    if ohms <= 0.0 {
        return [0, 0, 0, 7];
    }
    let exp = ohms.log10().floor() as i32 - 1;
    let mantissa = ohms / 10f64.powi(exp);
    let d1 = (mantissa / 10.0).floor().clamp(0.0, 9.0) as u8;
    let d2 = (mantissa % 10.0).floor().clamp(0.0, 9.0) as u8;
    let mult = exp.clamp(0, 9) as u8;
    [d1, d2, mult, 7] // gold = tolerance 5%
}

pub(crate) fn draw_resistor_with_bands(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    ohms: f64,
) {
    let bands = resistor_value_to_bands(ohms);
    draw_resistor_body(painter, rect, rotation, stroke, bands);
}

pub(crate) fn draw_resistor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    draw_resistor_body(painter, rect, rotation, stroke, [1, 0, 3, 7]);
}

pub(crate) fn draw_resistor_body(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    bands: [u8; 4],
) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), rect.center().y);
    let right = Pos2::new(rect.right(), rect.center().y);

    // Body rectangle
    let body_w = rect.width() * 0.55;
    let body_h = rect.height() * 0.48;
    let body = Rect::from_center_size(center, Vec2::new(body_w, body_h));

    // Leads
    let left_inner = Pos2::new(center.x - body_w * 0.5, center.y);
    let right_inner = Pos2::new(center.x + body_w * 0.5, center.y);
    let pts = [left, left_inner, right_inner, right];
    let rpts: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();
    painter.line_segment([rpts[0], rpts[1]], stroke);
    painter.line_segment([rpts[2], rpts[3]], stroke);

    // Body fill
    let body_corners: Vec<Pos2> = [
        body.left_top(),
        body.right_top(),
        body.right_bottom(),
        body.left_bottom(),
    ]
    .iter()
    .map(|&p| rotate_point(p, center, rotation))
    .collect();
    painter.add(egui::Shape::convex_polygon(
        body_corners,
        Color32::from_rgb(210, 175, 120),
        stroke,
    ));

    // Color bands (4-band) from value
    let band_positions = [0.18_f32, 0.32, 0.46, 0.74];
    let band_w = body_w * 0.10;
    let band_h = body_h * 0.95;
    // bands passed as parameter
    for (i, &frac) in band_positions.iter().enumerate() {
        let bx = body.left() + body_w * frac;
        let by = center.y;
        let color = resistor_band_color(bands[i]);
        let band_rect =
            Rect::from_center_size(Pos2::new(bx + band_w * 0.5, by), Vec2::new(band_w, band_h));
        let bcs: Vec<Pos2> = [
            band_rect.left_top(),
            band_rect.right_top(),
            band_rect.right_bottom(),
            band_rect.left_bottom(),
        ]
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();
        painter.add(egui::Shape::convex_polygon(bcs, color, egui::Stroke::NONE));
    }
}

pub(crate) fn draw_capacitor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), rect.center().y);
    let right = Pos2::new(rect.right(), rect.center().y);
    let plate_offset = rect.width() * 0.2;
    let plate_height = rect.height() * 0.5;
    let p1 = Pos2::new(center.x - plate_offset, rect.center().y - plate_height);
    let p2 = Pos2::new(center.x - plate_offset, rect.center().y + plate_height);
    let p3 = Pos2::new(center.x + plate_offset, rect.center().y - plate_height);
    let p4 = Pos2::new(center.x + plate_offset, rect.center().y + plate_height);

    let points = [left, right, p1, p2, p3, p4];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    let left = rotated[0];
    let right = rotated[1];
    let p1 = rotated[2];
    let p2 = rotated[3];
    let p3 = rotated[4];
    let p4 = rotated[5];

    painter.line_segment([left, p1.lerp(p2, 0.5)], stroke);
    painter.line_segment([p3.lerp(p4, 0.5), right], stroke);
    painter.line_segment([p1, p2], stroke);
    // Curved plate for positive terminal
    let steps = 12;
    let curve_h = rect.height() * 0.5;
    let curve_x = center.x + plate_offset;
    let curve_depth = rect.width() * 0.06;
    let mut prev = rotate_point(Pos2::new(curve_x, center.y - curve_h), center, rotation);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let angle = std::f32::consts::PI * t;
        let cx = curve_x + curve_depth * angle.sin();
        let cy = center.y - curve_h + curve_h * 2.0 * t;
        let next = rotate_point(Pos2::new(cx, cy), center, rotation);
        painter.line_segment([prev, next], stroke);
        prev = next;
    }
    // + polarity mark near positive plate
    let plus_pos = rotate_point(
        Pos2::new(
            center.x - plate_offset - rect.width() * 0.12,
            center.y - rect.height() * 0.25,
        ),
        center,
        rotation,
    );
    painter.text(
        plus_pos,
        Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(8.0),
        Color32::from_rgb(120, 220, 140),
    );
}

pub(crate) fn draw_inductor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let turns = 4;
    let body_w = rect.width() * 0.72;
    let body_start = center.x - body_w * 0.5;
    let step = body_w / turns as f32;
    let radius = rect.height() * 0.22;
    let seg_steps = 16;

    // Lead lines
    let lead_l = Pos2::new(body_start, center.y);
    let lead_r = Pos2::new(body_start + body_w, center.y);
    painter.line_segment(
        [
            rotate_point(left, center, rotation),
            rotate_point(lead_l, center, rotation),
        ],
        stroke,
    );
    painter.line_segment(
        [
            rotate_point(lead_r, center, rotation),
            rotate_point(right, center, rotation),
        ],
        stroke,
    );

    // Draw smooth arcs (upper semicircles)
    for i in 0..turns {
        let cx = body_start + step * (i as f32 + 0.5);
        let arc_center = Pos2::new(cx, center.y);
        let mut prev = rotate_point(Pos2::new(cx - radius, center.y), center, rotation);
        for s in 1..=seg_steps {
            let theta = std::f32::consts::PI * s as f32 / seg_steps as f32;
            let px = cx - radius * theta.cos();
            let py = center.y - radius * theta.sin();
            let next = rotate_point(Pos2::new(px, py), center, rotation);
            painter.line_segment([prev, next], stroke);
            prev = next;
        }
        let _ = arc_center;
    }
}

pub(crate) fn draw_diode(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    filled: bool,
) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let anode = Pos2::new(center.x - rect.width() * 0.18, center.y);
    let cathode = Pos2::new(center.x + rect.width() * 0.2, center.y);
    let tri_top = Pos2::new(
        center.x - rect.width() * 0.18,
        center.y - rect.height() * 0.42,
    );
    let tri_bottom = Pos2::new(
        center.x - rect.width() * 0.18,
        center.y + rect.height() * 0.42,
    );
    let cathode_top = Pos2::new(cathode.x, center.y - rect.height() * 0.42);
    let cathode_bottom = Pos2::new(cathode.x, center.y + rect.height() * 0.42);

    let points = [
        left,
        right,
        anode,
        cathode,
        tri_top,
        tri_bottom,
        cathode_top,
        cathode_bottom,
    ];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[2]], stroke);
    painter.line_segment([rotated[3], rotated[1]], stroke);
    let triangle = vec![rotated[4], rotated[5], rotated[3]];
    if filled {
        painter.add(egui::Shape::convex_polygon(
            triangle.clone(),
            Color32::from_rgba_unmultiplied(
                stroke.color.r(),
                stroke.color.g(),
                stroke.color.b(),
                40,
            ),
            stroke,
        ));
    } else {
        painter.line_segment([triangle[0], triangle[1]], stroke);
        painter.line_segment([triangle[1], triangle[2]], stroke);
        painter.line_segment([triangle[2], triangle[0]], stroke);
    }
    painter.line_segment([rotated[6], rotated[7]], stroke);
}

pub(crate) fn led_glow_colors(value: &str) -> (Color32, Color32) {
    // Scan each token in the value string for a recognized color keyword.
    // This handles "3.2V red", "red 2V", "green", "IR", etc.
    let v = value.trim().to_ascii_lowercase();
    for token in v.split_whitespace() {
        let colors = match token {
            "red" | "r" => Some((
                Color32::from_rgba_unmultiplied(255, 40, 40, 30),
                Color32::from_rgba_unmultiplied(255, 80, 80, 70),
            )),
            "green" | "g" => Some((
                Color32::from_rgba_unmultiplied(40, 255, 80, 30),
                Color32::from_rgba_unmultiplied(80, 255, 120, 70),
            )),
            "blue" | "b" => Some((
                Color32::from_rgba_unmultiplied(40, 100, 255, 30),
                Color32::from_rgba_unmultiplied(80, 140, 255, 70),
            )),
            "yellow" | "y" => Some((
                Color32::from_rgba_unmultiplied(255, 240, 40, 30),
                Color32::from_rgba_unmultiplied(255, 250, 80, 70),
            )),
            "white" | "w" => Some((
                Color32::from_rgba_unmultiplied(220, 220, 255, 30),
                Color32::from_rgba_unmultiplied(240, 240, 255, 70),
            )),
            "orange" | "o" => Some((
                Color32::from_rgba_unmultiplied(255, 140, 20, 30),
                Color32::from_rgba_unmultiplied(255, 165, 60, 70),
            )),
            "uv" | "purple" | "violet" => Some((
                Color32::from_rgba_unmultiplied(160, 40, 255, 30),
                Color32::from_rgba_unmultiplied(190, 80, 255, 70),
            )),
            "ir" | "infrared" => Some((
                Color32::from_rgba_unmultiplied(180, 20, 20, 20),
                Color32::from_rgba_unmultiplied(200, 40, 40, 50),
            )),
            _ => None,
        };
        if let Some(pair) = colors {
            return pair;
        }
    }
    // Default: warm yellow-white
    (
        Color32::from_rgba_unmultiplied(255, 220, 60, 30),
        Color32::from_rgba_unmultiplied(255, 235, 100, 70),
    )
}

pub(crate) fn draw_led(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    draw_diode(painter, rect, rotation, stroke, true);
    let center = rect.center();
    // Two emission arrows with proper arrowheads (45° angle, upper-right direction)
    let arrows = [
        (
            Pos2::new(
                center.x + rect.width() * 0.10,
                center.y - rect.height() * 0.48,
            ),
            Pos2::new(
                center.x + rect.width() * 0.30,
                center.y - rect.height() * 0.70,
            ),
        ),
        (
            Pos2::new(
                center.x + rect.width() * 0.22,
                center.y - rect.height() * 0.30,
            ),
            Pos2::new(
                center.x + rect.width() * 0.42,
                center.y - rect.height() * 0.52,
            ),
        ),
    ];
    for (raw_start, raw_end) in arrows {
        let s = rotate_point(raw_start, center, rotation);
        let e = rotate_point(raw_end, center, rotation);
        painter.line_segment([s, e], stroke);
        // Small arrowhead: two lines fanning back from tip
        let dir = (e - s).normalized();
        let perp = Vec2::new(-dir.y, dir.x);
        let head_len = rect.width() * 0.08;
        let back = e - dir * head_len;
        let head1 = back + perp * head_len * 0.45;
        let head2 = back - perp * head_len * 0.45;
        painter.line_segment([e, head1], Stroke::new(stroke.width * 0.85, stroke.color));
        painter.line_segment([e, head2], Stroke::new(stroke.width * 0.85, stroke.color));
    }
}

pub(crate) fn draw_switch(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    closed: bool,
) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let left_contact = Pos2::new(center.x - rect.width() * 0.25, center.y);
    let right_contact = Pos2::new(center.x + rect.width() * 0.25, center.y);
    let blade_end = if closed {
        // Blade horizontal → connects the two contacts
        Pos2::new(center.x + rect.width() * 0.25, center.y)
    } else {
        // Blade angled up → open
        Pos2::new(
            center.x + rect.width() * 0.18,
            center.y - rect.height() * 0.32,
        )
    };
    let points = [left, right, left_contact, right_contact, blade_end];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[2]], stroke);
    painter.line_segment([rotated[3], rotated[1]], stroke);
    painter.circle_filled(rotated[2], 3.2, stroke.color);
    painter.circle_filled(rotated[3], 3.2, stroke.color);
    painter.line_segment([rotated[2], rotated[4]], stroke);
}

pub(crate) fn draw_push_button(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    closed: bool,
) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let left_contact = Pos2::new(center.x - rect.width() * 0.24, center.y);
    let right_contact = Pos2::new(center.x + rect.width() * 0.24, center.y);
    // Bar position: lower when closed (touching contacts), raised when open
    let bar_y_offset = if closed { 0.0 } else { -rect.height() * 0.18 };
    let bar_left = Pos2::new(center.x - rect.width() * 0.18, center.y + bar_y_offset);
    let bar_right = Pos2::new(center.x + rect.width() * 0.18, center.y + bar_y_offset);
    let stem_top = Pos2::new(center.x, rect.top() + rect.height() * 0.08);
    let stem_bottom = Pos2::new(center.x, center.y + bar_y_offset);
    let points = [
        left,
        right,
        left_contact,
        right_contact,
        bar_left,
        bar_right,
        stem_top,
        stem_bottom,
    ];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[2]], stroke);
    painter.line_segment([rotated[3], rotated[1]], stroke);
    painter.circle_filled(rotated[2], 3.2, stroke.color);
    painter.circle_filled(rotated[3], 3.2, stroke.color);
    // Draw bar (filled rect when closed to show solid connection)
    if closed {
        let bar_rect = Rect::from_center_size(
            Pos2::new((rotated[4].x + rotated[5].x) / 2.0, rotated[4].y),
            Vec2::new((rotated[5].x - rotated[4].x).abs() + 3.0, 5.0),
        );
        painter.rect_filled(bar_rect, 0.0, stroke.color);
    } else {
        painter.line_segment([rotated[4], rotated[5]], stroke);
    }
    painter.line_segment([rotated[6], rotated[7]], stroke);
}

pub(crate) fn draw_oled(
    painter: &egui::Painter,
    _component: &Component,
    rect: Rect,
    stroke: Stroke,
    energized: bool,
    rotation: i32,
) {
    let rot = rotation.rem_euclid(360);
    let center = rect.center();

    let body_rect = if rot == 90 || rot == 270 {
        Rect::from_center_size(center, Vec2::new(rect.height(), rect.width()))
    } else {
        rect
    };
    let body_fill = if energized {
        Color32::from_rgb(22, 30, 42)
    } else {
        Color32::from_rgb(20, 26, 34)
    };
    painter.rect_filled(body_rect, 5.0, body_fill);
    painter.rect_stroke(body_rect, 5.0, stroke, StrokeKind::Outside);

    let nat_screen = Rect::from_min_max(
        rect.min + Vec2::new(10.0, 20.0),
        rect.max - Vec2::new(10.0, 10.0),
    );
    let sc_corners = [
        nat_screen.left_top(),
        nat_screen.right_top(),
        nat_screen.right_bottom(),
        nat_screen.left_bottom(),
    ]
    .map(|p| rotate_point(p, center, rotation));
    let screen_rect = Rect::from_points(&sc_corners);

    if energized {
        // True OLED black background (unlit pixels are pure black)
        painter.rect_filled(screen_rect, 3.0, Color32::BLACK);

        // Outer glow around screen
        painter.rect_stroke(
            screen_rect.expand(2.5),
            4.0,
            Stroke::new(2.0_f32, Color32::from_rgba_unmultiplied(80, 200, 255, 60)),
            StrokeKind::Outside,
        );
        painter.rect_stroke(
            screen_rect,
            3.0,
            Stroke::new(1.0_f32, Color32::from_rgb(40, 160, 220)),
            StrokeKind::Outside,
        );

        let sw = screen_rect.width();
        let sh = screen_rect.height();
        let sx = screen_rect.left();
        let sy = screen_rect.top();

        // Top title bar in bright cyan/white — characteristic OLED look
        let title_y = sy + sh * 0.18;
        painter.text(
            Pos2::new(sx + sw * 0.5, sy + sh * 0.08),
            Align2::CENTER_TOP,
            "CLUSTER",
            egui::FontId::monospace(7.0),
            Color32::from_rgb(255, 255, 255),
        );

        // Separator line
        painter.line_segment(
            [
                Pos2::new(sx + 4.0, title_y),
                Pos2::new(sx + sw - 4.0, title_y),
            ],
            Stroke::new(1.0_f32, Color32::from_rgb(60, 160, 220)),
        );

        // Signal-bar icon (3 bars, left side)
        let bar_x = sx + 5.0;
        let bar_bot = sy + sh * 0.52;
        for (i, h_frac) in [0.25_f32, 0.45, 0.65].iter().enumerate() {
            let bh = sh * h_frac;
            let bx = bar_x + i as f32 * 6.0;
            painter.rect_filled(
                Rect::from_min_size(Pos2::new(bx, bar_bot - bh), Vec2::new(4.0, bh)),
                1.0,
                if i < 2 {
                    Color32::from_rgb(80, 220, 180)
                } else {
                    Color32::from_rgb(40, 100, 80)
                },
            );
        }

        // "ON" status text
        painter.text(
            Pos2::new(sx + sw - 6.0, sy + sh * 0.38),
            Align2::RIGHT_CENTER,
            "ON",
            egui::FontId::monospace(7.0),
            Color32::from_rgb(80, 240, 130),
        );

        // Bottom address line (I2C address hint)
        painter.text(
            Pos2::new(sx + sw * 0.5, sy + sh * 0.72),
            Align2::CENTER_TOP,
            "0x3C",
            egui::FontId::monospace(7.0),
            Color32::from_rgb(130, 180, 255),
        );

        // Pixel-dot decoration row
        let dot_y = sy + sh * 0.88;
        let dot_count = ((sw - 10.0) / 5.0) as usize;
        for i in 0..dot_count {
            let brightness = if i % 3 == 0 { 180u8 } else { 60 };
            painter.circle_filled(
                Pos2::new(sx + 5.0 + i as f32 * 5.0, dot_y),
                1.2,
                Color32::from_rgb(brightness, brightness, 255),
            );
        }
    } else {
        // Screen off — OLED goes fully dark
        painter.rect_filled(screen_rect, 3.0, Color32::from_rgb(8, 9, 11));
        painter.rect_stroke(
            screen_rect,
            3.0,
            Stroke::new(1.0_f32, Color32::from_rgb(32, 36, 42)),
            StrokeKind::Outside,
        );
        painter.text(
            screen_rect.center(),
            Align2::CENTER_CENTER,
            "OFF",
            egui::FontId::proportional(9.0),
            Color32::from_rgb(45, 50, 56),
        );
    }

    // Pin header: natural positions at top edge, then rotated
    let step = (rect.width() - 16.0) / 3.0;
    for (i, label) in ["GND", "VCC", "SCL", "SDA"].iter().enumerate() {
        let nat_pin = Pos2::new(rect.left() + 8.0 + i as f32 * step, rect.top());
        let nat_stub = Pos2::new(nat_pin.x, rect.top() + 6.0);
        let nat_label = Pos2::new(nat_pin.x, rect.top() + 11.0);
        let pin_r = rotate_point(nat_pin, center, rotation);
        let stub_r = rotate_point(nat_stub, center, rotation);
        let label_r = rotate_point(nat_label, center, rotation);
        painter.line_segment([pin_r, stub_r], stroke);
        painter.circle_filled(pin_r, 2.5, stroke.color);
        painter.text(
            label_r,
            Align2::CENTER_CENTER,
            *label,
            egui::FontId::proportional(7.5),
            Color32::from_rgb(160, 170, 180),
        );
    }
}

pub(crate) fn draw_breadboard(painter: &egui::Painter, rect: Rect, stroke: Stroke) {
    painter.rect_filled(rect, 4.0, Color32::from_rgb(28, 31, 35));
    painter.rect_stroke(rect, 4.0, stroke, StrokeKind::Outside);
    let plus_y = rect.top() + 24.0;
    let minus_y = rect.top() + 44.0;
    painter.line_segment(
        [
            Pos2::new(rect.left() + 14.0, plus_y),
            Pos2::new(rect.right() - 14.0, plus_y),
        ],
        Stroke::new(2.0_f32, Color32::from_rgb(255, 185, 80)),
    );
    painter.line_segment(
        [
            Pos2::new(rect.left() + 14.0, minus_y),
            Pos2::new(rect.right() - 14.0, minus_y),
        ],
        Stroke::new(2.0_f32, Color32::from_rgb(120, 190, 255)),
    );
    painter.line_segment(
        [
            Pos2::new(rect.center().x, rect.top() + 66.0),
            Pos2::new(rect.center().x, rect.bottom() - 14.0),
        ],
        Stroke::new(1.4_f32, Color32::from_rgb(80, 85, 92)),
    );

    let hole = Color32::from_rgb(70, 76, 84);
    let mut x = rect.left() + 28.0;
    while x <= rect.right() - 28.0 {
        for row in 0..5 {
            let y = rect.top() + 78.0 + row as f32 * 14.0;
            painter.circle_filled(Pos2::new(x, y), 2.2, hole);
            painter.circle_filled(Pos2::new(x, y + 82.0), 2.2, hole);
        }
        x += 18.0;
    }
    painter.text(
        rect.left_top() + Vec2::new(12.0, 8.0),
        Align2::LEFT_TOP,
        "+  -",
        egui::FontId::proportional(12.0),
        Color32::from_rgb(220, 225, 230),
    );
}

pub(crate) fn draw_relay(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let box_rect =
        Rect::from_center_size(center, Vec2::new(rect.width() * 0.72, rect.height() * 0.72));
    painter.rect_stroke(box_rect, 4.0, stroke, StrokeKind::Outside);
    painter.text(
        center,
        Align2::CENTER_CENTER,
        "RELAY",
        egui::FontId::proportional(12.0),
        stroke.color,
    );
    let pins = [
        Pos2::new(rect.left(), center.y - rect.height() * 0.25),
        Pos2::new(box_rect.left(), center.y - rect.height() * 0.25),
        Pos2::new(rect.left(), center.y + rect.height() * 0.25),
        Pos2::new(box_rect.left(), center.y + rect.height() * 0.25),
        Pos2::new(box_rect.right(), center.y - rect.height() * 0.28),
        Pos2::new(rect.right(), center.y - rect.height() * 0.28),
        Pos2::new(box_rect.right(), center.y),
        Pos2::new(rect.right(), center.y),
        Pos2::new(box_rect.right(), center.y + rect.height() * 0.28),
        Pos2::new(rect.right(), center.y + rect.height() * 0.28),
    ];
    let rotated: Vec<Pos2> = pins
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();
    for segment in rotated.chunks(2) {
        painter.line_segment([segment[0], segment[1]], stroke);
    }
}

pub(crate) fn draw_dc_motor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let radius = rect.height() * 0.34;
    let rotated_left = rotate_point(left, center, rotation);
    let rotated_right = rotate_point(right, center, rotation);
    painter.line_segment(
        [
            rotated_left,
            rotate_point(Pos2::new(center.x - radius, center.y), center, rotation),
        ],
        stroke,
    );
    painter.line_segment(
        [
            rotate_point(Pos2::new(center.x + radius, center.y), center, rotation),
            rotated_right,
        ],
        stroke,
    );
    painter.circle_stroke(center, radius, stroke);
    painter.text(
        center,
        Align2::CENTER_CENTER,
        "M",
        egui::FontId::proportional(18.0),
        stroke.color,
    );
}

pub(crate) fn draw_servo(painter: &egui::Painter, rect: Rect, stroke: Stroke, energized: bool) {
    let fill = if energized {
        Color32::from_rgb(48, 38, 20)
    } else {
        Color32::from_rgb(26, 31, 36)
    };
    painter.rect_filled(rect.shrink(8.0), 4.0, fill);
    painter.rect_stroke(rect.shrink(8.0), 4.0, stroke, StrokeKind::Outside);
    let horn_center = Pos2::new(rect.right() - 24.0, rect.center().y);
    painter.circle_stroke(horn_center, 10.0, stroke);
    painter.line_segment([horn_center, horn_center + Vec2::new(24.0, -12.0)], stroke);
    painter.text(
        rect.center() - Vec2::new(12.0, 0.0),
        Align2::CENTER_CENTER,
        "SERVO",
        egui::FontId::proportional(11.0),
        stroke.color,
    );
}

pub(crate) fn draw_ground(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let stem_top = Pos2::new(rect.center().x, rect.top());
    let stem_bottom = Pos2::new(rect.center().x, rect.center().y);
    let line1_left = Pos2::new(rect.center().x - rect.width() * 0.3, rect.center().y);
    let line1_right = Pos2::new(rect.center().x + rect.width() * 0.3, rect.center().y);
    let line2_left = Pos2::new(
        rect.center().x - rect.width() * 0.2,
        rect.center().y + rect.height() * 0.2,
    );
    let line2_right = Pos2::new(
        rect.center().x + rect.width() * 0.2,
        rect.center().y + rect.height() * 0.2,
    );
    let line3_left = Pos2::new(
        rect.center().x - rect.width() * 0.1,
        rect.center().y + rect.height() * 0.35,
    );
    let line3_right = Pos2::new(
        rect.center().x + rect.width() * 0.1,
        rect.center().y + rect.height() * 0.35,
    );

    let points = [
        stem_top,
        stem_bottom,
        line1_left,
        line1_right,
        line2_left,
        line2_right,
        line3_left,
        line3_right,
    ];

    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[1]], stroke);
    painter.line_segment([rotated[2], rotated[3]], stroke);
    painter.line_segment([rotated[4], rotated[5]], stroke);
    painter.line_segment([rotated[6], rotated[7]], stroke);
}

pub(crate) fn draw_vsource(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.4;
    let left = Pos2::new(rect.left(), rect.center().y);
    let right = Pos2::new(rect.right(), rect.center().y);
    let circle_left = Pos2::new(center.x - radius, center.y);
    let circle_right = Pos2::new(center.x + radius, center.y);
    // + symbol near positive (right) side, - near negative (left) side
    let plus_top = Pos2::new(center.x + radius * 0.28, center.y - radius * 0.32);
    let plus_bottom = Pos2::new(center.x + radius * 0.28, center.y + radius * 0.32);
    let plus_left = Pos2::new(center.x + radius * 0.06, center.y);
    let plus_right = Pos2::new(center.x + radius * 0.5, center.y);
    let minus_left = Pos2::new(center.x - radius * 0.5, center.y);
    let minus_right = Pos2::new(center.x - radius * 0.06, center.y);

    let points = [
        left,
        right,
        circle_left,
        circle_right,
        plus_top,
        plus_bottom,
        plus_left,
        plus_right,
        minus_left,
        minus_right,
    ];

    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[2]], stroke);
    painter.line_segment([rotated[3], rotated[1]], stroke);
    painter.circle_stroke(center, radius, stroke);
    painter.line_segment([rotated[4], rotated[5]], stroke);
    painter.line_segment([rotated[6], rotated[7]], stroke);
    painter.line_segment([rotated[8], rotated[9]], stroke);
}

pub(crate) fn draw_isource(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.4;
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let circle_left = Pos2::new(center.x - radius, center.y);
    let circle_right = Pos2::new(center.x + radius, center.y);
    let arrow_start = Pos2::new(center.x - radius * 0.35, center.y);
    let arrow_end = Pos2::new(center.x + radius * 0.35, center.y);
    let head_a = Pos2::new(center.x + radius * 0.1, center.y - radius * 0.22);
    let head_b = Pos2::new(center.x + radius * 0.1, center.y + radius * 0.22);
    let points = [
        left,
        right,
        circle_left,
        circle_right,
        arrow_start,
        arrow_end,
        head_a,
        head_b,
    ];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[2]], stroke);
    painter.line_segment([rotated[3], rotated[1]], stroke);
    painter.circle_stroke(center, radius, stroke);
    painter.line_segment([rotated[4], rotated[5]], stroke);
    painter.line_segment([rotated[5], rotated[6]], stroke);
    painter.line_segment([rotated[5], rotated[7]], stroke);
}

pub(crate) fn draw_battery(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);

    // Two cells: each cell = short line (−) + long line (+)
    // Pairs at 1/3 and 2/3 horizontally
    let cell_centers = [
        center.x - rect.width() * 0.12,
        center.x + rect.width() * 0.12,
    ];
    let mut all_pts: Vec<Pos2> = vec![left, right];
    for &cx in &cell_centers {
        let sh = rect.height() * 0.25; // short half-height
        let lh = rect.height() * 0.44; // long half-height
        all_pts.push(Pos2::new(cx - rect.width() * 0.04, center.y - sh));
        all_pts.push(Pos2::new(cx - rect.width() * 0.04, center.y + sh));
        all_pts.push(Pos2::new(cx + rect.width() * 0.04, center.y - lh));
        all_pts.push(Pos2::new(cx + rect.width() * 0.04, center.y + lh));
    }
    let rotated: Vec<Pos2> = all_pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();
    let left_r = rotated[0];
    let right_r = rotated[1];

    // Indices: [0]=left, [1]=right
    // Cell 1: [2,3]=short(−), [4,5]=long(+)
    // Cell 2: [6,7]=short(−), [8,9]=long(+)
    let thick = Stroke::new(stroke.width * 1.5, stroke.color);
    let dim = Stroke::new(
        stroke.width * 0.9,
        Color32::from_rgba_unmultiplied(stroke.color.r(), stroke.color.g(), stroke.color.b(), 180),
    );

    // Lead wires
    painter.line_segment([left_r, midpoint(rotated[2], rotated[3])], stroke); // left → cell1 −
    painter.line_segment([midpoint(rotated[8], rotated[9]), right_r], stroke); // cell2 + → right

    // Cell 1
    painter.line_segment([rotated[2], rotated[3]], stroke); // short (−)
    painter.line_segment([rotated[4], rotated[5]], thick); // long  (+)

    // Inter-cell connection
    painter.line_segment(
        [
            midpoint(rotated[4], rotated[5]),
            midpoint(rotated[6], rotated[7]),
        ],
        dim,
    );

    // Cell 2
    painter.line_segment([rotated[6], rotated[7]], stroke); // short (−)
    painter.line_segment([rotated[8], rotated[9]], thick); // long  (+)

    // Polarity labels
    let minus_pos = rotate_point(
        Pos2::new(
            center.x - rect.width() * 0.35,
            center.y - rect.height() * 0.44,
        ),
        center,
        rotation,
    );
    let plus_pos = rotate_point(
        Pos2::new(
            center.x + rect.width() * 0.35,
            center.y - rect.height() * 0.44,
        ),
        center,
        rotation,
    );
    painter.text(
        minus_pos,
        Align2::CENTER_CENTER,
        "−",
        egui::FontId::proportional(13.0),
        Color32::from_rgb(140, 190, 255),
    );
    painter.text(
        plus_pos,
        Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(13.0),
        Color32::from_rgb(255, 210, 100),
    );
}

pub(crate) fn draw_opamp(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left_top = Pos2::new(rect.left(), rect.top());
    let left_bottom = Pos2::new(rect.left(), rect.bottom());
    let right = Pos2::new(rect.right(), center.y);
    let in_minus = Pos2::new(rect.left(), center.y - rect.height() * 0.22);
    let in_plus = Pos2::new(rect.left(), center.y + rect.height() * 0.22);
    let minus_text = Pos2::new(
        rect.left() + rect.width() * 0.25,
        center.y - rect.height() * 0.22,
    );
    let plus_text = Pos2::new(
        rect.left() + rect.width() * 0.25,
        center.y + rect.height() * 0.22,
    );
    let points = [
        left_top,
        left_bottom,
        right,
        in_minus,
        in_plus,
        minus_text,
        plus_text,
    ];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[1]], stroke);
    painter.line_segment([rotated[1], rotated[2]], stroke);
    painter.line_segment([rotated[2], rotated[0]], stroke);
    let minus_lead = rotate_point(Pos2::new(rect.left() - 8.0, in_minus.y), center, rotation);
    let plus_lead = rotate_point(Pos2::new(rect.left() - 8.0, in_plus.y), center, rotation);
    let out_lead = rotate_point(Pos2::new(rect.right() + 8.0, center.y), center, rotation);
    painter.line_segment([minus_lead, rotated[3]], stroke);
    painter.line_segment([plus_lead, rotated[4]], stroke);
    painter.line_segment([rotated[2], out_lead], stroke);
    painter.text(
        rotated[5],
        Align2::CENTER_CENTER,
        "-",
        egui::FontId::proportional(14.0),
        stroke.color,
    );
    painter.text(
        rotated[6],
        Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(14.0),
        stroke.color,
    );
}

pub(crate) fn draw_lamp(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.34;
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let a = Pos2::new(center.x - radius * 0.7, center.y - radius * 0.7);
    let b = Pos2::new(center.x + radius * 0.7, center.y + radius * 0.7);
    let c = Pos2::new(center.x + radius * 0.7, center.y - radius * 0.7);
    let d = Pos2::new(center.x - radius * 0.7, center.y + radius * 0.7);
    let points = [left, right, a, b, c, d];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], center], stroke);
    painter.line_segment([center, rotated[1]], stroke);
    painter.circle_stroke(center, radius, stroke);
    painter.line_segment([rotated[2], rotated[3]], stroke);
    painter.line_segment([rotated[4], rotated[5]], stroke);
}

#[allow(clippy::too_many_arguments)] // Explicit module geometry keeps call sites readable.
pub(crate) fn draw_module(
    painter: &egui::Painter,
    component: &Component,
    rect: Rect,
    stroke: Stroke,
    energized: bool,
    rotation: i32,
    title: &str,
    left_labels: &[&str],
    right_labels: &[&str],
) {
    let center = rect.center();
    let rot = rotation.rem_euclid(360);

    // Body: swap dims for 90/270
    let body_rect = if rot == 90 || rot == 270 {
        Rect::from_center_size(center, Vec2::new(rect.height(), rect.width()))
    } else {
        rect
    };
    let body_fill = if energized {
        Color32::from_rgb(62, 46, 22)
    } else {
        Color32::from_rgb(24, 30, 38)
    };
    painter.rect_filled(body_rect, 4.0, body_fill);
    painter.rect_stroke(
        body_rect,
        4.0,
        Stroke::new(stroke.width, stroke.color),
        StrokeKind::Outside,
    );

    painter.text(
        center + Vec2::new(0.0, -7.0),
        Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(14.0),
        stroke.color,
    );
    painter.text(
        center + Vec2::new(0.0, 10.0),
        Align2::CENTER_CENTER,
        &component.value,
        egui::FontId::proportional(10.0),
        Color32::from_rgb(150, 160, 170),
    );

    // Left pins: natural position on left edge, then rotated
    for (i, label) in left_labels.iter().enumerate() {
        let y = module_pin_y(rect, left_labels.len(), i);
        let nat_pin = Pos2::new(rect.left(), y);
        let nat_stub = nat_pin + Vec2::new(10.0, 0.0);
        let nat_label = nat_pin + Vec2::new(13.0, 0.0);
        let pin_r = rotate_point(nat_pin, center, rotation);
        let stub_r = rotate_point(nat_stub, center, rotation);
        let label_r = rotate_point(nat_label, center, rotation);
        painter.line_segment([pin_r, stub_r], stroke);
        painter.text(
            label_r,
            Align2::CENTER_CENTER,
            *label,
            egui::FontId::proportional(9.0),
            Color32::from_rgb(185, 195, 205),
        );
    }

    // Right pins: natural position on right edge, then rotated
    for (i, label) in right_labels.iter().enumerate() {
        let y = module_pin_y(rect, right_labels.len(), i);
        let nat_pin = Pos2::new(rect.right(), y);
        let nat_stub = nat_pin - Vec2::new(10.0, 0.0);
        let nat_label = nat_pin - Vec2::new(13.0, 0.0);
        let pin_r = rotate_point(nat_pin, center, rotation);
        let stub_r = rotate_point(nat_stub, center, rotation);
        let label_r = rotate_point(nat_label, center, rotation);
        painter.line_segment([pin_r, stub_r], stroke);
        painter.text(
            label_r,
            Align2::CENTER_CENTER,
            *label,
            egui::FontId::proportional(9.0),
            Color32::from_rgb(185, 195, 205),
        );
    }
}

pub(crate) fn midpoint(a: Pos2, b: Pos2) -> Pos2 {
    Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}
