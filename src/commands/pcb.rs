use crate::commands::ChangeSet;
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
    pub(crate) fn apply(self, app: &mut crate::CircuitApp) -> ChangeSet {
        let changed = match self {
            Self::MoveFootprint {
                footprint_id,
                position,
            } => app
                .document
                .board
                .footprints
                .iter_mut()
                .find(|footprint| footprint.id == footprint_id)
                .is_some_and(|footprint| {
                    footprint.position = position;
                    footprint.placed = true;
                    true
                }),
            Self::RotateFootprint {
                footprint_id,
                delta_deg,
            } => app
                .document
                .board
                .footprints
                .iter_mut()
                .find(|footprint| footprint.id == footprint_id)
                .is_some_and(|footprint| {
                    footprint.rotation_deg = (footprint.rotation_deg + delta_deg).rem_euclid(360.0);
                    true
                }),
            Self::AddTrack(track) => {
                app.document.board.tracks.push(track);
                true
            }
            Self::RemoveTrack { track_id } => {
                let before = app.document.board.tracks.len();
                app.document
                    .board
                    .tracks
                    .retain(|track| track.id != track_id);
                before != app.document.board.tracks.len()
            }
            Self::AddVia(via) => {
                app.document.board.vias.push(via);
                true
            }
            Self::RemoveVia { via_id } => {
                let before = app.document.board.vias.len();
                app.document.board.vias.retain(|via| via.id != via_id);
                before != app.document.board.vias.len()
            }
            Self::SetOutline(outline) => {
                app.document.board.outline = outline;
                true
            }
        };
        if changed {
            ChangeSet {
                board_changed: true,
                autosave_eligible: true,
                ..ChangeSet::none()
            }
        } else {
            ChangeSet::none()
        }
    }
}
