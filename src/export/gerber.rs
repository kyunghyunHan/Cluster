#![allow(dead_code)]

use crate::model::cad::Point2;
use crate::pcb::board::Board;
use crate::pcb::layer::BoardLayer;

pub(crate) fn gerber_for_layer(board: &Board, layer: BoardLayer) -> String {
    let mut out = String::new();
    out.push_str("G04 Cluster Gerber RS-274X*\n");
    out.push_str(&format!("G04 Layer {}*\n", layer.gerber_name()));
    out.push_str("%FSLAX46Y46*%\n");
    out.push_str("%MOMM*%\n");
    out.push_str("%LPD*%\n");
    out.push_str("%ADD10C,0.250*%\n");
    out.push_str("G01*\n");

    if layer == BoardLayer::EdgeCuts {
        write_polyline(&mut out, &board.outline.points, "10");
    } else {
        for track in board.tracks.iter().filter(|track| track.layer == layer) {
            out.push_str(&format!("%ADD10C,{:.3}*%\n", track.width_mm.max(0.001)));
            write_segment(&mut out, track.start, track.end, "10");
        }
    }

    out.push_str("M02*\n");
    out
}

pub(crate) fn excellon_drill(board: &Board) -> String {
    let mut out = String::new();
    out.push_str("M48\n");
    out.push_str("; Cluster Excellon drill file\n");
    out.push_str("METRIC,TZ\n");
    out.push_str("T1C0.400\n");
    out.push_str("%\n");
    for via in &board.vias {
        out.push_str(&format!(
            "T1\nX{}Y{}\n",
            coord(via.position.x),
            coord(via.position.y)
        ));
    }
    for footprint in &board.footprint_library {
        for pad in &footprint.pads {
            if pad.drill_mm.is_some() {
                out.push_str(&format!(
                    "; PAD {} {}\n",
                    footprint.footprint_id, pad.number
                ));
            }
        }
    }
    out.push_str("M30\n");
    out
}

fn write_polyline(out: &mut String, points: &[Point2], aperture: &str) {
    let Some((first, rest)) = points.split_first() else {
        return;
    };
    out.push_str(&format!("D{}*\n", aperture));
    out.push_str(&format!("X{}Y{}D02*\n", coord(first.x), coord(first.y)));
    for point in rest {
        out.push_str(&format!("X{}Y{}D01*\n", coord(point.x), coord(point.y)));
    }
}

fn write_segment(out: &mut String, start: Point2, end: Point2, aperture: &str) {
    out.push_str(&format!("D{}*\n", aperture));
    out.push_str(&format!("X{}Y{}D02*\n", coord(start.x), coord(start.y)));
    out.push_str(&format!("X{}Y{}D01*\n", coord(end.x), coord(end.y)));
}

fn coord(mm: f32) -> i32 {
    (mm * 1_000_000.0).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::cad::Point2;
    use crate::pcb::track::TrackSegment;

    #[test]
    fn gerber_contains_edge_cuts_and_copper_tracks() {
        let mut board = Board::new_two_layer(20.0, 10.0);
        board.tracks.push(TrackSegment {
            id: 1,
            net_id: 1,
            layer: BoardLayer::FrontCopper,
            start: Point2::new(1.0, 1.0),
            end: Point2::new(8.0, 1.0),
            width_mm: 0.25,
        });

        let copper = gerber_for_layer(&board, BoardLayer::FrontCopper);
        let edge = gerber_for_layer(&board, BoardLayer::EdgeCuts);

        assert!(copper.contains("Layer F.Cu"));
        assert!(copper.contains("D01*"));
        assert!(edge.contains("Layer Edge.Cuts"));
        assert!(edge.contains("X0Y0D02*"));
    }
}
