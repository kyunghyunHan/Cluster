#![allow(dead_code)]
use crate::model::{
    Component, ComponentKind, Counters, PinRef, SavedComponent, SavedPoint, SavedWire, Wire,
    WireEndpoint, component_pin_defs,
};
use egui::Pos2;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) const SCHEMA_VERSION: u32 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectFolderLayout {
    pub(crate) root: PathBuf,
    pub(crate) project_json: PathBuf,
    pub(crate) schematic_json: PathBuf,
    pub(crate) board_json: PathBuf,
    pub(crate) libraries_dir: PathBuf,
    pub(crate) exports_dir: PathBuf,
    pub(crate) backups_dir: PathBuf,
}

impl ProjectFolderLayout {
    pub(crate) fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            project_json: root.join("project.json"),
            schematic_json: root.join("schematic.json"),
            board_json: root.join("board.json"),
            libraries_dir: root.join("libraries"),
            exports_dir: root.join("exports"),
            backups_dir: root.join("backups"),
            root,
        }
    }

    pub(crate) fn create_dirs(&self) -> Result<(), String> {
        for dir in [
            &self.root,
            &self.libraries_dir,
            &self.exports_dir,
            &self.backups_dir,
        ] {
            std::fs::create_dir_all(dir)
                .map_err(|error| format!("Create {}: {error}", dir.display()))?;
        }
        Ok(())
    }
}

/// Serialize components into their saved form.
pub(crate) fn saved_components_from(components: &[Component]) -> Vec<SavedComponent> {
    components
        .iter()
        .map(|c| SavedComponent {
            id: c.id,
            kind: c.kind,
            x: c.pos.x,
            y: c.pos.y,
            rotation: c.rotation,
            label: c.label.clone(),
            value: c.value.clone(),
            part_id: c.part_id.clone(),
        })
        .collect()
}

/// Serialize wires into their saved form.
pub(crate) fn saved_wires_from(wires: &[Wire]) -> Vec<SavedWire> {
    wires
        .iter()
        .map(|wire| SavedWire {
            id: wire.id,
            points: wire
                .points
                .iter()
                .map(|p| SavedPoint { x: p.x, y: p.y })
                .collect(),
            start: Some(wire.start.saved()),
            end: Some(wire.end.saved()),
        })
        .collect()
}

/// Restore and validate a saved page, repairing common corruption.
pub(crate) fn repair_saved_page(
    name: String,
    saved_components: Vec<SavedComponent>,
    saved_wires: Vec<SavedWire>,
    saved_next_id: u64,
    saved_counters: Counters,
    load_notes: &mut Vec<String>,
) -> (String, Vec<Component>, Vec<Wire>, u64, Counters) {
    let mut used_ids = std::collections::HashSet::new();
    let mut repair_id = saved_components
        .iter()
        .map(|c| c.id)
        .chain(saved_wires.iter().map(|w| w.id))
        .max()
        .unwrap_or(0)
        .max(saved_next_id)
        + 1;

    let mut components = Vec::new();
    for sc in saved_components {
        if !sc.x.is_finite() || !sc.y.is_finite() {
            load_notes.push(format!("Skipped {} with invalid position.", sc.label));
            continue;
        }
        let mut id = sc.id;
        if id == 0 || !used_ids.insert(id) {
            id = repair_id;
            repair_id += 1;
            used_ids.insert(id);
            load_notes.push(format!(
                "Reassigned duplicate component id for {}.",
                sc.label
            ));
        }
        if sc.kind == ComponentKind::Custom
            && sc
                .part_id
                .as_deref()
                .and_then(crate::model::custom_part::custom_part)
                .is_none()
        {
            load_notes.push(format!(
                "Custom part definition {} is not loaded; {} keeps its wiring but has no pins. \
                 Put the part's JSON file in {} and reload.",
                sc.part_id.as_deref().unwrap_or("(missing id)"),
                sc.label,
                crate::model::custom_part::CUSTOM_PARTS_DIR,
            ));
        }
        components.push(Component {
            id,
            kind: sc.kind,
            pos: Pos2::new(sc.x, sc.y),
            rotation: sc.rotation.rem_euclid(360),
            label: if sc.label.trim().is_empty() {
                load_notes.push("Filled an empty component label.".to_string());
                component_kind_short_label(sc.kind).to_string()
            } else {
                sc.label
            },
            value: sc.value,
            part_id: sc.part_id,
        });
    }

    let mut wires = Vec::new();
    for sw in saved_wires {
        let points: Vec<Pos2> = sw
            .points
            .into_iter()
            .filter_map(|p| {
                if p.x.is_finite() && p.y.is_finite() {
                    Some(Pos2::new(p.x, p.y))
                } else {
                    load_notes.push("Dropped an invalid wire point.".to_string());
                    None
                }
            })
            .collect();
        if points.len() < 2 {
            load_notes.push(format!("Skipped wire {} with fewer than 2 points.", sw.id));
            continue;
        }
        let mut id = sw.id;
        if id == 0 || !used_ids.insert(id) {
            id = repair_id;
            repair_id += 1;
            used_ids.insert(id);
            load_notes.push("Reassigned duplicate wire id.".to_string());
        }
        let start = sw
            .start
            .map(WireEndpoint::from_saved)
            .unwrap_or_else(|| infer_legacy_endpoint(points[0], &components, load_notes));
        let end = sw.end.map(WireEndpoint::from_saved).unwrap_or_else(|| {
            infer_legacy_endpoint(
                *points.last().unwrap_or(&points[0]),
                &components,
                load_notes,
            )
        });
        wires.push(Wire::with_endpoints(id, points, start, end));
    }

    let max_id = components
        .iter()
        .map(|c| c.id)
        .chain(wires.iter().map(|w| w.id))
        .max()
        .unwrap_or(0);
    let next_id = saved_next_id.max(max_id + 1).max(repair_id);
    (name, components, wires, next_id, saved_counters)
}

fn infer_legacy_endpoint(
    point: Pos2,
    components: &[Component],
    load_notes: &mut Vec<String>,
) -> WireEndpoint {
    let mut best: Option<(f32, PinRef)> = None;
    for component in components {
        for pin in component_pin_defs(component) {
            let distance = point.distance(pin.pos);
            if distance <= 1.0
                && best
                    .as_ref()
                    .is_none_or(|(best_distance, _)| distance < *best_distance)
            {
                best = Some((
                    distance,
                    PinRef {
                        component_id: component.id,
                        pin_name: pin.label.to_string(),
                    },
                ));
            }
        }
    }
    if let Some((_, pin)) = best {
        load_notes.push(format!(
            "Migrated legacy wire endpoint to explicit PinRef {}.{}.",
            pin.component_id, pin.pin_name
        ));
        WireEndpoint::Pin(pin)
    } else {
        WireEndpoint::FreePoint(point)
    }
}

fn component_kind_short_label(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Resistor => "R",
        ComponentKind::Capacitor => "C",
        ComponentKind::Inductor => "L",
        ComponentKind::Diode | ComponentKind::SchottkyDiode | ComponentKind::TvsDiode => "D",
        ComponentKind::Led => "LED",
        ComponentKind::ZenerDiode => "ZD",
        ComponentKind::Battery => "BAT",
        ComponentKind::VSource => "V",
        ComponentKind::ISource => "I",
        ComponentKind::Ground => "GND",
        ComponentKind::Esp32 | ComponentKind::Esp32S3 | ComponentKind::Esp32C3 => "ESP",
        ComponentKind::ArduinoUno => "ARD",
        ComponentKind::RaspberryPiPico => "PICO",
        ComponentKind::Oled => "OLED",
        _ => "U",
    }
}

/// Write JSON to a file, creating a backup of the existing file first.
///
/// Data is written to a sibling temporary file, flushed, then atomically
/// renamed over the target so a partial write does not corrupt the save file.
pub(crate) fn write_with_backup(path: &str, content: &str) -> Result<(), String> {
    let target = Path::new(path);
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        return Err(format!("Save folder does not exist: {}", parent.display()));
    }

    // Back up existing file before overwrite so data is never lost.
    if target.exists() {
        let backup_path = format!("{path}.bak");
        if let Err(e) = std::fs::copy(path, &backup_path) {
            // Non-fatal: log but continue.
            eprintln!("Warning: could not create backup {backup_path}: {e}");
        }
    }

    let tmp_path = temporary_path_for(target);
    let result = (|| -> Result<(), String> {
        let mut file = std::fs::File::create(&tmp_path).map_err(|e| e.to_string())?;
        file.write_all(content.as_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&tmp_path, target).map_err(|e| e.to_string())?;
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }

    result
}

fn temporary_path_for(target: &Path) -> PathBuf {
    let mut tmp_path = target.to_path_buf();
    let extension = target
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!("{extension}.tmp"))
        .unwrap_or_else(|| "tmp".to_string());
    tmp_path.set_extension(extension);
    tmp_path
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn write_with_backup_replaces_file_and_keeps_previous_copy() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("cluster-save-test-{unique}.json"));
        let backup = PathBuf::from(format!("{}.bak", path.display()));
        let tmp = temporary_path_for(&path);

        write_with_backup(path.to_str().unwrap(), "old").unwrap();
        write_with_backup(path.to_str().unwrap(), "new").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "old");
        assert!(!tmp.exists());

        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(backup);
    }

    #[test]
    fn project_folder_layout_matches_cluster_project_structure() {
        let layout = ProjectFolderLayout::new("project.cluster");

        assert_eq!(
            layout.project_json,
            PathBuf::from("project.cluster/project.json")
        );
        assert_eq!(
            layout.schematic_json,
            PathBuf::from("project.cluster/schematic.json")
        );
        assert_eq!(
            layout.board_json,
            PathBuf::from("project.cluster/board.json")
        );
        assert_eq!(
            layout.libraries_dir,
            PathBuf::from("project.cluster/libraries")
        );
        assert_eq!(layout.exports_dir, PathBuf::from("project.cluster/exports"));
        assert_eq!(layout.backups_dir, PathBuf::from("project.cluster/backups"));
    }
}
