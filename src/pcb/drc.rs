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
        let edge_distance = distance_to_board_edge(board, track.start)
            .min(distance_to_board_edge(board, track.end));
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

    if board.outline.points.len() < 4 || board.outline.points.first() != board.outline.points.last()
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

    for (i, a) in board.tracks.iter().enumerate() {
        for b in board.tracks.iter().skip(i + 1) {
            if a.layer != b.layer || a.net_id == b.net_id {
                continue;
            }
            let distance = segment_distance(a.start, a.end, b.start, b.end);
            let required = rules.default_clearance_mm + (a.width_mm + b.width_mm) * 0.5;
            if distance < required {
                let short = distance <= f32::EPSILON;
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

pub(crate) fn run_drc_with_nets(board: &Board, nets: &[CadNet]) -> Vec<DrcViolation> {
    let mut violations = run_drc(board);
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
            severity: DrcSeverity::Warning,
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
    const CONTACT_MM: f32 = 0.05;
    board.footprints.iter().any(|footprint| {
        footprint.placed
            && ((footprint.position.x - point.x).powi(2) + (footprint.position.y - point.y).powi(2))
                .sqrt()
                <= CONTACT_MM
    }) || board.vias.iter().any(|via| {
        via.net_id == current.net_id
            && ((via.position.x - point.x).powi(2) + (via.position.y - point.y).powi(2)).sqrt()
                <= via.diameter_mm * 0.5
    }) || board.tracks.iter().any(|track| {
        track.id != current.id
            && track.net_id == current.net_id
            && track.layer == current.layer
            && (point_segment_distance(point, track.start, track.end) <= CONTACT_MM)
    })
}

fn midpoint(a: Point2, b: Point2) -> Point2 {
    Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}

fn board_bounds(board: &Board) -> Option<(Point2, Point2)> {
    let first = *board.outline.points.first()?;
    let mut min = first;
    let mut max = first;
    for point in &board.outline.points {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    Some((min, max))
}

fn point_inside_board_bounds(board: &Board, point: Point2) -> bool {
    let Some((min, max)) = board_bounds(board) else {
        return false;
    };
    point.x >= min.x && point.x <= max.x && point.y >= min.y && point.y <= max.y
}

fn distance_to_board_edge(board: &Board, point: Point2) -> f32 {
    let Some((min, max)) = board_bounds(board) else {
        return 0.0;
    };
    (point.x - min.x)
        .abs()
        .min((max.x - point.x).abs())
        .min((point.y - min.y).abs())
        .min((max.y - point.y).abs())
}

fn segment_distance(a1: Point2, a2: Point2, b1: Point2, b2: Point2) -> f32 {
    if segments_intersect(a1, a2, b1, b2) {
        return 0.0;
    }
    point_segment_distance(a1, b1, b2)
        .min(point_segment_distance(a2, b1, b2))
        .min(point_segment_distance(b1, a1, a2))
        .min(point_segment_distance(b2, a1, a2))
}

fn point_segment_distance(p: Point2, a: Point2, b: Point2) -> f32 {
    let ab = Point2::new(b.x - a.x, b.y - a.y);
    let ap = Point2::new(p.x - a.x, p.y - a.y);
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 <= f32::EPSILON {
        return ((p.x - a.x).powi(2) + (p.y - a.y).powi(2)).sqrt();
    }
    let t = ((ap.x * ab.x + ap.y * ab.y) / len2).clamp(0.0, 1.0);
    let closest = Point2::new(a.x + ab.x * t, a.y + ab.y * t);
    ((p.x - closest.x).powi(2) + (p.y - closest.y).powi(2)).sqrt()
}

fn segments_intersect(a1: Point2, a2: Point2, b1: Point2, b2: Point2) -> bool {
    fn orient(a: Point2, b: Point2, c: Point2) -> f32 {
        (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
    }
    let o1 = orient(a1, a2, b1);
    let o2 = orient(a1, a2, b2);
    let o3 = orient(b1, b2, a1);
    let o4 = orient(b1, b2, a2);
    o1.signum() != o2.signum() && o3.signum() != o4.signum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcb::layer::BoardLayer;
    use crate::pcb::track::TrackSegment;

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

        let violations = run_drc(&board);

        assert!(violations.iter().any(|v| v.title == "Clearance violation"));
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
