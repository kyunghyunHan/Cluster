use crate::editor::delta::{DocumentDelta, UndoableCommand};
use crate::model::CircuitSnapshot;

const HISTORY_MEMORY_BUDGET: usize = 16 * 1024 * 1024;
const HISTORY_ENTRY_LIMIT: usize = 512;

impl crate::CircuitApp {
    pub(crate) fn snapshot(&self) -> CircuitSnapshot {
        CircuitSnapshot {
            components: self.components.clone(),
            wires: self.wires.clone(),
            next_id: self.next_id,
            counters: self.counters.clone(),
            annotations: self.annotations.clone(),
            pages: self.effective_project_pages(),
            current_page: self.current_page,
            board: self.document.board.clone(),
        }
    }

    pub(crate) fn restore_snapshot(&mut self, snapshot: CircuitSnapshot) {
        self.components = snapshot.components;
        self.wires = snapshot.wires;
        self.next_id = snapshot.next_id;
        self.counters = snapshot.counters;
        self.annotations = snapshot.annotations;
        self.pages = if snapshot.pages.is_empty() {
            vec![crate::model::ProjectPage {
                name: "Page 1".to_string(),
                components: self.components.clone(),
                wires: self.wires.clone(),
                next_id: self.next_id,
                counters: self.counters.clone(),
                annotations: self.annotations.clone(),
            }]
        } else {
            snapshot.pages
        };
        self.current_page = snapshot
            .current_page
            .min(self.pages.len().saturating_sub(1));
        self.document.board = snapshot.board;
        self.editor.selected = None;
        self.editor.drag = None;
        self.editor.draft_wire.clear();
    }

    pub(crate) fn mark_dirty(&mut self) {
        self.finish_history_transaction();
        self.dispatch_changes(crate::commands::ChangeSet::schematic());
    }

    pub(crate) fn invalidate_connectivity_cache(&mut self) {
        self.analysis.cached_connectivity = None;
        self.analysis.cached_netlist = None;
        self.analysis.cached_connected_pins = None;
        self.invalidate_simulation_cache();
    }

    pub(crate) fn invalidate_simulation_cache(&mut self) {
        self.analysis.cached_simulation = None;
    }

    /// Compatibility boundary for non-command inputs such as custom-part
    /// registry reloads. Document edits should dispatch a typed ChangeSet.
    pub(crate) fn invalidate_analysis_cache(&mut self) {
        self.invalidate_connectivity_cache();
    }

    pub(crate) fn record_history(&mut self) {
        self.begin_history_transaction("Edit document", None);
    }

    pub(crate) fn begin_history_transaction(
        &mut self,
        description: &'static str,
        merge_key: Option<crate::commands::CommandMergeKey>,
    ) {
        if self.editor.history.pending.is_some() {
            return;
        }
        self.editor.history.pending = Some(crate::ui::app::PendingHistory {
            snapshot: self.snapshot(),
            description,
            merge_key,
        });
    }

    pub(crate) fn finish_history_transaction(&mut self) {
        let Some(pending) = self.editor.history.pending.take() else {
            return;
        };
        let delta = DocumentDelta::between(&pending.snapshot, &self.snapshot());
        self.push_history_delta(delta, pending.description, pending.merge_key);
    }

    pub(crate) fn push_history_delta(
        &mut self,
        delta: DocumentDelta,
        description: &'static str,
        merge_key: Option<crate::commands::CommandMergeKey>,
    ) {
        if delta.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        let merges_with_previous = merge_key.is_some()
            && self.editor.history.undo.back().is_some_and(|entry| {
                entry.merge_key == merge_key
                    && entry.created_at.elapsed() <= std::time::Duration::from_millis(750)
            });
        if merges_with_previous {
            if let Some(previous) = self.editor.history.undo.back_mut() {
                self.editor.history.undo_memory_bytes = self
                    .editor
                    .history
                    .undo_memory_bytes
                    .saturating_sub(previous.memory_cost);
                previous.delta.merge_with(&delta);
                previous.memory_cost = previous.delta.memory_cost();
                previous.created_at = now;
                self.editor.history.undo_memory_bytes = self
                    .editor
                    .history
                    .undo_memory_bytes
                    .saturating_add(previous.memory_cost);
            }
        } else {
            let memory_cost = delta.memory_cost();
            self.editor
                .history
                .undo
                .push_back(crate::ui::app::HistoryEntry {
                    delta,
                    description,
                    merge_key,
                    created_at: now,
                    memory_cost,
                });
            self.editor.history.undo_memory_bytes = self
                .editor
                .history
                .undo_memory_bytes
                .saturating_add(memory_cost);
        }
        while self.editor.history.undo.len() > HISTORY_ENTRY_LIMIT
            || self.editor.history.undo_memory_bytes > HISTORY_MEMORY_BUDGET
        {
            let Some(oldest) = self.editor.history.undo.pop_front() else {
                break;
            };
            self.editor.history.undo_memory_bytes = self
                .editor
                .history
                .undo_memory_bytes
                .saturating_sub(oldest.memory_cost);
        }
        self.editor.history.redo.clear();
    }

    pub(crate) fn undo(&mut self) {
        self.editor.history.pending = None;
        let Some(entry) = self.editor.history.undo.pop_back() else {
            self.status = "Nothing to undo.".to_string();
            return;
        };
        self.editor.history.undo_memory_bytes = self
            .editor
            .history
            .undo_memory_bytes
            .saturating_sub(entry.memory_cost);
        let description = entry.description;
        entry.delta.undo(&mut self.document);
        self.reset_editor_after_history_navigation();
        self.editor.history.redo.push_back(entry);
        self.dispatch_changes(crate::commands::ChangeSet::restored_document());
        self.status = format!("Undo: {description}.");
    }

    pub(crate) fn redo(&mut self) {
        self.editor.history.pending = None;
        let Some(entry) = self.editor.history.redo.pop_back() else {
            self.status = "Nothing to redo.".to_string();
            return;
        };
        let description = entry.description;
        entry.delta.apply(&mut self.document);
        self.reset_editor_after_history_navigation();
        self.editor.history.undo_memory_bytes = self
            .editor
            .history
            .undo_memory_bytes
            .saturating_add(entry.memory_cost);
        self.editor.history.undo.push_back(entry);
        self.dispatch_changes(crate::commands::ChangeSet::restored_document());
        self.status = format!("Redo: {description}.");
    }

    fn reset_editor_after_history_navigation(&mut self) {
        self.editor.selected = None;
        self.editor.multi_selected.clear();
        self.editor.drag = None;
        self.editor.draft_wire.clear();
    }
}
