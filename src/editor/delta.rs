use crate::model::{
    CircuitSnapshot, Component, Counters, ProjectDocument, ProjectPage, SchematicAnnotations, Wire,
};
use crate::pcb::board::{
    Board, BoardFootprint, BoardOutline, DesignRules, GridSettings, LayerVisibility, Zone,
};
use crate::pcb::layer::BoardLayer;
use crate::pcb::track::TrackSegment;
use crate::pcb::via::Via;
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

#[derive(Debug, Clone, PartialEq)]
struct BoardMetadata {
    schema_version: u32,
    outline: BoardOutline,
    layers: Vec<BoardLayer>,
    layer_visibility: Vec<LayerVisibility>,
    grid: GridSettings,
    zones: Vec<Zone>,
    footprint_library: Vec<crate::pcb::footprint::Footprint>,
    design_rules: DesignRules,
    net_classes: Vec<crate::model::cad::NetClass>,
}

impl BoardMetadata {
    fn from_board(board: &Board) -> Self {
        Self {
            schema_version: board.schema_version,
            outline: board.outline.clone(),
            layers: board.layers.clone(),
            layer_visibility: board.layer_visibility.clone(),
            grid: board.grid.clone(),
            zones: board.zones.clone(),
            footprint_library: board.footprint_library.clone(),
            design_rules: board.design_rules.clone(),
            net_classes: board.net_classes.clone(),
        }
    }

    fn apply_to(&self, board: &mut Board) {
        board.schema_version = self.schema_version;
        board.outline = self.outline.clone();
        board.layers.clone_from(&self.layers);
        board.layer_visibility.clone_from(&self.layer_visibility);
        board.grid = self.grid.clone();
        board.zones.clone_from(&self.zones);
        board.footprint_library.clone_from(&self.footprint_library);
        board.design_rules = self.design_rules.clone();
        board.net_classes.clone_from(&self.net_classes);
    }
}

/// Reversible user-document changes. Unchanged entities are not retained.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DocumentDelta {
    components: Vec<ListDelta<Component>>,
    wires: Vec<ListDelta<Wire>>,
    annotations: Option<(SchematicAnnotations, SchematicAnnotations)>,
    metadata: Option<(DocumentMetadata, DocumentMetadata)>,
    board_footprints: Vec<ListDelta<BoardFootprint>>,
    board_tracks: Vec<ListDelta<TrackSegment>>,
    board_vias: Vec<ListDelta<Via>>,
    board_metadata: Option<(BoardMetadata, BoardMetadata)>,
}

pub(crate) struct BoardDeltaCapture {
    footprint_ids: HashSet<u64>,
    track_ids: HashSet<u64>,
    via_ids: HashSet<u64>,
    footprint_ids_before: HashSet<u64>,
    before_footprints: HashMap<u64, ListValue<BoardFootprint>>,
    before_tracks: HashMap<u64, ListValue<TrackSegment>>,
    before_vias: HashMap<u64, ListValue<Via>>,
    before_metadata: Option<BoardMetadata>,
    capture_new_footprints: bool,
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
    pub(crate) fn empty() -> Self {
        Self {
            components: Vec::new(),
            wires: Vec::new(),
            annotations: None,
            metadata: None,
            board_footprints: Vec::new(),
            board_tracks: Vec::new(),
            board_vias: Vec::new(),
            board_metadata: None,
        }
    }

    pub(crate) fn capture_board(
        board: &Board,
        scope: crate::commands::pcb::PcbDeltaScope,
    ) -> BoardDeltaCapture {
        BoardDeltaCapture {
            before_footprints: capture_values(&board.footprints, &scope.footprint_ids, |value| {
                value.id
            }),
            before_tracks: capture_values(&board.tracks, &scope.track_ids, |value| value.id),
            before_vias: capture_values(&board.vias, &scope.via_ids, |value| value.id),
            footprint_ids_before: board
                .footprints
                .iter()
                .map(|footprint| footprint.id)
                .collect(),
            before_metadata: scope
                .board_metadata
                .then(|| BoardMetadata::from_board(board)),
            footprint_ids: scope.footprint_ids,
            track_ids: scope.track_ids,
            via_ids: scope.via_ids,
            capture_new_footprints: scope.capture_new_footprints,
        }
    }

    pub(crate) fn from_board_capture(capture: BoardDeltaCapture, board: &Board) -> Self {
        let mut footprint_ids = capture.footprint_ids;
        if capture.capture_new_footprints {
            footprint_ids.extend(
                board
                    .footprints
                    .iter()
                    .filter(|footprint| !capture.footprint_ids_before.contains(&footprint.id))
                    .map(|footprint| footprint.id),
            );
        }
        let after_footprints = capture_values(&board.footprints, &footprint_ids, |value| value.id);
        let after_tracks = capture_values(&board.tracks, &capture.track_ids, |value| value.id);
        let after_vias = capture_values(&board.vias, &capture.via_ids, |value| value.id);
        let before_metadata = capture.before_metadata;
        let after_metadata = before_metadata
            .as_ref()
            .map(|_| BoardMetadata::from_board(board));
        Self {
            components: Vec::new(),
            wires: Vec::new(),
            annotations: None,
            metadata: None,
            board_footprints: diff_captured(
                capture.before_footprints,
                after_footprints,
                &footprint_ids,
            ),
            board_tracks: diff_captured(capture.before_tracks, after_tracks, &capture.track_ids),
            board_vias: diff_captured(capture.before_vias, after_vias, &capture.via_ids),
            board_metadata: before_metadata
                .zip(after_metadata)
                .filter(|(before, after)| before != after),
        }
    }

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
            board_footprints: diff_list(
                &before.board.footprints,
                &after.board.footprints,
                |value| value.id,
            ),
            board_tracks: diff_list(&before.board.tracks, &after.board.tracks, |value| value.id),
            board_vias: diff_list(&before.board.vias, &after.board.vias, |value| value.id),
            board_metadata: {
                let before = BoardMetadata::from_board(&before.board);
                let after = BoardMetadata::from_board(&after.board);
                (before != after).then_some((before, after))
            },
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
        apply_list(
            &mut document.board.footprints,
            &self.board_footprints,
            forward,
            |value| value.id,
        );
        apply_list(
            &mut document.board.tracks,
            &self.board_tracks,
            forward,
            |value| value.id,
        );
        apply_list(
            &mut document.board.vias,
            &self.board_vias,
            forward,
            |value| value.id,
        );
        if let Some((before, after)) = &self.board_metadata {
            (if forward { after } else { before }).apply_to(&mut document.board);
        }
        document.board.rebuild_entity_index();
    }
}

fn capture_values<T: Clone>(
    values: &[T],
    ids: &HashSet<u64>,
    id: impl Fn(&T) -> u64,
) -> HashMap<u64, ListValue<T>> {
    values
        .iter()
        .enumerate()
        .filter(|(_, value)| ids.contains(&id(value)))
        .map(|(index, value)| {
            (
                id(value),
                ListValue {
                    index,
                    value: value.clone(),
                },
            )
        })
        .collect()
}

fn diff_captured<T: Clone + PartialEq>(
    mut before: HashMap<u64, ListValue<T>>,
    mut after: HashMap<u64, ListValue<T>>,
    ids: &HashSet<u64>,
) -> Vec<ListDelta<T>> {
    let mut ids = ids.iter().copied().collect::<Vec<_>>();
    ids.sort_unstable();
    ids.into_iter()
        .filter_map(|id| {
            let before = before.remove(&id);
            let after = after.remove(&id);
            (before != after).then_some(ListDelta { id, before, after })
        })
        .collect()
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
        merge_list(&mut self.board_footprints, &newer.board_footprints);
        merge_list(&mut self.board_tracks, &newer.board_tracks);
        merge_list(&mut self.board_vias, &newer.board_vias);
        merge_pair(&mut self.board_metadata, &newer.board_metadata);
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
            + list_delta_cost(&self.board_footprints)
            + list_delta_cost(&self.board_tracks)
            + list_delta_cost(&self.board_vias)
            + self.board_metadata.as_ref().map_or(0, |(before, after)| {
                approximate_board_metadata_cost(before) + approximate_board_metadata_cost(after)
            })
    }

    fn is_empty(&self) -> bool {
        self.components.is_empty()
            && self.wires.is_empty()
            && self.annotations.is_none()
            && self.metadata.is_none()
            && self.board_footprints.is_empty()
            && self.board_tracks.is_empty()
            && self.board_vias.is_empty()
            && self.board_metadata.is_none()
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

fn list_delta_cost<T>(values: &[ListDelta<T>]) -> usize {
    std::mem::size_of_val(values)
}

fn approximate_board_metadata_cost(metadata: &BoardMetadata) -> usize {
    size_of::<BoardMetadata>()
        + metadata.outline.points.capacity() * size_of::<crate::model::cad::Point2>()
        + metadata.layers.capacity() * size_of::<BoardLayer>()
        + metadata.layer_visibility.capacity() * size_of::<LayerVisibility>()
        + metadata.zones.capacity() * size_of::<Zone>()
        + metadata.footprint_library.capacity() * size_of::<crate::pcb::footprint::Footprint>()
        + metadata.net_classes.capacity() * size_of::<crate::model::cad::NetClass>()
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
