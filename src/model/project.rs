//! Runtime project document.
//!
//! This type owns user-created schematic data. It is deliberately free of
//! selections, tools, dialogs, caches, and simulation results. Persistence
//! continues through the schema-versioned `SavedCircuit` compatibility DTO.

use super::{Component, Counters, Wire};
use crate::pcb::board::Board;

#[allow(clippy::type_complexity)] // Replaced by ProjectPage in the next schema-neutral step.
pub(crate) type LegacyPageState = (String, Vec<Component>, Vec<Wire>, u64, Counters);

#[derive(Debug, Clone)]
pub(crate) struct ProjectDocument {
    pub(crate) components: Vec<Component>,
    pub(crate) wires: Vec<Wire>,
    pub(crate) next_id: u64,
    pub(crate) counters: Counters,
    pub(crate) pages: Vec<LegacyPageState>,
    pub(crate) current_page: usize,
    pub(crate) board: Board,
}

impl Default for ProjectDocument {
    fn default() -> Self {
        Self {
            components: Vec::new(),
            wires: Vec::new(),
            next_id: 1,
            counters: Counters::default(),
            pages: vec![(
                "Page 1".to_string(),
                Vec::new(),
                Vec::new(),
                1,
                Counters::default(),
            )],
            current_page: 0,
            board: Board::new_two_layer(80.0, 50.0),
        }
    }
}
