use super::*;

pub(crate) fn analyze_circuit(components: &[Component], wires: &[Wire]) -> Simulation {
    let connectivity = crate::engine::netlist::build_canonical_connectivity(components, wires);
    analyze_circuit_with_connectivity(components, wires, &connectivity)
}

pub(crate) fn analyze_circuit_with_connectivity(
    components: &[Component],
    wires: &[Wire],
    connectivity: &CanonicalConnectivity,
) -> Simulation {
    let mut nodes = CircuitNodes::default();
    let mut graph: Vec<HashSet<usize>> = Vec::new();
    let mut wire_graph: Vec<HashSet<usize>> = Vec::new();
    let mut positive_nodes = Vec::new();
    let mut return_nodes = Vec::new();
    let mut component_edges = Vec::new();
    let mut powered_module_edges = Vec::new();
    let mut wire_edges = Vec::new();
    let mut component_warnings: HashMap<u64, String> = HashMap::new();
    let mut canonical_members: HashMap<NetId, Vec<usize>> = HashMap::new();

    for wire in wires {
        for segment in wire.points.windows(2) {
            let a = nodes.node_for(segment[0]);
            let b = nodes.node_for(segment[1]);
            wire_edges.push((wire.id, a, b));
        }
        if let Some(&net_id) = connectivity.netlist.wire_nets.get(&wire.id) {
            canonical_members
                .entry(net_id)
                .or_default()
                .extend(wire.points.iter().map(|&point| nodes.node_for(point)));
        }
    }
    for component in components {
        for pin in component_pin_defs(component) {
            let node = nodes.node_for(pin.pos);
            if let Some(net_id) = connectivity.net_for_pin(&PinRef {
                component_id: component.id,
                pin_name: pin.label.to_string(),
            }) {
                canonical_members.entry(net_id).or_default().push(node);
            }
        }
    }
    for members in canonical_members.values() {
        for pair in members.windows(2) {
            connect(&mut graph, pair[0], pair[1]);
            connect(&mut wire_graph, pair[0], pair[1]);
        }
    }

    // Pass 1: sources/returns only. Component conductance is added after
    // wire-only reachability exists, so polarity checks can reject bad paths.
    for component in components {
        let pins = component_pin_defs(component);
        let pin_nodes: Vec<usize> = pins.iter().map(|pin| nodes.node_for(pin.pos)).collect();
        match component.kind {
            ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource => {
                for (pin, &node) in pins.iter().zip(&pin_nodes) {
                    match pin.role {
                        PinRole::Positive => positive_nodes.push(node),
                        PinRole::Ground => return_nodes.push(node),
                        _ => {}
                    }
                }
                if positive_nodes.is_empty() && return_nodes.is_empty() && pin_nodes.len() >= 2 {
                    return_nodes.push(pin_nodes[0]);
                    positive_nodes.push(pin_nodes[1]);
                }
            }
            ComponentKind::Ground => {
                for &node in &pin_nodes {
                    return_nodes.push(node);
                }
            }
            _ => {}
        }
    }

    // Wire-only reachability — used for polarity checking and short detection
    let wire_from_positive = reachable_nodes(&wire_graph, &positive_nodes);
    let wire_from_return = reachable_nodes(&wire_graph, &return_nodes);
    let (control_from_positive, control_from_return) = controlled_reachability_graph(
        components,
        &mut nodes,
        &graph,
        &wire_from_positive,
        &wire_from_return,
        &positive_nodes,
        &return_nodes,
    );

    // Pass 2: non-module conductor/load edges.
    for component in components {
        if matches!(
            component.kind,
            ComponentKind::VSource
                | ComponentKind::Battery
                | ComponentKind::ISource
                | ComponentKind::Ground
        ) || component_is_powered_module(component)
        {
            continue;
        }

        let conductance = component_conductance(component);
        if conductance == Conductance::Open {
            continue;
        }

        let pins = component_pin_defs(component);
        let pin_nodes: Vec<usize> = pins.iter().map(|pin| nodes.node_for(pin.pos)).collect();
        if pin_nodes.len() < 2 {
            continue;
        }

        if component_is_controlled_switch(component.kind)
            && !controlled_switch_is_enabled(
                component.kind,
                &pins,
                &pin_nodes,
                &control_from_positive,
                &control_from_return,
            )
        {
            let has_control_wire = pins.iter().zip(&pin_nodes).any(|(pin, &node)| {
                pin.role == PinRole::Control
                    && graph
                        .get(node)
                        .is_some_and(|neighbors| !neighbors.is_empty())
            });
            if has_control_wire {
                component_warnings.insert(
                    component.id,
                    "Control warning: transistor gate/base is not driven to an active level."
                        .to_string(),
                );
            } else {
                component_warnings.insert(
                    component.id,
                    "Control warning: transistor gate/base is open.".to_string(),
                );
            }
            continue;
        }

        if component_is_polarized_diode(component.kind)
            && diode_appears_reversed(&pins, &pin_nodes, &wire_from_positive, &wire_from_return)
        {
            component_warnings.insert(
                component.id,
                "Polarity warning: anode appears on return and cathode on source +.".to_string(),
            );
            continue;
        }

        let Some((a, b)) = conductive_terminal_nodes(component.kind, &pins, &pin_nodes) else {
            continue;
        };
        connect(&mut graph, a, b);
        component_edges.push((component.id, a, b, conductance == Conductance::Load));
        if component.kind == ComponentKind::Relay {
            let relay_positive_reach = reachable_nodes(&graph, &positive_nodes);
            let relay_return_reach = reachable_nodes(&graph, &return_nodes);
            if relay_coil_is_enabled(
                &pins,
                &pin_nodes,
                &relay_positive_reach,
                &relay_return_reach,
            ) && let Some((com, no)) = relay_contact_nodes(&pins, &pin_nodes)
            {
                connect(&mut graph, com, no);
                component_edges.push((component.id, com, no, false));
            }
        }
    }

    let external_graph = graph.clone();

    // Pass 3: powered modules — only connect if polarity is correct
    for component in components {
        if !component_is_powered_module(component) {
            continue;
        }
        let pins = component_pin_defs(component);
        let pin_nodes: Vec<usize> = pins.iter().map(|pin| nodes.node_for(pin.pos)).collect();

        let positives: Vec<usize> = pins
            .iter()
            .zip(&pin_nodes)
            .filter(|(pin, _)| pin.role == PinRole::Positive)
            .map(|(_, &node)| node)
            .collect();
        let grounds: Vec<usize> = pins
            .iter()
            .zip(&pin_nodes)
            .filter(|(pin, _)| pin.role == PinRole::Ground)
            .map(|(_, &node)| node)
            .collect();

        if positives.is_empty() || grounds.is_empty() {
            continue;
        }

        let vcc_on_positive = positives.iter().any(|&n| wire_from_positive.contains(&n));
        let gnd_on_return = grounds.iter().any(|&n| wire_from_return.contains(&n));

        if vcc_on_positive && gnd_on_return {
            for &pos in &positives {
                for &gnd in &grounds {
                    connect(&mut graph, pos, gnd);
                    powered_module_edges.push((component.id, pos, gnd));
                }
            }
        } else if !wire_from_positive.is_empty() && !wire_from_return.is_empty() {
            let vcc_on_return = positives.iter().any(|&n| wire_from_return.contains(&n));
            let gnd_on_positive = grounds.iter().any(|&n| wire_from_positive.contains(&n));
            if vcc_on_return || gnd_on_positive {
                component_warnings.insert(
                    component.id,
                    "Polarity reversed: swap VCC and GND connections.".to_string(),
                );
            }
        }
    }

    // Pass 4: modules powered by already-powered modules (e.g., OLED via ESP32's 3V3 output).
    // Collect positive/ground pin nodes from modules powered above, then check remaining modules.
    {
        let mut ext_positive = positive_nodes.clone();
        let mut ext_return = return_nodes.clone();
        for (powered_id, _, _) in &powered_module_edges {
            if let Some(c) = components.iter().find(|c| c.id == *powered_id) {
                for pin in component_pin_defs(c) {
                    let Some(n) = nodes.find_existing(pin.pos) else {
                        continue;
                    };
                    match pin.role {
                        PinRole::Positive => ext_positive.push(n),
                        PinRole::Ground => ext_return.push(n),
                        _ => {}
                    }
                }
            }
        }
        let ext_wire_pos = reachable_nodes(&wire_graph, &ext_positive);
        let ext_wire_ret = reachable_nodes(&wire_graph, &ext_return);

        for component in components {
            if !component_is_powered_module(component) {
                continue;
            }
            if powered_module_edges
                .iter()
                .any(|(id, _, _)| *id == component.id)
            {
                continue;
            }

            let pins = component_pin_defs(component);
            let positives: Vec<usize> = pins
                .iter()
                .filter(|p| p.role == PinRole::Positive)
                .filter_map(|p| nodes.find_existing(p.pos))
                .collect();
            let grounds: Vec<usize> = pins
                .iter()
                .filter(|p| p.role == PinRole::Ground)
                .filter_map(|p| nodes.find_existing(p.pos))
                .collect();

            if positives.is_empty() || grounds.is_empty() {
                continue;
            }

            let vcc_ok = positives.iter().any(|&n| ext_wire_pos.contains(&n));
            let gnd_ok = grounds.iter().any(|&n| ext_wire_ret.contains(&n));

            if vcc_ok && gnd_ok {
                for &pos in &positives {
                    for &gnd in &grounds {
                        connect(&mut graph, pos, gnd);
                        powered_module_edges.push((component.id, pos, gnd));
                    }
                }
            } else if !ext_wire_pos.is_empty() && !ext_wire_ret.is_empty() {
                let vcc_on_ret = positives.iter().any(|&n| ext_wire_ret.contains(&n));
                let gnd_on_pos = grounds.iter().any(|&n| ext_wire_pos.contains(&n));
                if vcc_on_ret || gnd_on_pos {
                    component_warnings.entry(component.id).or_insert_with(|| {
                        "Polarity reversed: swap VCC and GND connections.".to_string()
                    });
                }
            }
        }
    }

    let mut details = validate_i2c_links(components, &nodes, &wire_graph);

    if positive_nodes.is_empty() || return_nodes.is_empty() {
        details.push("Add a source/battery and GND return to run live simulation.".to_string());
        let (dc, dc_error) =
            match mna::solve_dc_detailed_with_connectivity(components, wires, connectivity) {
                Ok(dc) => (Some(dc), None),
                Err(error) => (None, Some(error)),
            };
        return Simulation {
            status: SimulationStatus::Warning,
            summary: "No source or return".to_string(),
            explanation:
                "Add a voltage/current source and a return path to GND before DC current can flow."
                    .to_string(),
            details,
            component_warnings,
            dc,
            dc_error,
            ..Simulation::default()
        };
    }

    let from_positive = reachable_nodes(&graph, &positive_nodes);
    let from_return = reachable_nodes(&graph, &return_nodes);
    let loop_nodes: HashSet<usize> = from_positive.intersection(&from_return).copied().collect();
    if loop_nodes.is_empty() {
        details.push("No closed path between source + and return/GND.".to_string());
        let (dc, dc_error) =
            match mna::solve_dc_detailed_with_connectivity(components, wires, connectivity) {
                Ok(dc) => (Some(dc), None),
                Err(error) => (None, Some(error)),
            };
        return Simulation {
            status: SimulationStatus::Warning,
            summary: "Open circuit".to_string(),
            explanation:
                "Voltage can exist on open nodes, but current is 0 A until a closed path reaches the return/GND node."
                    .to_string(),
            details,
            component_warnings,
            dc,
            dc_error,
            ..Simulation::default()
        };
    }

    let energized_component_edges: Vec<(u64, bool)> = component_edges
        .into_iter()
        .filter(|(_, a, b, _)| loop_nodes.contains(a) && loop_nodes.contains(b))
        .map(|(id, _, _, is_load)| (id, is_load))
        .collect();
    let energized_loads: HashSet<u64> = energized_component_edges
        .iter()
        .filter(|(_, is_load)| *is_load)
        .map(|(id, _)| *id)
        .chain(
            powered_module_edges
                .iter()
                .filter(|(_, a, b)| loop_nodes.contains(a) && loop_nodes.contains(b))
                .map(|(id, _, _)| *id),
        )
        .collect();

    let mut energized_components: HashSet<u64> = energized_component_edges
        .into_iter()
        .map(|(id, _)| id)
        .chain(
            powered_module_edges
                .into_iter()
                .filter(|(_, a, b)| loop_nodes.contains(a) && loop_nodes.contains(b))
                .map(|(id, _, _)| id),
        )
        .chain(
            components
                .iter()
                .filter(|component| {
                    matches!(
                        component.kind,
                        ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource
                    ) && component_pin_defs(component)
                        .iter()
                        .map(|pin| nodes.find_existing(pin.pos))
                        .all(|node| node.is_some_and(|node| loop_nodes.contains(&node)))
                })
                .map(|component| component.id),
        )
        .collect();

    let mut energized_wires: HashSet<u64> = wire_edges
        .into_iter()
        .filter(|(_, a, b)| loop_nodes.contains(a) && loop_nodes.contains(b))
        .map(|(id, _, _)| id)
        .collect();

    // Wire-only short detection reuses wire_from_positive/return already computed
    let direct_wire_short = wire_from_positive
        .intersection(&wire_from_return)
        .next()
        .is_some();
    let hard_direct_short = explicit_source_to_ground_wire_short(components, wires);
    // Short if + reaches return via bare wires, OR if the closed loop has no resistive/module load.
    let mut shorted = hard_direct_short || (direct_wire_short && energized_loads.is_empty());

    prune_uncontrolled_digital_output_paths(
        components,
        wires,
        &nodes,
        &external_graph,
        &return_nodes,
        &mut energized_components,
        &mut energized_wires,
    );
    mark_powered_digital_output_paths(
        components,
        wires,
        &nodes,
        &external_graph,
        &wire_graph,
        &return_nodes,
        &mut energized_components,
        &mut energized_wires,
    );

    // OLED / Sensor: always require I2C (SDA+SCL) to be wired to a controller.
    // The guard was previously `if !ctrl_sda.is_empty() || !ctrl_scl.is_empty()`,
    // but that skipped the check entirely when the controller's I2C pins had no wires,
    // letting OLED appear energized with wrong or missing connections.
    let (ctrl_sda, ctrl_scl) = collect_controller_i2c_nodes(components, &nodes);
    for component in components {
        if !matches!(component.kind, ComponentKind::Oled | ComponentKind::Sensor) {
            continue;
        }
        if !energized_components.contains(&component.id) {
            continue;
        }
        let mut sda_ok = false;
        let mut scl_ok = false;
        for pin in component_pin_defs(component) {
            let Some(node) = nodes.find_existing(pin.pos) else {
                continue;
            };
            let label = pin.label.to_lowercase();
            if label.contains("sda") {
                sda_ok = ctrl_sda
                    .iter()
                    .any(|&s| nodes_connected(&wire_graph, node, s));
            }
            if label.contains("scl") {
                scl_ok = ctrl_scl
                    .iter()
                    .any(|&s| nodes_connected(&wire_graph, node, s));
            }
        }
        if !sda_ok || !scl_ok {
            energized_components.remove(&component.id);
            component_warnings.entry(component.id).or_insert_with(|| {
                let msg = match (sda_ok, scl_ok) {
                    (false, false) => "SDA and SCL not connected — wire to an I2C controller.",
                    (false, true) => "SDA not connected — wire to controller SDA pin.",
                    (true, false) => "SCL not connected — wire to controller SCL pin.",
                    _ => unreachable!(),
                };
                msg.to_string()
            });
        }
    }

    if !hard_direct_short && has_energized_load_component(components, &energized_components) {
        shorted = false;
    }

    let voltage = estimate_loop_voltage(components, &nodes, &loop_nodes);
    let resistance = estimate_loop_resistance(components, &energized_loads);
    let current = match (voltage, resistance) {
        (Some(v), Some(r)) if r > 0.0 && !shorted => Some(v / r),
        _ => None,
    };

    if shorted {
        details.push("Source + reaches return/GND without a resistive load.".to_string());
    } else {
        details.push(format!("{} energized load(s).", energized_loads.len()));
    }

    let (dc, dc_error) =
        match mna::solve_dc_detailed_with_connectivity(components, wires, connectivity) {
            Ok(dc) => (Some(dc), None),
            Err(error) => {
                details.push(format!(
                    "DC solver: {error}. {}",
                    error.beginner_explanation()
                ));
                (None, Some(error))
            }
        };
    if let Some(dc) = &dc {
        if dc.max_kcl_residual > 1e-8 {
            details.push(format!(
                "KCL diagnostic: maximum residual {}.",
                mna::format_current(dc.max_kcl_residual)
            ));
        }
        if dc.nonlinear_iterations > 0 {
            if dc.nonlinear_converged {
                details.push(format!(
                    "Nonlinear devices: piecewise model converged in {} iteration(s).",
                    dc.nonlinear_iterations
                ));
            } else {
                details.push(
                    "Nonlinear devices: piecewise model reached the iteration limit; treat currents as approximate.".to_string(),
                );
            }
        }
    }
    apply_engineering_checks(
        components,
        dc.as_ref(),
        shorted,
        &mut component_warnings,
        &mut details,
    );
    prune_unphysical_energized_components(
        components,
        dc.as_ref(),
        shorted,
        &mut energized_components,
    );
    prune_unphysical_energized_wires(dc.as_ref(), &mut energized_wires);

    // Append per-component warnings to details after engineering checks.
    for component in components {
        if let Some(warning) = component_warnings.get(&component.id) {
            details.push(format!("{}: {}", component.label, warning));
        }
    }

    Simulation {
        status: simulation_status_from_solver(shorted, dc_error.as_ref()),
        closed: true,
        shorted,
        energized_components,
        energized_wires,
        summary: if shorted {
            "Short circuit".to_string()
        } else {
            "Current flowing".to_string()
        },
        explanation: if shorted {
            "Source positive reaches return/GND without enough load resistance, so the circuit is unsafe and current arrows are suppressed.".to_string()
        } else if dc_error.is_some() {
            "Connectivity shows a closed path, but the DC solver could not fully trust the numeric operating point. Check floating nodes or ideal-source conflicts.".to_string()
        } else {
            "A closed path exists from the source through at least one load and back to return/GND, so DC current can flow.".to_string()
        },
        details,
        voltage,
        resistance,
        current,
        component_warnings,
        dc,
        dc_error,
        ac: None,        // populated in current_simulation()
        transient: None, // populated in current_simulation()
        erc: Vec::new(), // populated after construction via run_erc()
    }
}

#[derive(Default)]
pub(crate) struct CircuitNodes {
    pub(crate) positions: Vec<Pos2>,
}

impl CircuitNodes {
    pub(crate) fn node_for(&mut self, pos: Pos2) -> usize {
        if let Some(index) = self.find_existing(pos) {
            return index;
        }
        self.positions.push(pos);
        self.positions.len() - 1
    }

    pub(crate) fn find_existing(&self, pos: Pos2) -> Option<usize> {
        self.positions
            .iter()
            .position(|existing| existing.distance(pos) <= 1.0)
    }
}

pub(crate) fn connect(graph: &mut Vec<HashSet<usize>>, a: usize, b: usize) {
    let needed = a.max(b) + 1;
    if graph.len() < needed {
        graph.resize_with(needed, HashSet::new);
    }
    graph[a].insert(b);
    graph[b].insert(a);
}

pub(crate) fn connect_wire_contacts(
    nodes: &mut CircuitNodes,
    graph: &mut Vec<HashSet<usize>>,
    wires: &[Wire],
    components: &[Component],
) {
    for contact in wire_contact_points(components, wires) {
        let contact_node = nodes.node_for(contact);
        for wire in wires {
            for segment in wire.points.windows(2) {
                if point_touches_wire_segment(contact, segment[0], segment[1]) {
                    let a = nodes.node_for(segment[0]);
                    let b = nodes.node_for(segment[1]);
                    connect(graph, contact_node, a);
                    connect(graph, contact_node, b);
                }
            }
        }
    }
}

pub(crate) fn wire_contact_points(components: &[Component], wires: &[Wire]) -> Vec<Pos2> {
    let mut points = Vec::new();
    for wire in wires {
        points.extend(wire.points.iter().copied());
    }
    for component in components {
        points.extend(component_pin_defs(component).into_iter().map(|pin| pin.pos));
    }
    points
}

pub(crate) fn reachable_nodes(graph: &[HashSet<usize>], starts: &[usize]) -> HashSet<usize> {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::new();
    for &start in starts {
        if seen.insert(start) {
            queue.push_back(start);
        }
    }

    while let Some(node) = queue.pop_front() {
        if let Some(neighbors) = graph.get(node) {
            for &neighbor in neighbors {
                if seen.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
    }
    seen
}

pub(crate) fn nodes_connected(graph: &[HashSet<usize>], a: usize, b: usize) -> bool {
    reachable_nodes(graph, &[a]).contains(&b)
}

pub(crate) fn prune_unphysical_energized_wires(
    dc: Option<&mna::DcResult>,
    energized_wires: &mut HashSet<u64>,
) {
    let Some(dc) = dc else {
        return;
    };

    energized_wires.retain(|wire_id| {
        let Some(&current) = dc.wire_current.get(wire_id) else {
            return true;
        };
        dc.wire_current_known.contains(wire_id) && current.abs() > 1.0e-12
    });
}

pub(crate) fn module_pin_can_drive_digital_load(pin: &CircuitPin) -> bool {
    matches!(pin.role, PinRole::Digital | PinRole::Output)
        || (pin.role == PinRole::I2c && pin.label.to_ascii_uppercase().contains("GPIO"))
}

pub(crate) fn prune_uncontrolled_digital_output_paths(
    components: &[Component],
    wires: &[Wire],
    nodes: &CircuitNodes,
    external_graph: &[HashSet<usize>],
    return_nodes: &[usize],
    energized_components: &mut HashSet<u64>,
    energized_wires: &mut HashSet<u64>,
) {
    if return_nodes.is_empty() {
        return;
    }

    for module in components
        .iter()
        .filter(|component| component_is_powered_module(component))
    {
        for pin in component_pin_defs(module)
            .iter()
            .filter(|pin| module_pin_can_drive_digital_load(pin))
        {
            let Some(digital_node) = nodes.find_existing(pin.pos) else {
                continue;
            };
            let path_nodes = reachable_nodes(external_graph, &[digital_node]);
            if !return_nodes
                .iter()
                .any(|return_node| path_nodes.contains(return_node))
            {
                continue;
            }

            for component in components {
                if component.id == module.id
                    || matches!(
                        component.kind,
                        ComponentKind::VSource
                            | ComponentKind::Battery
                            | ComponentKind::ISource
                            | ComponentKind::Ground
                    )
                    || component_is_powered_module(component)
                {
                    continue;
                }
                let comp_nodes = component_pin_defs(component)
                    .iter()
                    .filter_map(|pin| nodes.find_existing(pin.pos))
                    .collect::<Vec<_>>();
                if comp_nodes.len() >= 2 && comp_nodes.iter().all(|node| path_nodes.contains(node))
                {
                    energized_components.remove(&component.id);
                }
            }

            for wire in wires {
                if wire.points.windows(2).any(|segment| {
                    let a = nodes.find_existing(segment[0]);
                    let b = nodes.find_existing(segment[1]);
                    a.is_some_and(|node| path_nodes.contains(&node))
                        && b.is_some_and(|node| path_nodes.contains(&node))
                }) {
                    energized_wires.remove(&wire.id);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)] // Mutates coordinated connectivity result sets.
pub(crate) fn mark_powered_digital_output_paths(
    components: &[Component],
    wires: &[Wire],
    nodes: &CircuitNodes,
    external_graph: &[HashSet<usize>],
    wire_graph: &[HashSet<usize>],
    return_nodes: &[usize],
    energized_components: &mut HashSet<u64>,
    energized_wires: &mut HashSet<u64>,
) {
    if return_nodes.is_empty() {
        return;
    }

    let powered_module_ids = components
        .iter()
        .filter(|component| {
            component_is_powered_module(component) && energized_components.contains(&component.id)
        })
        .map(|component| component.id)
        .collect::<Vec<_>>();

    for module_id in powered_module_ids {
        let Some(module) = components
            .iter()
            .find(|component| component.id == module_id)
        else {
            continue;
        };
        let pins = component_pin_defs(module);
        let digital_pins = pins
            .iter()
            .filter(|pin| module_pin_can_drive_digital_load(pin))
            .filter_map(|pin| nodes.find_existing(pin.pos))
            .collect::<Vec<_>>();

        if digital_pins.is_empty()
            || !module_has_closed_digital_input(
                components,
                nodes,
                wire_graph,
                return_nodes,
                &digital_pins,
            )
        {
            continue;
        }

        for output_node in &digital_pins {
            let path_nodes = reachable_nodes(external_graph, &[*output_node]);
            if !return_nodes
                .iter()
                .any(|return_node| path_nodes.contains(return_node))
            {
                continue;
            }

            for component in components {
                if component.id == module.id
                    || matches!(
                        component.kind,
                        ComponentKind::VSource
                            | ComponentKind::Battery
                            | ComponentKind::ISource
                            | ComponentKind::Ground
                    )
                    || component_is_powered_module(component)
                {
                    continue;
                }
                let conductance = component_conductance(component);
                if conductance == Conductance::Open {
                    continue;
                }
                let comp_pins = component_pin_defs(component);
                let comp_nodes = comp_pins
                    .iter()
                    .filter_map(|pin| nodes.find_existing(pin.pos))
                    .collect::<Vec<_>>();
                if comp_nodes.len() >= 2 && comp_nodes.iter().all(|node| path_nodes.contains(node))
                {
                    energized_components.insert(component.id);
                }
            }

            for wire in wires {
                if wire.points.windows(2).any(|segment| {
                    let a = nodes.find_existing(segment[0]);
                    let b = nodes.find_existing(segment[1]);
                    a.is_some_and(|node| path_nodes.contains(&node))
                        && b.is_some_and(|node| path_nodes.contains(&node))
                }) {
                    energized_wires.insert(wire.id);
                }
            }
        }
    }
}

pub(crate) fn module_has_closed_digital_input(
    components: &[Component],
    nodes: &CircuitNodes,
    wire_graph: &[HashSet<usize>],
    return_nodes: &[usize],
    digital_pins: &[usize],
) -> bool {
    for switch in components.iter().filter(|component| {
        component_is_switch(component.kind) && component_conductance(component) != Conductance::Open
    }) {
        let switch_pins = component_pin_defs(switch);
        if switch_pins.len() < 2 {
            continue;
        }
        let Some(a) = nodes.find_existing(switch_pins[0].pos) else {
            continue;
        };
        let Some(b) = nodes.find_existing(switch_pins[1].pos) else {
            continue;
        };

        let a_at_return = return_nodes
            .iter()
            .any(|return_node| nodes_connected(wire_graph, a, *return_node));
        let b_at_return = return_nodes
            .iter()
            .any(|return_node| nodes_connected(wire_graph, b, *return_node));

        for digital_node in digital_pins {
            let digital_at_a = nodes_connected(wire_graph, *digital_node, a);
            let digital_at_b = nodes_connected(wire_graph, *digital_node, b);
            if (digital_at_a && b_at_return) || (digital_at_b && a_at_return) {
                return true;
            }
        }
    }

    false
}

pub(crate) fn controlled_reachability_graph(
    components: &[Component],
    nodes: &mut CircuitNodes,
    base_graph: &[HashSet<usize>],
    wire_from_positive: &HashSet<usize>,
    wire_from_return: &HashSet<usize>,
    positive_nodes: &[usize],
    return_nodes: &[usize],
) -> (HashSet<usize>, HashSet<usize>) {
    let mut graph = base_graph.to_vec();
    for component in components {
        if matches!(
            component.kind,
            ComponentKind::VSource
                | ComponentKind::Battery
                | ComponentKind::ISource
                | ComponentKind::Ground
        ) || component_is_powered_module(component)
            || component_is_controlled_switch(component.kind)
        {
            continue;
        }

        let conductance = component_conductance(component);
        if conductance == Conductance::Open {
            continue;
        }

        let pins = component_pin_defs(component);
        let pin_nodes = pins
            .iter()
            .map(|pin| nodes.node_for(pin.pos))
            .collect::<Vec<_>>();
        if component_is_polarized_diode(component.kind)
            && diode_appears_reversed(&pins, &pin_nodes, wire_from_positive, wire_from_return)
        {
            continue;
        }
        if let Some((a, b)) = conductive_terminal_nodes(component.kind, &pins, &pin_nodes) {
            connect(&mut graph, a, b);
        }
    }

    (
        reachable_nodes(&graph, positive_nodes),
        reachable_nodes(&graph, return_nodes),
    )
}

pub(crate) fn component_is_controlled_switch(kind: ComponentKind) -> bool {
    matches!(
        kind,
        ComponentKind::NpnTransistor
            | ComponentKind::PnpTransistor
            | ComponentKind::Nmosfet
            | ComponentKind::Pmosfet
    )
}

pub(crate) fn controlled_switch_is_enabled(
    kind: ComponentKind,
    pins: &[CircuitPin],
    pin_nodes: &[usize],
    control_from_positive: &HashSet<usize>,
    control_from_return: &HashSet<usize>,
) -> bool {
    let Some(control_node) = pins
        .iter()
        .zip(pin_nodes)
        .find(|(pin, _)| pin.role == PinRole::Control)
        .map(|(_, &node)| node)
    else {
        return false;
    };

    match kind {
        ComponentKind::NpnTransistor | ComponentKind::Nmosfet => {
            control_from_positive.contains(&control_node)
        }
        ComponentKind::PnpTransistor | ComponentKind::Pmosfet => {
            control_from_return.contains(&control_node)
        }
        _ => false,
    }
}

pub(crate) fn relay_coil_is_enabled(
    pins: &[CircuitPin],
    pin_nodes: &[usize],
    control_from_positive: &HashSet<usize>,
    control_from_return: &HashSet<usize>,
) -> bool {
    let by_label = |label: &str| {
        pins.iter()
            .zip(pin_nodes)
            .find(|(pin, _)| pin.label == label)
            .map(|(_, &node)| node)
    };
    let Some(coil_pos) = by_label("COIL+") else {
        return false;
    };
    let Some(coil_neg) = by_label("COIL-") else {
        return false;
    };
    control_from_positive.contains(&coil_pos) && control_from_return.contains(&coil_neg)
}

pub(crate) fn relay_contact_nodes(
    pins: &[CircuitPin],
    pin_nodes: &[usize],
) -> Option<(usize, usize)> {
    let by_label = |label: &str| {
        pins.iter()
            .zip(pin_nodes)
            .find(|(pin, _)| pin.label == label)
            .map(|(_, &node)| node)
    };
    Some((by_label("COM")?, by_label("NO")?))
}

pub(crate) fn conductive_terminal_nodes(
    kind: ComponentKind,
    pins: &[CircuitPin],
    pin_nodes: &[usize],
) -> Option<(usize, usize)> {
    let by_label = |label: &str| {
        pins.iter()
            .zip(pin_nodes)
            .find(|(pin, _)| pin.label == label)
            .map(|(_, &node)| node)
    };

    match kind {
        ComponentKind::NpnTransistor | ComponentKind::PnpTransistor => {
            Some((by_label("C")?, by_label("E")?))
        }
        ComponentKind::Nmosfet | ComponentKind::Pmosfet => Some((by_label("D")?, by_label("S")?)),
        ComponentKind::Relay => Some((by_label("COIL+")?, by_label("COIL-")?)),
        _ => Some((*pin_nodes.first()?, *pin_nodes.get(1)?)),
    }
}

pub(crate) fn collect_controller_i2c_nodes(
    components: &[Component],
    nodes: &CircuitNodes,
) -> (Vec<usize>, Vec<usize>) {
    let mut sda = Vec::new();
    let mut scl = Vec::new();
    for component in components {
        if !matches!(
            component.kind,
            ComponentKind::Esp32
                | ComponentKind::Esp32S3
                | ComponentKind::Esp32C3
                | ComponentKind::ArduinoUno
                | ComponentKind::RaspberryPiPico
        ) {
            continue;
        }
        for pin in component_pin_defs(component) {
            if pin.role != PinRole::I2c {
                continue;
            }
            let Some(node) = nodes.find_existing(pin.pos) else {
                continue;
            };
            let label = pin.label.to_lowercase();
            if label.contains("sda") {
                sda.push(node);
            } else if label.contains("scl") {
                scl.push(node);
            }
        }
    }
    (sda, scl)
}

pub(crate) fn validate_i2c_links(
    components: &[Component],
    nodes: &CircuitNodes,
    wire_graph: &[HashSet<usize>],
) -> Vec<String> {
    let (ctrl_sda, ctrl_scl) = collect_controller_i2c_nodes(components, nodes);
    if ctrl_sda.is_empty() && ctrl_scl.is_empty() {
        return Vec::new();
    }

    let mut details = Vec::new();
    for component in components {
        if !matches!(component.kind, ComponentKind::Oled | ComponentKind::Sensor) {
            continue;
        }
        let mut sda_ok = false;
        let mut scl_ok = false;
        for pin in component_pin_defs(component) {
            let Some(node) = nodes.find_existing(pin.pos) else {
                continue;
            };
            let label = pin.label.to_lowercase();
            if label.contains("sda") {
                sda_ok = ctrl_sda
                    .iter()
                    .any(|&ctrl| nodes_connected(wire_graph, node, ctrl));
            }
            if label.contains("scl") {
                scl_ok = ctrl_scl
                    .iter()
                    .any(|&ctrl| nodes_connected(wire_graph, node, ctrl));
            }
        }
        if sda_ok && scl_ok {
            details.push(format!("{} I2C OK.", component.label));
        } else {
            details.push(format!(
                "{} I2C incomplete: SDA {}, SCL {}.",
                component.label,
                if sda_ok { "ok" } else { "missing" },
                if scl_ok { "ok" } else { "missing" }
            ));
        }
    }
    details
}

pub(crate) fn estimate_loop_voltage(
    components: &[Component],
    nodes: &CircuitNodes,
    loop_nodes: &HashSet<usize>,
) -> Option<f32> {
    components
        .iter()
        .filter(|component| {
            matches!(
                component.kind,
                ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource
            )
        })
        .filter(|component| {
            component_pin_defs(component)
                .iter()
                .filter_map(|pin| nodes.find_existing(pin.pos))
                .any(|node| loop_nodes.contains(&node))
        })
        .filter_map(|component| parse_metric_value(&component.value, "v"))
        .next()
}

pub(crate) fn estimate_loop_resistance(
    components: &[Component],
    energized_loads: &HashSet<u64>,
) -> Option<f32> {
    let resistance = components
        .iter()
        .filter(|component| energized_loads.contains(&component.id))
        .filter_map(|component| match component.kind {
            ComponentKind::Resistor => parse_metric_value(&component.value, "ohm"),
            ComponentKind::Potentiometer => {
                parse_metric_value(&component.value, "ohm").map(|r| r * 0.5)
            }
            ComponentKind::Led | ComponentKind::Diode | ComponentKind::ZenerDiode => Some(220.0),
            ComponentKind::Lamp => Some(60.0),
            ComponentKind::NpnTransistor | ComponentKind::PnpTransistor => Some(100.0),
            ComponentKind::Nmosfet | ComponentKind::Pmosfet => Some(50.0),
            ComponentKind::VoltageReg => Some(10.0),
            ComponentKind::Fuse => Some(1.0),
            ComponentKind::Relay => Some(100.0),
            _ => None,
        })
        .sum::<f32>();
    (resistance > 0.0).then_some(resistance)
}

pub(crate) fn explicit_source_to_ground_wire_short(
    components: &[Component],
    wires: &[Wire],
) -> bool {
    let mut source_positive_pins = Vec::new();
    let mut return_pins = Vec::new();

    for component in components {
        for pin in component_pin_defs(component) {
            if matches!(
                component.kind,
                ComponentKind::Battery | ComponentKind::VSource | ComponentKind::ISource
            ) && pin.role == PinRole::Positive
            {
                source_positive_pins.push(pin.pos);
            }
            if component.kind == ComponentKind::Ground
                || (matches!(
                    component.kind,
                    ComponentKind::Battery | ComponentKind::VSource | ComponentKind::ISource
                ) && pin.role == PinRole::Ground)
            {
                return_pins.push(pin.pos);
            }
        }
    }

    wires.iter().any(|wire| {
        let touches_source = wire_touches_any_pin(wire, &source_positive_pins);
        let touches_return = wire_touches_any_pin(wire, &return_pins);
        touches_source && touches_return
    })
}

pub(crate) fn wire_touches_any_pin(wire: &Wire, pins: &[Pos2]) -> bool {
    pins.iter().any(|pin| {
        wire.points
            .first()
            .is_some_and(|point| point.distance(*pin) <= 5.0)
            || wire
                .points
                .last()
                .is_some_and(|point| point.distance(*pin) <= 5.0)
    })
}

pub(crate) fn has_energized_load_component(
    components: &[Component],
    energized_components: &HashSet<u64>,
) -> bool {
    components.iter().any(|component| {
        energized_components.contains(&component.id)
            && !matches!(
                component.kind,
                ComponentKind::Battery
                    | ComponentKind::VSource
                    | ComponentKind::ISource
                    | ComponentKind::Ground
                    | ComponentKind::TextNote
            )
            && (component_conductance(component) == Conductance::Load
                || component_is_powered_module(component))
    })
}

pub(crate) fn apply_engineering_checks(
    components: &[Component],
    dc: Option<&mna::DcResult>,
    shorted: bool,
    component_warnings: &mut HashMap<u64, String>,
    details: &mut Vec<String>,
) {
    if shorted {
        details.push("Engineering check: fault current path detected; loads are not treated as normally powered.".to_string());
    }

    let Some(dc) = dc else {
        if shorted {
            details
                .push("DC operating point is singular because the source is shorted.".to_string());
        }
        return;
    };

    let mut max_source_current = 0.0_f64;
    for component in components {
        let current = dc
            .branch_current
            .get(&component.id)
            .copied()
            .unwrap_or(0.0)
            .abs();
        let power = dc
            .component_power
            .get(&component.id)
            .copied()
            .unwrap_or(0.0)
            .abs();
        let voltage = dc
            .component_voltage
            .get(&component.id)
            .copied()
            .unwrap_or(0.0);

        if matches!(
            component.kind,
            ComponentKind::Battery | ComponentKind::VSource | ComponentKind::ISource
        ) {
            max_source_current = max_source_current.max(current);
        }

        if let Some(limit) = component_current_limit(component)
            && current > limit
        {
            component_warnings.entry(component.id).or_insert_with(|| {
                format!(
                    "Overcurrent risk: {} through {}, limit about {}.",
                    mna::format_current(current),
                    component.label,
                    mna::format_current(limit)
                )
            });
        }

        if let Some(limit) = component_power_limit(component)
            && power > limit
        {
            component_warnings.entry(component.id).or_insert_with(|| {
                format!(
                    "Overpower risk: {} in {}, limit about {}.",
                    mna::format_power(power),
                    component.label,
                    mna::format_power(limit)
                )
            });
        }

        if component.kind == ComponentKind::Led {
            let current_ma = current * 1000.0;
            if current > 0.0 {
                details.push(format!(
                    "{} LED current: {:.2} mA, Vf {:.2} V.",
                    component.label, current_ma, voltage
                ));
            }
        }
    }

    if max_source_current > 2.0 {
        details.push(format!(
            "Engineering check: source current {} is high for a beginner circuit.",
            mna::format_current(max_source_current)
        ));
    }
}

pub(crate) fn prune_unphysical_energized_components(
    components: &[Component],
    dc: Option<&mna::DcResult>,
    shorted: bool,
    energized_components: &mut HashSet<u64>,
) {
    if shorted {
        energized_components.retain(|id| {
            components
                .iter()
                .find(|component| component.id == *id)
                .is_some_and(|component| {
                    matches!(
                        component.kind,
                        ComponentKind::Battery
                            | ComponentKind::VSource
                            | ComponentKind::ISource
                            | ComponentKind::Ground
                    )
                })
        });
        return;
    }

    let Some(dc) = dc else {
        return;
    };

    energized_components.retain(|id| {
        let Some(component) = components.iter().find(|component| component.id == *id) else {
            return false;
        };
        if component_is_powered_module(component)
            || matches!(
                component.kind,
                ComponentKind::Battery
                    | ComponentKind::VSource
                    | ComponentKind::ISource
                    | ComponentKind::Ground
            )
        {
            return true;
        }
        if !component_has_dc_current_model(component.kind) {
            return true;
        }
        dc.branch_current
            .get(id)
            .is_some_and(|current| current.abs() > 1e-9)
    });
}

pub(crate) fn component_has_dc_current_model(kind: ComponentKind) -> bool {
    matches!(
        kind,
        ComponentKind::Resistor
            | ComponentKind::Potentiometer
            | ComponentKind::Thermistor
            | ComponentKind::Varistor
            | ComponentKind::Fuse
            | ComponentKind::Lamp
            | ComponentKind::Relay
            | ComponentKind::VoltageReg
            | ComponentKind::NpnTransistor
            | ComponentKind::PnpTransistor
            | ComponentKind::Nmosfet
            | ComponentKind::Pmosfet
            | ComponentKind::Diode
            | ComponentKind::Led
            | ComponentKind::ZenerDiode
            | ComponentKind::SchottkyDiode
            | ComponentKind::TvsDiode
            | ComponentKind::Phototransistor
            | ComponentKind::Timer555
            | ComponentKind::Voltmeter
            | ComponentKind::Ammeter
    )
}

pub(crate) fn component_current_limit(component: &Component) -> Option<f64> {
    match component.kind {
        ComponentKind::Led => Some(0.025),
        ComponentKind::Diode | ComponentKind::SchottkyDiode | ComponentKind::ZenerDiode => {
            Some(1.0)
        }
        ComponentKind::Fuse => parse_metric_value(&component.value, "a")
            .map(|value| value as f64)
            .filter(|value| *value > 0.0),
        ComponentKind::DcMotor => Some(2.0),
        ComponentKind::Relay => Some(0.2),
        ComponentKind::Ammeter => Some(10.0),
        _ => None,
    }
}

pub(crate) fn component_power_limit(component: &Component) -> Option<f64> {
    match component.kind {
        ComponentKind::Resistor => Some(0.25),
        ComponentKind::Potentiometer | ComponentKind::Thermistor => Some(0.125),
        ComponentKind::Led => Some(0.08),
        ComponentKind::Diode | ComponentKind::SchottkyDiode | ComponentKind::ZenerDiode => {
            Some(0.5)
        }
        ComponentKind::Fuse => Some(0.5),
        ComponentKind::Lamp => Some(40.0),
        ComponentKind::DcMotor => Some(12.0),
        ComponentKind::Relay => Some(1.0),
        ComponentKind::VoltageReg => Some(1.5),
        _ => None,
    }
}

pub(crate) fn format_resistance(ohms: f32) -> String {
    if ohms >= 1_000_000.0 {
        format!("{:.2} Mohm", ohms / 1_000_000.0)
    } else if ohms >= 1_000.0 {
        format!("{:.2} kohm", ohms / 1_000.0)
    } else {
        format!("{:.1} ohm", ohms)
    }
}

pub(crate) fn canvas_value_label(component: &Component) -> Option<String> {
    let raw = component.value.trim();
    if raw.is_empty() {
        return None;
    }
    let label = match component.kind {
        ComponentKind::Resistor => {
            if let Some(ohms) = parse_metric_value(raw, "ohm") {
                if ohms >= 1_000_000.0 {
                    let m = ohms / 1_000_000.0;
                    if m == m.floor() {
                        format!("{}MΩ", m as u32)
                    } else {
                        format!("{:.2}MΩ", m)
                    }
                } else if ohms >= 1_000.0 {
                    let k = ohms / 1_000.0;
                    if k == k.floor() {
                        format!("{}kΩ", k as u32)
                    } else {
                        format!("{:.1}kΩ", k)
                    }
                } else if ohms == ohms.floor() {
                    format!("{}Ω", ohms as u32)
                } else {
                    format!("{:.1}Ω", ohms)
                }
            } else {
                raw.to_string()
            }
        }
        ComponentKind::Capacitor => {
            if let Some(f) = parse_metric_value(raw, "f") {
                if f >= 1e-3 {
                    format!("{:.1}mF", f * 1e3)
                } else if f >= 1e-6 {
                    format!("{:.0}μF", f * 1e6)
                } else if f >= 1e-9 {
                    format!("{:.0}nF", f * 1e9)
                } else {
                    format!("{:.0}pF", f * 1e12)
                }
            } else {
                raw.to_string()
            }
        }
        ComponentKind::Inductor => {
            if let Some(h) = parse_metric_value(raw, "h") {
                if h >= 1.0 {
                    format!("{:.1}H", h)
                } else if h >= 1e-3 {
                    format!("{:.1}mH", h * 1e3)
                } else {
                    format!("{:.0}μH", h * 1e6)
                }
            } else {
                raw.to_string()
            }
        }
        ComponentKind::VSource | ComponentKind::Battery => {
            if let Some(v) = parse_metric_value(raw, "v") {
                if v >= 1.0 {
                    format!("{:.1}V", v)
                } else {
                    format!("{:.0}mV", v * 1e3)
                }
            } else {
                raw.to_string()
            }
        }
        _ => return Some(raw.to_string()),
    };
    Some(label)
}

pub(crate) fn format_current(amps: f32) -> String {
    if amps >= 1.0 {
        format!("{amps:.2} A")
    } else if amps >= 0.001 {
        format!("{:.2} mA", amps * 1_000.0)
    } else {
        format!("{:.2} uA", amps * 1_000_000.0)
    }
}

pub(crate) fn component_conductance(component: &Component) -> Conductance {
    match component.kind {
        ComponentKind::Resistor
        | ComponentKind::Diode
        | ComponentKind::ZenerDiode
        | ComponentKind::Led
        | ComponentKind::Lamp
        | ComponentKind::Fuse => Conductance::Load,
        ComponentKind::Potentiometer => Conductance::Load,
        ComponentKind::NpnTransistor
        | ComponentKind::PnpTransistor
        | ComponentKind::Nmosfet
        | ComponentKind::Pmosfet => Conductance::Load,
        ComponentKind::VoltageReg => Conductance::Load,
        ComponentKind::LogicNot
        | ComponentKind::LogicAnd
        | ComponentKind::LogicOr
        | ComponentKind::LogicNand
        | ComponentKind::LogicNor
        | ComponentKind::LogicXor => Conductance::Open,
        ComponentKind::Inductor => Conductance::Conductor,
        ComponentKind::Switch | ComponentKind::PushButton | ComponentKind::SlideSwitch => {
            let value = component.value.to_lowercase();
            if value.contains("open") || value.contains("off") {
                Conductance::Open
            } else {
                Conductance::Conductor
            }
        }
        ComponentKind::DcMotor | ComponentKind::Relay => Conductance::Load,
        ComponentKind::Capacitor | ComponentKind::OpAmp | ComponentKind::Breadboard => {
            Conductance::Open
        }
        ComponentKind::Esp32
        | ComponentKind::Esp32S3
        | ComponentKind::Esp32C3
        | ComponentKind::ArduinoUno
        | ComponentKind::RaspberryPiPico
        | ComponentKind::Stm32BluePill
        | ComponentKind::Stm32Nucleo64
        | ComponentKind::Servo
        | ComponentKind::Oled
        | ComponentKind::Sensor => Conductance::Open,
        ComponentKind::Ground
        | ComponentKind::VSource
        | ComponentKind::ISource
        | ComponentKind::Battery => Conductance::Open,
        ComponentKind::Thermistor
        | ComponentKind::Varistor
        | ComponentKind::SchottkyDiode
        | ComponentKind::TvsDiode
        | ComponentKind::Phototransistor => Conductance::Load,
        ComponentKind::NetLabel
        | ComponentKind::Crystal
        | ComponentKind::Display7Seg
        | ComponentKind::VoltageRef
        | ComponentKind::MotorDriver
        | ComponentKind::Optocoupler
        | ComponentKind::GenericIc => Conductance::Open,
        ComponentKind::Timer555 => Conductance::Load,
        ComponentKind::Transformer => Conductance::Conductor,
        ComponentKind::Voltmeter => Conductance::Open,
        ComponentKind::Ammeter => Conductance::Conductor,
        ComponentKind::TextNote => Conductance::Open,
        ComponentKind::Buzzer => Conductance::Load,
        ComponentKind::Dht11
        | ComponentKind::Dht22
        | ComponentKind::HcSr04
        | ComponentKind::NeoPixel
        | ComponentKind::PirSensor
        | ComponentKind::Custom => Conductance::Open,
    }
}

pub(crate) fn component_is_powered_module(component: &Component) -> bool {
    matches!(
        component.kind,
        ComponentKind::Esp32
            | ComponentKind::Esp32S3
            | ComponentKind::Esp32C3
            | ComponentKind::ArduinoUno
            | ComponentKind::RaspberryPiPico
            | ComponentKind::Servo
            | ComponentKind::Oled
            | ComponentKind::Sensor
    )
}

pub(crate) fn component_is_polarized_diode(kind: ComponentKind) -> bool {
    matches!(
        kind,
        ComponentKind::Diode | ComponentKind::Led | ComponentKind::ZenerDiode
    )
}

pub(crate) fn diode_appears_reversed(
    pins: &[CircuitPin],
    pin_nodes: &[usize],
    wire_from_positive: &HashSet<usize>,
    wire_from_return: &HashSet<usize>,
) -> bool {
    let Some((anode, cathode)) = diode_terminal_nodes(pins, pin_nodes) else {
        return false;
    };
    wire_from_return.contains(&anode) && wire_from_positive.contains(&cathode)
}

pub(crate) fn diode_terminal_nodes(
    pins: &[CircuitPin],
    pin_nodes: &[usize],
) -> Option<(usize, usize)> {
    if pins.len() < 2 || pin_nodes.len() < 2 {
        return None;
    }

    let anode = pins
        .iter()
        .zip(pin_nodes)
        .find(|(pin, _)| pin.label == "A")
        .map(|(_, &node)| node)
        .unwrap_or(pin_nodes[0]);
    let cathode = pins
        .iter()
        .zip(pin_nodes)
        .find(|(pin, _)| pin.label == "K" || pin.label == "B")
        .map(|(_, &node)| node)
        .unwrap_or(pin_nodes[1]);
    Some((anode, cathode))
}

pub(crate) fn component_is_switch(kind: ComponentKind) -> bool {
    matches!(
        kind,
        ComponentKind::Switch | ComponentKind::PushButton | ComponentKind::SlideSwitch
    )
}

pub(crate) fn component_bounds(component: &Component) -> Rect {
    let size = component_size(component);
    let rot = ((component.rotation % 360) + 360) % 360;
    let eff = if rot == 90 || rot == 270 {
        Vec2::new(size.y, size.x)
    } else {
        size
    };
    Rect::from_center_size(component.pos, eff)
}

#[cfg(test)]
pub(crate) fn run_erc(
    components: &[Component],
    wires: &[Wire],
    simulation: &Simulation,
) -> Vec<ErcViolation> {
    let netlist = build_circuit_netlist(components, wires);
    run_erc_with_netlist(components, wires, simulation, &netlist)
}

pub(crate) fn run_erc_with_netlist(
    components: &[Component],
    wires: &[Wire],
    simulation: &Simulation,
    netlist: &CircuitNetlist,
) -> Vec<ErcViolation> {
    let mut v: Vec<ErcViolation> = Vec::new();

    // 1. Unconnected pins: use netlist, not raw coordinates.
    for comp in components {
        // Skip purely decorative / reference components
        if matches!(
            comp.kind,
            ComponentKind::NetLabel | ComponentKind::Breadboard
        ) {
            continue;
        }
        for pin in netlist
            .pins
            .iter()
            .filter(|pin| pin.component_id == comp.id)
        {
            if !pin.connected_by_wire {
                let sev = if matches!(
                    pin.electrical_type,
                    ElectricalType::PowerIn | ElectricalType::Ground
                ) {
                    ErcSeverity::Error
                } else {
                    ErcSeverity::Warning
                };
                v.push(ErcViolation {
                    rule: ErcRule::FloatingConnectivity,
                    severity: sev,
                    component_id: Some(comp.id),
                    wire_id: None,
                    message: format!("{}: pin \"{}\" unconnected", comp.label, pin.pin_name),
                });
            }
        }
    }

    // 2. No ground reference
    let has_ground = components.iter().any(|c| c.kind == ComponentKind::Ground)
        || components.iter().any(|c| {
            matches!(
                c.kind,
                ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource
            ) && component_pin_defs(c)
                .iter()
                .any(|p| p.role == PinRole::Ground)
        });
    if !has_ground && !components.is_empty() {
        v.push(ErcViolation {
            rule: ErcRule::MissingGround,
            severity: ErcSeverity::Error,
            component_id: None,
            wire_id: None,
            message: "No ground (GND) reference in schematic.".to_string(),
        });
    }

    // 3. No voltage/current source
    let has_source = components.iter().any(|c| {
        matches!(
            c.kind,
            ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource
        )
    });
    if !has_source && !components.is_empty() {
        v.push(ErcViolation {
            rule: ErcRule::General,
            severity: ErcSeverity::Warning,
            component_id: None,
            wire_id: None,
            message: "No voltage or current source in schematic.".to_string(),
        });
    }

    // 4. Net-level power conflicts and short circuit
    let net_report = analyze_wire_nets_for_erc(components, wires);
    for conflict in &net_report.power_conflicts {
        v.push(ErcViolation {
            rule: ErcRule::PowerRailConflict,
            severity: ErcSeverity::Error,
            component_id: None,
            wire_id: conflict.wire_id,
            message: conflict.message.clone(),
        });
    }
    if simulation.shorted {
        let already_reported = !net_report.power_conflicts.is_empty();
        if !already_reported {
            v.push(ErcViolation {
                rule: ErcRule::PowerShort,
                severity: ErcSeverity::Error,
                component_id: None,
                wire_id: net_report.first_short_wire,
                message: "Short circuit detected: source + reaches GND without a load.".to_string(),
            });
        }
    }

    for warning in validate_beginner_rules(netlist) {
        v.push(warning);
    }

    // 5. Component polarity warnings from simulation
    for (id, warn) in &simulation.component_warnings {
        if let Some(comp) = components.iter().find(|c| c.id == *id) {
            v.push(ErcViolation {
                rule: ErcRule::General,
                severity: ErcSeverity::Error,
                component_id: Some(*id),
                wire_id: None,
                message: format!("{}: {}", comp.label, warn),
            });
        }
    }

    // 6. Zero-value resistors
    for comp in components {
        if comp.kind == ComponentKind::Resistor {
            if let Some(r) = parse_metric_value(&comp.value, "ohm") {
                if r <= 0.0 {
                    v.push(ErcViolation {
                        rule: ErcRule::MissingValue,
                        severity: ErcSeverity::Warning,
                        component_id: Some(comp.id),
                        wire_id: None,
                        message: format!(
                            "{}: zero or negative resistance value \"{}\"",
                            comp.label, comp.value
                        ),
                    });
                }
            } else {
                v.push(ErcViolation {
                    rule: ErcRule::MissingValue,
                    severity: ErcSeverity::Warning,
                    component_id: Some(comp.id),
                    wire_id: None,
                    message: format!(
                        "{}: cannot parse resistance value \"{}\"",
                        comp.label, comp.value
                    ),
                });
            }
        }
    }

    // 7. Duplicate labels
    let mut labels: HashMap<&str, Vec<u64>> = HashMap::new();
    for comp in components {
        labels.entry(comp.label.as_str()).or_default().push(comp.id);
    }
    for (label, ids) in &labels {
        if ids.len() > 1 {
            v.push(ErcViolation {
                rule: ErcRule::DuplicateReference,
                severity: ErcSeverity::Warning,
                component_id: Some(ids[0]),
                wire_id: None,
                message: format!("Duplicate label \"{label}\" on {} components.", ids.len()),
            });
        }
    }

    // 8. Broken wires
    for wire in wires {
        if wire.points.len() < 2 || wire_length(wire) <= 0.5 {
            v.push(ErcViolation {
                rule: ErcRule::FloatingConnectivity,
                severity: ErcSeverity::Warning,
                component_id: None,
                wire_id: Some(wire.id),
                message: format!("Wire {} has no usable length.", wire.id),
            });
        }
    }

    v
}

#[derive(Default)]
pub(crate) struct ErcNetReport {
    first_short_wire: Option<u64>,
    power_conflicts: Vec<ErcNetConflict>,
}

pub(crate) struct ErcNetConflict {
    wire_id: Option<u64>,
    message: String,
}

pub(crate) fn analyze_wire_nets_for_erc(components: &[Component], wires: &[Wire]) -> ErcNetReport {
    let mut nodes = CircuitNodes::default();
    let mut graph: Vec<HashSet<usize>> = Vec::new();
    let mut wire_nodes: HashMap<u64, Vec<usize>> = HashMap::new();

    for wire in wires {
        let mut used_nodes = Vec::new();
        for point in &wire.points {
            used_nodes.push(nodes.node_for(*point));
        }
        for segment in wire.points.windows(2) {
            let a = nodes.node_for(segment[0]);
            let b = nodes.node_for(segment[1]);
            connect(&mut graph, a, b);
            used_nodes.push(a);
            used_nodes.push(b);
        }
        used_nodes.sort_unstable();
        used_nodes.dedup();
        wire_nodes.insert(wire.id, used_nodes);
    }

    let mut positive_nodes = Vec::new();
    let mut ground_nodes = Vec::new();
    for component in components {
        for pin in component_pin_defs(component) {
            let node = nodes.node_for(pin.pos);
            match pin.role {
                PinRole::Positive => {
                    positive_nodes.push((node, component.label.clone(), pin.label))
                }
                PinRole::Ground => ground_nodes.push((node, component.label.clone(), pin.label)),
                _ => {}
            }
        }
        if component.kind == ComponentKind::Ground {
            for pin in component_pin_defs(component) {
                let node = nodes.node_for(pin.pos);
                ground_nodes.push((node, component.label.clone(), pin.label));
            }
        }
    }
    connect_wire_contacts(&mut nodes, &mut graph, wires, components);

    let positive_reach: Vec<HashSet<usize>> = positive_nodes
        .iter()
        .map(|(node, _, _)| reachable_nodes(&graph, &[*node]))
        .collect();
    let ground_reach: Vec<HashSet<usize>> = ground_nodes
        .iter()
        .map(|(node, _, _)| reachable_nodes(&graph, &[*node]))
        .collect();

    let mut report = ErcNetReport::default();
    let mut seen_conflicts = HashSet::new();
    for (pos_idx, pos_seen) in positive_reach.iter().enumerate() {
        for (gnd_idx, gnd_seen) in ground_reach.iter().enumerate() {
            if !pos_seen.contains(&ground_nodes[gnd_idx].0)
                && !gnd_seen.contains(&positive_nodes[pos_idx].0)
            {
                continue;
            }
            let wire_id = first_wire_touching_either_set(&wire_nodes, pos_seen, gnd_seen);
            report.first_short_wire = report.first_short_wire.or(wire_id);
            let key = (
                positive_nodes[pos_idx].1.clone(),
                positive_nodes[pos_idx].2,
                ground_nodes[gnd_idx].1.clone(),
                ground_nodes[gnd_idx].2,
                wire_id,
            );
            if seen_conflicts.insert(key) {
                report.power_conflicts.push(ErcNetConflict {
                    wire_id,
                    message: format!(
                        "Power net conflict: {} {} is tied to {} {}.",
                        positive_nodes[pos_idx].1,
                        positive_nodes[pos_idx].2,
                        ground_nodes[gnd_idx].1,
                        ground_nodes[gnd_idx].2
                    ),
                });
            }
        }
    }

    report
}

pub(crate) fn first_wire_touching_either_set(
    wire_nodes: &HashMap<u64, Vec<usize>>,
    a: &HashSet<usize>,
    b: &HashSet<usize>,
) -> Option<u64> {
    let mut ordered_wires = wire_nodes.iter().collect::<Vec<_>>();
    ordered_wires.sort_by_key(|&(&id, _)| id);
    ordered_wires
        .iter()
        .find(|(_, nodes)| {
            nodes.iter().any(|node| a.contains(node)) && nodes.iter().any(|node| b.contains(node))
        })
        .map(|&(&id, _)| id)
        .or_else(|| {
            ordered_wires
                .iter()
                .find(|(_, nodes)| {
                    nodes
                        .iter()
                        .any(|node| a.contains(node) || b.contains(node))
                })
                .map(|&(&id, _)| id)
        })
}
