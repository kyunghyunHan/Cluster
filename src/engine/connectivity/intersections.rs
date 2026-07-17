use egui::Pos2;

const EPSILON: f32 = 0.001;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::engine) enum SegmentIntersection {
    None,
    EndpointTouch(Pos2),
    TJunction(Pos2),
    Crossing(Pos2),
    CollinearOverlap,
}

pub(in crate::engine) fn classify(a0: Pos2, a1: Pos2, b0: Pos2, b1: Pos2) -> SegmentIntersection {
    let a = a1 - a0;
    let b = b1 - b0;
    let denominator = cross(a, b);
    let offset = b0 - a0;
    if denominator.abs() <= EPSILON {
        if cross(offset, a).abs() > EPSILON {
            return SegmentIntersection::None;
        }
        let axis_x = a.x.abs() >= a.y.abs();
        let project = |point: Pos2| if axis_x { point.x } else { point.y };
        let (a_min, a_max) = ordered(project(a0), project(a1));
        let (b_min, b_max) = ordered(project(b0), project(b1));
        let overlap_min = a_min.max(b_min);
        let overlap_max = a_max.min(b_max);
        if overlap_max < overlap_min - EPSILON {
            SegmentIntersection::None
        } else if (overlap_max - overlap_min).abs() <= EPSILON {
            let point = [a0, a1, b0, b1]
                .into_iter()
                .min_by(|left, right| {
                    (project(*left) - overlap_min)
                        .abs()
                        .total_cmp(&(project(*right) - overlap_min).abs())
                })
                .unwrap_or(a0);
            SegmentIntersection::EndpointTouch(point)
        } else {
            SegmentIntersection::CollinearOverlap
        }
    } else {
        let t = cross(offset, b) / denominator;
        let u = cross(offset, a) / denominator;
        if !(-EPSILON..=1.0 + EPSILON).contains(&t) || !(-EPSILON..=1.0 + EPSILON).contains(&u) {
            return SegmentIntersection::None;
        }
        let point = a0 + a * t;
        let a_endpoint = near_endpoint(t);
        let b_endpoint = near_endpoint(u);
        match (a_endpoint, b_endpoint) {
            (true, true) => SegmentIntersection::EndpointTouch(point),
            (true, false) | (false, true) => SegmentIntersection::TJunction(point),
            (false, false) => SegmentIntersection::Crossing(point),
        }
    }
}

fn cross(left: egui::Vec2, right: egui::Vec2) -> f32 {
    left.x * right.y - left.y * right.x
}

fn ordered(left: f32, right: f32) -> (f32, f32) {
    (left.min(right), left.max(right))
}

fn near_endpoint(parameter: f32) -> bool {
    parameter.abs() <= EPSILON || (parameter - 1.0).abs() <= EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_crossing_t_junction_and_overlap() {
        assert!(matches!(
            classify(
                Pos2::new(0.0, 0.0),
                Pos2::new(10.0, 0.0),
                Pos2::new(5.0, -5.0),
                Pos2::new(5.0, 5.0)
            ),
            SegmentIntersection::Crossing(_)
        ));
        assert!(matches!(
            classify(
                Pos2::new(0.0, 0.0),
                Pos2::new(10.0, 0.0),
                Pos2::new(5.0, -5.0),
                Pos2::new(5.0, 0.0)
            ),
            SegmentIntersection::TJunction(_)
        ));
        assert_eq!(
            classify(
                Pos2::new(0.0, 0.0),
                Pos2::new(10.0, 0.0),
                Pos2::new(5.0, 0.0),
                Pos2::new(15.0, 0.0)
            ),
            SegmentIntersection::CollinearOverlap
        );
    }
}
