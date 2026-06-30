use eframe::egui;
use egui::Color32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageTabsAction {
    SwitchTo(usize),
    RenameDefault(usize),
    AddPage,
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
                .color(Color32::from_rgb(120, 130, 140)),
        );
        for (idx, name) in page_names.iter().enumerate() {
            let active = idx == current_page;
            let btn_color = if active {
                Color32::from_rgb(80, 180, 130)
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
