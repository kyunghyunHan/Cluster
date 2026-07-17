//! Runtime project document.
//!
//! This type owns user-created schematic data. It is deliberately free of
//! selections, tools, dialogs, caches, and simulation results. Persistence
//! continues through the schema-versioned `SavedCircuit` compatibility DTO.

use super::{Component, Counters, SchematicAnnotations, Wire};
use crate::pcb::board::Board;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProjectPage {
    pub(crate) name: String,
    pub(crate) components: Vec<Component>,
    pub(crate) wires: Vec<Wire>,
    pub(crate) next_id: u64,
    pub(crate) counters: Counters,
    pub(crate) annotations: SchematicAnnotations,
}

impl ProjectPage {
    pub(crate) fn empty(name: String) -> Self {
        Self {
            name,
            components: Vec::new(),
            wires: Vec::new(),
            next_id: 1,
            counters: Counters::default(),
            annotations: SchematicAnnotations::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectDocument {
    pub(crate) components: Vec<Component>,
    pub(crate) wires: Vec<Wire>,
    pub(crate) next_id: u64,
    pub(crate) counters: Counters,
    pub(crate) annotations: SchematicAnnotations,
    pub(crate) pages: Vec<ProjectPage>,
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
            annotations: SchematicAnnotations::default(),
            pages: vec![ProjectPage::empty("Page 1".to_string())],
            current_page: 0,
            board: Board::new_two_layer(80.0, 50.0),
        }
    }
}
