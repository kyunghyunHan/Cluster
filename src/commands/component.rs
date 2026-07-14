use crate::commands::ChangeSet;
use crate::model::{Component, ComponentKind, Wire};
use egui::{Pos2, Vec2};
use std::collections::HashMap;
use std::collections::HashSet;

pub(crate) enum ComponentCommand {
    Place {
        kind: ComponentKind,
        position: Pos2,
    },
    PlaceCustom {
        part_id: String,
        position: Pos2,
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
    pub(crate) fn apply(self, app: &mut crate::CircuitApp) -> ChangeSet {
        match self {
            Self::Place { kind, position } => {
                app.place_component(kind, position);
                app.status = "Component placed. Drag to reposition, R to rotate.".to_string();
            }
            Self::PlaceCustom { part_id, position } => {
                app.place_custom_component(&part_id, position);
                app.status = "Custom part placed. Drag to reposition, R to rotate.".to_string();
            }
            Self::Paste {
                components,
                wires,
                offset,
            } => {
                let mut id_map = HashMap::new();
                let mut new_ids = Vec::new();
                for source in components {
                    let new_id = app.next_id();
                    id_map.insert(source.id, new_id);
                    let new_label = if source.kind == ComponentKind::Custom {
                        app.next_custom_label(source.part_id.as_deref())
                    } else {
                        app.next_label(source.kind)
                    };
                    app.components.push(Component {
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
                    let new_wire_id = app.next_id();
                    let points = source
                        .points
                        .into_iter()
                        .map(|point| point + offset)
                        .collect();
                    app.wires.push(Wire::new(new_wire_id, points));
                }
                app.editor.multi_selected = new_ids.iter().copied().collect();
                app.editor.selected = None;
                app.status = format!(
                    "Pasted {} component(s) + {} wire(s).",
                    new_ids.len(),
                    wire_count
                );
            }
            Self::Move {
                component_ids,
                delta,
            } => {
                if delta.length_sq() <= 0.01 {
                    return ChangeSet::none();
                }
                let old_pins = app
                    .components
                    .iter()
                    .filter(|component| component_ids.contains(&component.id))
                    .flat_map(crate::component_pins)
                    .collect::<Vec<_>>();
                for component in &mut app.components {
                    if component_ids.contains(&component.id) {
                        component.pos += delta;
                    }
                }
                let new_pins = app
                    .components
                    .iter()
                    .filter(|component| component_ids.contains(&component.id))
                    .flat_map(crate::component_pins)
                    .collect::<Vec<_>>();
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
            }
        }
        ChangeSet::schematic()
    }
}
