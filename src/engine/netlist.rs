use super::connectivity::diagnostics::geometry_diagnostics;
use super::connectivity::geometry::{normalized_wire_segments, wire_endpoint_contact_points};
use super::connectivity::labels::merge_key as label_merge_key;
use super::connectivity::union_find::{ConnectivityNodes, ConnectivityUnionFind};
use crate::model::*;
#[cfg(test)]
use egui::Pos2;
use std::collections::{HashMap, HashSet};

pub(crate) fn build_circuit_netlist(components: &[Component], wires: &[Wire]) -> CircuitNetlist {
    build_canonical_connectivity(components, wires).netlist
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn build_circuit_netlist_with_annotations(
    components: &[Component],
    wires: &[Wire],
    annotations: &NetlistAnnotations,
) -> CircuitNetlist {
    build_canonical_connectivity_with_annotations(components, wires, annotations).netlist
}

pub(crate) fn build_canonical_connectivity(
    components: &[Component],
    wires: &[Wire],
) -> CanonicalConnectivity {
    build_canonical_connectivity_with_annotations(components, wires, &NetlistAnnotations::default())
}

pub(crate) fn build_canonical_connectivity_with_annotations(
    components: &[Component],
    wires: &[Wire],
    annotations: &NetlistAnnotations,
) -> CanonicalConnectivity {
    build_canonical_connectivity_with_page_scopes(components, wires, annotations, &HashMap::new())
}

fn build_canonical_connectivity_with_page_scopes(
    components: &[Component],
    wires: &[Wire],
    annotations: &NetlistAnnotations,
    component_pages: &HashMap<u64, usize>,
) -> CanonicalConnectivity {
    // Stage 1: geometry normalization. Saved geometry is not mutated; invalid
    // spans become diagnostics while valid polylines retain source identity.
    let mut diagnostics = geometry_diagnostics(wires);
    let mut nodes = ConnectivityNodes::default();
    let mut nets = ConnectivityUnionFind::default();

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

    let mut named_pin_nodes: HashMap<PinRef, usize> = HashMap::new();
    for component in components {
        for pin in component_pin_defs(component) {
            let node = nodes.node_for(pin.pos);
            nets.ensure(node);
            let pin_ref = PinRef {
                component_id: component.id,
                pin_name: pin.label.to_string(),
            };
            if let Some(previous) = named_pin_nodes.insert(pin_ref, node) {
                nets.union(previous, node);
            }
        }
    }

    // Stage 2: typed endpoint resolution. Electrical identity follows the
    // stored PinRef even if legacy geometry has not yet been moved with a pin.
    for wire in wires {
        let endpoint_rows = [
            (&wire.start, wire.points.first().copied()),
            (&wire.end, wire.points.last().copied()),
        ];
        for (endpoint, geometry_position) in endpoint_rows {
            let WireEndpoint::Pin(pin_ref) = endpoint else {
                continue;
            };
            let pin_position = components
                .iter()
                .find(|component| component.id == pin_ref.component_id)
                .and_then(|component| {
                    component_pin_defs(component)
                        .into_iter()
                        .find(|pin| pin.label == pin_ref.pin_name)
                        .map(|pin| pin.pos)
                });
            match (geometry_position, pin_position) {
                (Some(geometry_position), Some(pin_position)) => {
                    let geometry_node = nodes.node_for(geometry_position);
                    let pin_node = nodes.node_for(pin_position);
                    nets.union(geometry_node, pin_node);
                }
                _ => diagnostics.push(ConnectivityDiagnostic::UnresolvedPinEndpoint {
                    wire_id: wire.id,
                    pin: pin_ref.clone(),
                }),
            }
        }
    }

    // Stage 3: junction resolution. Endpoints on segment interiors form T
    // junctions; an ordinary crossing only joins when explicitly annotated.
    for contact in wire_endpoint_contact_points(wires)
        .into_iter()
        .chain(annotations.junctions.iter().copied())
    {
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

    // Stages 4-5: label resolution and page/global merge.
    let mut label_nodes: HashMap<String, Vec<usize>> = HashMap::new();
    let mut label_occurrences: HashMap<String, usize> = HashMap::new();
    for component in components {
        if component.kind == ComponentKind::Ground {
            for pin in component_pin_defs(component) {
                label_nodes
                    .entry("global:gnd".to_string())
                    .or_default()
                    .push(nodes.node_for(pin.pos));
            }
            continue;
        }
        if component.kind != ComponentKind::NetLabel {
            continue;
        }
        let label = component.value.trim().to_ascii_lowercase();
        if label.is_empty() {
            continue;
        }
        *label_occurrences.entry(label.clone()).or_default() += 1;
        let scope = annotations
            .net_label_scopes
            .get(&component.id)
            .copied()
            .unwrap_or_default();
        let page = component_pages.get(&component.id).copied().unwrap_or(0);
        let Some(key) = label_merge_key(scope, page, &label) else {
            continue;
        };
        for pin in component_pin_defs(component) {
            label_nodes
                .entry(key.clone())
                .or_default()
                .push(nodes.node_for(pin.pos));
        }
    }
    for (normalized_name, count) in label_occurrences {
        if count > 1 {
            diagnostics.push(ConnectivityDiagnostic::DuplicateLabel { normalized_name });
        }
    }
    // Stage 6: union-find connectivity.
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
    for root in &roots {
        let next_id = root_to_id.len();
        root_to_id.insert(*root, next_id);
    }

    // Stage 7: canonical net generation. Root traversal is sorted, making IDs
    // and generated names deterministic for identical input documents.
    let mut generated = 1usize;
    let net_rows = roots
        .iter()
        .map(|&root| {
            let id = root_to_id[&root];
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

    let mut no_connects = Vec::new();
    let pins = pin_rows
        .into_iter()
        .filter_map(|(root, component, pin)| {
            let no_connect = annotations
                .no_connects
                .iter()
                .any(|marker| marker.distance(pin.pos) <= 1.0);
            if no_connect {
                no_connects.push(NoConnectMarker {
                    component_id: component.id,
                    pin_name: pin.label.to_string(),
                    position: pin.pos,
                });
            }
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
                no_connect,
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
                    diagnostics.push(ConnectivityDiagnostic::FloatingWire { wire_id: wire.id });
                } else if net.connected_pins.len() == 1 {
                    isolated_wires.push(wire.id);
                }
            }
        }
    }

    let wire_segments = normalized_wire_segments(wires, &annotations.junctions)
        .into_iter()
        .filter_map(|segment| {
            let root = nets.find(nodes.node_for(segment.points[0]));
            let net_id = root_to_id.get(&root).copied()?;
            Some(WireNetSegment {
                id: segment.id,
                source_wire_id: segment.wire_id,
                net_id,
                points: segment.points,
            })
        })
        .collect();

    let netlist = CircuitNetlist {
        nets: net_rows,
        pins,
        wire_nets,
        wire_segments,
        floating_wires,
        isolated_wires,
        explicit_junctions: annotations.junctions.clone(),
        no_connects,
    };

    let pin_nets = netlist
        .pins
        .iter()
        .map(|pin| {
            (
                PinRef {
                    component_id: pin.component_id,
                    pin_name: pin.pin_name.clone(),
                },
                pin.net_id,
            )
        })
        .collect();
    let junction_nets = annotations
        .junctions
        .iter()
        .filter_map(|position| {
            let node = nodes.node_for(*position);
            let root = nets.find(node);
            root_to_id
                .get(&root)
                .copied()
                .map(|net_id| (ConnectivityPoint::from(*position), net_id))
        })
        .collect();
    let mut wire_segment_nets = HashMap::new();
    for wire in wires {
        for (segment_index, segment) in wire.points.windows(2).enumerate() {
            let root = nets.find(nodes.node_for(segment[0]));
            if let Some(net_id) = root_to_id.get(&root).copied() {
                wire_segment_nets.insert(WireSegmentId::new(wire.id, segment_index), net_id);
            }
        }
    }

    // Stage 8: diagnostics are data; malformed connectivity does not abort the
    // graph or prevent unaffected nets from being consumed.
    CanonicalConnectivity {
        netlist,
        pin_nets,
        junction_nets,
        wire_segment_nets,
        diagnostics,
    }
}

#[allow(dead_code)]
pub(crate) fn build_multi_page_circuit_netlist(
    pages: &[(&[Component], &[Wire])],
) -> CircuitNetlist {
    build_multi_page_circuit_netlist_with_annotations(pages, &NetlistAnnotations::default())
}

#[allow(dead_code)]
pub(crate) fn build_multi_page_circuit_netlist_with_annotations(
    pages: &[(&[Component], &[Wire])],
    annotations: &NetlistAnnotations,
) -> CircuitNetlist {
    let mut components = Vec::new();
    let mut wires = Vec::new();
    let mut component_pages = HashMap::new();
    for (page_index, (page_components, page_wires)) in pages.iter().enumerate() {
        let offset = page_index as f32 * 1_000_000.0;
        components.extend(page_components.iter().cloned().map(|mut component| {
            component_pages.insert(component.id, page_index);
            component.pos.x += offset;
            component
        }));
        wires.extend(page_wires.iter().cloned().map(|mut wire| {
            for point in &mut wire.points {
                point.x += offset;
            }
            wire
        }));
    }
    build_canonical_connectivity_with_page_scopes(
        &components,
        &wires,
        annotations,
        &component_pages,
    )
    .netlist
}

fn electrical_type_for_role(role: PinRole) -> ElectricalType {
    match role {
        PinRole::Passive => ElectricalType::Passive,
        PinRole::Positive => ElectricalType::PowerIn,
        PinRole::PowerOutput => ElectricalType::PowerOutput,
        PinRole::Ground => ElectricalType::Ground,
        PinRole::Digital => ElectricalType::Digital,
        PinRole::I2c => ElectricalType::I2c,
        PinRole::Control => ElectricalType::Control,
        PinRole::Output => ElectricalType::Output,
        PinRole::Input => ElectricalType::Input,
        PinRole::Bidirectional => ElectricalType::Bidirectional,
        PinRole::OpenCollector => ElectricalType::OpenCollector,
        PinRole::NoConnect => ElectricalType::NoConnect,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type ConnectivitySignature = (
        Vec<(u64, String, NetId)>,
        Vec<(u64, usize, NetId)>,
        Vec<(i64, i64, NetId)>,
    );

    fn comp(id: u64, kind: ComponentKind, pos: Pos2, label: &str, value: &str) -> Component {
        Component {
            id,
            kind,
            pos,
            rotation: 0,
            label: label.to_string(),
            value: value.to_string(),
            part_id: None,
        }
    }

    fn wire(id: u64, points: Vec<Pos2>) -> Wire {
        Wire::new(id, points)
    }

    fn pin_position(component: &Component, name: &str) -> Pos2 {
        component_pin_defs(component)
            .into_iter()
            .find(|pin| pin.label == name)
            .unwrap()
            .pos
    }

    fn pin_ref(component_id: u64, pin_name: &str) -> PinRef {
        PinRef {
            component_id,
            pin_name: pin_name.to_string(),
        }
    }

    fn connectivity_signature(connectivity: &CanonicalConnectivity) -> ConnectivitySignature {
        let mut pins = connectivity
            .pin_nets
            .iter()
            .map(|(pin, &net)| (pin.component_id, pin.pin_name.clone(), net))
            .collect::<Vec<_>>();
        pins.sort();
        let mut segments = connectivity
            .wire_segment_nets
            .iter()
            .map(|(segment, &net)| (segment.wire_id, segment.segment_index, net))
            .collect::<Vec<_>>();
        segments.sort();
        let mut junctions = connectivity
            .junction_nets
            .iter()
            .map(|(point, &net)| (point.x_milli, point.y_milli, net))
            .collect::<Vec<_>>();
        junctions.sort();
        (pins, segments, junctions)
    }

    #[test]
    fn canonical_pin_to_pin_direct_wire_has_exact_mappings() {
        let left = comp(
            1,
            ComponentKind::Resistor,
            Pos2::new(100.0, 100.0),
            "R1",
            "1k",
        );
        let right = comp(
            2,
            ComponentKind::Resistor,
            Pos2::new(260.0, 100.0),
            "R2",
            "1k",
        );
        let a = pin_position(&left, "B");
        let b = pin_position(&right, "A");
        let wire = Wire::with_endpoints(
            10,
            vec![a, b],
            WireEndpoint::Pin(pin_ref(1, "B")),
            WireEndpoint::Pin(pin_ref(2, "A")),
        );

        let connectivity = build_canonical_connectivity(&[left, right], &[wire]);
        let expected = connectivity.net_for_pin(&pin_ref(1, "B")).unwrap();
        assert_eq!(connectivity.net_for_pin(&pin_ref(2, "A")), Some(expected));
        assert_eq!(
            connectivity.net_for_segment(WireSegmentId::new(10, 0)),
            Some(expected)
        );
        assert_eq!(connectivity.netlist.wire_nets[&10], expected);
    }

    #[test]
    fn canonical_crossing_and_explicit_junction_have_exact_segment_maps() {
        let horizontal = wire(1, vec![Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0)]);
        let vertical = wire(2, vec![Pos2::new(50.0, -50.0), Pos2::new(50.0, 50.0)]);
        let without = build_canonical_connectivity(&[], &[horizontal.clone(), vertical.clone()]);
        assert_ne!(
            without.net_for_segment(WireSegmentId::new(1, 0)),
            without.net_for_segment(WireSegmentId::new(2, 0))
        );

        let junction = Pos2::new(50.0, 0.0);
        let annotations = NetlistAnnotations {
            junctions: vec![junction],
            ..Default::default()
        };
        let with = build_canonical_connectivity_with_annotations(
            &[],
            &[horizontal, vertical],
            &annotations,
        );
        let expected = with.net_for_junction(junction).unwrap();
        assert_eq!(
            with.net_for_segment(WireSegmentId::new(1, 0)),
            Some(expected)
        );
        assert_eq!(
            with.net_for_segment(WireSegmentId::new(2, 0)),
            Some(expected)
        );
    }

    #[test]
    fn canonical_t_branch_overlap_and_shared_junction_are_one_net() {
        let wires = vec![
            wire(1, vec![Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0)]),
            wire(2, vec![Pos2::new(50.0, -50.0), Pos2::new(50.0, 0.0)]),
            wire(3, vec![Pos2::new(25.0, 0.0), Pos2::new(125.0, 0.0)]),
            wire(4, vec![Pos2::new(50.0, 0.0), Pos2::new(50.0, 60.0)]),
        ];
        let connectivity = build_canonical_connectivity(&[], &wires);
        let expected = connectivity.netlist.wire_nets[&1];
        for wire_id in 1..=4 {
            assert_eq!(connectivity.netlist.wire_nets[&wire_id], expected);
            assert_eq!(
                connectivity.net_for_segment(WireSegmentId::new(wire_id, 0)),
                Some(expected)
            );
        }
    }

    #[test]
    fn canonical_ignores_component_body_and_unrelated_pin_overflight() {
        let resistor = comp(
            1,
            ComponentKind::Resistor,
            Pos2::new(100.0, 100.0),
            "R1",
            "1k",
        );
        let pin_a = pin_position(&resistor, "A");
        let wires = vec![
            wire(10, vec![Pos2::new(100.0, 40.0), Pos2::new(100.0, 100.0)]),
            wire(
                11,
                vec![
                    Pos2::new(pin_a.x - 30.0, pin_a.y),
                    Pos2::new(pin_a.x + 30.0, pin_a.y),
                ],
            ),
        ];
        let connectivity = build_canonical_connectivity(&[resistor], &wires);
        let pin_net = connectivity.net_for_pin(&pin_ref(1, "A")).unwrap();
        assert_ne!(connectivity.netlist.wire_nets[&10], pin_net);
        assert_ne!(connectivity.netlist.wire_nets[&11], pin_net);
    }

    #[test]
    fn typed_endpoint_survives_component_move_and_rotation() {
        let original = comp(
            1,
            ComponentKind::Resistor,
            Pos2::new(100.0, 100.0),
            "R1",
            "1k",
        );
        let peer = comp(
            2,
            ComponentKind::Resistor,
            Pos2::new(280.0, 100.0),
            "R2",
            "1k",
        );
        let old_start = pin_position(&original, "B");
        let end = pin_position(&peer, "A");
        let wire = Wire::with_endpoints(
            10,
            vec![old_start, end],
            WireEndpoint::Pin(pin_ref(1, "B")),
            WireEndpoint::Pin(pin_ref(2, "A")),
        );
        let mut moved = original;
        moved.pos += egui::Vec2::new(40.0, 20.0);
        moved.rotation = 90;

        let connectivity = build_canonical_connectivity(&[moved, peer], &[wire]);
        assert_eq!(
            connectivity.net_for_pin(&pin_ref(1, "B")),
            connectivity.net_for_pin(&pin_ref(2, "A"))
        );
    }

    #[test]
    fn canonical_generation_is_deterministic_for_all_exact_maps() {
        let wires = vec![
            wire(7, vec![Pos2::new(0.0, 0.0), Pos2::new(80.0, 0.0)]),
            wire(8, vec![Pos2::new(40.0, -40.0), Pos2::new(40.0, 0.0)]),
        ];
        let first = build_canonical_connectivity(&[], &wires);
        let second = build_canonical_connectivity(&[], &wires);
        assert_eq!(
            connectivity_signature(&first),
            connectivity_signature(&second)
        );
        assert_eq!(first.diagnostics, second.diagnostics);
    }

    // ── Basic wire-to-pin connection ─────────────────────────────────────

    #[test]
    fn builds_net_from_wire_and_component_pins() {
        let r1 = comp(
            1,
            ComponentKind::Resistor,
            Pos2::new(100.0, 100.0),
            "R1",
            "1k",
        );
        let led = comp(
            2,
            ComponentKind::Led,
            Pos2::new(220.0, 100.0),
            "LED1",
            "red",
        );
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
        let r1_b = netlist
            .pins
            .iter()
            .find(|p| p.component_label == "R1" && p.pin_name == "B")
            .unwrap();
        let led_a = netlist
            .pins
            .iter()
            .find(|p| p.component_label == "LED1" && p.pin_name == "A")
            .unwrap();

        assert_eq!(r1_b.net_id, led_a.net_id);
        assert_eq!(netlist.wire_nets.len(), 1);
    }

    // ── GND net gets named "GND" ─────────────────────────────────────────

    #[test]
    fn ground_symbol_names_net_gnd() {
        let gnd = comp(
            1,
            ComponentKind::Ground,
            Pos2::new(100.0, 200.0),
            "GND1",
            "0V",
        );
        let netlist = build_circuit_netlist(&[gnd], &[]);
        assert!(netlist.nets.iter().any(|net| net.name == "GND"));
    }

    // ── Two isolated components → two separate nets ──────────────────────

    #[test]
    fn isolated_components_have_separate_nets() {
        let r1 = comp(
            1,
            ComponentKind::Resistor,
            Pos2::new(100.0, 100.0),
            "R1",
            "1k",
        );
        let r2 = comp(
            2,
            ComponentKind::Resistor,
            Pos2::new(400.0, 100.0),
            "R2",
            "2k",
        );
        let netlist = build_circuit_netlist(&[r1, r2], &[]);

        let r1_a = netlist
            .pins
            .iter()
            .find(|p| p.component_label == "R1" && p.pin_name == "A")
            .unwrap();
        let r2_a = netlist
            .pins
            .iter()
            .find(|p| p.component_label == "R2" && p.pin_name == "A")
            .unwrap();

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
        let horizontal_segments = netlist
            .wire_segments
            .iter()
            .filter(|segment| segment.source_wire_id == 1)
            .count();
        assert_eq!(
            horizontal_segments, 2,
            "A T-junction must split the touched wire into segment-level graph edges"
        );
    }

    #[test]
    fn crossing_wires_without_contact_point_stay_separate() {
        let horizontal = wire(1, vec![Pos2::new(0.0, 100.0), Pos2::new(200.0, 100.0)]);
        let vertical = wire(2, vec![Pos2::new(100.0, 0.0), Pos2::new(100.0, 200.0)]);

        let netlist = build_circuit_netlist(&[], &[horizontal, vertical]);
        assert_ne!(
            netlist.wire_nets[&1], netlist.wire_nets[&2],
            "Interior wire crossings must not connect without an explicit contact point"
        );
        assert_eq!(netlist.wire_segments.len(), 2);
        let segment_nets = netlist
            .wire_segments
            .iter()
            .map(|segment| segment.net_id)
            .collect::<HashSet<_>>();
        assert_eq!(
            segment_nets.len(),
            2,
            "Crossing wires without a junction remain separate segment nets"
        );
    }

    #[test]
    fn component_pin_touching_wire_midpoint_does_not_join_wire_net() {
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
        assert!(!pin.connected_by_wire);
        assert_ne!(netlist.wire_nets[&11], pin.net_id);
    }

    // ── Floating wire has no component pins ──────────────────────────────

    #[test]
    fn floating_wire_detected() {
        let w = wire(99, vec![Pos2::new(500.0, 500.0), Pos2::new(600.0, 500.0)]);
        let netlist = build_circuit_netlist(&[], &[w]);
        assert!(
            netlist.floating_wires.contains(&99),
            "Free-standing wire should be floating"
        );
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
        let gnd1 = comp(
            1,
            ComponentKind::Ground,
            Pos2::new(100.0, 200.0),
            "GND1",
            "0V",
        );
        let gnd2 = comp(
            2,
            ComponentKind::Ground,
            Pos2::new(300.0, 200.0),
            "GND2",
            "0V",
        );
        let netlist = build_circuit_netlist(&[gnd1, gnd2], &[]);
        let gnd_nets: Vec<_> = netlist.nets.iter().filter(|n| n.name == "GND").collect();
        assert_eq!(gnd_nets.len(), 1);
        let pin_nets = netlist
            .pins
            .iter()
            .filter(|pin| pin.component_kind == ComponentKind::Ground)
            .map(|pin| pin.net_id)
            .collect::<HashSet<_>>();
        assert_eq!(pin_nets.len(), 1);
    }

    // ── NetLabel names the net ───────────────────────────────────────────

    #[test]
    fn net_label_names_net() {
        let label = comp(
            1,
            ComponentKind::NetLabel,
            Pos2::new(100.0, 100.0),
            "VCC_LBL",
            "VCC",
        );
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

    #[test]
    fn duplicate_labels_are_connected_and_diagnosed() {
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
            "sense",
        );
        let connectivity = build_canonical_connectivity(&[label_a, label_b], &[]);

        assert!(
            connectivity
                .diagnostics
                .contains(&ConnectivityDiagnostic::DuplicateLabel {
                    normalized_name: "sense".to_string(),
                })
        );
        let net_ids = connectivity
            .pin_nets
            .values()
            .copied()
            .collect::<HashSet<_>>();
        assert_eq!(net_ids.len(), 1);
    }

    #[test]
    fn explicit_junction_dot_connects_crossing_wires() {
        let horizontal = wire(1, vec![Pos2::new(0.0, 100.0), Pos2::new(200.0, 100.0)]);
        let vertical = wire(2, vec![Pos2::new(100.0, 0.0), Pos2::new(100.0, 200.0)]);
        let annotations = NetlistAnnotations {
            junctions: vec![Pos2::new(100.0, 100.0)],
            no_connects: Vec::new(),
            net_label_scopes: HashMap::new(),
        };

        let netlist =
            build_circuit_netlist_with_annotations(&[], &[horizontal, vertical], &annotations);

        assert_eq!(netlist.wire_nets[&1], netlist.wire_nets[&2]);
        assert_eq!(netlist.explicit_junctions, annotations.junctions);
        assert_eq!(
            netlist.wire_segments.len(),
            4,
            "An explicit junction at a crossing splits both crossed wires"
        );
        let segment_nets = netlist
            .wire_segments
            .iter()
            .map(|segment| segment.net_id)
            .collect::<HashSet<_>>();
        assert_eq!(segment_nets.len(), 1);
    }

    #[test]
    fn no_connect_marker_marks_intentionally_open_pin() {
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
        let annotations = NetlistAnnotations {
            junctions: Vec::new(),
            no_connects: vec![pin_a],
            net_label_scopes: HashMap::new(),
        };

        let netlist = build_circuit_netlist_with_annotations(&[resistor], &[], &annotations);
        let pin = netlist
            .pins
            .iter()
            .find(|pin| pin.component_id == 1 && pin.pin_name == "A")
            .unwrap();

        assert!(pin.no_connect);
        assert_eq!(netlist.no_connects.len(), 1);
    }

    #[test]
    fn identical_net_labels_merge_across_pages_without_geometry_leakage() {
        let page_a_label = comp(
            1,
            ComponentKind::NetLabel,
            Pos2::new(100.0, 100.0),
            "SDA_A",
            "SDA",
        );
        let page_b_label = comp(
            2,
            ComponentKind::NetLabel,
            Pos2::new(900.0, 100.0),
            "SDA_B",
            "SDA",
        );
        let page_a_wire = wire(10, vec![Pos2::new(0.0, 0.0), Pos2::new(20.0, 0.0)]);
        let page_b_wire = wire(11, vec![Pos2::new(0.0, 0.0), Pos2::new(20.0, 0.0)]);

        let netlist = build_multi_page_circuit_netlist(&[
            (&[page_a_label][..], &[page_a_wire][..]),
            (&[page_b_label][..], &[page_b_wire][..]),
        ]);
        let label_nets = netlist
            .pins
            .iter()
            .filter(|pin| pin.component_kind == ComponentKind::NetLabel)
            .map(|pin| pin.net_id)
            .collect::<HashSet<_>>();

        assert_eq!(label_nets.len(), 1);
        assert_ne!(
            netlist.wire_nets[&10], netlist.wire_nets[&11],
            "Equal coordinates on different schematic pages must not connect unless labels do"
        );
    }

    #[test]
    fn local_net_labels_name_but_do_not_merge_remote_islands() {
        let label_a = comp(
            1,
            ComponentKind::NetLabel,
            Pos2::new(100.0, 100.0),
            "SIG_A",
            "SIG",
        );
        let label_b = comp(
            2,
            ComponentKind::NetLabel,
            Pos2::new(300.0, 100.0),
            "SIG_B",
            "SIG",
        );
        let annotations = NetlistAnnotations {
            net_label_scopes: [
                (label_a.id, NetLabelScope::Local),
                (label_b.id, NetLabelScope::Local),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        };

        let netlist =
            build_circuit_netlist_with_annotations(&[label_a, label_b], &[], &annotations);
        let label_nets = netlist
            .pins
            .iter()
            .filter(|pin| pin.component_kind == ComponentKind::NetLabel)
            .map(|pin| pin.net_id)
            .collect::<HashSet<_>>();
        let sig_net_count = netlist.nets.iter().filter(|net| net.name == "SIG").count();

        assert_eq!(label_nets.len(), 4);
        assert_eq!(
            sig_net_count, 4,
            "Local labels may share a display name but must not create a hidden remote connection"
        );
    }

    #[test]
    fn page_scoped_net_labels_merge_within_page_only() {
        let page1_a = comp(
            1,
            ComponentKind::NetLabel,
            Pos2::new(100.0, 100.0),
            "P1_A",
            "SENSE",
        );
        let page1_b = comp(
            2,
            ComponentKind::NetLabel,
            Pos2::new(300.0, 100.0),
            "P1_B",
            "SENSE",
        );
        let page2_a = comp(
            3,
            ComponentKind::NetLabel,
            Pos2::new(100.0, 100.0),
            "P2_A",
            "SENSE",
        );
        let page2_b = comp(
            4,
            ComponentKind::NetLabel,
            Pos2::new(300.0, 100.0),
            "P2_B",
            "SENSE",
        );
        let annotations = NetlistAnnotations {
            net_label_scopes: [
                (page1_a.id, NetLabelScope::Page),
                (page1_b.id, NetLabelScope::Page),
                (page2_a.id, NetLabelScope::Page),
                (page2_b.id, NetLabelScope::Page),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        };

        let netlist = build_multi_page_circuit_netlist_with_annotations(
            &[
                (&[page1_a, page1_b][..], &[][..]),
                (&[page2_a, page2_b][..], &[][..]),
            ],
            &annotations,
        );
        let net_by_label = |label: &str| {
            netlist
                .pins
                .iter()
                .find(|pin| pin.component_label == label)
                .map(|pin| pin.net_id)
                .unwrap()
        };

        assert_eq!(net_by_label("P1_A"), net_by_label("P1_B"));
        assert_eq!(net_by_label("P2_A"), net_by_label("P2_B"));
        assert_ne!(
            net_by_label("P1_A"),
            net_by_label("P2_A"),
            "Page-scoped labels must not merge across schematic pages"
        );
    }

    #[test]
    fn generated_net_ids_and_names_are_deterministic() {
        let r1 = comp(
            1,
            ComponentKind::Resistor,
            Pos2::new(100.0, 100.0),
            "R1",
            "1k",
        );
        let r2 = comp(
            2,
            ComponentKind::Resistor,
            Pos2::new(300.0, 100.0),
            "R2",
            "2k",
        );
        let w1 = wire(10, vec![Pos2::new(10.0, 10.0), Pos2::new(20.0, 10.0)]);
        let w2 = wire(11, vec![Pos2::new(30.0, 10.0), Pos2::new(40.0, 10.0)]);

        let first = build_circuit_netlist(&[r1.clone(), r2.clone()], &[w1.clone(), w2.clone()]);
        let second = build_circuit_netlist(&[r1, r2], &[w1, w2]);

        let first_rows = first
            .nets
            .iter()
            .map(|net| (net.id, net.name.clone()))
            .collect::<Vec<_>>();
        let second_rows = second
            .nets
            .iter()
            .map(|net| (net.id, net.name.clone()))
            .collect::<Vec<_>>();
        assert_eq!(first_rows, second_rows);
    }
}
