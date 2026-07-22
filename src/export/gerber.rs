#![allow(dead_code)]

use crate::model::cad::CadProjectData;
use crate::model::cad::Point2;
use crate::pcb::board::Board;
use crate::pcb::layer::BoardLayer;
use std::collections::BTreeMap;

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
        let mut aperture = 11_u32;
        for placed in board.footprints.iter().filter(|footprint| footprint.placed) {
            let Some(definition) = board
                .footprint_library
                .iter()
                .find(|definition| definition.footprint_id == placed.footprint_id)
            else {
                continue;
            };
            for pad in &definition.pads {
                let pad = placed.transform().transform_pad(pad);
                if !pad.layers.contains(&layer) {
                    continue;
                }
                out.push_str(&format!(
                    "%ADD{aperture}C,{:.3}*%\nD{aperture}*\nX{}Y{}D03*\n",
                    pad.size.w.max(pad.size.h).max(0.001),
                    coord(pad.position.x),
                    coord(pad.position.y),
                ));
                aperture += 1;
            }
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
    let mut drills = BTreeMap::<i32, Vec<(Point2, String)>>::new();
    for via in &board.vias {
        drills
            .entry((via.drill_mm * 1_000.0).round() as i32)
            .or_default()
            .push((via.position, format!("VIA {}", via.id)));
    }
    for placed in board.footprints.iter().filter(|footprint| footprint.placed) {
        let Some(definition) = board
            .footprint_library
            .iter()
            .find(|definition| definition.footprint_id == placed.footprint_id)
        else {
            continue;
        };
        for pad in definition.pads.iter().filter(|pad| pad.drill_mm.is_some()) {
            let drill_mm = pad.drill_mm.unwrap_or_default();
            drills
                .entry((drill_mm * 1_000.0).round() as i32)
                .or_default()
                .push((
                    placed.transform().local_to_board(pad.position),
                    format!("PAD {} {}", placed.reference, pad.number),
                ));
        }
    }
    for (tool_index, diameter_um) in drills.keys().enumerate() {
        out.push_str(&format!(
            "T{:02}C{:.3}\n",
            tool_index + 1,
            *diameter_um as f32 / 1_000.0
        ));
    }
    out.push_str("%\n");
    for (tool_index, (_, locations)) in drills.iter_mut().enumerate() {
        locations.sort_by(|(left, _), (right, _)| {
            left.x
                .total_cmp(&right.x)
                .then_with(|| left.y.total_cmp(&right.y))
        });
        out.push_str(&format!("T{:02}\n", tool_index + 1));
        for (position, label) in locations {
            out.push_str(&format!(
                "; {label}\nX{}Y{}\n",
                coord(position.x),
                coord(position.y)
            ));
        }
    }
    out.push_str("M30\n");
    out
}

pub(crate) fn bom_csv(project: &CadProjectData) -> String {
    let mut out = String::from("Reference,Value,Footprint,Manufacturer,MPN\n");
    for symbol in &project.symbols {
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            csv(&symbol.reference),
            csv(&symbol.value),
            csv(symbol.footprint_link.as_deref().unwrap_or("")),
            csv(symbol.fields.manufacturer.as_deref().unwrap_or("")),
            csv(symbol.fields.mpn.as_deref().unwrap_or(""))
        ));
    }
    out
}

pub(crate) fn cpl_csv(project: &CadProjectData) -> String {
    let mut out = String::from("Designator,Mid X,Mid Y,Layer,Rotation\n");
    let Some(board) = &project.board else {
        return out;
    };
    for footprint in &board.footprints {
        out.push_str(&format!(
            "{},{:.3},{:.3},{},{:.1}\n",
            csv(&footprint.reference),
            footprint.position.x,
            footprint.position.y,
            if footprint.flipped { "bottom" } else { "top" },
            footprint.rotation_deg
        ));
    }
    out
}

fn csv(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
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
    use crate::pcb::board::BoardFootprint;
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

    #[test]
    fn excellon_uses_placed_footprint_transform_and_actual_tool_sizes() {
        let mut board = Board::new_two_layer(20.0, 20.0);
        board.footprints.push(BoardFootprint {
            id: 7,
            symbol_instance_id: None,
            reference: "R1".to_string(),
            footprint_id: "R_THT_Axial".to_string(),
            position: Point2::new(10.0, 10.0),
            rotation_deg: 90.0,
            flipped: false,
            placed: true,
        });
        let drill = excellon_drill(&board);
        assert!(drill.contains("T01C0.800"));
        assert!(drill.contains("; PAD R1 1\nX10000000Y4920000"));
        assert!(drill.contains("; PAD R1 2\nX10000000Y15080000"));
    }

    #[test]
    fn copper_gerber_flashes_pads_at_transformed_coordinates() {
        let mut board = Board::new_two_layer(20.0, 20.0);
        board.footprints.push(BoardFootprint {
            id: 7,
            symbol_instance_id: None,
            reference: "R1".to_string(),
            footprint_id: "R_THT_Axial".to_string(),
            position: Point2::new(10.0, 10.0),
            rotation_deg: 90.0,
            flipped: false,
            placed: true,
        });
        let copper = gerber_for_layer(&board, BoardLayer::FrontCopper);
        assert!(copper.contains("X10000000Y4920000D03"));
        assert!(copper.contains("X10000000Y15080000D03"));
    }

    #[test]
    fn cpl_reports_flipped_footprints_on_bottom() {
        let mut project = CadProjectData {
            schema_version: 1,
            symbols: Vec::new(),
            nets: Vec::new(),
            net_classes: Vec::new(),
            board: Some(Board::new_two_layer(20.0, 20.0)),
            properties: std::collections::HashMap::new(),
        };
        project
            .board
            .as_mut()
            .unwrap()
            .footprints
            .push(BoardFootprint {
                id: 8,
                symbol_instance_id: None,
                reference: "U1".to_string(),
                footprint_id: "R_THT_Axial".to_string(),
                position: Point2::new(3.0, 4.0),
                rotation_deg: 270.0,
                flipped: true,
                placed: true,
            });
        assert!(cpl_csv(&project).contains("U1,3.000,4.000,bottom,270.0"));
    }
}
