use super::*;

pub(crate) fn snap_pos(pos: Pos2, _rect: Rect, grid: f32, snap: bool) -> Pos2 {
    if snap {
        Pos2::new((pos.x / grid).round() * grid, (pos.y / grid).round() * grid)
    } else {
        pos
    }
}

pub(crate) fn insert_wire_control_point(pos: Pos2, wires: &mut [Wire]) -> Option<(u64, usize)> {
    let threshold = 10.0;
    for wire in wires.iter_mut().rev() {
        for index in 0..wire.points.len().saturating_sub(1) {
            let a = wire.points[index];
            let b = wire.points[index + 1];
            if distance_to_segment(pos, a, b) <= threshold {
                let horizontal = (a.y - b.y).abs() <= 0.5;
                let vertical = (a.x - b.x).abs() <= 0.5;
                let inserted = if horizontal {
                    Pos2::new(pos.x.clamp(a.x.min(b.x), a.x.max(b.x)), a.y)
                } else if vertical {
                    Pos2::new(a.x, pos.y.clamp(a.y.min(b.y), a.y.max(b.y)))
                } else {
                    closest_point_on_segment(pos, a, b)
                };
                wire.points.insert(index + 1, inserted);
                return Some((wire.id, index + 1));
            }
        }
    }
    None
}

pub(crate) fn move_wire_control_point(
    wires: &mut [Wire],
    wire_id: u64,
    point_index: usize,
    pos: Pos2,
) {
    let Some(wire) = wires.iter_mut().find(|wire| wire.id == wire_id) else {
        return;
    };
    if point_index >= wire.points.len() {
        return;
    }
    wire.points[point_index] = pos;
    let is_endpoint = point_index == 0 || point_index + 1 == wire.points.len();
    if !is_endpoint {
        straighten_neighbor_segments(wire, point_index);
    }
}

pub(crate) fn straighten_neighbor_segments(wire: &mut Wire, point_index: usize) {
    let point = wire.points[point_index];
    if point_index > 0 {
        let prev = wire.points[point_index - 1];
        let dx = (point.x - prev.x).abs();
        let dy = (point.y - prev.y).abs();
        if dx <= dy {
            wire.points[point_index - 1].x = point.x;
        } else {
            wire.points[point_index - 1].y = point.y;
        }
    }
    if point_index + 1 < wire.points.len() {
        let next = wire.points[point_index + 1];
        let dx = (point.x - next.x).abs();
        let dy = (point.y - next.y).abs();
        if dx <= dy {
            wire.points[point_index + 1].x = point.x;
        } else {
            wire.points[point_index + 1].y = point.y;
        }
    }
}

pub(crate) fn is_connection_point(pos: Pos2, components: &[Component], wires: &[Wire]) -> bool {
    for component in components {
        for pin in component_pin_defs(component) {
            if pin.pos.distance(pos) < 6.0 {
                return true;
            }
        }
    }
    for wire in wires {
        // Endpoints
        for &ep in wire.points.first().iter().chain(wire.points.last().iter()) {
            if ep.distance(pos) < 6.0 {
                return true;
            }
        }
        // Mid-segment: a point on a segment is also a valid connection target
        for seg in wire.points.windows(2) {
            if distance_to_segment(pos, seg[0], seg[1]) < 4.0 {
                return true;
            }
        }
    }
    false
}

/// Returns (pin_label, component_label) when pos is within `radius` of a pin.
pub(crate) fn nearest_pin_at(
    pos: Pos2,
    components: &[Component],
    radius: f32,
) -> Option<(String, String)> {
    let mut best_dist = radius;
    let mut result = None;
    for comp in components {
        for pin in component_pin_defs(comp) {
            let d = pin.pos.distance(pos);
            if d < best_dist {
                best_dist = d;
                result = Some((pin.label.to_string(), comp.label.clone()));
            }
        }
    }
    result
}

pub(crate) fn snap_to_nearest_connection(
    pos: Pos2,
    components: &[Component],
    wires: &[Wire],
) -> Option<Pos2> {
    let mut best: Option<Pos2> = None;
    // Pins get priority (smaller threshold)
    let mut best_dist_pin = 30.0_f32;
    let mut best_dist_wire = 20.0_f32;

    // Component pins — highest priority
    for component in components {
        for pin in component_pin_defs(component) {
            let d = pin.pos.distance(pos);
            if d < best_dist_pin {
                best_dist_pin = d;
                best = Some(pin.pos);
            }
        }
    }

    // Wire endpoints and all intermediate points
    for wire in wires {
        for &pt in &wire.points {
            let d = pt.distance(pos);
            if d < best_dist_wire {
                best_dist_wire = d;
                // Only override if no pin is already closer
                if best.is_none() || d < best_dist_pin {
                    best = Some(pt);
                }
            }
        }
        // Also snap to the closest point on each segment (for T-junctions)
        for seg in wire.points.windows(2) {
            let snapped = closest_point_on_segment(pos, seg[0], seg[1]);
            let d = snapped.distance(pos);
            if d < best_dist_wire && best_dist_pin > d {
                best_dist_wire = d;
                best = Some(snapped);
            }
        }
    }

    best
}

pub(crate) fn snap_delta_for_moved_components(
    components: &[Component],
    wires: &[Wire],
    moved_ids: &HashSet<u64>,
    delta: Vec2,
    old_pins: &[Pos2],
) -> Option<Vec2> {
    let mut best_adjust = None;
    let mut best_dist = 28.0_f32;

    let moving_pins = components
        .iter()
        .filter(|component| moved_ids.contains(&component.id))
        .flat_map(|component| {
            let mut moved = component.clone();
            moved.pos += delta;
            component_pin_defs(&moved).into_iter().map(|pin| pin.pos)
        })
        .collect::<Vec<_>>();

    if moving_pins.is_empty() {
        return None;
    }

    for moving_pin in &moving_pins {
        for component in components {
            if moved_ids.contains(&component.id) {
                continue;
            }
            for target_pin in component_pin_defs(component) {
                let d = moving_pin.distance(target_pin.pos);
                if d < best_dist {
                    best_dist = d;
                    best_adjust = Some(target_pin.pos - *moving_pin);
                }
            }
        }

        for wire in wires {
            for &target in &wire.points {
                if old_pins
                    .iter()
                    .any(|old_pin| old_pin.distance(target) <= 1.0)
                {
                    continue;
                }
                let d = moving_pin.distance(target);
                if d < best_dist {
                    best_dist = d;
                    best_adjust = Some(target - *moving_pin);
                }
            }

            for segment in wire.points.windows(2) {
                if old_pins
                    .iter()
                    .any(|old_pin| point_touches_wire_segment(*old_pin, segment[0], segment[1]))
                {
                    continue;
                }
                let target = closest_point_on_segment(*moving_pin, segment[0], segment[1]);
                let d = moving_pin.distance(target);
                if d < best_dist {
                    best_dist = d;
                    best_adjust = Some(target - *moving_pin);
                }
            }
        }
    }

    best_adjust
}

pub(crate) fn is_on_wire_segment(pos: Pos2, wires: &[Wire]) -> bool {
    for wire in wires {
        for seg in wire.points.windows(2) {
            if distance_to_segment(pos, seg[0], seg[1]) < 2.5
                && pos.distance(seg[0]) > 2.0
                && pos.distance(seg[1]) > 2.0
            {
                return true;
            }
        }
    }
    false
}

pub(crate) fn closest_point_on_segment(p: Pos2, a: Pos2, b: Pos2) -> Pos2 {
    let ab = b - a;
    let ap = p - a;
    let ab_len_sq = ab.x * ab.x + ab.y * ab.y;
    if ab_len_sq == 0.0 {
        return a;
    }
    let t = ((ap.x * ab.x) + (ap.y * ab.y)) / ab_len_sq;
    a + ab * t.clamp(0.0, 1.0)
}

pub(crate) fn moved_pin_for_point(
    point: Pos2,
    old_pins: &[Pos2],
    new_pins: &[Pos2],
) -> Option<Pos2> {
    old_pins
        .iter()
        .zip(new_pins)
        .find(|(old_pin, _)| point.distance(**old_pin) <= 20.0)
        .map(|(_, &new_pin)| new_pin)
}

pub(crate) fn wire_path_pin_crossings(points: &[Pos2], pins: &[Pos2]) -> usize {
    pins.iter()
        .filter(|&&pin| {
            points
                .windows(2)
                .any(|segment| point_touches_wire_segment(pin, segment[0], segment[1]))
        })
        .count()
}

pub(crate) fn keep_wire_end_orthogonal(wire: &mut Wire, first: bool) {
    if wire.points.len() < 2 {
        return;
    }
    if wire.points.len() == 2 {
        let start = wire.points[0];
        let end = wire.points[1];
        if (start.x - end.x).abs() <= 0.5 || (start.y - end.y).abs() <= 0.5 {
            return;
        }
        let corner = Pos2::new(end.x, start.y);
        wire.points = simplify_wire(vec![start, corner, end]);
        return;
    }
    let (end_index, neighbor_index) = if first {
        (0, 1)
    } else {
        (wire.points.len() - 1, wire.points.len() - 2)
    };
    let end = wire.points[end_index];
    let neighbor = wire.points[neighbor_index];
    let dx = (end.x - neighbor.x).abs();
    let dy = (end.y - neighbor.y).abs();
    if dx <= dy {
        wire.points[neighbor_index].x = end.x;
    } else {
        wire.points[neighbor_index].y = end.y;
    }
}

pub(crate) fn simplify_wire(points: Vec<Pos2>) -> Vec<Pos2> {
    let mut deduped = Vec::new();
    for point in points {
        push_unique_point(&mut deduped, point);
    }

    let mut simplified: Vec<Pos2> = Vec::new();
    for point in deduped {
        simplified.push(point);
        while simplified.len() >= 3 {
            let len = simplified.len();
            let a = simplified[len - 3];
            let b = simplified[len - 2];
            let c = simplified[len - 1];
            let horizontal = (a.y - b.y).abs() < 0.5 && (b.y - c.y).abs() < 0.5;
            let vertical = (a.x - b.x).abs() < 0.5 && (b.x - c.x).abs() < 0.5;
            if horizontal || vertical {
                simplified.remove(len - 2);
            } else {
                break;
            }
        }
    }
    simplified
}

pub(crate) fn preview_wire_points(points: &[Pos2], cursor: Pos2, orthogonal: bool) -> Vec<Pos2> {
    let mut preview = points.to_vec();
    if orthogonal && let Some(&last) = preview.last() {
        let dx = (cursor.x - last.x).abs();
        let dy = (cursor.y - last.y).abs();
        if dx > 0.1 && dy > 0.1 {
            let corner = if dx >= dy {
                Pos2::new(cursor.x, last.y)
            } else {
                Pos2::new(last.x, cursor.y)
            };
            push_unique_point(&mut preview, corner);
        }
    }
    push_unique_point(&mut preview, cursor);
    preview
}

/// Replaces intermediate points with a minimal 1-bend orthogonal route.
pub(crate) fn tidy_wire_points(wire: &mut Wire) {
    if wire.points.len() < 2 {
        return;
    }
    let start = wire.points[0];
    let Some(&end) = wire.points.last() else {
        return;
    };
    let dx = (end.x - start.x).abs();
    let dy = (end.y - start.y).abs();
    let new_points = if dx < 0.5 || dy < 0.5 {
        // Already axis-aligned — straight line
        vec![start, end]
    } else {
        // One L-bend: pick whichever option has the shorter total length
        let corner_h = Pos2::new(end.x, start.y); // horizontal-first
        let corner_v = Pos2::new(start.x, end.y); // vertical-first
        let len_h = start.distance(corner_h) + corner_h.distance(end);
        let len_v = start.distance(corner_v) + corner_v.distance(end);
        let corner = if len_h <= len_v { corner_h } else { corner_v };
        vec![start, corner, end]
    };
    wire.points = simplify_wire(new_points);
}

pub(crate) fn wire_length(wire: &Wire) -> f32 {
    wire.points
        .windows(2)
        .map(|segment| segment[0].distance(segment[1]))
        .sum()
}

pub(crate) fn wire_midpoint(wire: &Wire) -> Pos2 {
    midpoint_of_polyline(&wire.points)
        .or_else(|| wire.points.first().copied())
        .unwrap_or(Pos2::ZERO)
}
