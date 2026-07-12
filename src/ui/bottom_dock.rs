use crate::engine::simulation::Simulation;
use crate::engine::transient::TransientKind;
use crate::engine::validation::{ErcSeverity, ErcViolation};
use crate::ui::theme;
use crate::ui::validation_panel::{ValidationPanelAction, render_validation_panel};
use eframe::egui;
use egui::{Color32, Pos2, Rect, Stroke, StrokeKind, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageTabsAction {
    SwitchTo(usize),
    RenameDefault(usize),
    AddPage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BottomDockTab {
    Erc,
    Simulation,
    Netlist,
    Breadboard,
    Pcb,
    Logs,
}

#[derive(Debug, Clone)]
pub(crate) struct PcbDockSummary {
    pub(crate) footprint_count: usize,
    pub(crate) unplaced_count: usize,
    pub(crate) ratsnest_count: usize,
    pub(crate) drc_errors: usize,
    pub(crate) drc_warnings: usize,
    pub(crate) dirty: bool,
    pub(crate) footprints: Vec<String>,
    pub(crate) ratsnest: Vec<String>,
    pub(crate) drc: Vec<PcbDrcRow>,
    pub(crate) preview: PcbPreviewData,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PcbPreviewData {
    pub(crate) width_mm: f32,
    pub(crate) height_mm: f32,
    pub(crate) footprints: Vec<PcbPreviewFootprint>,
    pub(crate) tracks: Vec<PcbPreviewTrack>,
    pub(crate) ratsnest: Vec<PcbPreviewRatsnest>,
    pub(crate) diagnostics: Vec<PcbPreviewDiagnostic>,
}

#[derive(Debug, Clone)]
pub(crate) struct PcbDrcRow {
    pub(crate) index: usize,
    pub(crate) severity: PcbDrcSeverity,
    pub(crate) title: String,
    pub(crate) selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PcbDrcSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub(crate) struct PcbPreviewFootprint {
    pub(crate) reference: String,
    pub(crate) x_mm: f32,
    pub(crate) y_mm: f32,
    pub(crate) placed: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PcbPreviewTrack {
    pub(crate) start_x_mm: f32,
    pub(crate) start_y_mm: f32,
    pub(crate) end_x_mm: f32,
    pub(crate) end_y_mm: f32,
    pub(crate) front_layer: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PcbPreviewRatsnest {
    pub(crate) start_x_mm: f32,
    pub(crate) start_y_mm: f32,
    pub(crate) end_x_mm: f32,
    pub(crate) end_y_mm: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct PcbPreviewDiagnostic {
    pub(crate) x_mm: f32,
    pub(crate) y_mm: f32,
    pub(crate) severity: PcbDrcSeverity,
    pub(crate) selected: bool,
}

pub(crate) struct BottomDockModel<'a> {
    pub(crate) active_tab: BottomDockTab,
    pub(crate) violations: &'a [ErcViolation],
    pub(crate) has_components: bool,
    pub(crate) simulation: &'a Simulation,
    pub(crate) breadboard_enabled: bool,
    pub(crate) pcb: &'a PcbDockSummary,
    pub(crate) status: &'a str,
}

pub(crate) enum BottomDockAction {
    SetTab(BottomDockTab),
    Validation(ValidationPanelAction),
    OpenBreadboard,
    UpdatePcb,
    AutoPlacePcb,
    FitPcbBoard,
    RoutePcbRatsnest,
    SelectPcbDrc(usize),
    ExportPcbFabrication,
    SavePcbProject,
    LoadPcbProject,
}

pub(crate) fn render_page_tabs(
    ui: &mut egui::Ui,
    page_names: &[String],
    current_page: usize,
) -> Option<PageTabsAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Pages:")
                .size(11.0)
                .color(theme::TEXT_MUTED),
        );
        for (idx, name) in page_names.iter().enumerate() {
            let active = idx == current_page;
            let btn_color = if active {
                theme::OK
            } else {
                Color32::from_rgb(90, 100, 115)
            };
            let resp = ui.add(
                egui::Button::new(egui::RichText::new(name).size(11.0).color(btn_color))
                    .frame(active),
            );
            if resp.clicked() && !active {
                action = Some(PageTabsAction::SwitchTo(idx));
            }
            if resp.double_clicked() {
                action = Some(PageTabsAction::RenameDefault(idx));
            }
        }
        if ui.small_button("+").clicked() {
            action = Some(PageTabsAction::AddPage);
        }
    });
    action
}

pub(crate) fn render_bottom_dock(
    ui: &mut egui::Ui,
    model: BottomDockModel<'_>,
) -> Option<BottomDockAction> {
    let mut action = None;
    theme::panel_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            for (tab, label) in [
                (
                    BottomDockTab::Erc,
                    erc_tab_label(model.violations).replace("ERC", "Problems"),
                ),
                (BottomDockTab::Simulation, "Simulation".to_string()),
                (BottomDockTab::Netlist, "Netlist".to_string()),
                (BottomDockTab::Breadboard, "Breadboard".to_string()),
                (BottomDockTab::Pcb, pcb_tab_label(model.pcb)),
                (BottomDockTab::Logs, "Output".to_string()),
            ] {
                if theme::tool_button(ui, &label, model.active_tab == tab).clicked() {
                    action = Some(BottomDockAction::SetTab(tab));
                }
            }
        });
        ui.separator();
        match model.active_tab {
            BottomDockTab::Erc => {
                if let Some(validation_action) =
                    render_validation_panel(ui, model.violations, model.has_components)
                {
                    action = Some(BottomDockAction::Validation(validation_action));
                }
            }
            BottomDockTab::Simulation => render_simulation_tab(ui, model.simulation),
            BottomDockTab::Netlist => {
                ui.label(
                    egui::RichText::new(
                        "Export → Netlist TXT creates the deterministic netlist document.",
                    )
                    .size(11.0)
                    .color(theme::TEXT_SECONDARY),
                );
            }
            BottomDockTab::Breadboard => {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(if model.breadboard_enabled {
                            "Breadboard view is open."
                        } else {
                            "Open the breadboard assistant to inspect jumpers."
                        })
                        .size(11.0)
                        .color(theme::TEXT_SECONDARY),
                    );
                    if ui.small_button("Open Breadboard").clicked() {
                        action = Some(BottomDockAction::OpenBreadboard);
                    }
                });
            }
            BottomDockTab::Pcb => render_pcb_tab(ui, model.pcb, &mut action),
            BottomDockTab::Logs => {
                ui.label(
                    egui::RichText::new(if model.status.is_empty() {
                        "No recent app messages."
                    } else {
                        model.status
                    })
                    .size(11.0)
                    .color(theme::TEXT_SECONDARY),
                );
            }
        }
    });
    action
}

fn pcb_tab_label(summary: &PcbDockSummary) -> String {
    if summary.dirty {
        "PCB *".to_string()
    } else if summary.ratsnest_count > 0 {
        format!("PCB ({})", summary.ratsnest_count)
    } else {
        "PCB".to_string()
    }
}

fn render_pcb_tab(
    ui: &mut egui::Ui,
    summary: &PcbDockSummary,
    action: &mut Option<BottomDockAction>,
) {
    ui.horizontal_wrapped(|ui| {
        if ui.small_button("Update PCB").clicked() {
            *action = Some(BottomDockAction::UpdatePcb);
        }
        if ui.small_button("Auto-place").clicked() {
            *action = Some(BottomDockAction::AutoPlacePcb);
        }
        if ui.small_button("Fit board").clicked() {
            *action = Some(BottomDockAction::FitPcbBoard);
        }
        if ui.small_button("Route ratsnest").clicked() {
            *action = Some(BottomDockAction::RoutePcbRatsnest);
        }
        if ui.small_button("Save project").clicked() {
            *action = Some(BottomDockAction::SavePcbProject);
        }
        if ui.small_button("Load project").clicked() {
            *action = Some(BottomDockAction::LoadPcbProject);
        }
        if ui.small_button("Export fab").clicked() {
            *action = Some(BottomDockAction::ExportPcbFabrication);
        }
        metric(ui, "Footprints", &summary.footprint_count.to_string());
        metric(ui, "Unplaced", &summary.unplaced_count.to_string());
        metric(ui, "Ratsnest", &summary.ratsnest_count.to_string());
        metric(ui, "DRC errors", &summary.drc_errors.to_string());
        metric(ui, "Warnings", &summary.drc_warnings.to_string());
    });
    ui.add_space(4.0);
    let message = if summary.dirty {
        "Schematic changed. Update PCB to refresh footprints, ratsnest, and DRC."
    } else if summary.footprint_count == 0 {
        "No PCB footprints yet. Add supported schematic parts, then update PCB."
    } else if summary.ratsnest_count > 0 {
        "Footprints are synced. Ratsnest shows connections that still need routing."
    } else {
        "PCB data is synced with the current schematic."
    };
    ui.label(
        egui::RichText::new(message)
            .size(11.0)
            .color(theme::TEXT_SECONDARY),
    );
    ui.add_space(4.0);
    render_pcb_preview(ui, &summary.preview);
    ui.add_space(4.0);
    ui.columns(3, |columns| {
        detail_list(&mut columns[0], "Footprints", &summary.footprints);
        detail_list(&mut columns[1], "Ratsnest", &summary.ratsnest);
        drc_list(&mut columns[2], &summary.drc, action);
    });
}

fn render_pcb_preview(ui: &mut egui::Ui, preview: &PcbPreviewData) {
    let desired = Vec2::new(ui.available_width().max(120.0), 130.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(18, 24, 31));
    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, Color32::from_rgb(50, 61, 74)),
        StrokeKind::Inside,
    );

    if preview.width_mm <= 0.1 || preview.height_mm <= 0.1 {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "No PCB preview",
            egui::FontId::proportional(11.0),
            theme::TEXT_MUTED,
        );
        return;
    }

    let margin = 10.0;
    let board_scale = ((rect.width() - margin * 2.0) / preview.width_mm)
        .min((rect.height() - margin * 2.0) / preview.height_mm)
        .max(0.1);
    let board_size = Vec2::new(
        preview.width_mm * board_scale,
        preview.height_mm * board_scale,
    );
    let board_rect = Rect::from_center_size(rect.center(), board_size);
    painter.rect_filled(board_rect, 2.0, Color32::from_rgb(24, 70, 55));
    painter.rect_stroke(
        board_rect,
        2.0,
        Stroke::new(1.3, Color32::from_rgb(120, 170, 145)),
        StrokeKind::Inside,
    );

    let map = |x_mm: f32, y_mm: f32| -> Pos2 {
        Pos2::new(
            board_rect.left() + x_mm * board_scale,
            board_rect.top() + y_mm * board_scale,
        )
    };

    for track in &preview.tracks {
        let color = if track.front_layer {
            Color32::from_rgb(220, 115, 85)
        } else {
            Color32::from_rgb(95, 155, 225)
        };
        painter.line_segment(
            [
                map(track.start_x_mm, track.start_y_mm),
                map(track.end_x_mm, track.end_y_mm),
            ],
            Stroke::new(2.0, color),
        );
    }

    for ratsnest in &preview.ratsnest {
        painter.line_segment(
            [
                map(ratsnest.start_x_mm, ratsnest.start_y_mm),
                map(ratsnest.end_x_mm, ratsnest.end_y_mm),
            ],
            Stroke::new(1.0, Color32::from_rgb(210, 205, 95)),
        );
    }

    for diagnostic in &preview.diagnostics {
        let center = map(diagnostic.x_mm, diagnostic.y_mm);
        let color = match diagnostic.severity {
            PcbDrcSeverity::Error => Color32::from_rgb(245, 80, 80),
            PcbDrcSeverity::Warning => Color32::from_rgb(245, 190, 70),
        };
        let radius = if diagnostic.selected { 6.5 } else { 4.5 };
        painter.circle_stroke(center, radius, Stroke::new(1.8, color));
        painter.line_segment(
            [center - Vec2::splat(radius), center + Vec2::splat(radius)],
            Stroke::new(1.0, color),
        );
        painter.line_segment(
            [
                center + Vec2::new(-radius, radius),
                center + Vec2::new(radius, -radius),
            ],
            Stroke::new(1.0, color),
        );
    }

    for footprint in &preview.footprints {
        let center = map(footprint.x_mm, footprint.y_mm);
        let size = Vec2::splat(if footprint.placed { 8.0 } else { 6.0 });
        let fp_rect = Rect::from_center_size(center, size);
        let color = if footprint.placed {
            Color32::from_rgb(230, 230, 210)
        } else {
            Color32::from_rgb(125, 132, 142)
        };
        painter.rect_filled(fp_rect, 1.0, color);
        painter.text(
            center + Vec2::new(0.0, 8.0),
            egui::Align2::CENTER_TOP,
            &footprint.reference,
            egui::FontId::proportional(8.5),
            Color32::from_rgb(220, 225, 230),
        );
    }
}

fn drc_list(ui: &mut egui::Ui, rows: &[PcbDrcRow], action: &mut Option<BottomDockAction>) {
    ui.label(
        egui::RichText::new("DRC")
            .size(10.5)
            .strong()
            .color(theme::TEXT_SECONDARY),
    );
    if rows.is_empty() {
        ui.label(
            egui::RichText::new("None")
                .size(10.0)
                .color(theme::TEXT_MUTED),
        );
        return;
    }
    for row in rows.iter().take(5) {
        let prefix = match row.severity {
            PcbDrcSeverity::Error => "ERR",
            PcbDrcSeverity::Warning => "WARN",
        };
        let color = match row.severity {
            PcbDrcSeverity::Error => theme::ERROR,
            PcbDrcSeverity::Warning => theme::WARNING,
        };
        let text = egui::RichText::new(format!("{prefix}: {}", row.title))
            .size(10.0)
            .color(color);
        if ui.selectable_label(row.selected, text).clicked() {
            *action = Some(BottomDockAction::SelectPcbDrc(row.index));
        }
    }
    if rows.len() > 5 {
        ui.label(
            egui::RichText::new(format!("+{} more", rows.len() - 5))
                .size(10.0)
                .color(theme::TEXT_MUTED),
        );
    }
}

fn detail_list(ui: &mut egui::Ui, title: &str, rows: &[String]) {
    ui.label(
        egui::RichText::new(title)
            .size(10.5)
            .strong()
            .color(theme::TEXT_SECONDARY),
    );
    if rows.is_empty() {
        ui.label(
            egui::RichText::new("None")
                .size(10.0)
                .color(theme::TEXT_MUTED),
        );
        return;
    }
    for row in rows.iter().take(5) {
        ui.label(
            egui::RichText::new(row)
                .size(10.0)
                .color(theme::TEXT_SECONDARY),
        );
    }
    if rows.len() > 5 {
        ui.label(
            egui::RichText::new(format!("+{} more", rows.len() - 5))
                .size(10.0)
                .color(theme::TEXT_MUTED),
        );
    }
}

fn render_simulation_tab(ui: &mut egui::Ui, simulation: &Simulation) {
    ui.horizontal_wrapped(|ui| {
        metric(ui, "Status", &simulation.summary);
        metric(ui, "Confidence", simulation_status_label(simulation.status));
        if let Some(voltage) = simulation.voltage {
            metric(ui, "Voltage", &format!("{voltage:.2} V"));
        }
        if let Some(current) = simulation.current {
            metric(
                ui,
                "Current",
                &crate::engine::mna::format_current(current.into()),
            );
        }
        if let Some(resistance) = simulation.resistance {
            metric(ui, "Load R", &format!("{resistance:.0} ohm"));
        }
        if simulation.transient.is_some() {
            metric(ui, "Transient", "RC/PWM preview");
        }
    });
    if !simulation.explanation.is_empty() {
        ui.label(
            egui::RichText::new(&simulation.explanation)
                .size(11.0)
                .color(theme::TEXT_SECONDARY),
        );
    }
    if let Some(error) = &simulation.dc_error {
        ui.label(
            egui::RichText::new(format!("DC solver: {error}"))
                .size(11.0)
                .color(theme::WARNING),
        );
    }
    if let Some(transient) = &simulation.transient {
        ui.label(
            egui::RichText::new(&transient.summary)
                .size(11.0)
                .color(theme::TEXT_SECONDARY),
        );
        if let (Some(first), Some(last)) = (transient.samples.first(), transient.samples.last()) {
            let kind = match transient.kind {
                TransientKind::RcStep => "RC step",
                TransientKind::PwmRc => "PWM RC",
            };
            ui.label(
                egui::RichText::new(format!(
                    "{kind}: source {} -> {}, capacitor {} at t={:.3}s -> {} at t={:.3}s",
                    crate::engine::mna::format_voltage(first.source_v),
                    crate::engine::mna::format_voltage(last.source_v),
                    crate::engine::mna::format_voltage(first.v_cap),
                    first.t_s,
                    crate::engine::mna::format_voltage(last.v_cap),
                    last.t_s
                ))
                .size(11.0)
                .color(theme::TEXT_SECONDARY),
            );
        }
        for limitation in &transient.limitations {
            ui.label(
                egui::RichText::new(format!("Simplified: {limitation}"))
                    .size(10.5)
                    .color(theme::TEXT_MUTED),
            );
        }
    }
}

fn metric(ui: &mut egui::Ui, label: &str, value: &str) {
    theme::card_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(label)
                .size(9.5)
                .color(theme::TEXT_MUTED),
        );
        ui.label(
            egui::RichText::new(value)
                .size(11.0)
                .strong()
                .color(theme::TEXT_PRIMARY),
        );
    });
}

fn erc_tab_label(violations: &[ErcViolation]) -> String {
    let errors = violations
        .iter()
        .filter(|violation| violation.severity == ErcSeverity::Error)
        .count();
    let warnings = violations
        .iter()
        .filter(|violation| violation.severity == ErcSeverity::Warning)
        .count();
    match (errors, warnings) {
        (0, 0) => "ERC".to_string(),
        (errors, 0) => format!("ERC E:{errors}"),
        (0, warnings) => format!("ERC W:{warnings}"),
        (errors, warnings) => format!("ERC E:{errors} W:{warnings}"),
    }
}

fn simulation_status_label(status: crate::engine::simulation::SimulationStatus) -> &'static str {
    match status {
        crate::engine::simulation::SimulationStatus::Ok => "OK",
        crate::engine::simulation::SimulationStatus::Warning => "Warning",
        crate::engine::simulation::SimulationStatus::Failed => "Failed",
    }
}
