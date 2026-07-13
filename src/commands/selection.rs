use crate::app::{AlignDir, Selection};
use crate::commands::CommandDirtyState;
use crate::{Component, component_pins};
use egui::Vec2;

pub(crate) enum SelectionCommand {
    Delete,
    Rotate,
    Duplicate,
    Align(AlignDir),
    Distribute { vertical: bool },
}

impl SelectionCommand {
    pub(crate) fn apply(self, app: &mut crate::CircuitApp) -> CommandDirtyState {
        match self {
            Self::Delete => {
                if !app.multi_selected.is_empty() {
                    let count = app.multi_selected.len();
                    app.components
                        .retain(|component| !app.multi_selected.contains(&component.id));
                    app.multi_selected.clear();
                    app.selected = None;
                    app.status = format!("Deleted {count} component(s).");
                } else {
                    match app.selected.take() {
                        Some(Selection::Component(id)) => {
                            app.components.retain(|component| component.id != id);
                            app.status = "Component deleted.".to_string();
                        }
                        Some(Selection::Wire(id)) => {
                            app.wires.retain(|wire| wire.id != id);
                            app.status = "Wire deleted.".to_string();
                        }
                        None => {
                            app.status = "Nothing selected to delete.".to_string();
                            return CommandDirtyState::none();
                        }
                    }
                }
            }
            Self::Rotate => {
                let Some(Selection::Component(id)) = app.selected else {
                    return CommandDirtyState::none();
                };
                let Some(index) = app
                    .components
                    .iter()
                    .position(|component| component.id == id)
                else {
                    return CommandDirtyState::none();
                };
                let old_pins = component_pins(&app.components[index]);
                app.components[index].rotation = (app.components[index].rotation + 90) % 360;
                let new_pins = component_pins(&app.components[index]);
                crate::move_attached_wire_endpoints(&mut app.wires, &old_pins, &new_pins);
                for wire in &mut app.wires {
                    if wire.points.len() > 2 {
                        let first = wire.points[0];
                        let Some(&last) = wire.points.last() else {
                            continue;
                        };
                        if old_pins.iter().any(|pin| first.distance(*pin) <= 20.0)
                            || old_pins.iter().any(|pin| last.distance(*pin) <= 20.0)
                        {
                            crate::tidy_wire_points(wire);
                        }
                    }
                }
                app.status = "Rotated and kept attached wires on pins.".to_string();
            }
            Self::Duplicate => {
                let sources = if !app.multi_selected.is_empty() {
                    app.components
                        .iter()
                        .filter(|component| app.multi_selected.contains(&component.id))
                        .cloned()
                        .collect::<Vec<_>>()
                } else if let Some(Selection::Component(id)) = app.selected {
                    app.components
                        .iter()
                        .find(|component| component.id == id)
                        .cloned()
                        .into_iter()
                        .collect()
                } else {
                    app.status = "Select a component to duplicate.".to_string();
                    return CommandDirtyState::none();
                };
                let offset = Vec2::new(app.grid * 2.0, app.grid * 2.0);
                let mut new_ids = Vec::new();
                for source in sources {
                    let mut duplicate: Component = source;
                    duplicate.id = app.next_id();
                    duplicate.pos += offset;
                    duplicate.label = app.next_label(duplicate.kind);
                    new_ids.push(duplicate.id);
                    app.components.push(duplicate);
                }
                if new_ids.len() == 1 {
                    app.selected = Some(Selection::Component(new_ids[0]));
                    app.status = "Component duplicated.".to_string();
                } else {
                    app.multi_selected = new_ids.iter().copied().collect();
                    app.selected = None;
                    app.status = format!("Duplicated {} component(s).", new_ids.len());
                }
            }
            Self::Align(direction) => {
                let ids = selected_component_ids(app);
                let positions = app
                    .components
                    .iter()
                    .filter(|component| ids.contains(&component.id))
                    .map(|component| component.pos)
                    .collect::<Vec<_>>();
                if positions.len() < 2 {
                    return CommandDirtyState::none();
                }
                let target = match direction {
                    AlignDir::Left => positions
                        .iter()
                        .map(|position| position.x)
                        .fold(f32::INFINITY, f32::min),
                    AlignDir::Right => positions
                        .iter()
                        .map(|position| position.x)
                        .fold(f32::NEG_INFINITY, f32::max),
                    AlignDir::Top => positions
                        .iter()
                        .map(|position| position.y)
                        .fold(f32::INFINITY, f32::min),
                    AlignDir::Bottom => positions
                        .iter()
                        .map(|position| position.y)
                        .fold(f32::NEG_INFINITY, f32::max),
                    AlignDir::CenterH => {
                        positions.iter().map(|position| position.x).sum::<f32>()
                            / positions.len() as f32
                    }
                    AlignDir::CenterV => {
                        positions.iter().map(|position| position.y).sum::<f32>()
                            / positions.len() as f32
                    }
                };
                for component in &mut app.components {
                    if ids.contains(&component.id) {
                        match direction {
                            AlignDir::Left | AlignDir::Right | AlignDir::CenterH => {
                                component.pos.x = target
                            }
                            AlignDir::Top | AlignDir::Bottom | AlignDir::CenterV => {
                                component.pos.y = target
                            }
                        }
                    }
                }
                app.status = format!("Aligned {} components.", positions.len());
            }
            Self::Distribute { vertical } => {
                let ids = selected_component_ids(app);
                if ids.len() < 3 {
                    return CommandDirtyState::none();
                }
                let mut ordered = app
                    .components
                    .iter()
                    .filter(|component| ids.contains(&component.id))
                    .map(|component| {
                        (
                            component.id,
                            if vertical {
                                component.pos.y
                            } else {
                                component.pos.x
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                ordered.sort_by(|left, right| left.1.total_cmp(&right.1));
                let (Some(first), Some(last)) = (
                    ordered.first().map(|row| row.1),
                    ordered.last().map(|row| row.1),
                ) else {
                    return CommandDirtyState::none();
                };
                let step = (last - first) / (ordered.len() as f32 - 1.0);
                for (index, (id, _)) in ordered.iter().enumerate() {
                    if let Some(component) = app
                        .components
                        .iter_mut()
                        .find(|component| component.id == *id)
                    {
                        if vertical {
                            component.pos.y = first + step * index as f32;
                        } else {
                            component.pos.x = first + step * index as f32;
                        }
                    }
                }
                app.status = format!("Distributed {} components.", ids.len());
            }
        }
        CommandDirtyState::document()
    }
}

fn selected_component_ids(app: &crate::CircuitApp) -> Vec<u64> {
    if !app.multi_selected.is_empty() {
        app.multi_selected.iter().copied().collect()
    } else if let Some(Selection::Component(id)) = app.selected {
        vec![id]
    } else {
        Vec::new()
    }
}
