//! Publish a complete export generation with one directory rename.
use std::collections::HashSet;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static GENERATION: AtomicU64 = AtomicU64::new(0);

pub(crate) fn publish_text_bundle(
    root: &Path,
    files: &[(&str, String)],
) -> Result<PathBuf, String> {
    publish_with_writer(root, files, write_new_synced)
}

fn write_new_synced(path: &Path, contents: &str) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("Create {}: {error}", path.display()))?;
    file.write_all(contents.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("Write {}: {error}", path.display()))
}

fn publish_with_writer(
    root: &Path,
    files: &[(&str, String)],
    mut write: impl FnMut(&Path, &str) -> Result<(), String>,
) -> Result<PathBuf, String> {
    let mut names = HashSet::new();
    if files.is_empty() {
        return Err("Export bundle is empty.".into());
    }
    for (name, _) in files {
        let mut components = Path::new(name).components();
        if !matches!(components.next(), Some(Component::Normal(_)))
            || components.next().is_some()
            || *name == "manifest.json"
            || !names.insert(*name)
        {
            return Err(format!("Invalid or duplicate export filename: {name}"));
        }
    }
    std::fs::create_dir_all(root).map_err(|error| format!("Create {}: {error}", root.display()))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Read export timestamp: {error}"))?
        .as_nanos();
    let name = format!(
        "pcb-{timestamp}-{}-{}",
        std::process::id(),
        GENERATION.fetch_add(1, Ordering::Relaxed)
    );
    let stage_path = root.join(format!(".{name}.tmp"));
    let published = root.join(name);
    std::fs::create_dir(&stage_path).map_err(|error| {
        format!(
            "Create export staging folder {}: {error}",
            stage_path.display()
        )
    })?;
    let stage = StagedDirectory(stage_path);
    for (name, contents) in files {
        write(&stage.0.join(name), contents)?;
    }
    let manifest = serde_json::json!({
        "schema_version": 1,
        "generator": format!("Cluster {}", env!("CARGO_PKG_VERSION")),
        "experimental": true,
        "files": files.iter().map(|(name, contents)| serde_json::json!({"name": name, "bytes": contents.len()})).collect::<Vec<_>>(),
    });
    let manifest = serde_json::to_string_pretty(&manifest)
        .map_err(|error| format!("Create export manifest: {error}"))?;
    write(&stage.0.join("manifest.json"), &manifest)?;
    super::save::sync_directory(&stage.0)?;
    std::fs::rename(&stage.0, &published)
        .map_err(|error| format!("Publish export {}: {error}", published.display()))?;
    super::save::sync_directory(root)?;
    Ok(published)
}

struct StagedDirectory(PathBuf);
impl Drop for StagedDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "cluster-bundle-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            GENERATION.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn published_generations_are_complete_and_do_not_mix() {
        let root = test_root();
        let first = publish_text_bundle(
            &root,
            &[
                ("front.gbr", "old front".into()),
                ("drill.drl", "old drill".into()),
            ],
        )
        .unwrap();
        let second = publish_text_bundle(
            &root,
            &[
                ("front.gbr", "new front".into()),
                ("drill.drl", "new drill".into()),
            ],
        )
        .unwrap();
        assert_ne!(first, second);
        assert_eq!(
            std::fs::read_to_string(first.join("front.gbr")).unwrap(),
            "old front"
        );
        assert_eq!(
            std::fs::read_to_string(second.join("drill.drl")).unwrap(),
            "new drill"
        );
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(second.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["files"].as_array().unwrap().len(), 2);
        for file in manifest["files"].as_array().unwrap() {
            let bytes = std::fs::read(second.join(file["name"].as_str().unwrap())).unwrap();
            assert_eq!(bytes.len() as u64, file["bytes"].as_u64().unwrap());
        }
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_generation_never_replaces_previous_export() {
        let root = test_root();
        let old = publish_text_bundle(&root, &[("front.gbr", "original".into())]).unwrap();
        for fail_at in 0..3 {
            let mut writes = 0;
            let result = publish_with_writer(
                &root,
                &[
                    ("front.gbr", "replacement".into()),
                    ("drill.drl", "drill".into()),
                ],
                |path, contents| {
                    let current = writes;
                    writes += 1;
                    if current == fail_at {
                        return Err("Injected disk write failure".into());
                    }
                    write_new_synced(path, contents)
                },
            );
            assert!(result.is_err());
            assert_eq!(
                std::fs::read_to_string(old.join("front.gbr")).unwrap(),
                "original"
            );
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_bundle_paths_cannot_write_outside_generation() {
        let root = test_root();
        for names in [
            vec!["../escape.gbr"],
            vec!["manifest.json"],
            vec!["front.gbr", "front.gbr"],
            vec![],
        ] {
            let files = names
                .into_iter()
                .map(|name| (name, "data".into()))
                .collect::<Vec<_>>();
            assert!(publish_text_bundle(&root, &files).is_err());
            assert!(!root.exists());
        }
    }
}
