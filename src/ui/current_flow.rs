use crate::engine::mna::DcResult;
use crate::model::Wire;
use crate::ui::app::CanvasView;
use crate::ui::theme;
use egui::{Color32, Painter, Pos2, Rect, Stroke};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlowQuality {
    Low,
    Normal,
    High,
}

impl FlowQuality {
    pub(crate) fn fps(self) -> u64 {
        match self {
            Self::Low => 15,
            Self::Normal => 30,
            Self::High => 60,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CurrentFlowSettings {
    pub(crate) enabled: bool,
    pub(crate) speed_multiplier: f32,
    pub(crate) quality: FlowQuality,
    pub(crate) show_tail: bool,
    pub(crate) minimum_visible_current_a: f64,
}

impl Default for CurrentFlowSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            speed_multiplier: 1.0,
            quality: FlowQuality::Normal,
            show_tail: true,
            minimum_visible_current_a: 1.0e-9,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FlowSegment {
    pub(crate) start: Pos2,
    pub(crate) end: Pos2,
    pub(crate) start_distance: f32,
    pub(crate) length: f32,
    pub(crate) signed_current_a: Option<f64>,
}

#[derive(Debug, Clone)]
pub(crate) struct WireFlowGeometry {
    pub(crate) wire_id: u64,
    pub(crate) segments: Vec<FlowSegment>,
    pub(crate) total_length: f32,
    pub(crate) world_bounds: Rect,
}

#[derive(Default)]
pub(crate) struct CurrentFlowCache {
    revision: u64,
    pub(crate) wires: Vec<WireFlowGeometry>,
}

impl CurrentFlowCache {
    pub(crate) fn rebuild_if_needed(
        &mut self,
        revision: u64,
        wires: &[Wire],
        dc: Option<&DcResult>,
    ) {
        if self.revision == revision {
            return;
        }
        self.revision = revision;
        self.wires.clear();
        let Some(dc) = dc else { return };
        for wire in wires {
            let known = dc.wire_current_known.contains(&wire.id);
            let fallback = known
                .then(|| dc.wire_current.get(&wire.id).copied())
                .flatten();
            let mut distance = 0.0;
            let mut segments = Vec::new();
            let mut bounds = Rect::NOTHING;
            for (index, pair) in wire.points.windows(2).enumerate() {
                let (start, end) = (pair[0], pair[1]);
                let length = start.distance(end);
                if !length.is_finite() || length <= 0.01 || !start.is_finite() || !end.is_finite() {
                    continue;
                }
                let segment_id = wire
                    .id
                    .saturating_mul(1_000)
                    .saturating_add(index as u64 + 1);
                let current = dc
                    .wire_segment_currents
                    .get(&segment_id)
                    .copied()
                    .or(fallback);
                segments.push(FlowSegment {
                    start,
                    end,
                    start_distance: distance,
                    length,
                    signed_current_a: current.filter(|v| v.is_finite()),
                });
                distance += length;
                bounds.extend_with(start);
                bounds.extend_with(end);
            }
            if distance > 0.01 {
                self.wires.push(WireFlowGeometry {
                    wire_id: wire.id,
                    segments,
                    total_length: distance,
                    world_bounds: bounds,
                });
            }
        }
    }
}

pub(crate) struct FlowRenderInput<'a> {
    pub(crate) painter: &'a Painter,
    pub(crate) viewport: Rect,
    pub(crate) view: CanvasView,
    pub(crate) time_seconds: f64,
    pub(crate) settings: &'a CurrentFlowSettings,
    pub(crate) selected_wire: Option<u64>,
    pub(crate) highlighted_wires: &'a std::collections::HashSet<u64>,
}

/// Returns true only when moving particles were actually rendered.
pub(crate) fn render_current_flow(cache: &CurrentFlowCache, input: FlowRenderInput<'_>) -> bool {
    if !input.settings.enabled || input.view.zoom < 0.12 {
        return false;
    }
    let static_only = input.view.zoom < 0.32;
    let mut animated = false;
    for wire in &cache.wires {
        let screen_bounds = Rect::from_two_pos(
            input.view.to_screen(wire.world_bounds.min),
            input.view.to_screen(wire.world_bounds.max),
        )
        .expand(8.0);
        if !screen_bounds.intersects(input.viewport) {
            continue;
        }
        let current = wire.segments.first().and_then(|s| s.signed_current_a);
        let Some(current) = current else { continue };
        if current.abs() < input.settings.minimum_visible_current_a || !current.is_finite() {
            continue;
        }
        let emphasized = input.selected_wire == Some(wire.wire_id)
            || input.highlighted_wires.contains(&wire.wire_id);
        let level = ((current.abs() / input.settings.minimum_visible_current_a)
            .max(1.0)
            .log10()
            / 7.0)
            .clamp(0.0, 1.0) as f32;
        let alpha = if emphasized {
            150
        } else {
            (45.0 + 70.0 * level) as u8
        };
        for segment in &wire.segments {
            input.painter.line_segment(
                [
                    input.view.to_screen(segment.start),
                    input.view.to_screen(segment.end),
                ],
                Stroke::new(
                    if emphasized { 4.5 } else { 3.2 },
                    theme::CURRENT_GLOW.gamma_multiply(alpha as f32 / 255.0),
                ),
            );
        }
        if static_only {
            continue;
        }
        animated = true;
        let screen_length = wire.total_length * input.view.zoom;
        let spacing = match input.settings.quality {
            FlowQuality::Low => 92.0,
            FlowQuality::Normal => 68.0,
            FlowQuality::High => 54.0,
        };
        let count = ((screen_length / spacing).ceil() as usize).clamp(1, 180);
        let speed = (22.0 + 58.0 * level).min(90.0) * input.settings.speed_multiplier;
        let direction = current.signum() as f32;
        for particle in 0..count {
            let base = particle as f32 * wire.total_length / count as f32;
            let d = (base
                + direction * input.time_seconds as f32 * speed / input.view.zoom.max(0.1))
            .rem_euclid(wire.total_length);
            if let Some((pos, tangent)) = point_and_tangent(wire, d) {
                let screen = input.view.to_screen(pos);
                let dir = tangent.normalized() * direction;
                let color = theme::CURRENT_PARTICLE.gamma_multiply(if emphasized {
                    1.0
                } else {
                    0.65 + 0.35 * level
                });
                if input.settings.show_tail {
                    input.painter.line_segment(
                        [screen - dir * (7.0 + 5.0 * level), screen],
                        Stroke::new(1.5 + level, color.gamma_multiply(0.55)),
                    );
                }
                input.painter.circle_filled(screen, 2.0 + level, color);
            }
        }
    }
    animated
}

fn point_and_tangent(wire: &WireFlowGeometry, distance: f32) -> Option<(Pos2, egui::Vec2)> {
    let segment = wire
        .segments
        .iter()
        .find(|s| distance <= s.start_distance + s.length)
        .or_else(|| wire.segments.last())?;
    let t = ((distance - segment.start_distance) / segment.length).clamp(0.0, 1.0);
    Some((
        segment.start.lerp(segment.end, t),
        segment.end - segment.start,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zero_length_segments_are_ignored() {
        let mut cache = CurrentFlowCache::default();
        let wire = Wire::new(7, vec![Pos2::ZERO, Pos2::ZERO]);
        let mut dc = DcResult::default();
        dc.wire_current.insert(7, 0.01);
        dc.wire_current_known.insert(7);
        cache.rebuild_if_needed(1, &[wire], Some(&dc));
        assert!(cache.wires.is_empty());
    }
}
