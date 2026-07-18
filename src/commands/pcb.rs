use crate::commands::ChangeSet;
use crate::commands::context::{CommandContext, CommandOutcome};
use crate::model::cad::{CadNet, NetClass, Point2, SymbolInstance};
use crate::pcb::board::{BoardOutline, RemovedFootprintPolicy};
use crate::pcb::track::TrackSegment;
use crate::pcb::via::Via;

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
    ChangeNetClass(NetClass),
    ApplyEco {
        symbols: Vec<SymbolInstance>,
        nets: Vec<CadNet>,
        removed_policy: RemovedFootprintPolicy,
    },
}

impl PcbCommand {
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
