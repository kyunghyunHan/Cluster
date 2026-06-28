use crate::engine::validation::{ErcAutoFix, ErcSeverity, ErcViolation};
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
            for violation in violations {
                let (icon, col) = match violation.severity {
                    ErcSeverity::Error => ("✗", Color32::from_rgb(255, 110, 95)),
                    ErcSeverity::Warning => ("⚠", Color32::from_rgb(255, 200, 80)),
                    ErcSeverity::Info => ("i", Color32::from_rgb(130, 170, 210)),
                };
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
                    resp.on_hover_text(&violation.message);

                    if let Some(auto_fix) = violation.auto_fix()
                        && ui
                            .small_button(egui::RichText::new("Auto fix").size(10.0))
                            .on_hover_text(
                                "Insert a safe helper part or note without rewiring existing nets.",
                            )
                            .clicked()
                    {
                        action = Some(ValidationPanelAction::ApplyAutoFix(auto_fix));
                    }
                });
                if let Some(suggestion) = violation.fix_suggestion() {
                    ui.label(
                        egui::RichText::new(format!("  Fix: {suggestion}"))
                            .size(10.0)
                            .color(Color32::from_rgb(155, 170, 185)),
                    );
                }
            }
        });
    action
}
