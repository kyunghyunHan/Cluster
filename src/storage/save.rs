#![allow(dead_code)]
use crate::model::{
    Component, ComponentKind, Counters, PinRef, SavedComponent, SavedPoint, SavedWire, Wire,
    WireEndpoint, component_pin_defs,
};
use egui::Pos2;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
        .saturating_add(1)
        .max(saved_next_id);

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
    write_bytes_with_backup(path, content.as_bytes())
}

pub(crate) fn write_bytes_with_backup(path: &str, content: &[u8]) -> Result<(), String> {
    let target = Path::new(path);
    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        return Err(format!("Save folder does not exist: {}", parent.display()));
    }

    // OS locks are released on process exit, including crashes. Keep the lock
    // file in place: unlinking it would let another writer lock a new inode.
    let _lock = acquire_save_lock(target)?;
    let (mut file, temporary) = TemporaryFile::create(target)?;
    file.write_all(content)
        .map_err(|error| format!("Write {}: {error}", temporary.path.display()))?;
    file.sync_all()
        .map_err(|error| format!("Sync {}: {error}", temporary.path.display()))?;
    drop(file);

    // Prepare the new content before rotating backups. A failed write must
    // not consume any of the previous generations.
    if target.exists() {
        for backup in std::iter::once(path.to_string())
            .chain(std::iter::once(format!("{path}.bak")))
            .chain((1..=3).map(|i| format!("{path}.bak.{i}")))
        {
            if let Ok(metadata) = std::fs::symlink_metadata(&backup)
                && !metadata.is_file()
            {
                return Err(format!("Save blocked: {} is not a regular file.", backup));
            }
        }
        // Publish a complete, synced backup rather than copying over .bak.
        let backup_path = PathBuf::from(format!("{path}.bak"));
        let (mut backup_file, backup_temporary) = TemporaryFile::create(&backup_path)?;
        let mut original = std::fs::File::open(target)
            .map_err(|error| format!("Read previous save {}: {error}", target.display()))?;
        std::io::copy(&mut original, &mut backup_file)
            .map_err(|error| format!("Create backup {}: {error}", backup_path.display()))?;
        backup_file
            .sync_all()
            .map_err(|error| format!("Sync backup {}: {error}", backup_path.display()))?;
        drop(backup_file);
        drop(original);
        rotate_backup_generations(path)?;
        std::fs::rename(&backup_temporary.path, &backup_path)
            .map_err(|error| format!("Replace backup {}: {error}", backup_path.display()))?;
    }

    std::fs::rename(&temporary.path, target)
        .map_err(|error| format!("Replace {}: {error}", target.display()))?;
    sync_directory(parent)
}

pub(crate) fn sync_directory(parent: &Path) -> Result<(), String> {
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("Sync folder {}: {error}", parent.display()))?;
    #[cfg(not(unix))]
    let _ = parent;
    Ok(())
}

fn save_lock_path(target: &Path) -> PathBuf {
    let mut name = std::ffi::OsString::from(".");
    name.push(target.file_name().unwrap_or_default());
    name.push(".cluster-save.lock");
    target.with_file_name(name)
}

fn acquire_save_lock(target: &Path) -> Result<std::fs::File, String> {
    let path = save_lock_path(target);
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| format!("Open save lock {}: {error}", path.display()))?;
    file.try_lock().map_err(|error| {
        format!(
            "Cannot save {}: {error}. Another save may be in progress; retry.",
            target.display()
        )
    })?;
    Ok(file)
}

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TemporaryFile {
    path: PathBuf,
}

impl TemporaryFile {
    fn create(target: &Path) -> Result<(std::fs::File, Self), String> {
        for _ in 0..128 {
            let path = temporary_path_for(target);
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => return Ok((file, Self { path })),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!("Create temporary save {}: {error}", path.display()));
                }
            }
        }
        Err(format!(
            "Could not create a unique temporary save for {}.",
            target.display()
        ))
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn rotate_backup_generations(path: &str) -> Result<(), String> {
    for generation in (1..=3).rev() {
        let source = if generation == 1 {
            format!("{path}.bak")
        } else {
            format!("{path}.bak.{}", generation - 1)
        };
        let destination = format!("{path}.bak.{generation}");
        if Path::new(&source).exists() {
            if Path::new(&destination).exists() {
                std::fs::remove_file(&destination)
                    .map_err(|error| format!("Remove old backup {destination}: {error}"))?;
            }
            std::fs::rename(&source, &destination)
                .map_err(|error| format!("Rotate backup {source} to {destination}: {error}"))?;
        }
    }
    Ok(())
}

fn temporary_path_for(target: &Path) -> PathBuf {
    let mut name = std::ffi::OsString::from(".");
    name.push(target.file_name().unwrap_or_default());
    name.push(format!(
        ".{}-{}.tmp",
        std::process::id(),
        TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    target.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn relative_export_filename_is_saved_with_backup() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = format!("cluster-export-test-{}-{unique}.ino", std::process::id());
        write_with_backup(&path, "first sketch").unwrap();
        write_with_backup(&path, "second sketch").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second sketch");
        assert_eq!(
            std::fs::read_to_string(format!("{path}.bak")).unwrap(),
            "first sketch"
        );
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(format!("{path}.bak")).unwrap();
        std::fs::remove_file(save_lock_path(Path::new(&path))).unwrap();
    }

    #[test]
    fn write_with_backup_replaces_file_and_keeps_previous_copy() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("cluster-save-test-{unique}.json"));
        let backup = PathBuf::from(format!("{}.bak", path.display()));
        let backup_1 = PathBuf::from(format!("{}.bak.1", path.display()));
        let tmp = temporary_path_for(&path);

        write_with_backup(path.to_str().unwrap(), "old").unwrap();
        write_with_backup(path.to_str().unwrap(), "new").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "old");
        assert!(!tmp.exists());

        write_with_backup(path.to_str().unwrap(), "newer").unwrap();
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(&backup_1).unwrap(), "old");

        let _ = std::fs::remove_file(save_lock_path(&path));
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(backup);
        let _ = std::fs::remove_file(backup_1);
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

    fn temporary_test_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("cluster-{label}-{}-{unique}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn save_lock_prevents_competing_writers_and_releases_on_drop() {
        let root = temporary_test_dir("writer-lock");
        let path = root.join("circuit.json");
        write_with_backup(path.to_str().unwrap(), "original").unwrap();
        let lock = acquire_save_lock(&path).unwrap();
        assert!(write_with_backup(path.to_str().unwrap(), "competing save").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
        assert!(!root.join("circuit.json.bak").exists());
        drop(lock);
        write_with_backup(path.to_str().unwrap(), "next save").unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("circuit.json.bak")).unwrap(),
            "original"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn writer_lock_is_respected_across_processes() {
        const CHILD_PATH: &str = "CLUSTER_TEST_LOCKED_SAVE";
        if let Some(path) = std::env::var_os(CHILD_PATH) {
            assert!(write_with_backup(Path::new(&path).to_str().unwrap(), "child save").is_err());
            return;
        }
        let root = temporary_test_dir("process-lock");
        let path = root.join("circuit.json");
        write_with_backup(path.to_str().unwrap(), "parent save").unwrap();
        let lock = acquire_save_lock(&path).unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "storage::save::tests::writer_lock_is_respected_across_processes",
                "--nocapture",
            ])
            .env(CHILD_PATH, &path)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stderr)
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "parent save");
        drop(lock);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_backup_preserves_current_save_and_cleans_temporary_files() {
        let root = temporary_test_dir("backup-failure");
        let path = root.join("circuit.json");
        write_with_backup(path.to_str().unwrap(), "original").unwrap();
        std::fs::create_dir(root.join("circuit.json.bak")).unwrap();
        assert!(write_with_backup(path.to_str().unwrap(), "replacement").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
        assert!(root.join("circuit.json.bak").is_dir());
        assert!(
            !std::fs::read_dir(&root).unwrap().any(|e| e
                .unwrap()
                .path()
                .extension()
                .is_some_and(|s| s == "tmp"))
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn abandoned_legacy_temporary_file_is_not_truncated_or_published() {
        let root = temporary_test_dir("abandoned-save");
        let path = root.join("circuit.json");
        let abandoned = root.join("circuit.json.tmp");
        std::fs::write(&abandoned, "recoverable draft").unwrap();
        write_with_backup(path.to_str().unwrap(), "new save").unwrap();
        assert_eq!(
            std::fs::read_to_string(&abandoned).unwrap(),
            "recoverable draft"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new save");
        std::fs::remove_dir_all(root).unwrap();
    }
}
