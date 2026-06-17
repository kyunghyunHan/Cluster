#![allow(dead_code)]
use crate::model::{
    Component, ComponentKind, Counters, SavedComponent, SavedPoint, SavedWire, Wire,
};
use egui::Pos2;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) const SCHEMA_VERSION: u32 = 2;

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
        wires.push(Wire { id, points });
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
}
