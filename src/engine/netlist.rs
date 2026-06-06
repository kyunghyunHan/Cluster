use crate::model::*;
use crate::{component_pin_defs, point_touches_wire_segment};
use egui::Pos2;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
struct NetlistNodes {
    positions: Vec<Pos2>,
}

impl NetlistNodes {
    fn node_for(&mut self, pos: Pos2) -> usize {
        if let Some(index) = self
            .positions
            .iter()
            .position(|existing| existing.distance(pos) <= 4.0)
        {
            return index;
        }
        self.positions.push(pos);
        self.positions.len() - 1
    }
}

#[derive(Default)]
struct NetlistUnionFind {
    parent: Vec<usize>,
}

impl NetlistUnionFind {
    fn ensure(&mut self, index: usize) {
        while self.parent.len() <= index {
            self.parent.push(self.parent.len());
        }
    }

    fn find(&mut self, index: usize) -> usize {
        self.ensure(index);
        if self.parent[index] != index {
            self.parent[index] = self.find(self.parent[index]);
        }
        self.parent[index]
    }

    fn union(&mut self, a: usize, b: usize) {
        let a = self.find(a);
        let b = self.find(b);
        if a != b {
            self.parent[b] = a;
        }
    }
}

pub(crate) fn build_circuit_netlist(components: &[Component], wires: &[Wire]) -> CircuitNetlist {
    let mut nodes = NetlistNodes::default();
    let mut nets = NetlistUnionFind::default();

    for wire in wires {
        for point in &wire.points {
            let node = nodes.node_for(*point);
            nets.ensure(node);
        }
        for segment in wire.points.windows(2) {
            let a = nodes.node_for(segment[0]);
            let b = nodes.node_for(segment[1]);
            nets.ensure(a);
            nets.ensure(b);
            nets.union(a, b);
        }
    }

    for component in components {
        for pin in component_pin_defs(component) {
            let node = nodes.node_for(pin.pos);
            nets.ensure(node);
        }
    }

    for contact in wire_contact_points(components, wires) {
        let contact_node = nodes.node_for(contact);
        nets.ensure(contact_node);
        for wire in wires {
            for segment in wire.points.windows(2) {
                if point_touches_wire_segment(contact, segment[0], segment[1]) {
                    let a = nodes.node_for(segment[0]);
                    let b = nodes.node_for(segment[1]);
                    nets.ensure(a);
                    nets.ensure(b);
                    nets.union(contact_node, a);
                    nets.union(contact_node, b);
                }
            }
        }
    }

    let mut root_has_wire = HashSet::new();
    let mut wire_root_sets: HashMap<u64, HashSet<usize>> = HashMap::new();
    for wire in wires {
        for point in &wire.points {
            let node = nodes.node_for(*point);
            let root = nets.find(node);
            root_has_wire.insert(root);
            wire_root_sets.entry(wire.id).or_default().insert(root);
        }
        for segment in wire.points.windows(2) {
            let a = nets.find(nodes.node_for(segment[0]));
            let b = nets.find(nodes.node_for(segment[1]));
            root_has_wire.insert(a);
            root_has_wire.insert(b);
            wire_root_sets.entry(wire.id).or_default().insert(a);
            wire_root_sets.entry(wire.id).or_default().insert(b);
        }
    }

    let mut root_pins: HashMap<usize, Vec<PinRef>> = HashMap::new();
    let mut pin_rows = Vec::new();
    let mut root_name_hints: HashMap<usize, String> = HashMap::new();
    for component in components {
        for pin in component_pin_defs(component) {
            let root = nets.find(nodes.node_for(pin.pos));
            root_pins.entry(root).or_default().push(PinRef {
                component_id: component.id,
                pin_name: pin.label.to_string(),
            });
            if component.kind == ComponentKind::Ground || pin.role == PinRole::Ground {
                root_name_hints
                    .entry(root)
                    .or_insert_with(|| "GND".to_string());
            }
            if component.kind == ComponentKind::NetLabel {
                let name = component.value.trim();
                if !name.is_empty() {
                    root_name_hints
                        .entry(root)
                        .or_insert_with(|| name.to_string());
                }
            }
            pin_rows.push((root, component, pin));
        }
    }

    let mut roots = (0..nodes.positions.len())
        .map(|idx| nets.find(idx))
        .collect::<Vec<_>>();
    roots.sort_unstable();
    roots.dedup();

    let mut root_to_id = HashMap::new();
    for root in roots {
        let next_id = root_to_id.len();
        root_to_id.insert(root, next_id);
    }

    let mut generated = 1usize;
    let mut net_rows = root_to_id
        .iter()
        .map(|(&root, &id)| {
            let name = root_name_hints.get(&root).cloned().unwrap_or_else(|| {
                let name = format!("NET_{generated:03}");
                generated += 1;
                name
            });
            Net {
                id,
                name,
                connected_pins: root_pins.remove(&root).unwrap_or_default(),
            }
        })
        .collect::<Vec<_>>();
    net_rows.sort_by_key(|net| net.id);

    let pins = pin_rows
        .into_iter()
        .filter_map(|(root, component, pin)| {
            Some(NetlistPin {
                component_id: component.id,
                component_label: component.label.clone(),
                component_kind: component.kind,
                component_value: component.value.clone(),
                pin_name: pin.label.to_string(),
                electrical_type: electrical_type_for_role(pin.role),
                position: pin.pos,
                net_id: *root_to_id.get(&root)?,
                connected_by_wire: root_has_wire.contains(&root),
            })
        })
        .collect::<Vec<_>>();

    let mut wire_nets = HashMap::new();
    let mut floating_wires = Vec::new();
    for wire in wires {
        let roots = wire_root_sets.remove(&wire.id).unwrap_or_default();
        let root = roots.iter().next().copied();
        if let Some(root) = root.and_then(|root| root_to_id.get(&root).copied()) {
            wire_nets.insert(wire.id, root);
            if net_rows
                .get(root)
                .is_some_and(|net| net.connected_pins.is_empty())
            {
                floating_wires.push(wire.id);
            }
        }
    }

    CircuitNetlist {
        nets: net_rows,
        pins,
        wire_nets,
        floating_wires,
    }
}

fn electrical_type_for_role(role: PinRole) -> ElectricalType {
    match role {
        PinRole::Passive => ElectricalType::Passive,
        PinRole::Positive => ElectricalType::PowerIn,
        PinRole::Ground => ElectricalType::Ground,
        PinRole::Digital => ElectricalType::Digital,
        PinRole::I2c => ElectricalType::I2c,
        PinRole::Control => ElectricalType::Control,
        PinRole::Output => ElectricalType::Output,
    }
}

fn wire_contact_points(components: &[Component], wires: &[Wire]) -> Vec<Pos2> {
    let mut points = Vec::new();
    for wire in wires {
        points.extend(wire.points.iter().copied());
    }
    for component in components {
        points.extend(component_pin_defs(component).into_iter().map(|pin| pin.pos));
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    fn component(id: u64, kind: ComponentKind, pos: Pos2, label: &str, value: &str) -> Component {
        Component {
            id,
            kind,
            pos,
            rotation: 0,
            label: label.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn builds_net_from_wire_and_component_pins() {
        let r1 = component(
            1,
            ComponentKind::Resistor,
            Pos2::new(100.0, 100.0),
            "R1",
            "1k",
        );
        let led = component(
            2,
            ComponentKind::Led,
            Pos2::new(220.0, 100.0),
            "LED1",
            "red",
        );
        let wire = Wire {
            id: 3,
            points: vec![Pos2::new(136.0, 100.0), Pos2::new(192.0, 100.0)],
        };

        let netlist = build_circuit_netlist(&[r1, led], &[wire]);
        let r1_b = netlist
            .pins
            .iter()
            .find(|pin| pin.component_label == "R1" && pin.pin_name == "B")
            .unwrap();
        let led_a = netlist
            .pins
            .iter()
            .find(|pin| pin.component_label == "LED1" && pin.pin_name == "A")
            .unwrap();

        assert_eq!(r1_b.net_id, led_a.net_id);
        assert_eq!(netlist.wire_nets.len(), 1);
    }
}
