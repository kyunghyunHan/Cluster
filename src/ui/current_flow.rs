use crate::engine::mna::DcResult;
use crate::model::{Wire, WireSegmentId};
use crate::ui::canvas::CanvasView;
use crate::ui::theme;
use egui::{Painter, Pos2, Rect, Stroke};

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
    pub(crate) max_particles_per_frame: usize,
}

impl Default for CurrentFlowSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            speed_multiplier: 1.0,
            quality: FlowQuality::Normal,
            show_tail: true,
            minimum_visible_current_a: 1.0e-9,
            max_particles_per_frame: 3_000,
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

#[allow(dead_code)] // Raw geometry is retained for diagnostics and regression tests.
#[derive(Debug, Clone)]
pub(crate) struct WireFlowGeometry {
    pub(crate) wire_id: u64,
    pub(crate) segments: Vec<FlowSegment>,
    pub(crate) total_length: f32,
    pub(crate) world_bounds: Rect,
    pub(crate) runs: Vec<FlowRun>,
}

#[derive(Debug, Clone)]
pub(crate) struct FlowRun {
    pub(crate) segments: Vec<FlowSegment>,
    pub(crate) signed_current_a: f64,
    pub(crate) total_length: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct FlowCacheKey {
    pub(crate) geometry_revision: u64,
    pub(crate) simulation_revision: u64,
}

#[derive(Default)]
pub(crate) struct CurrentFlowCache {
    key: FlowCacheKey,
    pub(crate) wires: Vec<WireFlowGeometry>,
}

impl CurrentFlowCache {
    pub(crate) fn rebuild_if_needed(
        &mut self,
        key: FlowCacheKey,
        wires: &[Wire],
        dc: Option<&DcResult>,
    ) -> bool {
        if self.key == key {
            return false;
        }
        self.key = key;
        self.wires.clear();
        let Some(dc) = dc else { return true };
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
                let segment_id = WireSegmentId::new(wire.id, index);
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
                let runs = build_flow_runs(&segments);
                self.wires.push(WireFlowGeometry {
                    wire_id: wire.id,
                    segments,
                    total_length: distance,
                    world_bounds: bounds,
                    runs,
                });
            }
        }
        true
    }
}

fn build_flow_runs(segments: &[FlowSegment]) -> Vec<FlowRun> {
    let mut runs: Vec<FlowRun> = Vec::new();
    for segment in segments {
        let Some(current) = segment
            .signed_current_a
            .filter(|current| current.is_finite())
        else {
            continue;
        };
        let can_extend = runs.last().is_some_and(|run| {
            let tolerance = current.abs().max(run.signed_current_a.abs()).max(1.0) * 1.0e-9;
            (run.signed_current_a - current).abs() <= tolerance
                && run
                    .segments
                    .last()
                    .is_some_and(|previous| previous.end.distance(segment.start) <= 0.01)
        });
        if can_extend {
            let run = runs.last_mut().expect("checked above");
            let mut relative = segment.clone();
            relative.start_distance = run.total_length;
            run.total_length += relative.length;
            run.segments.push(relative);
        } else {
            let mut first = segment.clone();
            first.start_distance = 0.0;
            runs.push(FlowRun {
                segments: vec![first],
                signed_current_a: current,
                total_length: segment.length,
            });
        }
    }
    runs
}

#[cfg(test)]
impl WireFlowGeometry {
    /// A particle may traverse the whole polyline only when every drawable
    /// segment has the same solved signed current. Mixed/unknown currents are
    /// intentionally not collapsed into a misleading wire-wide direction.
    pub(crate) fn uniform_signed_current(&self) -> Option<f64> {
        let first = self.segments.first()?.signed_current_a?;
        let tolerance = first.abs().max(1.0) * 1.0e-9;
        self.segments
            .iter()
            .all(|segment| {
                segment
                    .signed_current_a
                    .is_some_and(|current| (current - first).abs() <= tolerance)
            })
            .then_some(first)
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
    pub(crate) startup_progress: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FlowRenderStats {
    pub(crate) needs_repaint: bool,
    pub(crate) particle_count: usize,
    pub(crate) visible_wire_count: usize,
}

pub(crate) fn render_current_flow(
    cache: &CurrentFlowCache,
    input: FlowRenderInput<'_>,
) -> FlowRenderStats {
    if !input.settings.enabled || input.view.zoom < 0.12 {
        return FlowRenderStats::default();
    }
    let static_only = input.view.zoom < 0.32;
    let mut animated = false;
    let mut particle_count = 0;
    let mut visible_wire_count = 0;
    let mut wires = cache.wires.iter().collect::<Vec<_>>();
    wires.sort_by_key(|wire| {
        !(input.selected_wire == Some(wire.wire_id)
            || input.highlighted_wires.contains(&wire.wire_id))
    });
    for wire in wires {
        let screen_bounds = Rect::from_two_pos(
            input.view.to_screen(wire.world_bounds.min),
            input.view.to_screen(wire.world_bounds.max),
        )
        .expand(8.0);
        if !screen_bounds.intersects(input.viewport) {
            continue;
        }
        visible_wire_count += 1;
        let emphasized = input.selected_wire == Some(wire.wire_id)
            || input.highlighted_wires.contains(&wire.wire_id);
        for run in &wire.runs {
            let current = run.signed_current_a;
            if !current_is_visible(input.settings, current) {
                continue;
            }
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
            for segment in &run.segments {
                input.painter.line_segment(
                    [
                        input.view.to_screen(segment.start),
                        input.view.to_screen(segment.end),
                    ],
                    Stroke::new(
                        if emphasized { 4.5 } else { 3.2 },
                        theme::CURRENT_GLOW
                            .gamma_multiply(alpha as f32 / 255.0 * input.startup_progress),
                    ),
                );
            }
            if static_only
                || input.startup_progress < 0.55
                || particle_count >= input.settings.max_particles_per_frame
            {
                continue;
            }
            animated = true;
            let spacing = match input.settings.quality {
                FlowQuality::Low => 92.0,
                FlowQuality::Normal => 68.0,
                FlowQuality::High => 54.0,
            };
            let desired =
                ((run.total_length * input.view.zoom / spacing).ceil() as usize).clamp(1, 180);
            let count = desired.min(input.settings.max_particles_per_frame - particle_count);
            particle_count += count;
            let speed = (22.0 + 58.0 * level).min(90.0) * input.settings.speed_multiplier;
            let direction = current.signum() as f32;
            for particle in 0..count {
                let base = particle as f32 * run.total_length / count as f32;
                let d = (base
                    + direction * input.time_seconds as f32 * speed / input.view.zoom.max(0.1))
                .rem_euclid(run.total_length);
                if let Some((pos, tangent)) = point_and_tangent(&run.segments, d) {
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
    }
    FlowRenderStats {
        needs_repaint: animated || (input.startup_progress < 1.0 && visible_wire_count > 0),
        particle_count,
        visible_wire_count,
    }
}

pub(crate) fn current_is_visible(settings: &CurrentFlowSettings, current: f64) -> bool {
    settings.enabled && current.is_finite() && current.abs() >= settings.minimum_visible_current_a
}

fn point_and_tangent(segments: &[FlowSegment], distance: f32) -> Option<(Pos2, egui::Vec2)> {
    let segment = segments
        .iter()
        .find(|s| distance <= s.start_distance + s.length)
        .or_else(|| segments.last())?;
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
        cache.rebuild_if_needed(
            FlowCacheKey {
                geometry_revision: 1,
                simulation_revision: 1,
            },
            &[wire],
            Some(&dc),
        );
        assert!(cache.wires.is_empty());
    }

    #[test]
    fn mixed_segment_currents_do_not_become_one_wire_direction() {
        let wire = WireFlowGeometry {
            wire_id: 1,
            segments: vec![
                FlowSegment {
                    start: Pos2::ZERO,
                    end: Pos2::new(10.0, 0.0),
                    start_distance: 0.0,
                    length: 10.0,
                    signed_current_a: Some(0.01),
                },
                FlowSegment {
                    start: Pos2::new(10.0, 0.0),
                    end: Pos2::new(20.0, 0.0),
                    start_distance: 10.0,
                    length: 10.0,
                    signed_current_a: Some(0.005),
                },
            ],
            total_length: 20.0,
            world_bounds: Rect::from_min_max(Pos2::ZERO, Pos2::new(20.0, 0.0)),
            runs: Vec::new(),
        };
        assert_eq!(wire.uniform_signed_current(), None);
    }

    #[test]
    fn cache_does_not_rebuild_for_animation_only_frames() {
        let mut cache = CurrentFlowCache::default();
        let wire = Wire::new(3, vec![Pos2::ZERO, Pos2::new(20.0, 0.0)]);
        let mut dc = DcResult::default();
        dc.wire_current.insert(3, 0.01);
        dc.wire_current_known.insert(3);
        dc.wire_segment_currents
            .insert(WireSegmentId::new(3, 0), 0.01);
        let key = FlowCacheKey {
            geometry_revision: 9,
            simulation_revision: 2,
        };
        cache.rebuild_if_needed(key, std::slice::from_ref(&wire), Some(&dc));
        let original_length = cache.wires[0].total_length;
        let changed_wire = Wire::new(3, vec![Pos2::ZERO, Pos2::new(200.0, 0.0)]);
        cache.rebuild_if_needed(key, &[changed_wire], Some(&dc));
        assert_eq!(cache.wires[0].total_length, original_length);
    }

    #[test]
    fn closed_loop_current_is_visible_but_open_current_is_not() {
        let settings = CurrentFlowSettings::default();
        assert!(current_is_visible(&settings, 0.009));
        assert!(!current_is_visible(&settings, 0.0));
    }

    #[test]
    fn reversed_source_preserves_negative_particle_direction() {
        let geometry = WireFlowGeometry {
            wire_id: 5,
            segments: vec![FlowSegment {
                start: Pos2::ZERO,
                end: Pos2::new(20.0, 0.0),
                start_distance: 0.0,
                length: 20.0,
                signed_current_a: Some(-0.004),
            }],
            total_length: 20.0,
            world_bounds: Rect::from_min_max(Pos2::ZERO, Pos2::new(20.0, 0.0)),
            runs: Vec::new(),
        };
        assert!(
            geometry
                .uniform_signed_current()
                .is_some_and(|current| current < 0.0)
        );
    }

    #[test]
    fn parallel_branches_keep_independent_magnitudes() {
        let branch = |id, current| WireFlowGeometry {
            wire_id: id,
            segments: vec![FlowSegment {
                start: Pos2::ZERO,
                end: Pos2::new(20.0, 0.0),
                start_distance: 0.0,
                length: 20.0,
                signed_current_a: Some(current),
            }],
            total_length: 20.0,
            world_bounds: Rect::from_min_max(Pos2::ZERO, Pos2::new(20.0, 0.0)),
            runs: Vec::new(),
        };
        assert_eq!(branch(1, 0.010).uniform_signed_current(), Some(0.010));
        assert_eq!(branch(2, 0.005).uniform_signed_current(), Some(0.005));
    }

    #[test]
    fn disabled_animation_suppresses_even_finite_current() {
        let settings = CurrentFlowSettings {
            enabled: false,
            ..CurrentFlowSettings::default()
        };
        assert!(!current_is_visible(&settings, 0.01));
    }

    #[test]
    fn segment_keys_do_not_collide_after_one_thousand_segments() {
        assert_ne!(WireSegmentId::new(1, 1_001), WireSegmentId::new(2, 1));
    }

    #[test]
    fn mixed_known_segments_form_independent_flow_runs() {
        let segments = vec![
            FlowSegment {
                start: Pos2::ZERO,
                end: Pos2::new(10.0, 0.0),
                start_distance: 0.0,
                length: 10.0,
                signed_current_a: Some(0.01),
            },
            FlowSegment {
                start: Pos2::new(10.0, 0.0),
                end: Pos2::new(20.0, 0.0),
                start_distance: 10.0,
                length: 10.0,
                signed_current_a: Some(0.005),
            },
            FlowSegment {
                start: Pos2::new(20.0, 0.0),
                end: Pos2::new(30.0, 0.0),
                start_distance: 20.0,
                length: 10.0,
                signed_current_a: None,
            },
        ];
        let runs = build_flow_runs(&segments);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].signed_current_a, 0.01);
        assert_eq!(runs[1].signed_current_a, 0.005);
    }

    #[test]
    fn particle_budget_defaults_to_three_thousand() {
        assert_eq!(
            CurrentFlowSettings::default().max_particles_per_frame,
            3_000
        );
    }
}
