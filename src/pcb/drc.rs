#![allow(dead_code)]

use crate::model::cad::{CadNet, Point2};
use crate::pcb::board::Board;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DrcSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct DrcViolation {
    pub(crate) severity: DrcSeverity,
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) location: Option<Point2>,
    pub(crate) object_id: Option<u64>,
}

pub(crate) fn run_drc(board: &Board) -> Vec<DrcViolation> {
    let mut violations = Vec::new();
    let rules = &board.design_rules;

    for track in &board.tracks {
        if track.width_mm < rules.min_track_width_mm {
            violations.push(DrcViolation {
                severity: DrcSeverity::Error,
                title: "Track too narrow".to_string(),
                message: format!(
                    "Track {} is {:.2} mm, below minimum {:.2} mm.",
                    track.id, track.width_mm, rules.min_track_width_mm
                ),
                location: Some(midpoint(track.start, track.end)),
                object_id: Some(track.id),
            });
        }
        if !point_inside_board_bounds(board, track.start)
            || !point_inside_board_bounds(board, track.end)
        {
            violations.push(DrcViolation {
                severity: DrcSeverity::Error,
                title: "Copper outside board".to_string(),
                message: format!("Track {} extends outside Edge.Cuts.", track.id),
                location: Some(midpoint(track.start, track.end)),
                object_id: Some(track.id),
            });
        }
        let edge_distance =
            super::geometry::edge_distance(&board.outline.points, track.start, track.end)
                - track.width_mm * 0.5;
        if edge_distance < rules.board_edge_clearance_mm {
            violations.push(DrcViolation {
                severity: DrcSeverity::Error,
                title: "Copper too close to edge".to_string(),
                message: format!(
                    "Track {} is {:.2} mm from board edge, below minimum {:.2} mm.",
                    track.id, edge_distance, rules.board_edge_clearance_mm
                ),
                location: Some(midpoint(track.start, track.end)),
                object_id: Some(track.id),
            });
        }
    }

    for via in &board.vias {
        if via.drill_mm < rules.min_via_drill_mm {
            violations.push(DrcViolation {
                severity: DrcSeverity::Error,
                title: "Via drill too small".to_string(),
                message: format!(
                    "Via {} drill is {:.2} mm, below minimum {:.2} mm.",
                    via.id, via.drill_mm, rules.min_via_drill_mm
                ),
                location: Some(via.position),
                object_id: Some(via.id),
            });
        }
        if via.diameter_mm < rules.min_via_diameter_mm {
            violations.push(DrcViolation {
                severity: DrcSeverity::Error,
                title: "Via diameter too small".to_string(),
                message: format!(
                    "Via {} diameter is {:.2} mm, below minimum {:.2} mm.",
                    via.id, via.diameter_mm, rules.min_via_diameter_mm
                ),
                location: Some(via.position),
                object_id: Some(via.id),
            });
        }
        if (via.diameter_mm - via.drill_mm) * 0.5 < 0.15 {
            violations.push(DrcViolation {
                severity: DrcSeverity::Error,
                title: "Annular ring too small".to_string(),
                message: format!(
                    "Via {} annular ring is {:.2} mm, below minimum 0.15 mm.",
                    via.id,
                    (via.diameter_mm - via.drill_mm) * 0.5
                ),
                location: Some(via.position),
                object_id: Some(via.id),
            });
        }
    }

    if board.outline.points.len() < 4
        || board.outline.points.first() != board.outline.points.last()
        || !crate::pcb::board::valid_polygon(&board.outline.points)
    {
        violations.push(DrcViolation {
            severity: DrcSeverity::Error,
            title: "Board outline not closed".to_string(),
            message: "Edge.Cuts outline must be a closed polygon before manufacturing export."
                .to_string(),
            location: board.outline.points.first().copied(),
            object_id: None,
        });
    }

    for (left_id, right_id) in board.track_candidate_pairs() {
        if let Some((a, b)) = board.track(left_id).zip(board.track(right_id)) {
            if a.layer != b.layer || a.net_id == b.net_id {
                continue;
            }
            let distance = segment_distance(a.start, a.end, b.start, b.end);
            let required = rules.default_clearance_mm + (a.width_mm + b.width_mm) * 0.5;
            if distance < required {
                let short = distance <= (a.width_mm + b.width_mm) * 0.5;
                violations.push(DrcViolation {
                    severity: DrcSeverity::Error,
                    title: if short {
                        "Different-net copper short".to_string()
                    } else {
                        "Clearance violation".to_string()
                    },
                    message: format!(
                        "Tracks {} and {} have {:.2} mm centerline spacing; {:.2} mm is required including copper width.",
                        a.id, b.id, distance, required
                    ),
                    location: Some(midpoint(a.start, a.end)),
                    object_id: Some(a.id),
                });
            }
        }
    }

    let mut references = std::collections::HashMap::<&str, u64>::new();
    for footprint in &board.footprints {
        if let Some(previous_id) = references.insert(&footprint.reference, footprint.id) {
            violations.push(DrcViolation {
                severity: DrcSeverity::Error,
                title: "Duplicate footprint reference".to_string(),
                message: format!(
                    "{} is used by footprints {} and {}.",
                    footprint.reference, previous_id, footprint.id
                ),
                location: Some(footprint.position),
                object_id: Some(footprint.id),
            });
        }
        if !point_inside_board_bounds(board, footprint.position) {
            violations.push(DrcViolation {
                severity: DrcSeverity::Error,
                title: "Footprint outside board".to_string(),
                message: format!(
                    "{} is outside the board outline. Move it inside Edge.Cuts before export.",
                    footprint.reference
                ),
                location: Some(footprint.position),
                object_id: Some(footprint.id),
            });
        }
    }

    // Check physical pad extents, not just footprint origins. Arbitrary
    // rotation uses the same transform as rendering, snapping and drilling.
    for footprint in board.footprints.iter().filter(|f| f.placed) {
        let Some(definition) = board
            .footprint_library
            .iter()
            .find(|d| d.footprint_id == footprint.footprint_id)
        else {
            continue;
        };
        for pad in &definition.pads {
            let center = footprint.transform().local_to_board(pad.position);
            let half_w = pad.size.w * 0.5;
            let half_h = pad.size.h * 0.5;
            let corners = [
                (-half_w, -half_h),
                (half_w, -half_h),
                (half_w, half_h),
                (-half_w, half_h),
            ]
            .map(|(x, y)| {
                footprint
                    .transform()
                    .local_to_board(Point2::new(pad.position.x + x, pad.position.y + y))
            });
            let outside = if pad.shape == crate::pcb::footprint::PadShape::Circle {
                !point_inside_board_bounds(board, center)
                    || super::geometry::edge_distance(&board.outline.points, center, center)
                        < half_w
            } else {
                corners
                    .iter()
                    .any(|p| !point_inside_board_bounds(board, *p))
            };
            let copper_edge = board
                .outline
                .points
                .windows(2)
                .map(|edge| {
                    super::geometry::pad_segment_distance(
                        pad,
                        footprint.transform(),
                        edge[0],
                        edge[1],
                    )
                })
                .fold(f32::INFINITY, f32::min);
            if outside || copper_edge < rules.board_edge_clearance_mm {
                violations.push(DrcViolation {
                    severity: DrcSeverity::Error,
                    title: if outside {
                        "Pad outside board"
                    } else {
                        "Pad too close to edge"
                    }
                    .into(),
                    message: format!(
                        "{} pad {} violates Edge.Cuts clearance.",
                        footprint.reference, pad.number
                    ),
                    location: Some(center),
                    object_id: Some(footprint.id),
                });
            }
        }
    }
    for via in &board.vias {
        if !point_inside_board_bounds(board, via.position)
            || super::geometry::edge_distance(&board.outline.points, via.position, via.position)
                - via.diameter_mm * 0.5
                < rules.board_edge_clearance_mm
        {
            violations.push(DrcViolation {
                severity: DrcSeverity::Error,
                title: "Via outside board or too close to edge".into(),
                message: format!("Via {} violates Edge.Cuts clearance.", via.id),
                location: Some(via.position),
                object_id: Some(via.id),
            });
        }
    }

    for track in &board.tracks {
        let start_connected = copper_endpoint_connected(board, track, track.start);
        let end_connected = copper_endpoint_connected(board, track, track.end);
        if !start_connected || !end_connected {
            violations.push(DrcViolation {
                severity: DrcSeverity::Warning,
                title: "Dangling track".to_string(),
                message: format!(
                    "Track {} has {} unconnected endpoint(s).",
                    track.id,
                    usize::from(!start_connected) + usize::from(!end_connected)
                ),
                location: Some(if !start_connected {
                    track.start
                } else {
                    track.end
                }),
                object_id: Some(track.id),
            });
        }
    }

    for via in &board.vias {
        if !board.tracks.iter().any(|track| {
            track.net_id == via.net_id
                && point_segment_distance(via.position, track.start, track.end)
                    <= via.diameter_mm * 0.5
        }) {
            violations.push(DrcViolation {
                severity: DrcSeverity::Warning,
                title: "Dangling via".to_string(),
                message: format!("Via {} is not connected to same-net copper.", via.id),
                location: Some(via.position),
                object_id: Some(via.id),
            });
        }
    }

    violations
}

pub(crate) fn run_local_drc(
    board: &Board,
    affected_track_ids: &std::collections::HashSet<u64>,
) -> Vec<DrcViolation> {
    let mut violations = Vec::new();
    let rules = &board.design_rules;
    let max_track_width = board
        .tracks
        .iter()
        .map(|track| track.width_mm)
        .fold(0.0_f32, f32::max);
    let mut checked_pairs = std::collections::HashSet::new();
    for id in affected_track_ids {
        let Some(track) = board.track(*id) else {
            continue;
        };
        if track.width_mm < rules.min_track_width_mm {
            violations.push(DrcViolation {
                severity: DrcSeverity::Error,
                title: "Track too narrow".to_string(),
                message: format!(
                    "Track {} is {:.2} mm, below minimum {:.2} mm.",
                    track.id, track.width_mm, rules.min_track_width_mm
                ),
                location: Some(midpoint(track.start, track.end)),
                object_id: Some(track.id),
            });
        }
        let margin = rules.default_clearance_mm + (track.width_mm + max_track_width) * 0.5;
        let min = Point2::new(
            track.start.x.min(track.end.x) - margin,
            track.start.y.min(track.end.y) - margin,
        );
        let max = Point2::new(
            track.start.x.max(track.end.x) + margin,
            track.start.y.max(track.end.y) + margin,
        );
        for candidate in board
            .track_candidates_in_bounds(min, max)
            .into_iter()
            .filter_map(|candidate| board.track(candidate))
        {
            if candidate.id == track.id
                || candidate.layer != track.layer
                || candidate.net_id == track.net_id
            {
                continue;
            }
            let pair = if track.id < candidate.id {
                (track.id, candidate.id)
            } else {
                (candidate.id, track.id)
            };
            if !checked_pairs.insert(pair) {
                continue;
            }
            let distance = segment_distance(track.start, track.end, candidate.start, candidate.end);
            let required = rules.default_clearance_mm + (track.width_mm + candidate.width_mm) * 0.5;
            if distance < required {
                violations.push(DrcViolation {
                    severity: DrcSeverity::Error,
                    title: if distance <= f32::EPSILON {
                        "Different-net copper short".to_string()
                    } else {
                        "Clearance violation".to_string()
                    },
                    message: format!(
                        "Tracks {} and {} have {:.2} mm centerline spacing; {:.2} mm is required including copper width.",
                        track.id, candidate.id, distance, required
                    ),
                    location: Some(midpoint(track.start, track.end)),
                    object_id: Some(track.id),
                });
            }
        }
    }
    violations
}

pub(crate) fn run_drc_with_nets(board: &Board, nets: &[CadNet]) -> Vec<DrcViolation> {
    let mut violations = run_drc(board);
    let pads = super::copper::resolve_pads(board, nets);
    let clearance = |a: usize, b: usize| {
        let mut required = board.design_rules.default_clearance_mm;
        for id in [a, b] {
            if let Some(class) = nets
                .iter()
                .find(|n| n.net_id == id)
                .and_then(|n| board.net_classes.iter().find(|c| c.class_id == n.class_id))
            {
                required = required.max(class.clearance_mm);
            }
        }
        required
    };
    for track in &board.tracks {
        if !nets.iter().any(|n| n.net_id == track.net_id) {
            violations.push(DrcViolation {severity:DrcSeverity::Error,title:"Unverified copper net".into(),message:format!("Track {} has an orphan or ambiguous ECO assignment; verify its net before fabrication.",track.id),location:Some(track.start),object_id:Some(track.id)});
        }
    }
    // Per-instance assignments prevent shared library pad definitions from
    // connecting every instance of a part to the same net.
    for (i, a) in pads.iter().enumerate().filter(|(_, p)| {
        board
            .footprints
            .iter()
            .any(|f| f.id == p.footprint_id && f.placed)
    }) {
        for b in pads.iter().skip(i + 1).filter(|p| {
            board
                .footprints
                .iter()
                .any(|f| f.id == p.footprint_id && f.placed)
        }) {
            if a.net_id == b.net_id || !a.layers.iter().any(|l| b.layers.contains(l)) {
                continue;
            }
            let distance = super::geometry::pad_distance(&a.pad, a.transform, &b.pad, b.transform);
            if distance < clearance(a.net_id, b.net_id) {
                violations.push(DrcViolation {
                    severity: DrcSeverity::Error,
                    title: "Pad copper clearance".into(),
                    message: format!(
                        "Pads {}.{} and {}.{} have {:.3} mm copper clearance.",
                        a.footprint_id, a.number, b.footprint_id, b.number, distance
                    ),
                    location: Some(a.position),
                    object_id: Some(a.footprint_id),
                });
            }
        }
        for track in &board.tracks {
            if a.net_id == track.net_id || !a.layers.contains(&track.layer) {
                continue;
            }
            let distance =
                super::geometry::pad_segment_distance(&a.pad, a.transform, track.start, track.end)
                    - track.width_mm * 0.5;
            if distance < clearance(a.net_id, track.net_id) {
                violations.push(DrcViolation {
                    severity: DrcSeverity::Error,
                    title: "Track to pad clearance".into(),
                    message: format!(
                        "Track {} violates pad {}.{} copper clearance.",
                        track.id, a.footprint_id, a.number
                    ),
                    location: Some(a.position),
                    object_id: Some(track.id),
                });
            }
        }
        for via in &board.vias {
            if a.net_id == via.net_id {
                continue;
            }
            let distance = super::geometry::pad_segment_distance(
                &a.pad,
                a.transform,
                via.position,
                via.position,
            ) - via.diameter_mm * 0.5;
            if distance < clearance(a.net_id, via.net_id) {
                violations.push(DrcViolation {
                    severity: DrcSeverity::Error,
                    title: "Via to pad clearance".into(),
                    message: format!(
                        "Via {} violates pad {}.{} copper clearance.",
                        via.id, a.footprint_id, a.number
                    ),
                    location: Some(via.position),
                    object_id: Some(via.id),
                });
            }
        }
    }
    for (i, a) in board.tracks.iter().enumerate() {
        for b in board.tracks.iter().skip(i + 1) {
            if a.net_id == b.net_id
                || a.layer != b.layer
                || clearance(a.net_id, b.net_id) <= board.design_rules.default_clearance_mm
            {
                continue;
            }
            if segment_distance(a.start, a.end, b.start, b.end) - (a.width_mm + b.width_mm) * 0.5
                < clearance(a.net_id, b.net_id)
            {
                violations.push(DrcViolation {
                    severity: DrcSeverity::Error,
                    title: "Net class clearance".into(),
                    message: format!("Tracks {} and {} violate net class clearance.", a.id, b.id),
                    location: Some(a.start),
                    object_id: Some(a.id),
                });
            }
        }
        for via in &board.vias {
            if a.net_id == via.net_id {
                continue;
            }
            if point_segment_distance(via.position, a.start, a.end)
                - (via.diameter_mm + a.width_mm) * 0.5
                < clearance(a.net_id, via.net_id)
            {
                violations.push(DrcViolation {
                    severity: DrcSeverity::Error,
                    title: "Track to via clearance".into(),
                    message: format!("Track {} violates via {} copper clearance.", a.id, via.id),
                    location: Some(via.position),
                    object_id: Some(a.id),
                });
            }
        }
    }
    for (i, a) in board.vias.iter().enumerate() {
        for b in board.vias.iter().skip(i + 1) {
            if a.net_id != b.net_id
                && (a.position.x - b.position.x).hypot(a.position.y - b.position.y)
                    - (a.diameter_mm + b.diameter_mm) * 0.5
                    < clearance(a.net_id, b.net_id)
            {
                violations.push(DrcViolation {
                    severity: DrcSeverity::Error,
                    title: "Via copper clearance".into(),
                    message: format!("Vias {} and {} violate copper clearance.", a.id, b.id),
                    location: Some(a.position),
                    object_id: Some(a.id),
                });
            }
        }
    }
    for footprint in &board.footprints {
        let Some(symbol) = footprint.symbol_instance_id else {
            continue;
        };
        for net in nets {
            for pin in net
                .connected_pins
                .iter()
                .filter(|p| p.component_id == symbol)
            {
                let known = pads.iter().any(|p| {
                    p.footprint_id == footprint.id
                        && p.net_id == net.net_id
                        && p.schematic_pin == pin.pin_name
                });
                if !known {
                    violations.push(DrcViolation {
                        severity: DrcSeverity::Error,
                        title: "Unmapped schematic pin".into(),
                        message: format!(
                            "{} pin {} has no verified footprint pad mapping.",
                            footprint.reference, pin.pin_name
                        ),
                        location: Some(footprint.position),
                        object_id: Some(footprint.id),
                    });
                }
            }
        }
    }
    for ratsnest in board.ratsnest_edges(nets) {
        let from = board
            .footprints
            .iter()
            .find(|footprint| footprint.id == ratsnest.from_footprint_id);
        let to = board
            .footprints
            .iter()
            .find(|footprint| footprint.id == ratsnest.to_footprint_id);
        violations.push(DrcViolation {
            severity: DrcSeverity::Error,
            title: "Unrouted ratsnest".to_string(),
            message: format!(
                "Net {} still has an unrouted connection between footprints {} and {}.",
                ratsnest.net_id, ratsnest.from_footprint_id, ratsnest.to_footprint_id
            ),
            location: from
                .zip(to)
                .map(|(from, to)| midpoint(from.position, to.position)),
            object_id: Some(ratsnest.from_footprint_id),
        });
    }
    violations
}

fn copper_endpoint_connected(
    board: &Board,
    current: &crate::pcb::track::TrackSegment,
    point: Point2,
) -> bool {
    const CONTACT_MM: f32 = 0.000001;
    board
        .footprints
        .iter()
        .filter(|f| f.placed)
        .any(|footprint| {
            board
                .footprint_library
                .iter()
                .find(|d| d.footprint_id == footprint.footprint_id)
                .is_some_and(|definition| {
                    definition.pads.iter().any(|pad| {
                        footprint
                            .transform()
                            .transform_pad(pad)
                            .layers
                            .contains(&current.layer)
                            && super::geometry::pad_segment_distance(
                                pad,
                                footprint.transform(),
                                point,
                                point,
                            ) <= current.width_mm * 0.5
                    })
                })
        })
        || board.vias.iter().any(|via| {
            via.net_id == current.net_id
                && ((via.position.x - point.x).powi(2) + (via.position.y - point.y).powi(2)).sqrt()
                    <= via.diameter_mm * 0.5
        })
        || board.tracks.iter().any(|track| {
            track.id != current.id
                && track.net_id == current.net_id
                && track.layer == current.layer
                && (point_segment_distance(point, track.start, track.end) <= CONTACT_MM)
        })
}

fn midpoint(a: Point2, b: Point2) -> Point2 {
    Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}

fn point_inside_board_bounds(board: &Board, point: Point2) -> bool {
    super::geometry::inside_polygon(&board.outline.points, point)
}

use super::geometry::{point_segment_distance, segment_distance};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcb::layer::BoardLayer;
    use crate::pcb::track::TrackSegment;

    #[test]
    fn audit_concave_outline_and_copper_width_are_checked() {
        let mut board = Board::new_two_layer(10.0, 10.0);
        board.outline.points = vec![
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 0.0),
            Point2::new(10.0, 10.0),
            Point2::new(6.0, 10.0),
            Point2::new(6.0, 4.0),
            Point2::new(4.0, 4.0),
            Point2::new(4.0, 10.0),
            Point2::new(0.0, 10.0),
            Point2::new(0.0, 0.0),
        ];
        board.tracks.push(TrackSegment {
            id: 1,
            net_id: 1,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(2.0, 8.0),
            end: Point2::new(8.0, 8.0),
            width_mm: 0.25,
        });
        assert!(run_drc(&board).iter().any(|v| v.title == "Copper too close to edge" || v.title == "Copper outside board"));
        board.outline = crate::pcb::board::BoardOutline::rectangular(10.0, 10.0);
        board.tracks[0].start = Point2::new(0.4, 2.0);
        board.tracks[0].end = Point2::new(0.4, 8.0);
        board.tracks[0].width_mm = 0.4;
        assert!(
            run_drc(&board)
                .iter()
                .any(|v| v.title == "Copper too close to edge")
        );
    }

    #[test]
    fn audit_every_schematic_pin_needs_its_own_pad_mapping() {
        let mut board = Board::new_two_layer(40.0, 30.0);
        board.footprints.push(crate::pcb::board::BoardFootprint {
            id: 1,
            symbol_instance_id: Some(1),
            reference: "R1".into(),
            footprint_id: "R_THT_Axial".into(),
            position: Point2::new(20.0, 15.0),
            rotation_deg: 0.0,
            flipped: false,
            placed: true,
        });
        board.rebuild_entity_index();
        let nets = vec![CadNet {
            net_id: 1,
            name: "LOAD".into(),
            class_id: "Default".into(),
            connected_pins: ["A", "UNKNOWN"]
                .into_iter()
                .map(|name| crate::PinRef {
                    component_id: 1,
                    pin_name: name.into(),
                })
                .collect(),
        }];
        assert!(
            run_drc_with_nets(&board, &nets)
                .iter()
                .any(|v| v.title == "Unmapped schematic pin" && v.message.contains("UNKNOWN"))
        );
    }

    #[test]
    fn audit_pad_extent_outside_board_is_not_hidden_by_footprint_origin() {
        let mut board = Board::new_two_layer(10.0, 10.0);
        board.footprints.push(crate::pcb::board::BoardFootprint {
            id: 1,
            symbol_instance_id: None,
            reference: "R1".into(),
            footprint_id: "R_THT_Axial".into(),
            position: Point2::new(5.0, 5.0),
            rotation_deg: 0.0,
            flipped: false,
            placed: true,
        });
        board.rebuild_entity_index();
        assert!(
            run_drc(&board)
                .iter()
                .any(|v| v.title == "Pad outside board")
        );
    }

    #[test]
    fn drc_reports_track_width_below_minimum() {
        let mut board = Board::new_two_layer(40.0, 30.0);
        board.tracks.push(TrackSegment {
            id: 7,
            net_id: 1,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(0.0, 0.0),
            end: Point2::new(10.0, 0.0),
            width_mm: 0.1,
        });
        board.rebuild_entity_index();

        let violations = run_drc(&board);

        assert!(violations.iter().any(|violation| {
            violation.severity == DrcSeverity::Error
                && violation.title == "Track too narrow"
                && violation.object_id == Some(7)
        }));
    }

    #[test]
    fn drc_reports_clearance_and_open_outline() {
        let mut board = Board::new_two_layer(40.0, 30.0);
        board.outline.points.pop();
        board.tracks.push(TrackSegment {
            id: 1,
            net_id: 1,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(0.0, 0.0),
            end: Point2::new(10.0, 0.0),
            width_mm: 0.25,
        });
        board.tracks.push(TrackSegment {
            id: 2,
            net_id: 2,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(0.0, 0.1),
            end: Point2::new(10.0, 0.1),
            width_mm: 0.25,
        });
        board.rebuild_entity_index();

        let violations = run_drc(&board);

        assert!(
            violations.iter().any(
                |v| v.title == "Clearance violation" || v.title == "Different-net copper short"
            )
        );
        assert!(
            violations
                .iter()
                .any(|v| v.title == "Board outline not closed")
        );
    }

    #[test]
    fn drc_reports_edge_clearance_footprint_outside_and_unrouted_net() {
        use crate::model::PinRef;
        use crate::model::cad::CadNet;
        use crate::pcb::board::BoardFootprint;

        let mut board = Board::new_two_layer(40.0, 30.0);
        board.tracks.push(TrackSegment {
            id: 3,
            net_id: 1,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(0.05, 5.0),
            end: Point2::new(5.0, 5.0),
            width_mm: 0.25,
        });
        board.footprints.push(BoardFootprint {
            id: 10,
            symbol_instance_id: Some(1),
            reference: "R1".to_string(),
            footprint_id: "R_THT_Axial".to_string(),
            position: Point2::new(5.0, 5.0),
            rotation_deg: 0.0,
            flipped: false,
            placed: true,
        });
        board.footprints.push(BoardFootprint {
            id: 11,
            symbol_instance_id: Some(2),
            reference: "R2".to_string(),
            footprint_id: "R_THT_Axial".to_string(),
            position: Point2::new(50.0, 5.0),
            rotation_deg: 0.0,
            flipped: false,
            placed: true,
        });
        let nets = vec![CadNet {
            net_id: 2,
            name: "NET_002".to_string(),
            connected_pins: vec![
                PinRef {
                    component_id: 1,
                    pin_name: "A".to_string(),
                },
                PinRef {
                    component_id: 2,
                    pin_name: "A".to_string(),
                },
            ],
            class_id: "Default".to_string(),
        }];
        board.rebuild_entity_index();

        let violations = run_drc_with_nets(&board, &nets);

        assert!(
            violations
                .iter()
                .any(|v| v.title == "Copper too close to edge")
        );
        assert!(
            violations
                .iter()
                .any(|v| v.title == "Footprint outside board")
        );
        assert!(violations.iter().any(|v| v.title == "Unrouted ratsnest"));
    }

    #[test]
    fn drc_distinguishes_short_and_reports_dangling_copper() {
        let mut board = Board::new_two_layer(40.0, 30.0);
        board.tracks.push(TrackSegment {
            id: 1,
            net_id: 1,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(5.0, 5.0),
            end: Point2::new(20.0, 5.0),
            width_mm: 0.25,
        });
        board.tracks.push(TrackSegment {
            id: 2,
            net_id: 2,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(10.0, 1.0),
            end: Point2::new(10.0, 10.0),
            width_mm: 0.25,
        });
        board.rebuild_entity_index();

        let violations = run_drc(&board);
        assert!(
            violations
                .iter()
                .any(|violation| violation.title == "Different-net copper short")
        );
        assert!(
            violations
                .iter()
                .any(|violation| violation.title == "Dangling track")
        );
    }
}
