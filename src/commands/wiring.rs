use crate::commands::ChangeSet;
use crate::commands::context::{CommandContext, CommandOutcome};
use crate::model::Wire;
use egui::Pos2;

pub(crate) enum WiringCommand {
    Add {
        points: Vec<Pos2>,
    },
    MoveControlPoint {
        wire_id: u64,
        point_index: usize,
        position: Pos2,
    },
    InsertControlPoint {
        position: Pos2,
    },
    Tidy {
        wire_id: Option<u64>,
    },
}

impl WiringCommand {
    pub(crate) fn apply(self, context: &mut CommandContext<'_>) -> CommandOutcome {
        match self {
            Self::Add { points } => {
                let points = crate::simplify_wire(points);
                if points.len() < 2 {
                    return CommandOutcome::unchanged();
                }
                let endpoints = [points.first().copied(), points.last().copied()];
                for endpoint in endpoints.into_iter().flatten() {
                    context.split_wire_at_point(endpoint);
                }
                let start = context.infer_wire_endpoint(points[0]);
                let end = context.infer_wire_endpoint(*points.last().unwrap_or(&points[0]));
                let id = context.next_id();
                context
                    .wires_mut()
                    .push(Wire::with_endpoints(id, points, start, end));
                return CommandOutcome::new(ChangeSet::schematic()).with_status("Wire placed.");
            }
            Self::MoveControlPoint {
                wire_id,
                point_index,
                position,
            } => crate::ui::app::move_wire_control_point(
                context.wires_mut(),
                wire_id,
                point_index,
                position,
            ),
            Self::InsertControlPoint { position } => {
                let Some((wire_id, point_index)) =
                    crate::ui::app::insert_wire_control_point(position, context.wires_mut())
                else {
                    return CommandOutcome::unchanged();
                };
                context.set_drag(Some(crate::DragState::WirePoint {
                    wire_id,
                    point_index,
                }));
                context.set_selected(Some(crate::app::Selection::Wire(wire_id)));
            }
            Self::Tidy { wire_id } => {
                let mut count = 0;
                for wire in context.wires_mut() {
                    if wire_id.is_none_or(|id| id == wire.id) {
                        crate::tidy_wire_points(wire);
                        count += 1;
                    }
                }
                let status = if wire_id.is_some() {
                    "Wire straightened.".to_string()
                } else {
                    format!("Tidied {count} wire(s).")
                };
                return CommandOutcome::new(ChangeSet::schematic()).with_status(status);
            }
        }
        CommandOutcome::new(ChangeSet::schematic())
    }
}
