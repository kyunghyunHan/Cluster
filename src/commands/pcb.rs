use crate::commands::ChangeSet;
use crate::commands::context::{CommandContext, CommandOutcome};
use crate::model::cad::Point2;
use crate::pcb::board::BoardOutline;
use crate::pcb::track::TrackSegment;
use crate::pcb::via::Via;

#[allow(dead_code)]
pub(crate) enum PcbCommand {
    MoveFootprint { footprint_id: u64, position: Point2 },
    RotateFootprint { footprint_id: u64, delta_deg: f32 },
    AddTrack(TrackSegment),
    RemoveTrack { track_id: u64 },
    AddVia(Via),
    RemoveVia { via_id: u64 },
    SetOutline(BoardOutline),
}

impl PcbCommand {
    pub(crate) fn apply(self, context: &mut CommandContext<'_>) -> CommandOutcome {
        let changed = match self {
            Self::MoveFootprint {
                footprint_id,
                position,
            } => context.move_footprint(footprint_id, position),
            Self::RotateFootprint {
                footprint_id,
                delta_deg,
            } => context.rotate_footprint(footprint_id, delta_deg),
            Self::AddTrack(track) => {
                context.add_track(track);
                true
            }
            Self::RemoveTrack { track_id } => context.remove_track(track_id),
            Self::AddVia(via) => {
                context.add_via(via);
                true
            }
            Self::RemoveVia { via_id } => context.remove_via(via_id),
            Self::SetOutline(outline) => {
                context.set_outline(outline);
                true
            }
        };
        if changed {
            CommandOutcome::new(ChangeSet::board())
        } else {
            CommandOutcome::unchanged()
        }
    }
}
