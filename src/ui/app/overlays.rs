use super::*;

pub(crate) fn draw_minimap(
    painter: &egui::Painter,
    canvas: Rect,
    components: &[crate::model::Component],
    wires: &[Wire],
    view: CanvasView,
) {
    let mm_w = 140.0_f32;
    let mm_h = 90.0_f32;
    let margin = 10.0_f32;
    let mm_rect = Rect::from_min_size(
        Pos2::new(
            canvas.right() - mm_w - margin,
            canvas.bottom() - mm_h - margin,
        ),
        Vec2::new(mm_w, mm_h),
    );

    // Find world bounds
    let mut wmin = Pos2::new(f32::INFINITY, f32::INFINITY);
    let mut wmax = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for c in components {
        wmin.x = wmin.x.min(c.pos.x - 50.0);
        wmin.y = wmin.y.min(c.pos.y - 50.0);
        wmax.x = wmax.x.max(c.pos.x + 50.0);
        wmax.y = wmax.y.max(c.pos.y + 50.0);
    }
    for w in wires {
        for &p in &w.points {
            wmin.x = wmin.x.min(p.x);
            wmin.y = wmin.y.min(p.y);
            wmax.x = wmax.x.max(p.x);
            wmax.y = wmax.y.max(p.y);
        }
    }
    if !wmin.x.is_finite() {
        return;
    }

    let world_w = (wmax.x - wmin.x).max(100.0);
    let world_h = (wmax.y - wmin.y).max(100.0);
    let scale_x = mm_w / world_w;
    let scale_y = mm_h / world_h;
    let scale = scale_x.min(scale_y) * 0.9;

    let to_mm = |p: Pos2| -> Pos2 {
        let nx = (p.x - wmin.x) * scale;
        let ny = (p.y - wmin.y) * scale;
        Pos2::new(
            mm_rect.left() + nx + (mm_w - world_w * scale) * 0.5,
            mm_rect.top() + ny + (mm_h - world_h * scale) * 0.5,
        )
    };

    // Background
    painter.rect_filled(
        mm_rect,
        4.0,
        Color32::from_rgba_unmultiplied(10, 14, 20, 210),
    );
    painter.rect_stroke(
        mm_rect,
        4.0,
        Stroke::new(1.0, Color32::from_rgb(50, 65, 85)),
        egui::StrokeKind::Middle,
    );

    // Draw wires
    for wire in wires {
        for seg in wire.points.windows(2) {
            painter.line_segment(
                [to_mm(seg[0]), to_mm(seg[1])],
                Stroke::new(1.0, Color32::from_rgb(70, 130, 200)),
            );
        }
    }

    // Draw components as dots
    for comp in components {
        let p = to_mm(comp.pos);
        painter.circle_filled(p, 2.5, Color32::from_rgb(120, 200, 160));
    }

    // Viewport indicator
    let vp_tl = view.to_world(canvas.min);
    let vp_br = view.to_world(canvas.max);
    let vp_mm_tl = to_mm(vp_tl);
    let vp_mm_br = to_mm(vp_br);
    let vp_rect = Rect::from_two_pos(vp_mm_tl, vp_mm_br).intersect(mm_rect);
    if vp_rect.is_positive() {
        painter.rect_filled(
            vp_rect,
            2.0,
            Color32::from_rgba_unmultiplied(80, 160, 255, 30),
        );
        painter.rect_stroke(
            vp_rect,
            2.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(80, 180, 255, 140)),
            egui::StrokeKind::Middle,
        );
    }
}

pub(crate) fn osc_bar_rows(
    ui: &mut egui::Ui,
    rows: &[(String, f64)],
    max_val: f64,
    is_voltage: bool,
) {
    let bar_width = (ui.available_width() - 200.0).max(30.0);
    for (label, value) in rows {
        ui.horizontal(|ui| {
            ui.add_sized(
                Vec2::new(110.0, 16.0),
                egui::Label::new(
                    egui::RichText::new(label)
                        .monospace()
                        .size(10.5)
                        .color(Color32::from_rgb(200, 210, 220)),
                ),
            );
            let norm = ((value / max_val).abs() as f32).min(1.0);
            let fill = if is_voltage {
                if *value >= 0.0 {
                    Color32::from_rgb(60, 180, 120)
                } else {
                    Color32::from_rgb(220, 80, 80)
                }
            } else {
                Color32::from_rgb(80, 160, 255)
            };
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(bar_width, 13.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 2.0, Color32::from_rgba_unmultiplied(40, 50, 60, 200));
            let bar_rect = egui::Rect::from_min_size(rect.min, Vec2::new(bar_width * norm, 13.0));
            ui.painter().rect_filled(bar_rect, 2.0, fill);
            let val_str = if is_voltage {
                mna::format_voltage(*value)
            } else {
                mna::format_current(*value)
            };
            ui.add_sized(
                Vec2::new(70.0, 16.0),
                egui::Label::new(
                    egui::RichText::new(val_str)
                        .monospace()
                        .size(10.5)
                        .color(Color32::from_rgb(240, 230, 120)),
                ),
            );
        });
    }
}

/// Draw small voltage circles at wire junction/endpoint positions when DC is available.
pub(crate) fn draw_node_voltage_indicators(
    painter: &egui::Painter,
    wires: &[Wire],
    dc: &mna::DcResult,
    view: CanvasView,
    vmax: f64,
) {
    // Collect unique wire endpoints (junction points get drawn once)
    let mut seen: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    for wire in wires {
        for &pt in &wire.points {
            let key = (pt.x.round() as i32, pt.y.round() as i32);
            if !seen.insert(key) {
                continue; // already drawn
            }
            // Find the voltage at this wire point
            if let Some((&_wid, &_net)) = dc.wire_voltage.iter().next() {
                // Use wire_voltage for any wire that contains this point
                let v_opt = wires.iter().find_map(|w| {
                    if w.points
                        .iter()
                        .any(|p| (p.x.round() as i32, p.y.round() as i32) == key)
                    {
                        dc.wire_voltage.get(&w.id).copied()
                    } else {
                        None
                    }
                });
                if let Some(v) = v_opt {
                    let sp = view.to_screen(pt);
                    let col = mna::voltage_color(v, vmax);
                    // Only draw if it's actually a junction (multiple wires meet)
                    let junction_count = wires
                        .iter()
                        .filter(|w| {
                            w.points
                                .first()
                                .map(|p| (p.x.round() as i32, p.y.round() as i32) == key)
                                .unwrap_or(false)
                                || w.points
                                    .last()
                                    .map(|p| (p.x.round() as i32, p.y.round() as i32) == key)
                                    .unwrap_or(false)
                        })
                        .count();
                    if junction_count >= 2 {
                        painter.circle_filled(sp, 5.5, col);
                        painter.circle_stroke(
                            sp,
                            5.5,
                            Stroke::new(1.0, Color32::from_rgb(20, 24, 30)),
                        );
                        // Show voltage label at junctions (only when zoom is high enough)
                        if view.zoom >= 0.8 {
                            painter.text(
                                sp + Vec2::new(7.0, -7.0),
                                Align2::LEFT_BOTTOM,
                                mna::format_voltage(v),
                                egui::FontId::proportional(9.0),
                                Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 210),
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Draw a compact simulation summary box in the top-right corner of the canvas.
pub(crate) fn draw_sim_summary(
    painter: &egui::Painter,
    canvas: Rect,
    simulation: &crate::engine::simulation::Simulation,
) {
    let lines: Vec<String> = {
        let mut v = Vec::new();
        if simulation.shorted {
            v.push("⚡ SHORT CIRCUIT".to_string());
        } else if simulation.closed {
            v.push(format!(
                "Status: {}",
                simulation_status_label(simulation.status)
            ));
            if let Some(dc) = &simulation.dc {
                let total_p: f64 = dc.component_power.values().sum();
                if total_p > 1e-12 {
                    v.push(format!("P total: {}", mna::format_power(total_p)));
                }
                if let Some(&vmax) = dc
                    .net_voltages
                    .values()
                    .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                {
                    v.push(format!("V max: {}", mna::format_voltage(vmax)));
                }
                v.push(format!("Nodes: {}", dc.net_voltages.len()));
            }
            if v.is_empty() {
                v.push("Sim: closed".to_string());
            }
        } else {
            v.push("Sim: open circuit".to_string());
            if !simulation.explanation.is_empty() {
                v.push(simulation.explanation.clone());
            }
        }
        v
    };

    if lines.is_empty() {
        return;
    }

    let font = egui::FontId::proportional(10.5);
    let line_h = 14.0;
    let padding = Vec2::new(8.0, 5.0);
    let w = lines
        .iter()
        .map(|l| l.len() as f32 * 5.8)
        .fold(0.0_f32, f32::max)
        + padding.x * 2.0;
    let h = lines.len() as f32 * line_h + padding.y * 2.0;
    let top_right = canvas.right_top() + Vec2::new(-w - 8.0, 8.0);
    let bg = Rect::from_min_size(top_right, Vec2::new(w, h));

    painter.rect_filled(bg, 4.0, Color32::from_rgba_unmultiplied(15, 20, 28, 215));
    painter.rect_stroke(
        bg,
        4.0,
        Stroke::new(
            1.0,
            if simulation.shorted {
                Color32::from_rgb(220, 60, 60)
            } else if simulation.status == SimulationStatus::Ok {
                Color32::from_rgb(60, 180, 100)
            } else if simulation.status == SimulationStatus::Warning {
                Color32::from_rgb(220, 170, 70)
            } else {
                Color32::from_rgb(80, 90, 100)
            },
        ),
        StrokeKind::Outside,
    );

    for (i, line) in lines.iter().enumerate() {
        let pos = bg.min + Vec2::new(padding.x, padding.y + i as f32 * line_h);
        painter.text(
            pos,
            Align2::LEFT_TOP,
            line,
            font.clone(),
            if simulation.shorted {
                Color32::from_rgb(255, 100, 100)
            } else if simulation.status == SimulationStatus::Ok {
                Color32::from_rgb(130, 230, 160)
            } else if simulation.status == SimulationStatus::Warning {
                Color32::from_rgb(255, 210, 100)
            } else {
                Color32::from_rgb(140, 150, 165)
            },
        );
    }
}

pub(crate) fn draw_title_block(
    painter: &egui::Painter,
    canvas: Rect,
    components: &[Component],
    wires: &[Wire],
    simulation: &Simulation,
) {
    let erc_errors = simulation
        .erc
        .iter()
        .filter(|e| e.severity == ErcSeverity::Error)
        .count();
    let erc_warns = simulation
        .erc
        .iter()
        .filter(|e| e.severity == ErcSeverity::Warning)
        .count();

    let size = Vec2::new(272.0, 148.0);
    let rect = Rect::from_min_size(canvas.right_bottom() - size - Vec2::new(18.0, 18.0), size);
    painter.rect_filled(rect, 4.0, Color32::from_rgba_unmultiplied(14, 17, 22, 238));
    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, Color32::from_rgb(60, 70, 82)),
        StrokeKind::Outside,
    );

    let divider = |y: f32| {
        painter.line_segment(
            [
                Pos2::new(rect.left() + 10.0, rect.top() + y),
                Pos2::new(rect.right() - 10.0, rect.top() + y),
            ],
            Stroke::new(1.0, Color32::from_rgb(50, 58, 68)),
        )
    };

    let mono = |y: f32, txt: String, col: Color32| {
        painter.text(
            rect.left_top() + Vec2::new(12.0, y),
            Align2::LEFT_TOP,
            txt,
            egui::FontId::monospace(10.5),
            col,
        )
    };

    let dim = Color32::from_rgb(110, 120, 132);
    let bright = Color32::from_rgb(200, 210, 222);

    // Header
    painter.text(
        rect.left_top() + Vec2::new(12.0, 9.0),
        Align2::LEFT_TOP,
        "CLUSTER CIRCUIT",
        egui::FontId::proportional(12.0),
        Color32::from_rgb(220, 230, 240),
    );
    painter.text(
        rect.right_top() + Vec2::new(-12.0, 9.0),
        Align2::RIGHT_TOP,
        "v0.3",
        egui::FontId::monospace(10.0),
        dim,
    );
    divider(28.0);

    // Stats row
    mono(
        36.0,
        format!("Parts {:>3}  Wires {:>3}", components.len(), wires.len()),
        bright,
    );

    // Simulation status
    let status_color = if simulation.shorted {
        Color32::from_rgb(255, 95, 80)
    } else if simulation.closed {
        Color32::from_rgb(100, 220, 140)
    } else {
        Color32::from_rgb(130, 140, 155)
    };
    mono(
        53.0,
        format!(
            "Status  {} / {}",
            simulation.summary,
            simulation_status_label(simulation.status)
        ),
        status_color,
    );

    // DC values
    let dc_col = Color32::from_rgb(100, 200, 160);
    if let Some(dc) = &simulation.dc {
        let mut nets: Vec<f64> = dc.net_voltages.values().copied().collect();
        nets.sort_by(|a, b| b.total_cmp(a));
        nets.dedup();
        if let Some(&vmax) = nets.first() {
            mono(70.0, format!("Vmax  {}", mna::format_voltage(vmax)), dc_col);
        }
        if let Some(i) = simulation.current {
            mono(86.0, format!("Iloop {}", format_current(i)), dc_col);
        }
    } else {
        if let Some(v) = simulation.voltage {
            mono(70.0, format!("Vsrc  {:.2} V", v), dc_col);
        }
        if let Some(i) = simulation.current {
            mono(86.0, format!("Iloop {}", format_current(i)), dc_col);
        }
    }
    divider(102.0);

    // ERC summary
    let (erc_str, erc_col) = if erc_errors > 0 {
        (
            format!("ERC  ✗{erc_errors} error(s)  ⚠{erc_warns} warn(s)"),
            Color32::from_rgb(255, 100, 85),
        )
    } else if erc_warns > 0 {
        (
            format!("ERC  ⚠{erc_warns} warning(s)"),
            Color32::from_rgb(255, 200, 80),
        )
    } else if components.is_empty() {
        ("ERC  (no schematic)".to_string(), dim)
    } else {
        (
            "ERC  ✓ No violations".to_string(),
            Color32::from_rgb(100, 200, 140),
        )
    };
    mono(109.0, erc_str, erc_col);
    divider(127.0);
    mono(133.0, "Cluster Workbench  —  cluster.io".to_string(), dim);
}

pub(crate) fn draw_empty_canvas_hint(painter: &egui::Painter, canvas: Rect) {
    let rect = Rect::from_center_size(canvas.center(), Vec2::new(360.0, 120.0));
    painter.rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(20, 24, 30, 225));
    painter.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(58, 66, 76)),
        StrokeKind::Outside,
    );
    painter.text(
        rect.center_top() + Vec2::new(0.0, 24.0),
        Align2::CENTER_TOP,
        "Start a schematic",
        egui::FontId::proportional(18.0),
        Color32::from_rgb(228, 234, 240),
    );
    painter.text(
        rect.center() + Vec2::new(0.0, 6.0),
        Align2::CENTER_CENTER,
        "Pick a part on the left, then click the grid.",
        egui::FontId::proportional(12.0),
        Color32::from_rgb(156, 166, 176),
    );
    painter.text(
        rect.center_bottom() - Vec2::new(0.0, 22.0),
        Align2::CENTER_BOTTOM,
        "Use Wire to connect pins. Enter finishes a wire.",
        egui::FontId::proportional(12.0),
        Color32::from_rgb(156, 166, 176),
    );
}

/// Returns grid-rounded positions of all component pins that have a snapped
/// wire endpoint/control point on the pin. A wire merely passing nearby is not
/// a connection.
pub(crate) fn connected_pin_positions(components: &[Component], wires: &[Wire]) -> Vec<(i32, i32)> {
    let mut connected = Vec::new();
    for component in components {
        for pin in component_pin_defs(component) {
            let key = (pin.pos.x.round() as i32, pin.pos.y.round() as i32);
            let is_conn = wires.iter().any(|w| {
                w.points
                    .windows(2)
                    .any(|segment| point_touches_wire_segment(pin.pos, segment[0], segment[1]))
            });
            if is_conn {
                connected.push(key);
            }
        }
    }
    connected
}

// ─────────────────────────────────────────────────────────────────────────────
//  ERC — Electrical Rules Check
// ─────────────────────────────────────────────────────────────────────────────
