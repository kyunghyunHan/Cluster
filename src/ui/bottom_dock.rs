use crate::engine::simulation::Simulation;
use crate::engine::transient::TransientKind;
use crate::engine::validation::{ErcSeverity, ErcViolation};
use crate::ui::theme;
use crate::ui::validation_panel::{ValidationPanelAction, render_validation_panel};
use eframe::egui;
use egui::Color32;

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
    Breadboard,
    Logs,
}

pub(crate) struct BottomDockModel<'a> {
    pub(crate) active_tab: BottomDockTab,
    pub(crate) violations: &'a [ErcViolation],
    pub(crate) has_components: bool,
    pub(crate) simulation: &'a Simulation,
    pub(crate) breadboard_enabled: bool,
    pub(crate) status: &'a str,
}

pub(crate) enum BottomDockAction {
    SetTab(BottomDockTab),
    Validation(ValidationPanelAction),
    OpenBreadboard,
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
                (BottomDockTab::Erc, erc_tab_label(model.violations)),
                (BottomDockTab::Simulation, "Simulation".to_string()),
                (BottomDockTab::Breadboard, "Breadboard".to_string()),
                (BottomDockTab::Logs, "Logs".to_string()),
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
