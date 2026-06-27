use egui::Pos2;

#[derive(Debug, Clone)]
pub(crate) struct Wire {
    pub(crate) id: u64,
    pub(crate) points: Vec<Pos2>,
}

pub(crate) fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let ab_len_sq = ab.x * ab.x + ab.y * ab.y;
    if ab_len_sq == 0.0 {
        return ap.length();
    }
    let t = ((ap.x * ab.x) + (ap.y * ab.y)) / ab_len_sq;
    let t = t.clamp(0.0, 1.0);
    let closest = a + ab * t;
    (p - closest).length()
}

pub(crate) fn point_touches_wire_segment(point: Pos2, a: Pos2, b: Pos2) -> bool {
    distance_to_segment(point, a, b) <= 1.0
}
