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
        self.selected = None;
        self.drag = None;
        self.draft_wire.clear();
    }

    pub(crate) fn mark_dirty(&mut self) {
        self.history_state.dirty = true;
        self.dirty_flags.mark_document_changed();
        self.invalidate_analysis_cache();
        self.simulation_run_state = if self.simulate {
            crate::ui::app::SimulationRunState::Dirty
        } else {
            crate::ui::app::SimulationRunState::Stopped
        };
    }

    pub(crate) fn invalidate_analysis_cache(&mut self) {
        self.circuit_revision = self.circuit_revision.saturating_add(1);
        self.cached_connectivity = None;
        self.cached_simulation = None;
        self.cached_connected_pins = None;
    }

    pub(crate) fn record_history(&mut self) {
        self.history_state.undo.push(self.snapshot());
        if self.history_state.undo.len() > 80 {
            self.history_state.undo.remove(0);
        }
        self.history_state.redo.clear();
        self.mark_dirty();
    }

    pub(crate) fn undo(&mut self) {
        let Some(snapshot) = self.history_state.undo.pop() else {
            self.status = "Nothing to undo.".to_string();
            return;
        };
        self.history_state.redo.push(self.snapshot());
        self.restore_snapshot(snapshot);
        self.mark_dirty();
        self.status = "Undo.".to_string();
    }

    pub(crate) fn redo(&mut self) {
        let Some(snapshot) = self.history_state.redo.pop() else {
            self.status = "Nothing to redo.".to_string();
            return;
        };
        self.history_state.undo.push(self.snapshot());
        self.restore_snapshot(snapshot);
        self.mark_dirty();
        self.status = "Redo.".to_string();
    }
}
