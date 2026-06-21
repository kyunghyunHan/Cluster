#![allow(dead_code)]

use crate::model::cad::Point2;
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
    }

    violations
}

fn midpoint(a: Point2, b: Point2) -> Point2 {
    Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
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
}
