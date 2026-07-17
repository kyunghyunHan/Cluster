use crate::model::{
    CircuitSnapshot, Component, Counters, ProjectDocument, ProjectPage, SchematicAnnotations, Wire,
};
use crate::pcb::board::Board;
use std::collections::{HashMap, HashSet};
use std::mem::size_of;

#[derive(Debug, Clone, PartialEq)]
struct ListValue<T> {
    index: usize,
    value: T,
}

#[derive(Debug, Clone, PartialEq)]
struct ListDelta<T> {
    id: u64,
    before: Option<ListValue<T>>,
    after: Option<ListValue<T>>,
}

#[derive(Debug, Clone, PartialEq)]
struct DocumentMetadata {
    next_id: u64,
    counters: Counters,
    pages: Vec<ProjectPage>,
    current_page: usize,
}

/// Reversible user-document changes. Unchanged entities are not retained.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DocumentDelta {
    components: Vec<ListDelta<Component>>,
    wires: Vec<ListDelta<Wire>>,
    annotations: Option<(SchematicAnnotations, SchematicAnnotations)>,
    metadata: Option<(DocumentMetadata, DocumentMetadata)>,
    board: Option<(Board, Board)>,
}

/// Common reversible command behavior used by schematic and PCB history.
pub(crate) trait UndoableCommand {
    fn apply(&self, document: &mut ProjectDocument);
    fn undo(&self, document: &mut ProjectDocument);
    fn merge_with(&mut self, newer: &Self) -> bool;
    fn memory_cost(&self) -> usize;
    fn is_empty(&self) -> bool;
}

impl DocumentDelta {
    pub(crate) fn between(before: &CircuitSnapshot, after: &CircuitSnapshot) -> Self {
        let before_metadata = DocumentMetadata {
            next_id: before.next_id,
            counters: before.counters.clone(),
            pages: before.pages.clone(),
            current_page: before.current_page,
        };
        let after_metadata = DocumentMetadata {
            next_id: after.next_id,
            counters: after.counters.clone(),
            pages: after.pages.clone(),
            current_page: after.current_page,
        };
        Self {
            components: diff_list(&before.components, &after.components, |value| value.id),
            wires: diff_list(&before.wires, &after.wires, |value| value.id),
            annotations: (before.annotations != after.annotations)
                .then(|| (before.annotations.clone(), after.annotations.clone())),
            metadata: (before_metadata != after_metadata)
                .then_some((before_metadata, after_metadata)),
            board: (before.board != after.board)
                .then(|| (before.board.clone(), after.board.clone())),
        }
    }

    fn apply_direction(&self, document: &mut ProjectDocument, forward: bool) {
        apply_list(
            &mut document.components,
            &self.components,
            forward,
            |value| value.id,
        );
        apply_list(&mut document.wires, &self.wires, forward, |value| value.id);
        if let Some((before, after)) = &self.annotations {
            document.annotations = if forward { after } else { before }.clone();
        }
        if let Some((before, after)) = &self.metadata {
            let metadata = if forward { after } else { before };
            document.next_id = metadata.next_id;
            document.counters = metadata.counters.clone();
            document.pages = metadata.pages.clone();
            document.current_page = metadata.current_page;
        }
        if let Some((before, after)) = &self.board {
            document.board = if forward { after } else { before }.clone();
        }
    }
}

impl UndoableCommand for DocumentDelta {
    fn apply(&self, document: &mut ProjectDocument) {
        self.apply_direction(document, true);
    }

    fn undo(&self, document: &mut ProjectDocument) {
        self.apply_direction(document, false);
    }

    fn merge_with(&mut self, newer: &Self) -> bool {
        merge_list(&mut self.components, &newer.components);
        merge_list(&mut self.wires, &newer.wires);
        merge_pair(&mut self.annotations, &newer.annotations);
        merge_pair(&mut self.metadata, &newer.metadata);
        merge_pair(&mut self.board, &newer.board);
        true
    }

    fn memory_cost(&self) -> usize {
        let component_strings = self
            .components
            .iter()
            .flat_map(|delta| [delta.before.as_ref(), delta.after.as_ref()])
            .flatten()
            .map(|value| {
                value.value.label.capacity()
                    + value.value.value.capacity()
                    + value.value.part_id.as_ref().map_or(0, String::capacity)
            })
            .sum::<usize>();
        let wire_points = self
            .wires
            .iter()
            .flat_map(|delta| [delta.before.as_ref(), delta.after.as_ref()])
            .flatten()
            .map(|value| value.value.points.capacity() * size_of::<egui::Pos2>())
            .sum::<usize>();
        size_of::<Self>()
            + self.components.capacity() * size_of::<ListDelta<Component>>()
            + self.wires.capacity() * size_of::<ListDelta<Wire>>()
            + component_strings
            + wire_points
            + self.annotations.as_ref().map_or(0, |(before, after)| {
                (before.junction_dots.capacity()
                    + before.no_connect_markers.capacity()
                    + after.junction_dots.capacity()
                    + after.no_connect_markers.capacity())
                    * size_of::<egui::Pos2>()
            })
            + self.board.as_ref().map_or(0, |(before, after)| {
                approximate_board_cost(before) + approximate_board_cost(after)
            })
    }

    fn is_empty(&self) -> bool {
        self.components.is_empty()
            && self.wires.is_empty()
            && self.annotations.is_none()
            && self.metadata.is_none()
            && self.board.is_none()
    }
}

fn diff_list<T: Clone + PartialEq>(
    before: &[T],
    after: &[T],
    id: impl Fn(&T) -> u64,
) -> Vec<ListDelta<T>> {
    let before_by_id = before
        .iter()
        .enumerate()
        .map(|(index, value)| (id(value), (index, value)))
        .collect::<HashMap<_, _>>();
    let after_by_id = after
        .iter()
        .enumerate()
        .map(|(index, value)| (id(value), (index, value)))
        .collect::<HashMap<_, _>>();
    let mut ids = before_by_id
        .keys()
        .chain(after_by_id.keys())
        .copied()
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter()
        .filter_map(|entity_id| {
            let before = before_by_id
                .get(&entity_id)
                .map(|(index, value)| ListValue {
                    index: *index,
                    value: (*value).clone(),
                });
            let after = after_by_id.get(&entity_id).map(|(index, value)| ListValue {
                index: *index,
                value: (*value).clone(),
            });
            (before != after).then_some(ListDelta {
                id: entity_id,
                before,
                after,
            })
        })
        .collect()
}

fn apply_list<T: Clone>(
    target: &mut Vec<T>,
    changes: &[ListDelta<T>],
    forward: bool,
    id: impl Fn(&T) -> u64,
) {
    let changed_ids = changes
        .iter()
        .map(|change| change.id)
        .collect::<HashSet<_>>();
    target.retain(|value| !changed_ids.contains(&id(value)));
    let mut insertions = changes
        .iter()
        .filter_map(|change| {
            if forward {
                change.after.as_ref()
            } else {
                change.before.as_ref()
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    insertions.sort_by_key(|value| value.index);
    for insertion in insertions {
        let index = insertion.index.min(target.len());
        target.insert(index, insertion.value);
    }
}

fn merge_list<T: Clone + PartialEq>(target: &mut Vec<ListDelta<T>>, newer: &[ListDelta<T>]) {
    for change in newer {
        if let Some(existing) = target.iter_mut().find(|existing| existing.id == change.id) {
            existing.after = change.after.clone();
        } else {
            target.push(change.clone());
        }
    }
    target.retain(|change| change.before != change.after);
}

fn merge_pair<T: Clone + PartialEq>(target: &mut Option<(T, T)>, newer: &Option<(T, T)>) {
    let Some((newer_before, newer_after)) = newer else {
        return;
    };
    match target {
        Some((before, after)) => {
            *after = newer_after.clone();
            if before == after {
                *target = None;
            }
        }
        None if newer_before != newer_after => {
            *target = Some((newer_before.clone(), newer_after.clone()));
        }
        None => {}
    }
}

fn approximate_board_cost(board: &Board) -> usize {
    size_of::<Board>()
        + board.tracks.capacity() * size_of::<crate::pcb::track::TrackSegment>()
        + board.vias.capacity() * size_of::<crate::pcb::via::Via>()
        + board.footprints.capacity() * size_of::<crate::pcb::board::BoardFootprint>()
        + board.outline.points.capacity() * size_of::<crate::model::cad::Point2>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ComponentKind;
    use egui::Pos2;

    fn component(id: u64, x: f32) -> Component {
        Component {
            id,
            kind: ComponentKind::Resistor,
            pos: Pos2::new(x, 0.0),
            rotation: 0,
            label: format!("R{id}"),
            value: "1k".to_string(),
            part_id: None,
        }
    }

    #[test]
    fn entity_delta_round_trips_add_move_and_delete() {
        let mut before = crate::CircuitApp::new();
        before.document.components = vec![component(1, 10.0), component(2, 20.0)];
        let before_snapshot = before.snapshot();
        before.document.components.remove(0);
        before.document.components[0].pos.x = 30.0;
        before.document.components.push(component(3, 40.0));
        let after_snapshot = before.snapshot();
        let delta = DocumentDelta::between(&before_snapshot, &after_snapshot);

        delta.undo(&mut before.document);
        assert_eq!(before.document.components, before_snapshot.components);
        delta.apply(&mut before.document);
        assert_eq!(before.document.components, after_snapshot.components);
    }
}
