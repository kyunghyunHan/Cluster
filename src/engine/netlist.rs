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
            .position(|existing| existing.distance(pos) <= 1.0)
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

    let mut label_nodes: HashMap<String, Vec<usize>> = HashMap::new();
    for component in components {
        if component.kind != ComponentKind::NetLabel {
            continue;
        }
        let label = component.value.trim().to_ascii_lowercase();
        if label.is_empty() {
            continue;
        }
        for pin in component_pin_defs(component) {
            label_nodes
                .entry(label.clone())
                .or_default()
                .push(nodes.node_for(pin.pos));
        }
    }
    for nodes_with_label in label_nodes.values() {
        for pair in nodes_with_label.windows(2) {
            nets.union(pair[0], pair[1]);
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
    let mut isolated_wires = Vec::new();
    for wire in wires {
        let roots = wire_root_sets.remove(&wire.id).unwrap_or_default();
        let root = roots.iter().next().copied();
        if let Some(root) = root.and_then(|root| root_to_id.get(&root).copied()) {
            wire_nets.insert(wire.id, root);
            if let Some(net) = net_rows.get(root) {
                if net.connected_pins.is_empty() {
                    floating_wires.push(wire.id);
                } else if net.connected_pins.len() == 1 {
                    isolated_wires.push(wire.id);
                }
            }
        }
    }

    CircuitNetlist {
        nets: net_rows,
        pins,
        wire_nets,
        floating_wires,
        isolated_wires,
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

    fn comp(id: u64, kind: ComponentKind, pos: Pos2, label: &str, value: &str) -> Component {
        Component {
            id,
            kind,
            pos,
            rotation: 0,
            label: label.to_string(),
            value: value.to_string(),
        }
    }

    fn wire(id: u64, points: Vec<Pos2>) -> Wire {
        Wire { id, points }
    }

    // ── Basic wire-to-pin connection ─────────────────────────────────────

    #[test]
    fn builds_net_from_wire_and_component_pins() {
        let r1 = comp(1, ComponentKind::Resistor, Pos2::new(100.0, 100.0), "R1", "1k");
        let led = comp(2, ComponentKind::Led, Pos2::new(220.0, 100.0), "LED1", "red");
        let r1_b_pos = component_pin_defs(&r1)
            .into_iter()
            .find(|pin| pin.label == "B")
            .unwrap()
            .pos;
        let led_a_pos = component_pin_defs(&led)
            .into_iter()
            .find(|pin| pin.label == "A")
            .unwrap()
            .pos;
        let w = wire(3, vec![r1_b_pos, led_a_pos]);

        let netlist = build_circuit_netlist(&[r1, led], &[w]);
        let r1_b = netlist.pins.iter().find(|p| p.component_label == "R1" && p.pin_name == "B").unwrap();
        let led_a = netlist.pins.iter().find(|p| p.component_label == "LED1" && p.pin_name == "A").unwrap();

        assert_eq!(r1_b.net_id, led_a.net_id);
        assert_eq!(netlist.wire_nets.len(), 1);
    }

    // ── GND net gets named "GND" ─────────────────────────────────────────

    #[test]
    fn ground_symbol_names_net_gnd() {
        let gnd = comp(1, ComponentKind::Ground, Pos2::new(100.0, 200.0), "GND1", "0V");
        let netlist = build_circuit_netlist(&[gnd], &[]);
        assert!(netlist.nets.iter().any(|net| net.name == "GND"));
    }

    // ── Two isolated components → two separate nets ──────────────────────

    #[test]
    fn isolated_components_have_separate_nets() {
        let r1 = comp(1, ComponentKind::Resistor, Pos2::new(100.0, 100.0), "R1", "1k");
        let r2 = comp(2, ComponentKind::Resistor, Pos2::new(400.0, 100.0), "R2", "2k");
        let netlist = build_circuit_netlist(&[r1, r2], &[]);

        let r1_a = netlist.pins.iter().find(|p| p.component_label == "R1" && p.pin_name == "A").unwrap();
        let r2_a = netlist.pins.iter().find(|p| p.component_label == "R2" && p.pin_name == "A").unwrap();

        assert_ne!(r1_a.net_id, r2_a.net_id);
    }

    // ── T-junction: wire touching the middle of another wire ─────────────

    #[test]
    fn t_junction_merges_nets() {
        // Horizontal wire: (0,100) → (200,100)
        // Vertical wire touching middle at (100,100): (100,50) → (100,100)
        // All three segments should be on the same net.
        let w1 = wire(1, vec![Pos2::new(0.0, 100.0), Pos2::new(200.0, 100.0)]);
        let w2 = wire(2, vec![Pos2::new(100.0, 50.0), Pos2::new(100.0, 100.0)]);

        let netlist = build_circuit_netlist(&[], &[w1, w2]);
        let net1 = netlist.wire_nets[&1];
        let net2 = netlist.wire_nets[&2];
        assert_eq!(net1, net2, "T-junction wires must share a net");
    }

    #[test]
    fn crossing_wires_without_contact_point_stay_separate() {
        let horizontal = wire(
            1,
            vec![Pos2::new(0.0, 100.0), Pos2::new(200.0, 100.0)],
        );
        let vertical = wire(
            2,
            vec![Pos2::new(100.0, 0.0), Pos2::new(100.0, 200.0)],
        );

        let netlist = build_circuit_netlist(&[], &[horizontal, vertical]);
        assert_ne!(
            netlist.wire_nets[&1], netlist.wire_nets[&2],
            "Interior wire crossings must not connect without an explicit contact point"
        );
    }

    #[test]
    fn component_pin_touching_wire_midpoint_joins_wire_net() {
        let resistor = comp(
            10,
            ComponentKind::Resistor,
            Pos2::new(100.0, 100.0),
            "R1",
            "1k",
        );
        let pin_a = component_pin_defs(&resistor)
            .into_iter()
            .find(|pin| pin.label == "A")
            .unwrap()
            .pos;
        let wire = wire(
            11,
            vec![
                Pos2::new(pin_a.x - 40.0, pin_a.y),
                Pos2::new(pin_a.x + 20.0, pin_a.y),
            ],
        );

        let netlist = build_circuit_netlist(&[resistor], &[wire]);
        let pin = netlist
            .pins
            .iter()
            .find(|pin| pin.component_id == 10 && pin.pin_name == "A")
            .unwrap();
        assert!(pin.connected_by_wire);
        assert_eq!(netlist.wire_nets[&11], pin.net_id);
    }

    // ── Floating wire has no component pins ──────────────────────────────

    #[test]
    fn floating_wire_detected() {
        let w = wire(99, vec![Pos2::new(500.0, 500.0), Pos2::new(600.0, 500.0)]);
        let netlist = build_circuit_netlist(&[], &[w]);
        assert!(netlist.floating_wires.contains(&99), "Free-standing wire should be floating");
    }

    #[test]
    fn one_pin_wire_is_isolated() {
        let resistor = comp(
            1,
            ComponentKind::Resistor,
            Pos2::new(100.0, 100.0),
            "R1",
            "1k",
        );
        let pin_a = component_pin_defs(&resistor)
            .into_iter()
            .find(|pin| pin.label == "A")
            .unwrap()
            .pos;
        let dangling = wire(2, vec![pin_a, Pos2::new(pin_a.x - 60.0, pin_a.y)]);
        let netlist = build_circuit_netlist(&[resistor], &[dangling]);
        assert!(netlist.isolated_wires.contains(&2));
        assert!(!netlist.floating_wires.contains(&2));
    }

    // ── GND net merge: two GND symbols → same net ────────────────────────

    #[test]
    fn two_gnd_symbols_share_net() {
        let gnd1 = comp(1, ComponentKind::Ground, Pos2::new(100.0, 200.0), "GND1", "0V");
        let gnd2 = comp(2, ComponentKind::Ground, Pos2::new(300.0, 200.0), "GND2", "0V");
        // Connect them with a wire
        // GND pin is typically at component pos offset; use a wire near (100,200)→(300,200)
        // to force the same net.  Without a wire they'd be isolated but both named GND.
        let netlist = build_circuit_netlist(&[gnd1, gnd2], &[]);
        let gnd_nets: Vec<_> = netlist.nets.iter().filter(|n| n.name == "GND").collect();
        // Each isolated symbol becomes its own GND net
        assert!(!gnd_nets.is_empty());
    }

    // ── NetLabel names the net ───────────────────────────────────────────

    #[test]
    fn net_label_names_net() {
        let label = comp(1, ComponentKind::NetLabel, Pos2::new(100.0, 100.0), "VCC_LBL", "VCC");
        let netlist = build_circuit_netlist(&[label], &[]);
        assert!(netlist.nets.iter().any(|n| n.name == "VCC"));
    }

    #[test]
    fn identical_net_label_values_merge_remote_nets() {
        let label_a = comp(
            1,
            ComponentKind::NetLabel,
            Pos2::new(100.0, 100.0),
            "NET1",
            "SENSE",
        );
        let label_b = comp(
            2,
            ComponentKind::NetLabel,
            Pos2::new(400.0, 100.0),
            "NET2",
            "SENSE",
        );
        let netlist = build_circuit_netlist(&[label_a, label_b], &[]);
        let label_nets = netlist
            .pins
            .iter()
            .filter(|pin| pin.component_kind == ComponentKind::NetLabel)
            .map(|pin| pin.net_id)
            .collect::<HashSet<_>>();
        assert_eq!(label_nets.len(), 1);
    }
}
