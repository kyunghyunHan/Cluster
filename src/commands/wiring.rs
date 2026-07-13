use crate::commands::CommandDirtyState;
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
    Tidy {
        wire_id: Option<u64>,
    },
}

impl WiringCommand {
    pub(crate) fn apply(self, app: &mut crate::CircuitApp) -> CommandDirtyState {
        match self {
            Self::Add { points } => {
                let points = crate::simplify_wire(points);
                if points.len() < 2 {
                    return CommandDirtyState::none();
                }
                let endpoints = [points.first().copied(), points.last().copied()];
                for endpoint in endpoints.into_iter().flatten() {
                    app.split_wire_at_point(endpoint);
                }
                let start = app.infer_wire_endpoint(points[0]);
                let end = app.infer_wire_endpoint(*points.last().unwrap_or(&points[0]));
                let id = app.next_id();
                app.wires.push(Wire::with_endpoints(id, points, start, end));
                app.status = "Wire placed.".to_string();
            }
            Self::MoveControlPoint {
                wire_id,
                point_index,
                position,
            } => crate::ui::app::move_wire_control_point(
                &mut app.wires,
                wire_id,
                point_index,
                position,
            ),
            Self::Tidy { wire_id } => {
                let mut count = 0;
                for wire in &mut app.wires {
                    if wire_id.is_none_or(|id| id == wire.id) {
                        crate::tidy_wire_points(wire);
                        count += 1;
                    }
                }
                app.status = if wire_id.is_some() {
                    "Wire straightened.".to_string()
                } else {
                    format!("Tidied {count} wire(s).")
                };
            }
        }
        CommandDirtyState::document()
    }
}
