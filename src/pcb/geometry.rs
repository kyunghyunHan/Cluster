//! Shared closed-copper geometry; distances are in millimeters.
use crate::model::cad::Point2;
use crate::pcb::footprint::{FootprintTransform, Pad, PadShape};

/// Conservative copper bounds, used only to reject distant exact checks.
#[derive(Clone, Copy)]
pub(crate) struct CopperBounds {
    min: Point2,
    max: Point2,
}

impl CopperBounds {
    pub(crate) fn segment(a: Point2, b: Point2, radius: f32) -> Self {
        Self {
            min: Point2::new(a.x.min(b.x) - radius, a.y.min(b.y) - radius),
            max: Point2::new(a.x.max(b.x) + radius, a.y.max(b.y) + radius),
        }
    }

    pub(crate) fn pad(pad: &Pad, transform: FootprintTransform) -> Self {
        let (w, h) = if pad.shape == PadShape::Circle {
            (pad.size.w, pad.size.w)
        } else {
            (pad.size.w, pad.size.h)
        };
        let corners = [(-w, -h), (-w, h), (w, -h), (w, h)].map(|(x, y)| {
            transform.local_to_board(Point2::new(
                pad.position.x + x * 0.5,
                pad.position.y + y * 0.5,
            ))
        });
        Self {
            min: Point2::new(
                corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min),
                corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min),
            ),
            max: Point2::new(
                corners
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::NEG_INFINITY, f32::max),
                corners
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max),
            ),
        }
    }

    pub(crate) fn may_touch(self, other: Self, clearance: f32) -> bool {
        let values = [
            self.min.x,
            self.min.y,
            self.max.x,
            self.max.y,
            other.min.x,
            other.min.y,
            other.max.x,
            other.max.y,
            clearance,
        ];
        if values.iter().any(|v| !v.is_finite()) {
            return true;
        }
        // Keep rounding at large offsets from turning a real violation into
        // a rejected candidate. Invalid geometry is left to the exact checks.
        let rounding =
            values.iter().map(|v| v.abs()).fold(1.0_f32, f32::max) * f32::EPSILON * 8.0 + 1e-5;
        let margin = clearance.max(0.0) + rounding;
        self.min.x <= other.max.x + margin
            && other.min.x <= self.max.x + margin
            && self.min.y <= other.max.y + margin
            && other.min.y <= self.max.y + margin
    }
}

pub(crate) fn point_segment_distance(p: Point2, a: Point2, b: Point2) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 {
        ((p.x - a.x) * dx + (p.y - a.y) * dy) / length
    } else {
        0.0
    }
    .clamp(0.0, 1.0);
    (p.x - a.x - dx * t).hypot(p.y - a.y - dy * t)
}

pub(crate) fn segment_distance(a: Point2, b: Point2, c: Point2, d: Point2) -> f32 {
    let cross =
        |p: Point2, q: Point2, r: Point2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let o1 = cross(a, b, c);
    let o2 = cross(a, b, d);
    let o3 = cross(c, d, a);
    let o4 = cross(c, d, b);
    if o1 * o2 < 0.0 && o3 * o4 < 0.0 {
        return 0.0;
    }
    point_segment_distance(a, c, d)
        .min(point_segment_distance(b, c, d))
        .min(point_segment_distance(c, a, b))
        .min(point_segment_distance(d, a, b))
}

pub(crate) fn inside_polygon(points: &[Point2], p: Point2) -> bool {
    if points.len() < 3 {
        return false;
    }
    let mut inside = false;
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        if point_segment_distance(p, *a, *b) <= 1e-6 {
            return true;
        }
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
            inside = !inside;
        }
    }
    inside
}

pub(crate) fn edge_distance(points: &[Point2], a: Point2, b: Point2) -> f32 {
    points
        .windows(2)
        .map(|edge| segment_distance(a, b, edge[0], edge[1]))
        .fold(f32::INFINITY, f32::min)
}

/// Distance from a segment to the pad's copper, without track width.
pub(crate) fn pad_segment_distance(
    pad: &Pad,
    transform: FootprintTransform,
    a: Point2,
    b: Point2,
) -> f32 {
    let local = |p| {
        let p = transform.board_to_local(p);
        Point2::new(p.x - pad.position.x, p.y - pad.position.y)
    };
    let a = local(a);
    let b = local(b);
    let half_w = pad.size.w * 0.5;
    let half_h = pad.size.h * 0.5;
    match pad.shape {
        PadShape::Circle => (point_segment_distance(Point2::new(0.0, 0.0), a, b) - half_w).max(0.0),
        PadShape::Oval => {
            let radius = half_w.min(half_h);
            let delta = Point2::new((half_w - radius).max(0.0), (half_h - radius).max(0.0));
            (segment_distance(a, b, Point2::new(-delta.x, -delta.y), delta) - radius).max(0.0)
        }
        // RoundRect lacks a radius in the model. Its enclosing rectangle is
        // conservative for DRC; fabrication is blocked for this shape.
        PadShape::Rect | PadShape::RoundRect => {
            let contains = |p: Point2| p.x.abs() <= half_w && p.y.abs() <= half_h;
            if contains(a) || contains(b) {
                return 0.0;
            }
            let corners = [
                Point2::new(-half_w, -half_h),
                Point2::new(half_w, -half_h),
                Point2::new(half_w, half_h),
                Point2::new(-half_w, half_h),
            ];
            corners
                .iter()
                .zip(corners.iter().cycle().skip(1))
                .take(4)
                .map(|(c, d)| segment_distance(a, b, *c, *d))
                .fold(f32::INFINITY, f32::min)
        }
    }
}

pub(crate) fn pad_distance(
    a: &Pad,
    ta: FootprintTransform,
    b: &Pad,
    tb: FootprintTransform,
) -> f32 {
    fn core(p: &Pad, t: FootprintTransform) -> ([Point2; 4], f32) {
        let radius = match p.shape {
            PadShape::Circle => p.size.w * 0.5,
            PadShape::Oval => p.size.w.min(p.size.h) * 0.5,
            _ => 0.0,
        };
        let w = (p.size.w * 0.5 - radius).max(0.0);
        let h = (p.size.h * 0.5 - radius).max(0.0);
        (
            [(-w, -h), (w, -h), (w, h), (-w, h)]
                .map(|(x, y)| t.local_to_board(Point2::new(p.position.x + x, p.position.y + y))),
            radius,
        )
    }
    let (ca, ra) = core(a, ta);
    let (cb, rb) = core(b, tb);
    let mut distance = f32::INFINITY;
    for (a1, a2) in ca.iter().zip(ca.iter().cycle().skip(1)).take(4) {
        for (b1, b2) in cb.iter().zip(cb.iter().cycle().skip(1)).take(4) {
            distance = distance.min(segment_distance(*a1, *a2, *b1, *b2));
        }
    }
    if inside_polygon(&ca, cb[0]) || inside_polygon(&cb, ca[0]) {
        distance = 0.0;
    }
    (distance - ra - rb).max(0.0)
}
