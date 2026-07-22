use super::*;

#[derive(Debug, Clone, Copy)]
pub(crate) enum StatusTone {
    Neutral,
    Live,
    Warning,
    Error,
}

pub(crate) struct LessonReport {
    pub(crate) title: String,
    pub(crate) checks: Vec<LessonCheck>,
    pub(crate) next_action: String,
}

pub(crate) struct LessonCheck {
    pub(crate) label: String,
    pub(crate) passed: bool,
    pub(crate) detail: String,
}

pub(crate) fn apply_app_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = Color32::from_rgb(18, 21, 26);
    visuals.panel_fill = Color32::from_rgb(18, 21, 26);
    visuals.extreme_bg_color = Color32::from_rgb(12, 14, 18);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(31, 36, 43);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(43, 50, 59);
    visuals.widgets.active.bg_fill = Color32::from_rgb(46, 58, 68);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(52, 58, 66));
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.visuals = ctx.style().visuals.clone();
    ctx.set_style(style);
}

pub(crate) fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(11.0)
            .strong()
            .color(Color32::from_rgb(138, 149, 160)),
    );
}

pub(crate) fn compact_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(Color32::from_rgb(215, 222, 230)))
            .fill(Color32::from_rgb(31, 36, 43))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(56, 64, 74)))
            .min_size(Vec2::new(74.0, 26.0)),
    )
}

pub(crate) fn palette_action(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add_sized(
        Vec2::new(ui.available_width(), 22.0),
        egui::Button::new(
            egui::RichText::new(label)
                .size(10.5)
                .color(Color32::from_rgb(216, 224, 232)),
        )
        .fill(Color32::from_rgb(28, 33, 39))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(48, 56, 64))),
    )
}

/// Simulation-support badge row for the inspector: a compact colored pill
/// (not just plain text) so the confidence level of a component's model is
/// scannable at a glance, matching the same badge style used for wire/pin
/// status. `ExactDc` reads as neutral; every reduced-confidence level
/// (`ApproximateDc`, `DigitalOnly`, `SymbolOnly`, `Unsupported`) reads as a
/// warning, mirroring `SimulationSupport::needs_inspector_warning`.
pub(crate) fn simulation_support_row(ui: &mut egui::Ui, label: &str, support: SimulationSupport) {
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new(label)
                .size(11.0)
                .color(Color32::from_rgb(135, 146, 156)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let tone = if support.needs_inspector_warning() {
                StatusTone::Warning
            } else {
                StatusTone::Neutral
            };
            status_pill(ui, support.label(), tone);
        });
    });
}

pub(crate) fn status_pill(ui: &mut egui::Ui, text: &str, tone: StatusTone) {
    let (fill, stroke, color) = match tone {
        StatusTone::Neutral => (
            Color32::from_rgb(30, 36, 43),
            Color32::from_rgb(58, 68, 78),
            Color32::from_rgb(210, 218, 226),
        ),
        StatusTone::Live => (
            Color32::from_rgb(54, 42, 22),
            Color32::from_rgb(132, 92, 34),
            Color32::from_rgb(255, 198, 92),
        ),
        StatusTone::Warning => (
            Color32::from_rgb(54, 45, 22),
            Color32::from_rgb(150, 115, 38),
            Color32::from_rgb(255, 215, 110),
        ),
        StatusTone::Error => (
            Color32::from_rgb(58, 30, 30),
            Color32::from_rgb(142, 64, 58),
            Color32::from_rgb(255, 128, 112),
        ),
    };
    egui::Frame::NONE
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, stroke))
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(11.0).strong().color(color));
        });
}

pub(crate) fn simulation_tone(simulation: &Simulation) -> StatusTone {
    let has_erc_error = simulation
        .erc
        .iter()
        .any(|e| e.severity == ErcSeverity::Error);
    if simulation.shorted || has_erc_error {
        StatusTone::Error
    } else {
        match simulation.status {
            SimulationStatus::Ok => StatusTone::Live,
            SimulationStatus::Warning => StatusTone::Warning,
            SimulationStatus::Failed => StatusTone::Error,
        }
    }
}

pub(crate) fn simulation_status_label(status: SimulationStatus) -> &'static str {
    match status {
        SimulationStatus::Ok => "OK",
        SimulationStatus::Warning => "Warning",
        SimulationStatus::Failed => "Failed",
    }
}

pub(crate) fn simulation_status_from_solver(
    shorted: bool,
    dc_error: Option<&mna::SimulationError>,
) -> SimulationStatus {
    if shorted {
        SimulationStatus::Failed
    } else if dc_error.is_some() {
        SimulationStatus::Warning
    } else {
        SimulationStatus::Ok
    }
}

pub(crate) fn simulation_text_color(simulation: &Simulation) -> Color32 {
    match simulation_tone(simulation) {
        StatusTone::Error => Color32::from_rgb(255, 128, 112),
        StatusTone::Live => Color32::from_rgb(255, 198, 92),
        StatusTone::Warning => Color32::from_rgb(255, 215, 110),
        StatusTone::Neutral => Color32::from_rgb(152, 162, 172),
    }
}

pub(crate) fn simulation_warning_count(simulation: &Simulation) -> usize {
    simulation
        .erc
        .iter()
        .filter(|e| matches!(e.severity, ErcSeverity::Error | ErcSeverity::Warning))
        .count()
}

pub(crate) fn flow_overlay_enabled(simulation: &Simulation, simulate_enabled: bool) -> bool {
    simulate_enabled && !simulation.shorted && !simulation.energized_wires.is_empty()
}

pub(crate) fn lesson_report(
    components: &[Component],
    simulation: &Simulation,
) -> Option<LessonReport> {
    let notes = components
        .iter()
        .filter(|component| component.kind == ComponentKind::TextNote)
        .map(|component| component.value.trim())
        .filter(|value| value.to_ascii_lowercase().contains("expect:"))
        .collect::<Vec<_>>();
    if notes.is_empty() {
        return None;
    }

    let joined = notes.join("\n");
    let lower = joined.to_ascii_lowercase();
    let mut checks = Vec::new();

    let erc_errors = simulation
        .erc
        .iter()
        .filter(|violation| violation.severity == ErcSeverity::Error)
        .count();
    let erc_warnings = simulation
        .erc
        .iter()
        .filter(|violation| violation.severity == ErcSeverity::Warning)
        .count();
    let led_ids = components
        .iter()
        .filter(|component| component.kind == ComponentKind::Led)
        .map(|component| component.id)
        .collect::<Vec<_>>();
    let motor_ids = components
        .iter()
        .filter(|component| component.kind == ComponentKind::DcMotor)
        .map(|component| component.id)
        .collect::<Vec<_>>();
    let capacitor_ids = components
        .iter()
        .filter(|component| component.kind == ComponentKind::Capacitor)
        .map(|component| component.id)
        .collect::<Vec<_>>();
    let energized_leds = led_ids
        .iter()
        .filter(|id| simulation.energized_components.contains(id))
        .count();
    let energized_motors = motor_ids
        .iter()
        .filter(|id| simulation.energized_components.contains(id))
        .count();
    let energized_caps = capacitor_ids
        .iter()
        .filter(|id| simulation.energized_components.contains(id))
        .count();

    let expects_on = lower.contains("expect: on") || lower.contains("expect: both on");
    let expects_off = lower.contains("expect: off") || lower.contains("open circuit");
    let expects_error = lower.contains("expect: error") || lower.contains("short circuit");
    let expects_warning = lower.contains("expect: warning") || lower.contains("warning");

    if expects_on {
        checks.push(LessonCheck {
            label: "Closed path".to_string(),
            passed: simulation.closed && !simulation.shorted && erc_errors == 0,
            detail: simulation.summary.clone(),
        });
        if !led_ids.is_empty() {
            checks.push(LessonCheck {
                label: "LED output".to_string(),
                passed: energized_leds == led_ids.len(),
                detail: format!("{energized_leds}/{} lit", led_ids.len()),
            });
        }
        if !motor_ids.is_empty() {
            checks.push(LessonCheck {
                label: "Motor output".to_string(),
                passed: energized_motors == motor_ids.len(),
                detail: format!("{energized_motors}/{} running", motor_ids.len()),
            });
        }
    }

    if expects_off {
        checks.push(LessonCheck {
            label: "No closed current path".to_string(),
            passed: !simulation.closed && !simulation.shorted,
            detail: simulation.summary.clone(),
        });
        if !led_ids.is_empty() {
            checks.push(LessonCheck {
                label: "LED stays off".to_string(),
                passed: energized_leds == 0,
                detail: format!("{energized_leds}/{} lit", led_ids.len()),
            });
        }
        if lower.contains("capacitor") && !capacitor_ids.is_empty() {
            checks.push(LessonCheck {
                label: "Capacitor blocks DC".to_string(),
                passed: energized_caps == 0,
                detail: format!("{energized_caps}/{} conducting", capacitor_ids.len()),
            });
        }
    }

    if expects_error {
        checks.push(LessonCheck {
            label: "Error detected".to_string(),
            passed: simulation.shorted || erc_errors > 0,
            detail: format!("{erc_errors} ERC error(s)"),
        });
    }

    if expects_warning {
        checks.push(LessonCheck {
            label: "Warning detected".to_string(),
            passed: erc_errors + erc_warnings > 0,
            detail: format!("{erc_errors} error(s), {erc_warnings} warning(s)"),
        });
        if lower.contains("gpio") && lower.contains("motor") {
            let gpio_motor_warning = simulation.erc.iter().any(|violation| {
                violation.message.contains("GPIO") && violation.message.contains("motor")
            });
            checks.push(LessonCheck {
                label: "GPIO motor rule".to_string(),
                passed: gpio_motor_warning,
                detail: "Use a driver, transistor, or relay".to_string(),
            });
            if !motor_ids.is_empty() {
                checks.push(LessonCheck {
                    label: "Motor stays off".to_string(),
                    passed: energized_motors == 0,
                    detail: format!("{energized_motors}/{} running", motor_ids.len()),
                });
            }
        }
    }

    if checks.is_empty() {
        checks.push(LessonCheck {
            label: "Simulation state".to_string(),
            passed: !simulation.shorted,
            detail: simulation.summary.clone(),
        });
    }

    let passed = checks.iter().filter(|check| check.passed).count();
    let total = checks.len();
    let next_action = if passed == total {
        "Matches the EXPECT note. Try editing one wire or value and watch this change.".to_string()
    } else if expects_off {
        "Find the unwanted live path or missing break, then compare orange highlights.".to_string()
    } else if expects_error || expects_warning {
        "Open the ERC panel and click the reported item to locate the fault.".to_string()
    } else {
        "Complete the source-load-return path, then verify the orange live path.".to_string()
    };

    Some(LessonReport {
        title: format!("Lesson Check {passed}/{total}"),
        checks,
        next_action,
    })
}

pub(crate) fn metric_row(ui: &mut egui::Ui, label: impl Into<String>, value: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new(label.into())
                .size(11.0)
                .color(Color32::from_rgb(135, 146, 156)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value.into())
                    .size(11.0)
                    .color(Color32::from_rgb(210, 218, 226)),
            );
        });
    });
}

pub(crate) fn render_lesson_report(ui: &mut egui::Ui, report: &LessonReport) {
    let all_passed = report.checks.iter().all(|check| check.passed);
    let (fill, stroke, title_color) = if all_passed {
        (
            Color32::from_rgb(20, 42, 34),
            Color32::from_rgb(58, 150, 105),
            Color32::from_rgb(150, 245, 185),
        )
    } else {
        (
            Color32::from_rgb(48, 36, 22),
            Color32::from_rgb(170, 115, 48),
            Color32::from_rgb(255, 205, 115),
        )
    };

    egui::Frame::NONE
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, stroke))
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::symmetric(9, 7))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(&report.title)
                    .size(12.0)
                    .strong()
                    .color(title_color),
            );
            ui.add_space(4.0);
            for check in &report.checks {
                let mark = if check.passed { "PASS" } else { "CHECK" };
                let color = if check.passed {
                    Color32::from_rgb(150, 235, 180)
                } else {
                    Color32::from_rgb(255, 190, 115)
                };
                ui.horizontal(|ui| {
                    ui.add_sized(
                        Vec2::new(42.0, 16.0),
                        egui::Label::new(
                            egui::RichText::new(mark)
                                .size(10.0)
                                .monospace()
                                .strong()
                                .color(color),
                        ),
                    );
                    ui.label(
                        egui::RichText::new(&check.label)
                            .size(11.0)
                            .color(Color32::from_rgb(220, 228, 235)),
                    );
                });
                ui.label(
                    egui::RichText::new(&check.detail)
                        .size(10.5)
                        .color(Color32::from_rgb(150, 160, 170)),
                );
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(&report.next_action)
                    .size(11.0)
                    .color(Color32::from_rgb(205, 214, 222)),
            );
        });
}

pub(crate) fn dc_metric_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new(label)
                .size(11.0)
                .color(Color32::from_rgb(130, 200, 160)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value)
                    .size(12.0)
                    .strong()
                    .color(Color32::from_rgb(140, 230, 180)),
            );
        });
    });
}

pub(crate) fn edit_row(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    ui.label(
        egui::RichText::new(label)
            .size(11.0)
            .color(Color32::from_rgb(135, 146, 156)),
    );
    ui.add_sized(
        Vec2::new(ui.available_width(), 25.0),
        egui::TextEdit::singleline(value)
            .text_color(Color32::from_rgb(230, 235, 240))
            .background_color(Color32::from_rgb(12, 15, 19)),
    )
    .changed()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SectionMode {
    Open,
    Collapsed,
}

pub(crate) fn palette_section(
    ui: &mut egui::Ui,
    title: &str,
    mode: SectionMode,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    ui.add_space(3.0);
    egui::Frame::NONE
        .fill(Color32::from_rgb(23, 28, 35))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(58, 68, 80)))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(5, 3))
        .show(ui, |ui| {
            let title = egui::RichText::new(title.to_uppercase())
                .size(10.0)
                .strong()
                .color(Color32::from_rgb(190, 204, 218));
            match mode {
                SectionMode::Open => {
                    ui.label(title);
                    ui.add_space(3.0);
                    add_contents(ui);
                }
                SectionMode::Collapsed => {
                    egui::CollapsingHeader::new(title)
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.add_space(2.0);
                            add_contents(ui);
                        });
                }
            }
        });
}

pub(crate) fn push_unique_point(points: &mut Vec<Pos2>, pos: Pos2) {
    if points.last().is_some_and(|last| last.distance(pos) < 0.5) {
        return;
    }
    points.push(pos);
}
