use crate::model::CircuitSnapshot;

impl crate::CircuitApp {
    pub(crate) fn snapshot(&self) -> CircuitSnapshot {
        CircuitSnapshot {
            components: self.components.clone(),
            wires: self.wires.clone(),
            next_id: self.next_id,
            counters: self.counters.clone(),
            pages: self.effective_pages(),
            current_page: self.current_page,
            board: self.document.board.clone(),
        }
    }

    pub(crate) fn restore_snapshot(&mut self, snapshot: CircuitSnapshot) {
        self.components = snapshot.components;
        self.wires = snapshot.wires;
        self.next_id = snapshot.next_id;
        self.counters = snapshot.counters;
        self.pages = if snapshot.pages.is_empty() {
            vec![(
                "Page 1".to_string(),
                self.components.clone(),
                self.wires.clone(),
                self.next_id,
                self.counters.clone(),
            )]
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
        self.dispatch_changes(crate::commands::ChangeSet::schematic());
    }

    pub(crate) fn invalidate_connectivity_cache(&mut self) {
        self.analysis.circuit_revision = self.analysis.circuit_revision.saturating_add(1);
        self.analysis.cached_connectivity = None;
        self.invalidate_simulation_cache();
    }

    pub(crate) fn invalidate_simulation_cache(&mut self) {
        self.analysis.cached_simulation = None;
        self.analysis.cached_connected_pins = None;
    }

    /// Compatibility boundary for non-command inputs such as custom-part
    /// registry reloads. Document edits should dispatch a typed ChangeSet.
    pub(crate) fn invalidate_analysis_cache(&mut self) {
        self.invalidate_connectivity_cache();
    }

    pub(crate) fn record_history(&mut self) {
        self.editor.history.undo.push(crate::ui::app::HistoryEntry {
            snapshot: self.snapshot(),
            description: "Edit document",
            merge_key: None,
            created_at: std::time::Instant::now(),
        });
        if self.editor.history.undo.len() > 80 {
            self.editor.history.undo.remove(0);
        }
        self.editor.history.redo.clear();
        self.mark_dirty();
    }

    pub(crate) fn undo(&mut self) {
        let Some(entry) = self.editor.history.undo.pop() else {
            self.status = "Nothing to undo.".to_string();
            return;
        };
        self.editor.history.redo.push(crate::ui::app::HistoryEntry {
            snapshot: self.snapshot(),
            description: entry.description,
            merge_key: entry.merge_key,
            created_at: std::time::Instant::now(),
        });
        self.restore_snapshot(entry.snapshot);
        self.mark_dirty();
        self.status = format!("Undo: {}.", entry.description);
    }

    pub(crate) fn redo(&mut self) {
        let Some(entry) = self.editor.history.redo.pop() else {
            self.status = "Nothing to redo.".to_string();
            return;
        };
        self.editor.history.undo.push(crate::ui::app::HistoryEntry {
            snapshot: self.snapshot(),
            description: entry.description,
            merge_key: entry.merge_key,
            created_at: std::time::Instant::now(),
        });
        self.restore_snapshot(entry.snapshot);
        self.mark_dirty();
        self.status = format!("Redo: {}.", entry.description);
    }
}
