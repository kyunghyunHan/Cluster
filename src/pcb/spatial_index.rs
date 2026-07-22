use crate::model::cad::Point2;
use crate::pcb::board::{BoardFootprint, BoardOutline};
use crate::pcb::footprint::Footprint;
use crate::pcb::track::TrackSegment;
use crate::pcb::via::Via;
use std::collections::{HashMap, HashSet};

const CELL_MM: f32 = 8.0;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PadRef {
    pub(crate) footprint_id: u64,
    pub(crate) number: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PcbSpatialIndex {
    footprints: Grid<u64>,
    tracks: Grid<u64>,
    vias: Grid<u64>,
    pads: Grid<PadRef>,
    board_edges: Grid<usize>,
}

impl PcbSpatialIndex {
    pub(crate) fn build(
        footprints: &[BoardFootprint],
        tracks: &[TrackSegment],
        vias: &[Via],
        outline: &BoardOutline,
        library: &[Footprint],
    ) -> Self {
        let mut index = Self::default();
        for footprint in footprints {
            index.footprints.insert_bounds(
                footprint.id,
                Point2::new(footprint.position.x - 6.5, footprint.position.y - 3.5),
                Point2::new(footprint.position.x + 6.5, footprint.position.y + 3.5),
            );
            if let Some(definition) = library
                .iter()
                .find(|definition| definition.footprint_id == footprint.footprint_id)
            {
                for pad in &definition.pads {
                    let pad = footprint.transform().transform_pad(pad);
                    let position = pad.position;
                    index.pads.insert_bounds(
                        PadRef {
                            footprint_id: footprint.id,
                            number: pad.number.clone(),
                        },
                        Point2::new(position.x - pad.size.w * 0.5, position.y - pad.size.h * 0.5),
                        Point2::new(position.x + pad.size.w * 0.5, position.y + pad.size.h * 0.5),
                    );
                }
            }
        }
        for track in tracks {
            let margin = track.width_mm * 0.5 + 0.4;
            index.tracks.insert_bounds(
                track.id,
                Point2::new(
                    track.start.x.min(track.end.x) - margin,
                    track.start.y.min(track.end.y) - margin,
                ),
                Point2::new(
                    track.start.x.max(track.end.x) + margin,
                    track.start.y.max(track.end.y) + margin,
                ),
            );
        }
        for via in vias {
            let radius = via.diameter_mm * 0.75;
            index.vias.insert_bounds(
                via.id,
                Point2::new(via.position.x - radius, via.position.y - radius),
                Point2::new(via.position.x + radius, via.position.y + radius),
            );
        }
        for (edge, pair) in outline.points.windows(2).enumerate() {
            index.board_edges.insert_bounds(
                edge,
                Point2::new(pair[0].x.min(pair[1].x), pair[0].y.min(pair[1].y)),
                Point2::new(pair[0].x.max(pair[1].x), pair[0].y.max(pair[1].y)),
            );
        }
        index
    }

    pub(crate) fn add_footprint(&mut self, footprint: &BoardFootprint, library: &[Footprint]) {
        self.footprints.insert_bounds(
            footprint.id,
            Point2::new(footprint.position.x - 6.5, footprint.position.y - 3.5),
            Point2::new(footprint.position.x + 6.5, footprint.position.y + 3.5),
        );
        if let Some(definition) = library
            .iter()
            .find(|definition| definition.footprint_id == footprint.footprint_id)
        {
            for pad in &definition.pads {
                let pad = footprint.transform().transform_pad(pad);
                let position = pad.position;
                self.pads.insert_bounds(
                    PadRef {
                        footprint_id: footprint.id,
                        number: pad.number.clone(),
                    },
                    Point2::new(position.x - pad.size.w * 0.5, position.y - pad.size.h * 0.5),
                    Point2::new(position.x + pad.size.w * 0.5, position.y + pad.size.h * 0.5),
                );
            }
        }
    }

    pub(crate) fn update_footprint(&mut self, footprint: &BoardFootprint, library: &[Footprint]) {
        self.remove_footprint(footprint.id);
        self.add_footprint(footprint, library);
    }

    pub(crate) fn remove_footprint(&mut self, id: u64) {
        self.footprints.remove_value(&id);
        self.pads.retain(|pad| pad.footprint_id != id);
    }

    pub(crate) fn add_track(&mut self, track: &TrackSegment) {
        let margin = track.width_mm * 0.5 + 0.4;
        self.tracks.insert_bounds(
            track.id,
            Point2::new(
                track.start.x.min(track.end.x) - margin,
                track.start.y.min(track.end.y) - margin,
            ),
            Point2::new(
                track.start.x.max(track.end.x) + margin,
                track.start.y.max(track.end.y) + margin,
            ),
        );
    }

    pub(crate) fn update_track(&mut self, track: &TrackSegment) {
        self.tracks.remove_value(&track.id);
        self.add_track(track);
    }

    pub(crate) fn remove_track(&mut self, id: u64) {
        self.tracks.remove_value(&id);
    }

    pub(crate) fn add_via(&mut self, via: &Via) {
        let radius = via.diameter_mm * 0.75;
        self.vias.insert_bounds(
            via.id,
            Point2::new(via.position.x - radius, via.position.y - radius),
            Point2::new(via.position.x + radius, via.position.y + radius),
        );
    }

    pub(crate) fn update_via(&mut self, via: &Via) {
        self.vias.remove_value(&via.id);
        self.add_via(via);
    }

    pub(crate) fn remove_via(&mut self, id: u64) {
        self.vias.remove_value(&id);
    }

    pub(crate) fn update_outline(&mut self, outline: &BoardOutline) {
        self.board_edges = Grid::default();
        for (edge, pair) in outline.points.windows(2).enumerate() {
            self.board_edges.insert_bounds(
                edge,
                Point2::new(pair[0].x.min(pair[1].x), pair[0].y.min(pair[1].y)),
                Point2::new(pair[0].x.max(pair[1].x), pair[0].y.max(pair[1].y)),
            );
        }
    }

    pub(crate) fn footprint_candidates(&self, point: Point2) -> Vec<u64> {
        self.footprints.query_point(point)
    }

    pub(crate) fn footprints_in_rect(&self, min: Point2, max: Point2) -> Vec<u64> {
        self.footprints.query_bounds(min, max)
    }

    pub(crate) fn track_candidates(&self, point: Point2) -> Vec<u64> {
        self.tracks.query_point(point)
    }

    pub(crate) fn track_candidates_in_bounds(&self, min: Point2, max: Point2) -> Vec<u64> {
        self.tracks.query_bounds(min, max)
    }

    pub(crate) fn track_candidate_pairs(&self) -> Vec<(u64, u64)> {
        let mut pairs = HashSet::new();
        for candidates in self.tracks.buckets.values() {
            for (index, left) in candidates.iter().enumerate() {
                for right in &candidates[index + 1..] {
                    if left != right {
                        pairs.insert(if left < right {
                            (*left, *right)
                        } else {
                            (*right, *left)
                        });
                    }
                }
            }
        }
        pairs.into_iter().collect()
    }

    pub(crate) fn via_candidates(&self, point: Point2) -> Vec<u64> {
        self.vias.query_point(point)
    }

    pub(crate) fn via_candidates_in_bounds(&self, min: Point2, max: Point2) -> Vec<u64> {
        self.vias.query_bounds(min, max)
    }

    pub(crate) fn pad_candidates(&self, point: Point2) -> Vec<PadRef> {
        self.pads.query_point(point)
    }

    pub(crate) fn edge_candidates(&self, point: Point2) -> Vec<usize> {
        self.board_edges.query_point(point)
    }

    pub(crate) fn edges_in_rect(&self, min: Point2, max: Point2) -> Vec<usize> {
        self.board_edges.query_bounds(min, max)
    }
}

#[derive(Debug, Clone)]
struct Grid<T> {
    buckets: HashMap<(i32, i32), Vec<T>>,
}

impl<T> Default for Grid<T> {
    fn default() -> Self {
        Self {
            buckets: HashMap::new(),
        }
    }
}

impl<T: Clone + Eq + std::hash::Hash> Grid<T> {
    fn insert_bounds(&mut self, value: T, min: Point2, max: Point2) {
        for x in cell(min.x)..=cell(max.x) {
            for y in cell(min.y)..=cell(max.y) {
                self.buckets.entry((x, y)).or_default().push(value.clone());
            }
        }
    }

    fn remove_value(&mut self, value: &T) {
        self.retain(|candidate| candidate != value);
    }

    fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        self.buckets.retain(|_, values| {
            values.retain(|value| keep(value));
            !values.is_empty()
        });
    }

    fn query_point(&self, point: Point2) -> Vec<T> {
        self.buckets
            .get(&(cell(point.x), cell(point.y)))
            .cloned()
            .unwrap_or_default()
    }

    fn query_bounds(&self, min: Point2, max: Point2) -> Vec<T> {
        let mut result = HashSet::new();
        for x in cell(min.x)..=cell(max.x) {
            for y in cell(min.y)..=cell(max.y) {
                result.extend(self.buckets.get(&(x, y)).into_iter().flatten().cloned());
            }
        }
        result.into_iter().collect()
    }
}

fn cell(value: f32) -> i32 {
    (value / CELL_MM).floor() as i32
}
