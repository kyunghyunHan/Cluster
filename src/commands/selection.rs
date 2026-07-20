use crate::Component;
use crate::app::{AlignDir, Selection};
use crate::commands::ChangeSet;
use crate::commands::context::{CommandContext, CommandOutcome};
use egui::Vec2;

pub(crate) enum SelectionCommand {
    Delete,
    Rotate,
    Duplicate,
    Align(AlignDir),
    Distribute { vertical: bool },
}

impl SelectionCommand {
    pub(crate) fn apply(self, context: &mut CommandContext<'_>) -> CommandOutcome {
        match self {
            Self::Delete => {
                if !context.multi_selected().is_empty() {
                    let selected = context.multi_selected().clone();
                    let count = selected.len();
                    context.remove_components(&selected);
                    context.clear_multi_selected();
                    context.set_selected(None);
                    CommandOutcome::new(ChangeSet::schematic())
                        .with_status(format!("Deleted {count} component(s)."))
                } else {
                    match context.take_selected() {
                        Some(Selection::Component(id)) => {
                            if context.remove_component(id) {
                                CommandOutcome::new(ChangeSet::schematic())
                                    .with_status("Component deleted.")
                            } else {
                                CommandOutcome::unchanged()
                            }
                        }
                        Some(Selection::Wire(id)) => {
                            if context.remove_wire(id) {
                                CommandOutcome::new(ChangeSet::schematic())
                                    .with_status("Wire deleted.")
                            } else {
                                CommandOutcome::unchanged()
                            }
                        }
                        None => {
                            CommandOutcome::unchanged().with_status("Nothing selected to delete.")
                        }
                    }
                }
            }
            Self::Rotate => {
                let Some(Selection::Component(id)) = context.selected() else {
                    return CommandOutcome::unchanged();
                };
                if !context.rotate_component(id) {
                    return CommandOutcome::unchanged();
                }
                CommandOutcome::new(ChangeSet::schematic())
                    .with_status("Rotated and kept attached wires on pins.")
            }
            Self::Duplicate => {
                let sources = if !context.multi_selected().is_empty() {
                    context
                        .components()
                        .iter()
                        .filter(|component| context.multi_selected().contains(&component.id))
                        .cloned()
                        .collect::<Vec<_>>()
                } else if let Some(Selection::Component(id)) = context.selected() {
                    context
                        .components()
                        .iter()
                        .find(|component| component.id == id)
                        .cloned()
                        .into_iter()
                        .collect()
                } else {
                    return CommandOutcome::unchanged()
                        .with_status("Select a component to duplicate.");
                };
                let offset = Vec2::new(context.grid() * 2.0, context.grid() * 2.0);
                let mut new_ids = Vec::new();
                for source in sources {
                    let mut duplicate: Component = source;
                    duplicate.id = context.next_id();
                    duplicate.pos += offset;
                    duplicate.label = context.next_label(duplicate.kind);
                    new_ids.push(duplicate.id);
                    context.insert_component(duplicate);
                }
                if new_ids.len() == 1 {
                    context.set_selected(Some(Selection::Component(new_ids[0])));
                    CommandOutcome::new(ChangeSet::schematic()).with_status("Component duplicated.")
                } else {
                    context.set_multi_selected(new_ids.iter().copied().collect());
                    context.set_selected(None);
                    CommandOutcome::new(ChangeSet::schematic())
                        .with_status(format!("Duplicated {} component(s).", new_ids.len()))
                }
            }
            Self::Align(direction) => {
                let ids = selected_component_ids(context);
                let positions = context
                    .components()
                    .iter()
                    .filter(|component| ids.contains(&component.id))
                    .map(|component| component.pos)
                    .collect::<Vec<_>>();
                if positions.len() < 2 {
                    return CommandOutcome::unchanged();
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
                let vertical = matches!(
                    direction,
                    AlignDir::Top | AlignDir::Bottom | AlignDir::CenterV
                );
                for id in &ids {
                    context.set_component_axis_position(*id, vertical, target);
                }
                CommandOutcome::new(ChangeSet::schematic())
                    .with_status(format!("Aligned {} components.", positions.len()))
            }
            Self::Distribute { vertical } => {
                let ids = selected_component_ids(context);
                if ids.len() < 3 {
                    return CommandOutcome::unchanged();
                }
                let mut ordered = context
                    .components()
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
                    return CommandOutcome::unchanged();
                };
                let step = (last - first) / (ordered.len() as f32 - 1.0);
                for (index, (id, _)) in ordered.iter().enumerate() {
                    context.set_component_axis_position(*id, vertical, first + step * index as f32);
                }
                CommandOutcome::new(ChangeSet::schematic())
                    .with_status(format!("Distributed {} components.", ids.len()))
            }
        }
    }
}

fn selected_component_ids(context: &CommandContext<'_>) -> Vec<u64> {
    if !context.multi_selected().is_empty() {
        context.multi_selected().iter().copied().collect()
    } else if let Some(Selection::Component(id)) = context.selected() {
        vec![id]
    } else {
        Vec::new()
    }
}
