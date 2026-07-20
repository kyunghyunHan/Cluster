use crate::commands::ChangeSet;
use crate::commands::context::{CommandContext, CommandOutcome};
use crate::model::{Component, ComponentKind, Wire};
use egui::{Pos2, Vec2};
use std::collections::HashMap;
use std::collections::HashSet;

pub(crate) enum ComponentCommand {
    Place {
        kind: ComponentKind,
        position: Pos2,
        value: String,
    },
    PlaceCustom {
        part_id: String,
        position: Pos2,
        value: String,
    },
    Paste {
        components: Vec<Component>,
        wires: Vec<Wire>,
        offset: Vec2,
    },
    Move {
        component_ids: HashSet<u64>,
        delta: Vec2,
    },
}

impl ComponentCommand {
    pub(crate) fn apply(self, context: &mut CommandContext<'_>) -> CommandOutcome {
        match self {
            Self::Place {
                kind,
                position,
                value,
            } => {
                context.place_component(kind, position, value, None);
                return CommandOutcome::new(ChangeSet::schematic())
                    .with_status("Component placed. Drag to reposition, R to rotate.");
            }
            Self::PlaceCustom {
                part_id,
                position,
                value,
            } => {
                context.place_component(ComponentKind::Custom, position, value, Some(part_id));
                return CommandOutcome::new(ChangeSet::schematic())
                    .with_status("Custom part placed. Drag to reposition, R to rotate.");
            }
            Self::Paste {
                components,
                wires,
                offset,
            } => {
                let mut id_map = HashMap::new();
                let mut new_ids = Vec::new();
                for source in components {
                    let new_id = context.next_id();
                    id_map.insert(source.id, new_id);
                    let new_label = if source.kind == ComponentKind::Custom {
                        context.next_custom_label(source.part_id.as_deref())
                    } else {
                        context.next_label(source.kind)
                    };
                    context.insert_component(Component {
                        id: new_id,
                        kind: source.kind,
                        pos: source.pos + offset,
                        rotation: source.rotation,
                        label: new_label,
                        value: source.value,
                        part_id: source.part_id,
                    });
                    new_ids.push(new_id);
                }
                let wire_count = wires.len();
                for source in wires {
                    let new_wire_id = context.next_id();
                    let points = source
                        .points
                        .into_iter()
                        .map(|point| point + offset)
                        .collect();
                    context.insert_wire(Wire::new(new_wire_id, points));
                }
                context.set_multi_selected(new_ids.iter().copied().collect());
                context.set_selected(None);
                return CommandOutcome::new(ChangeSet::schematic()).with_status(format!(
                    "Pasted {} component(s) + {} wire(s).",
                    new_ids.len(),
                    wire_count
                ));
            }
            Self::Move {
                component_ids,
                delta,
            } => {
                if delta.length_sq() <= 0.01 {
                    return CommandOutcome::unchanged();
                }
                if !context.move_components(&component_ids, delta) {
                    return CommandOutcome::unchanged();
                }
            }
        }
        CommandOutcome::new(ChangeSet::schematic())
    }
}
