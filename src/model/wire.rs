use egui::Pos2;

#[derive(Debug, Clone)]
pub(crate) struct Wire {
    pub(crate) id: u64,
    pub(crate) points: Vec<Pos2>,
}
