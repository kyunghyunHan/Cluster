use chrono::{DateTime, FixedOffset, Local};
use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const NOTES_DIR: &str = "notes";

#[derive(Clone, Serialize, Deserialize)]
struct NoteFrontmatter {
    id: String,
    title: String,
    tags: Vec<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Clone)]
struct Note {
    meta: NoteFrontmatter,
    body: String,
    file_path: PathBuf,
}

#[derive(Default)]
struct DraftNote {
    body: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortMode {
    Recent,
    Title,
}

struct NotesApp {
    notes: Vec<Note>,
    selected_index: Option<usize>,
    draft: DraftNote,
    markdown_cache: CommonMarkCache,
    sort_mode: SortMode,
    status: String,
    errors: Vec<String>,
}

impl NotesApp {
    fn load() -> Self {
        let mut app = Self {
            notes: Vec::new(),
            selected_index: None,
            draft: DraftNote::default(),
            markdown_cache: CommonMarkCache::default(),
            sort_mode: SortMode::Recent,
            status: String::new(),
            errors: Vec::new(),
        };

        if let Err(err) = fs::create_dir_all(NOTES_DIR) {
            app.errors
                .push(format!("Failed to create notes directory: {err}"));
            return app;
        }

        let entries = match fs::read_dir(NOTES_DIR) {
            Ok(entries) => entries,
            Err(err) => {
                app.errors
                    .push(format!("Failed to read notes directory: {err}"));
                return app;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            match parse_note_file(&path) {
                Ok(note) => app.notes.push(note),
                Err(err) => app
                    .errors
                    .push(format!("Failed to load note ({}): {err}", path.display())),
            }
        }

        app.sort_notes();
        if !app.notes.is_empty() {
            app.select_note(0);
        }
        app
    }

    fn sort_notes(&mut self) {
        match self.sort_mode {
            SortMode::Recent => self.notes.sort_by(|a, b| {
                let a_time = parse_datetime(&a.meta.created_at);
                let b_time = parse_datetime(&b.meta.created_at);
                match (a_time, b_time) {
                    (Some(a_time), Some(b_time)) => b_time.cmp(&a_time),
                    _ => b.meta.created_at.cmp(&a.meta.created_at),
                }
            }),
            SortMode::Title => self.notes.sort_by(|a, b| a.meta.title.cmp(&b.meta.title)),
        }
    }

    fn select_note(&mut self, index: usize) {
        if index >= self.notes.len() {
            self.selected_index = None;
            return;
        }
        self.selected_index = Some(index);
        let note = &self.notes[index];
        self.draft.body = note.body.clone();
    }

    fn clear_selection(&mut self) {
        self.selected_index = None;
        self.draft = DraftNote::default();
    }

    fn save_current(&mut self) {
        let (title, tags) = parse_title_and_tags(&self.draft.body);
        if title.is_empty() {
            self.status = "Add a '# Title' line to save.".to_string();
            return;
        }

        let now = Local::now();

        let (meta, body, file_path) = if let Some(index) = self.selected_index {
            let existing = &mut self.notes[index];
            existing.meta.title = title.clone();
            existing.meta.tags = tags;
            existing.meta.updated_at = now.to_rfc3339();
            existing.body = self.draft.body.clone();
            (
                existing.meta.clone(),
                existing.body.clone(),
                existing.file_path.clone(),
            )
        } else {
            let created_at = now.to_rfc3339();
            let id = self.next_id_for_date(&now);
            (
                NoteFrontmatter {
                    id,
                    title: title.clone(),
                    tags,
                    created_at: created_at.clone(),
                    updated_at: created_at,
                },
                self.draft.body.clone(),
                PathBuf::new(),
            )
        };

        let date = meta
            .created_at
            .split('T')
            .next()
            .unwrap_or("unknown-date");
        let slug = slugify(&meta.title);
        let new_path = Path::new(NOTES_DIR).join(format!("{date}-{slug}.md"));

        if !file_path.as_os_str().is_empty() && file_path != new_path {
            if let Err(err) = fs::rename(&file_path, &new_path) {
                self.status = format!("Failed to rename file: {err}");
                return;
            }
        }

        if let Err(err) = write_note_file(&new_path, &meta, &body) {
            self.status = format!("Failed to save: {err}");
            return;
        }

        if let Some(index) = self.selected_index {
            self.notes[index].meta = meta;
            self.notes[index].body = body;
            self.notes[index].file_path = new_path;
        } else {
            self.notes.push(Note {
                meta,
                body,
                file_path: new_path,
            });
            self.sort_notes();
            if let Some(pos) = self
                .notes
                .iter()
                .position(|note| note.meta.title == title)
            {
                self.select_note(pos);
            }
        }

        self.status = "Saved.".to_string();
    }

    fn delete_current(&mut self) {
        let Some(index) = self.selected_index else {
            self.status = "No note selected to delete.".to_string();
            return;
        };

        let note = &self.notes[index];
        if let Err(err) = fs::remove_file(&note.file_path) {
            self.status = format!("Failed to delete: {err}");
            return;
        }

        self.notes.remove(index);
        self.clear_selection();
        self.status = "Deleted.".to_string();
    }

    fn next_id_for_date(&self, now: &DateTime<Local>) -> String {
        let date = now.format("%Y-%m-%d").to_string();
        let mut max_seq = 0;
        for note in &self.notes {
            if let Some((prefix, seq)) = note.meta.id.rsplit_once('-') {
                if prefix == date {
                    if let Ok(seq_num) = seq.parse::<i32>() {
                        if seq_num > max_seq {
                            max_seq = seq_num;
                        }
                    }
                }
            }
        }
        format!("{date}-{:03}", max_seq + 1)
    }

    fn backlinks_for(&self, title: &str) -> Vec<String> {
        let link_pattern = format!(r"\[\[{}\]\]", regex::escape(title));
        let regex = Regex::new(&link_pattern).unwrap_or_else(|_| Regex::new("$^").unwrap());
        let mut links = Vec::new();
        for note in &self.notes {
            if note.meta.title == title {
                continue;
            }
            if regex.is_match(&note.body) {
                links.push(note.meta.title.clone());
            }
        }
        links
    }

    fn tag_cloud(&self) -> BTreeMap<String, usize> {
        let mut tags = BTreeMap::new();
        for note in &self.notes {
            for tag in &note.meta.tags {
                *tags.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        tags
    }
}

impl eframe::App for NotesApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("New Note").clicked() {
                    self.clear_selection();
                }
                if ui.button("Save").clicked() {
                    self.save_current();
                }
                if ui.button("Delete").clicked() {
                    self.delete_current();
                }
                ui.separator();
                ui.label("Note");
                let mut picked_index = None;
                egui::ComboBox::from_id_source("note_selector")
                    .selected_text(
                        self.selected_index
                            .and_then(|idx| self.notes.get(idx))
                            .map(|note| note.meta.title.as_str())
                            .unwrap_or("Untitled"),
                    )
                    .show_ui(ui, |ui| {
                        for (index, note) in self.notes.iter().enumerate() {
                            if ui
                                .selectable_label(self.selected_index == Some(index), &note.meta.title)
                                .clicked()
                            {
                                picked_index = Some(index);
                            }
                        }
                    });
                if let Some(index) = picked_index {
                    self.select_note(index);
                }
                if !self.status.is_empty() {
                    ui.separator();
                    ui.label(&self.status);
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |columns| {
                columns[0].heading("Editor");
                egui::Frame::none()
                    .fill(egui::Color32::BLACK)
                    .show(&mut columns[0], |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.draft.body)
                                .desired_rows(18)
                                .frame(false),
                        );
                    });

                columns[1].heading("Preview");
                egui::Frame::none()
                    .fill(egui::Color32::BLACK)
                    .show(&mut columns[1], |ui| {
                        CommonMarkViewer::new()
                            .show(ui, &mut self.markdown_cache, &self.draft.body);
                    });
            });
        });

        if !self.errors.is_empty() {
            egui::Window::new("Load Errors").show(ctx, |ui| {
                for err in &self.errors {
                    ui.label(err);
                }
            });
        }
    }
}

fn parse_note_file(path: &Path) -> Result<Note, String> {
    let contents = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let (frontmatter, body) = split_frontmatter(&contents)?;
    let meta: NoteFrontmatter =
        serde_yaml::from_str(frontmatter).map_err(|err| err.to_string())?;
    Ok(Note {
        meta,
        body: body.to_string(),
        file_path: path.to_path_buf(),
    })
}

fn split_frontmatter(contents: &str) -> Result<(&str, &str), String> {
    if !contents.starts_with("---\n") {
        return Err("Missing YAML frontmatter start.".to_string());
    }
    let start = 4;
    if let Some(end) = contents[start..].find("\n---\n") {
        let frontmatter = &contents[start..start + end];
        let body = &contents[start + end + 5..];
        return Ok((frontmatter, body.trim_start()));
    }
    if let Some(end) = contents[start..].find("\n---") {
        let frontmatter = &contents[start..start + end];
        let body = &contents[start + end + 4..];
        return Ok((frontmatter, body.trim_start()));
    }
    Err("Missing YAML frontmatter end.".to_string())
}

fn write_note_file(path: &Path, meta: &NoteFrontmatter, body: &str) -> Result<(), String> {
    let frontmatter = serde_yaml::to_string(meta).map_err(|err| err.to_string())?;
    let contents = format!("---\n{frontmatter}---\n\n{body}\n");
    fs::write(path, contents).map_err(|err| err.to_string())?;
    Ok(())
}

fn parse_tags(tags: &str) -> Vec<String> {
    let mut set = BTreeSet::new();
    let cleaned = tags.trim();
    let cleaned = cleaned.strip_prefix('[').unwrap_or(cleaned);
    let cleaned = cleaned.strip_suffix(']').unwrap_or(cleaned);
    for tag in cleaned.split(',') {
        let trimmed = tag.trim();
        if !trimmed.is_empty() {
            set.insert(trimmed.to_string());
        }
    }
    set.into_iter().collect()
}

fn parse_title_and_tags(body: &str) -> (String, Vec<String>) {
    let mut title = String::new();
    let mut tags = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if title.is_empty() && trimmed.starts_with("# ") {
            title = trimmed[2..].trim().to_string();
        }
        if trimmed.to_lowercase().starts_with("tags:") {
            let rest = trimmed[5..].trim();
            tags = parse_tags(rest);
        }
    }
    (title, tags)
}

fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    while slug.starts_with('-') {
        slug.remove(0);
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "note".to_string()
    } else {
        slug
    }
}

fn parse_datetime(value: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value).ok()
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Cluster Notes",
        options,
        Box::new(|_cc| Ok(Box::new(NotesApp::load()))),
    )
}
