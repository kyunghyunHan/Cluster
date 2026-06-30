use crate::engine::validation::{ErcAutoFix, ErcRelated, ErcSeverity, ErcViolation};
use eframe::egui;
use egui::{Color32, Sense};

pub(crate) enum ValidationPanelAction {
    SelectComponent(u64),
    SelectWire(u64),
    ApplyAutoFix(ErcAutoFix),
}

pub(crate) fn render_validation_panel(
    ui: &mut egui::Ui,
    violations: &[ErcViolation],
    has_components: bool,
) -> Option<ValidationPanelAction> {
    if violations.is_empty() {
        if has_components {
            ui.label(
                egui::RichText::new("No violations found.")
                    .size(11.0)
                    .color(Color32::from_rgb(120, 200, 140)),
            );
        } else {
            ui.label(
                egui::RichText::new("Place components to run ERC.")
                    .size(11.0)
                    .color(Color32::from_rgb(120, 130, 140)),
            );
        }
        return None;
    }

    let mut action = None;
    egui::ScrollArea::vertical()
        .max_height(160.0)
        .show(ui, |ui| {
            for severity in [ErcSeverity::Error, ErcSeverity::Warning, ErcSeverity::Info] {
                let count = violations
                    .iter()
                    .filter(|violation| violation.severity == severity)
                    .count();
                if count == 0 {
                    continue;
                }
                let (_, color, label) = severity_style(severity);
                ui.label(
                    egui::RichText::new(format!("{label} ({count})"))
                        .size(10.0)
                        .strong()
                        .color(color),
                );
                for violation in violations
                    .iter()
                    .filter(|violation| violation.severity == severity)
                {
                    if let Some(row_action) = render_violation_row(ui, violation) {
                        action = Some(row_action);
                    }
                }
                ui.add_space(4.0);
            }
        });
    action
}

fn render_violation_row(
    ui: &mut egui::Ui,
    violation: &ErcViolation,
) -> Option<ValidationPanelAction> {
    let (icon, col, _) = severity_style(violation.severity);
    let rule_id = violation.rule_id();
    let target = match violation.related() {
        ErcRelated::Component(id) => format!("component #{id}"),
        ErcRelated::Wire(id) => format!("wire #{id}"),
        ErcRelated::ComponentAndWire {
            component_id,
            wire_id,
        } => format!("component #{component_id}, wire #{wire_id}"),
        ErcRelated::Schematic => "schematic".to_string(),
    };

    let mut action = None;
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::Label::new(
                egui::RichText::new(format!("{icon} {}", violation.message))
                    .size(10.5)
                    .color(col),
            )
            .sense(Sense::click()),
        );
        if resp.clicked() {
            if let Some(id) = violation.component_id {
                action = Some(ValidationPanelAction::SelectComponent(id));
            } else if let Some(id) = violation.wire_id {
                action = Some(ValidationPanelAction::SelectWire(id));
            }
        }
        resp.on_hover_text(format!(
            "{rule_id}\nTarget: {target}\n\nWhy this matters: {}",
            violation.explanation()
        ));

        if let Some(auto_fix) = violation.auto_fix()
            && ui
                .small_button(egui::RichText::new("Auto fix").size(10.0))
                .on_hover_text("Insert a safe helper part or note without rewiring existing nets.")
                .clicked()
        {
            action = Some(ValidationPanelAction::ApplyAutoFix(auto_fix));
        }
    });
    ui.label(
        egui::RichText::new(format!("  Why: {}", violation.explanation()))
            .size(10.0)
            .color(Color32::from_rgb(135, 150, 165)),
    );
    if let Some(suggestion) = violation.fix_hint() {
        ui.label(
            egui::RichText::new(format!("  Fix: {suggestion}"))
                .size(10.0)
                .color(Color32::from_rgb(155, 170, 185)),
        );
    }
    action
}

fn severity_style(severity: ErcSeverity) -> (&'static str, Color32, &'static str) {
    match severity {
        ErcSeverity::Error => ("✗", Color32::from_rgb(255, 110, 95), "Errors"),
        ErcSeverity::Warning => ("⚠", Color32::from_rgb(255, 200, 80), "Warnings"),
        ErcSeverity::Info => ("i", Color32::from_rgb(130, 170, 210), "Info"),
    }
}
