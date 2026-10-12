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
    // Aperture definitions are immutable and emitted before any drawing.
    let mut apertures = BTreeMap::<String, u32>::new();
    let mut drawing = String::new();
    let mut aperture_for = |definition: String| -> u32 {
        let next = apertures.len() as u32 + 10;
        *apertures.entry(definition).or_insert(next)
    };
    if layer == BoardLayer::EdgeCuts {
        let code = aperture_for("C,0.250000".into());
        write_polyline(&mut drawing, &board.outline.points, &code.to_string());
    } else {
        for track in board.tracks.iter().filter(|track| track.layer == layer) {
            let code = aperture_for(format!("C,{:.6}", track.width_mm));
            write_segment(&mut drawing, track.start, track.end, &code.to_string());
        }
        if matches!(layer, BoardLayer::FrontCopper | BoardLayer::BackCopper) {
            for via in &board.vias {
                let code = aperture_for(format!("C,{:.6}", via.diameter_mm));
                write_flash(&mut drawing, via.position, code);
            }
        }
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
                let definition = match pad.shape {
                    crate::pcb::footprint::PadShape::Circle => format!("C,{:.6}", pad.size.w),
                    crate::pcb::footprint::PadShape::Oval => {
                        format!("O,{:.6}X{:.6}", pad.size.w, pad.size.h)
                    }
                    crate::pcb::footprint::PadShape::Rect
                    | crate::pcb::footprint::PadShape::RoundRect => {
                        format!("R,{:.6}X{:.6}", pad.size.w, pad.size.h)
                    }
                };
                let code = aperture_for(definition);
                write_flash(&mut drawing, pad.position, code);
            }
        }
    }
    for (definition, code) in &apertures {
        out.push_str(&format!("%ADD{code}{definition}*%\n"));
    }
    out.push_str("G01*\n");
    out.push_str(&drawing);

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
                "; {label}\nX{:.6}Y{:.6}\n",
                position.x, position.y
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

fn write_flash(out: &mut String, position: Point2, aperture: u32) {
    out.push_str(&format!(
        "D{aperture}*\nX{}Y{}D03*\n",
        coord(position.x),
        coord(position.y)
    ));
}

/// Reject model geometry that this exporter cannot faithfully represent.
pub(crate) fn manufacturing_limitations(board: &Board) -> Vec<String> {
    let mut errors = Vec::new();
    if !board.zones.is_empty() {
        errors.push("Copper zones are not supported by the exporter.".into());
    }
    if board.footprints.iter().any(|f| !f.placed) {
        errors.push("Place all footprints before export.".into());
    }
    for placed in board.footprints.iter().filter(|f| f.placed) {
        let quadrant = placed.rotation_deg / 90.0;
        if (quadrant - quadrant.round()).abs() > 1e-5 {
            errors.push(format!(
                "{} needs a 90-degree rotation for export.",
                placed.reference
            ));
        }
        if let Some(definition) = board
            .footprint_library
            .iter()
            .find(|d| d.footprint_id == placed.footprint_id)
        {
            if definition
                .pads
                .iter()
                .any(|p| p.shape == crate::pcb::footprint::PadShape::RoundRect)
            {
                errors.push(format!(
                    "{} has rounded pads without a defined corner radius.",
                    placed.reference
                ));
            }
            if definition.pads.iter().any(|p| {
                p.shape == crate::pcb::footprint::PadShape::Circle
                    && (p.size.w - p.size.h).abs() > 1e-6
            }) {
                errors.push(format!(
                    "{} has a circular pad with unequal diameters.",
                    placed.reference
                ));
            }
        }
    }
    errors
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

fn coord(mm: f32) -> i64 {
    (f64::from(mm) * 1_000_000.0).round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::cad::Point2;
    use crate::pcb::board::BoardFootprint;
    use crate::pcb::track::TrackSegment;

    #[test]
    fn audit_manufacturing_matches_independent_mirror_rotation_golden_files() {
        use crate::pcb::footprint::{Footprint, Pad, PadShape};
        let mut board = Board::new_two_layer(20.0, 30.0);
        board.footprint_library.push(Footprint {
            footprint_id: "golden".into(),
            display_name: "Reference fixture".into(),
            pads: vec![Pad {
                number: "1".into(),
                net_id: None,
                position: Point2::new(2.0, 1.0),
                size: crate::model::cad::Size2 { w: 2.0, h: 1.0 },
                drill_mm: Some(0.5),
                shape: PadShape::Rect,
                layers: vec![BoardLayer::FrontCopper],
            }],
            courtyard: vec![],
            silkscreen: vec![],
            fabrication: vec![],
            model_3d_path: None,
        });
        board.footprints.push(BoardFootprint {
            id: 1,
            symbol_instance_id: None,
            reference: "J1".into(),
            footprint_id: "golden".into(),
            position: Point2::new(10.0, 20.0),
            rotation_deg: 90.0,
            flipped: true,
            placed: true,
        });
        board.tracks.push(TrackSegment {
            id: 1,
            net_id: 1,
            layer: BoardLayer::BackCopper,
            start: Point2::new(2.0, 3.0),
            end: Point2::new(4.0, 3.0),
            width_mm: 0.25,
        });
        board.vias.push(crate::pcb::via::Via {
            id: 2,
            net_id: 1,
            position: Point2::new(6.0, 7.0),
            diameter_mm: 0.8,
            drill_mm: 0.4,
        });
        assert_eq!(
            gerber_for_layer(&board, BoardLayer::BackCopper),
            include_str!("../../docs/audit/fixtures/mirrored-pad-B_Cu.gbr")
        );
        assert_eq!(
            excellon_drill(&board),
            include_str!("../../docs/audit/fixtures/mirrored-pad.drl")
        );
        assert!(!gerber_for_layer(&board, BoardLayer::FrontCopper).contains("R,1.000000X2.000000"));
    }

    #[test]
    fn audit_unsupported_geometry_blocks_manufacturing() {
        let mut board = Board::new_two_layer(20.0, 20.0);
        board.zones.push(crate::pcb::board::Zone {
            id: 1,
            net_id: 1,
            layer: BoardLayer::FrontCopper,
            outline: vec![
                Point2::new(1.0, 1.0),
                Point2::new(2.0, 1.0),
                Point2::new(2.0, 2.0),
            ],
        });
        assert!(!manufacturing_limitations(&board).is_empty());
    }

    #[test]
    fn audit_gerber_preserves_pad_shape_and_via_copper() {
        let mut board = Board::new_two_layer(20.0, 20.0);
        board.footprints.push(BoardFootprint {
            id: 1,
            symbol_instance_id: None,
            reference: "D1".into(),
            footprint_id: "LED_THT_5mm".into(),
            position: Point2::new(10.0, 10.0),
            rotation_deg: 0.0,
            flipped: false,
            placed: true,
        });
        board.vias.push(crate::pcb::via::Via {
            id: 2,
            net_id: 1,
            position: Point2::new(5.0, 5.0),
            diameter_mm: 0.8,
            drill_mm: 0.4,
        });
        for layer in [BoardLayer::FrontCopper, BoardLayer::BackCopper] {
            let gerber = gerber_for_layer(&board, layer);
            assert!(
                gerber.contains("R,1.800000X1.800000"),
                "square cathode pad must remain square"
            );
            assert!(gerber.contains("C,0.800"));
            assert!(gerber.contains("X5000000Y5000000D03"));
        }
    }

    #[test]
    fn audit_track_apertures_are_not_redefined() {
        let mut board = Board::new_two_layer(20.0, 20.0);
        for (id, width) in [(1, 0.25), (2, 0.5)] {
            board.tracks.push(TrackSegment {
                id,
                net_id: 1,
                layer: BoardLayer::FrontCopper,
                start: Point2::new(2.0, 2.0),
                end: Point2::new(5.0, 2.0),
                width_mm: width,
            });
        }
        let gerber = gerber_for_layer(&board, BoardLayer::FrontCopper);
        let codes = gerber
            .lines()
            .filter(|l| l.starts_with("%ADD"))
            .map(|l| l.split(['C', 'R', 'O']).next().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            codes.iter().collect::<std::collections::HashSet<_>>().len(),
            codes.len()
        );
    }

    #[test]
    fn audit_drill_coordinates_have_explicit_decimal_units() {
        let mut board = Board::new_two_layer(20.0, 20.0);
        board.vias.push(crate::pcb::via::Via {
            id: 1,
            net_id: 1,
            position: Point2::new(1.25, -2.5),
            diameter_mm: 0.8,
            drill_mm: 0.4,
        });
        assert!(excellon_drill(&board).contains("X1.250000Y-2.500000"));
    }

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
        assert!(drill.contains("; PAD R1 1\nX10.000000Y4.920000"));
        assert!(drill.contains("; PAD R1 2\nX10.000000Y15.080000"));
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
            schematic: None,
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
