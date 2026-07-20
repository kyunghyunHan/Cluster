use crate::commands::ChangeSet;
use crate::commands::context::{CommandContext, CommandOutcome};
use crate::model::cad::{CadNet, NetClass, Point2, SymbolInstance};
use crate::pcb::board::{BoardOutline, RemovedFootprintPolicy};
use crate::pcb::track::TrackSegment;
use crate::pcb::via::Via;
use std::collections::HashSet;

pub(crate) struct PcbDeltaScope {
    pub(crate) footprint_ids: HashSet<u64>,
    pub(crate) track_ids: HashSet<u64>,
    pub(crate) via_ids: HashSet<u64>,
    pub(crate) capture_new_footprints: bool,
    pub(crate) board_metadata: bool,
}

impl PcbDeltaScope {
    fn empty() -> Self {
        Self {
            footprint_ids: HashSet::new(),
            track_ids: HashSet::new(),
            via_ids: HashSet::new(),
            capture_new_footprints: false,
            board_metadata: false,
        }
    }
}

#[allow(dead_code)]
pub(crate) enum PcbCommand {
    MoveFootprint {
        footprint_id: u64,
        position: Point2,
    },
    MoveFootprints(Vec<(u64, Point2)>),
    RotateFootprint {
        footprint_id: u64,
        delta_deg: f32,
    },
    RotateFootprints {
        footprint_ids: Vec<u64>,
        delta_deg: f32,
    },
    FlipFootprints {
        footprint_ids: Vec<u64>,
    },
    AddTrack(TrackSegment),
    AddRoute {
        tracks: Vec<TrackSegment>,
        vias: Vec<Via>,
    },
    RemoveTrack {
        track_id: u64,
    },
    DeleteTracks {
        track_ids: Vec<u64>,
    },
    EditTrack(TrackSegment),
    AddVia(Via),
    RemoveVia {
        via_id: u64,
    },
    DeleteVias {
        via_ids: Vec<u64>,
    },
    SetOutline(BoardOutline),
    SetGeometry {
        footprint_positions: Vec<(u64, Point2)>,
        tracks: Vec<TrackSegment>,
        vias: Vec<Via>,
        outline: BoardOutline,
    },
    ChangeNetClass(NetClass),
    ApplyEco {
        symbols: Vec<SymbolInstance>,
        nets: Vec<CadNet>,
        removed_policy: RemovedFootprintPolicy,
    },
}

pub(crate) struct PcbAnalysisImpact {
    pub(crate) track_ids: HashSet<u64>,
    pub(crate) net_ids: HashSet<usize>,
}

impl PcbCommand {
    pub(crate) fn local_analysis_impact(
        &self,
        board: &crate::pcb::board::Board,
    ) -> Option<PcbAnalysisImpact> {
        let mut track_ids = HashSet::new();
        let mut net_ids = HashSet::new();
        match self {
            Self::AddTrack(track) | Self::EditTrack(track) => {
                track_ids.insert(track.id);
                net_ids.insert(track.net_id);
                if let Some(previous) = board.track(track.id) {
                    net_ids.insert(previous.net_id);
                }
            }
            Self::AddRoute { tracks, .. } => {
                track_ids.extend(tracks.iter().map(|track| track.id));
                net_ids.extend(tracks.iter().map(|track| track.net_id));
            }
            Self::RemoveTrack { track_id } => {
                track_ids.insert(*track_id);
                net_ids.extend(board.track(*track_id).map(|track| track.net_id));
            }
            Self::DeleteTracks { track_ids: ids } => {
                track_ids.extend(ids);
                net_ids.extend(
                    ids.iter()
                        .filter_map(|id| board.track(*id))
                        .map(|track| track.net_id),
                );
            }
            _ => return None,
        }
        Some(PcbAnalysisImpact { track_ids, net_ids })
    }

    pub(crate) fn delta_scope(&self, board: &crate::pcb::board::Board) -> PcbDeltaScope {
        let mut scope = PcbDeltaScope::empty();
        match self {
            Self::MoveFootprint { footprint_id, .. }
            | Self::RotateFootprint { footprint_id, .. } => {
                scope.footprint_ids.insert(*footprint_id);
            }
            Self::MoveFootprints(moves) => {
                scope.footprint_ids.extend(moves.iter().map(|(id, _)| *id));
            }
            Self::RotateFootprints { footprint_ids, .. }
            | Self::FlipFootprints { footprint_ids } => {
                scope.footprint_ids.extend(footprint_ids);
            }
            Self::AddTrack(track) | Self::EditTrack(track) => {
                scope.track_ids.insert(track.id);
            }
            Self::AddRoute { tracks, vias } => {
                scope.track_ids.extend(tracks.iter().map(|track| track.id));
                scope.via_ids.extend(vias.iter().map(|via| via.id));
            }
            Self::RemoveTrack { track_id } => {
                scope.track_ids.insert(*track_id);
            }
            Self::DeleteTracks { track_ids } => scope.track_ids.extend(track_ids),
            Self::AddVia(via) => {
                scope.via_ids.insert(via.id);
            }
            Self::RemoveVia { via_id } => {
                scope.via_ids.insert(*via_id);
            }
            Self::DeleteVias { via_ids } => scope.via_ids.extend(via_ids),
            Self::SetOutline(_) | Self::ChangeNetClass(_) => scope.board_metadata = true,
            Self::SetGeometry {
                footprint_positions,
                tracks,
                vias,
                ..
            } => {
                scope
                    .footprint_ids
                    .extend(footprint_positions.iter().map(|(id, _)| *id));
                scope.track_ids.extend(tracks.iter().map(|track| track.id));
                scope.via_ids.extend(vias.iter().map(|via| via.id));
                scope.board_metadata = true;
            }
            Self::ApplyEco {
                symbols,
                nets,
                removed_policy,
            } => {
                let report = board.eco_report(symbols, nets);
                scope.footprint_ids.extend(
                    report
                        .removed_footprints
                        .iter()
                        .chain(&report.changed_assignments)
                        .chain(&report.renamed_references)
                        .copied(),
                );
                scope.capture_new_footprints = true;
                scope.board_metadata = true;
                if *removed_policy == RemovedFootprintPolicy::RemoveFootprintAndTracks {
                    let removed_nets = report.removed_nets.iter().copied().collect::<HashSet<_>>();
                    scope.track_ids.extend(
                        board
                            .tracks
                            .iter()
                            .filter(|track| removed_nets.contains(&track.net_id))
                            .map(|track| track.id),
                    );
                    scope.via_ids.extend(
                        board
                            .vias
                            .iter()
                            .filter(|via| removed_nets.contains(&via.net_id))
                            .map(|via| via.id),
                    );
                }
            }
        }
        scope
    }

    pub(crate) fn apply(self, context: &mut CommandContext<'_>) -> CommandOutcome {
        let changed = match self {
            Self::MoveFootprint {
                footprint_id,
                position,
            } => context.move_footprint(footprint_id, position),
            Self::MoveFootprints(moves) => {
                moves.into_iter().fold(false, |changed, (id, position)| {
                    context.move_footprint(id, position) || changed
                })
            }
            Self::RotateFootprint {
                footprint_id,
                delta_deg,
            } => context.rotate_footprint(footprint_id, delta_deg),
            Self::RotateFootprints {
                footprint_ids,
                delta_deg,
            } => {
                let mut changed = false;
                for id in footprint_ids {
                    changed |= context.rotate_footprint(id, delta_deg);
                }
                changed
            }
            Self::FlipFootprints { footprint_ids } => {
                let mut changed = false;
                for id in footprint_ids {
                    changed |= context.flip_footprint(id);
                }
                changed
            }
            Self::AddTrack(track) => {
                context.add_track(track);
                true
            }
            Self::AddRoute { tracks, vias } => {
                let changed = !tracks.is_empty() || !vias.is_empty();
                for track in tracks {
                    context.add_track(track);
                }
                for via in vias {
                    context.add_via(via);
                }
                changed
            }
            Self::RemoveTrack { track_id } => context.remove_track(track_id),
            Self::DeleteTracks { track_ids } => {
                let mut changed = false;
                for id in track_ids {
                    changed |= context.remove_track(id);
                }
                changed
            }
            Self::EditTrack(track) => context.edit_track(track),
            Self::AddVia(via) => {
                context.add_via(via);
                true
            }
            Self::RemoveVia { via_id } => context.remove_via(via_id),
            Self::DeleteVias { via_ids } => {
                let mut changed = false;
                for id in via_ids {
                    changed |= context.remove_via(id);
                }
                changed
            }
            Self::SetOutline(outline) => {
                context.set_outline(outline);
                true
            }
            Self::SetGeometry {
                footprint_positions,
                tracks,
                vias,
                outline,
            } => context.set_board_geometry(footprint_positions, tracks, vias, outline),
            Self::ChangeNetClass(net_class) => context.set_net_class(net_class),
            Self::ApplyEco {
                symbols,
                nets,
                removed_policy,
            } => context.apply_eco(&symbols, &nets, removed_policy),
        };
        if changed {
            CommandOutcome::new(ChangeSet::board())
        } else {
            CommandOutcome::unchanged()
        }
    }
}
