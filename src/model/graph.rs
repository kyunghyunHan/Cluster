//! Explicit connectivity graph for the schematic.
//!
//! The graph replaces fuzzy Pos2 distance matching with typed node IDs.
//! Rules enforced by the builder:
//! - Wires crossing without an explicit junction do **not** connect.
//! - T-junctions (wire endpoint on another wire's interior) **do** connect;
//!   the crossed wire is split at that point into two `WireSegment`s.
//! - Component pins connect only when their position snaps to a graph node.
//! - Netlist generation uses [`NodeId`], not Pos2 proximity.

use egui::Pos2;
use std::collections::HashMap;

// ── Identifiers ───────────────────────────────────────────────────────────────

/// Opaque identifier for a schematic connectivity node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct NodeId(pub(crate) u64);

// ── Graph element types ───────────────────────────────────────────────────────

/// A node in the schematic connectivity graph.
///
/// Created at wire endpoints, T-junction points, explicit junction markers,
/// and component pin positions that touch a wire.
#[derive(Debug, Clone)]
pub(crate) struct SchematicNode {
    pub(crate) id: NodeId,
    pub(crate) position: Pos2,
}

/// A junction in the schematic.
///
/// * `explicit = true` — the user placed a junction dot at a wire crossing.
/// * `explicit = false` — a T-junction was auto-detected (wire endpoint on
///   another wire's interior); this is NOT a crossing without a junction.
#[derive(Debug, Clone)]
pub(crate) struct Junction {
    pub(crate) node_id: NodeId,
    pub(crate) position: Pos2,
    /// Whether the junction was explicitly placed by the user.
    pub(crate) explicit: bool,
}

/// A wire segment between exactly two schematic nodes.
///
/// A single `Wire` polyline may be split into multiple `WireSegment`s when
/// another wire's endpoint or a component pin touches its interior (T-junction).
#[derive(Debug, Clone)]
pub(crate) struct WireSegment {
    pub(crate) id: u64,
    pub(crate) from_node: NodeId,
    pub(crate) to_node: NodeId,
    /// Full routing polyline including both endpoint positions.
    pub(crate) points: Vec<Pos2>,
    /// ID of the original `Wire` this segment was split from.
    pub(crate) source_wire_id: u64,
}

/// An explicit connection between a component pin and a schematic node.
///
/// This is the only way a component pin can be part of a net; fuzzy position
/// matching at build time creates the connection, but the result is stored
/// as a typed edge in the graph.
#[derive(Debug, Clone)]
pub(crate) struct PinConnection {
    pub(crate) component_id: u64,
    pub(crate) pin_name: String,
    pub(crate) node_id: NodeId,
}

/// A fully resolved electrical net: a set of nodes, pin connections, and wire
/// segments that are all electrically equivalent.
#[derive(Debug, Clone)]
pub(crate) struct SchematicNet {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) nodes: Vec<NodeId>,
    pub(crate) pins: Vec<PinConnection>,
    /// IDs of `WireSegment`s that belong to this net.
    pub(crate) segments: Vec<u64>,
}

/// What element forms a simulation branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BranchKind {
    /// A two-terminal component (resistor, LED, battery, …).
    Component(u64),
    /// A wire segment used as a probe branch (ammeter shunt, etc.).
    WireSegment(u64),
}

/// A branch in the circuit: a two-terminal element between two distinct nodes.
///
/// Current flow is well-defined per branch. Wire segments that are part of
/// a multi-branch net junction do **not** form branches; only component
/// elements and series wires are modelled as branches.
#[derive(Debug, Clone)]
pub(crate) struct Branch {
    pub(crate) id: u64,
    pub(crate) kind: BranchKind,
    pub(crate) from_node: NodeId,
    pub(crate) to_node: NodeId,
}

/// The complete explicit connectivity graph derived from a schematic page.
#[derive(Debug, Clone, Default)]
pub(crate) struct SchematicGraph {
    pub(crate) nodes: Vec<SchematicNode>,
    pub(crate) junctions: Vec<Junction>,
    pub(crate) segments: Vec<WireSegment>,
    pub(crate) nets: Vec<SchematicNet>,
    pub(crate) pin_connections: Vec<PinConnection>,
    pub(crate) branches: Vec<Branch>,
}

impl SchematicGraph {
    pub(crate) fn node_by_id(&self, id: NodeId) -> Option<&SchematicNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub(crate) fn net_for_node(&self, id: NodeId) -> Option<&SchematicNet> {
        self.nets.iter().find(|net| net.nodes.contains(&id))
    }

    pub(crate) fn net_for_segment(&self, segment_id: u64) -> Option<&SchematicNet> {
        self.nets.iter().find(|net| net.segments.contains(&segment_id))
    }

    /// Returns the net that the given original `Wire` (by ID) belongs to.
    pub(crate) fn net_for_wire(&self, wire_id: u64) -> Option<&SchematicNet> {
        let seg_id = self
            .segments
            .iter()
            .find(|s| s.source_wire_id == wire_id)
            .map(|s| s.id)?;
        self.net_for_segment(seg_id)
    }

    /// True if two nodes belong to the same net.
    pub(crate) fn same_net(&self, a: NodeId, b: NodeId) -> bool {
        self.nets
            .iter()
            .any(|net| net.nodes.contains(&a) && net.nodes.contains(&b))
    }

    /// Segments whose current is well-defined: the segment is not at a
    /// multi-branch junction (degree-1 nodes on both ends within the graph).
    pub(crate) fn series_segments(&self) -> impl Iterator<Item = &WireSegment> {
        self.segments.iter().filter(|seg| {
            let from_degree = self
                .segments
                .iter()
                .filter(|s| s.from_node == seg.from_node || s.to_node == seg.from_node)
                .count();
            let to_degree = self
                .segments
                .iter()
                .filter(|s| s.from_node == seg.to_node || s.to_node == seg.to_node)
                .count();
            from_degree == 1 && to_degree == 1
        })
    }
}

// ── Private helpers for the builder ──────────────────────────────────────────

fn find_node_id_in(nodes: &[SchematicNode], pos: Pos2) -> Option<NodeId> {
    nodes
        .iter()
        .find(|n| n.position.distance(pos) <= 1.0)
        .map(|n| n.id)
}

fn find_node_idx_in(nodes: &[SchematicNode], pos: Pos2) -> Option<usize> {
    nodes.iter().position(|n| n.position.distance(pos) <= 1.0)
}

fn node_idx_by_id(nodes: &[SchematicNode], id: NodeId) -> Option<usize> {
    nodes.iter().position(|n| n.id == id)
}

/// Arc-length parameter of `pos` along a polyline.  Returns `None` if `pos`
/// does not lie within `snap` units of any segment.
fn polyline_param(points: &[Pos2], pos: Pos2, snap: f32) -> Option<f32> {
    let mut cum = 0.0f32;
    for seg in points.windows(2) {
        let (a, b) = (seg[0], seg[1]);
        let ab = b - a;
        let len_sq = ab.length_sq();
        if len_sq < 1e-12 {
            continue;
        }
        let t = (ab.dot(pos - a) / len_sq).clamp(0.0, 1.0);
        let seg_len = len_sq.sqrt();
        if (a + ab * t).distance(pos) <= snap {
            return Some(cum + t * seg_len);
        }
        cum += seg_len;
    }
    None
}

struct Uf {
    parent: Vec<usize>,
}

impl Uf {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, i: usize) -> usize {
        let mut root = i;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        let mut cur = i;
        while cur != root {
            let next = self.parent[cur];
            self.parent[cur] = root;
            cur = next;
        }
        root
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent[rb] = ra;
        }
    }
}

fn add_unique_pos(list: &mut Vec<Pos2>, pos: Pos2) {
    if !list.iter().any(|p: &Pos2| p.distance(pos) <= 1.0) {
        list.push(pos);
    }
}

// ── Public builder ────────────────────────────────────────────────────────────

/// Build a [`SchematicGraph`] from raw schematic elements.
///
/// ## Connectivity rules
/// - Wire endpoints snap to nodes with ≤ 1 px tolerance.
/// - Wires that **cross** without an explicit junction at the crossing
///   remain on separate nets.
/// - A **T-junction** (wire endpoint lands on another wire's interior)
///   creates a node on the crossed wire and splits it into two segments.
/// - Component pins connect only via [`PinConnection`], not by proximity to
///   a wire's interior — the pin must already land on a node position.
pub(crate) fn build_schematic_graph(
    components: &[crate::model::Component],
    wires: &[crate::model::Wire],
    explicit_junctions: &[Pos2],
) -> SchematicGraph {
    use crate::model::ComponentKind;
    use crate::model::{component_pin_defs, point_touches_wire_segment};

    const SNAP: f32 = 1.0;

    // ── 1. Collect all positions that must become nodes ───────────────────────
    let mut pos_list: Vec<Pos2> = Vec::new();

    for wire in wires {
        if let Some(&p) = wire.points.first() {
            add_unique_pos(&mut pos_list, p);
        }
        if let Some(&p) = wire.points.last() {
            add_unique_pos(&mut pos_list, p);
        }
    }
    for &pos in explicit_junctions {
        add_unique_pos(&mut pos_list, pos);
    }
    for comp in components {
        for pin in component_pin_defs(comp) {
            add_unique_pos(&mut pos_list, pin.pos);
        }
    }

    // T-junction detection: any existing node position that lies on a wire's
    // interior also becomes a node on that wire (splitting it).
    // Repeat to fixpoint (usually 1 pass).
    loop {
        let before = pos_list.len();
        let snapshot = pos_list.clone();
        for &contact in &snapshot {
            for wire in wires {
                for seg in wire.points.windows(2) {
                    let (a, b) = (seg[0], seg[1]);
                    if contact.distance(a) > SNAP
                        && contact.distance(b) > SNAP
                        && point_touches_wire_segment(contact, a, b)
                    {
                        add_unique_pos(&mut pos_list, contact);
                    }
                }
            }
        }
        if pos_list.len() == before {
            break;
        }
    }

    // ── 2. Build SchematicNode objects ────────────────────────────────────────
    let mut next_node_id = 1u64;
    let nodes: Vec<SchematicNode> = pos_list
        .iter()
        .map(|&position| {
            let id = NodeId(next_node_id);
            next_node_id += 1;
            SchematicNode { id, position }
        })
        .collect();

    // ── 3. Detect junctions ───────────────────────────────────────────────────
    let mut junctions: Vec<Junction> = Vec::new();

    for &pos in explicit_junctions {
        if let Some(nid) = find_node_id_in(&nodes, pos) {
            if !junctions.iter().any(|j: &Junction| j.node_id == nid) {
                junctions.push(Junction {
                    node_id: nid,
                    position: pos,
                    explicit: true,
                });
            }
        }
    }

    for wire in wires {
        let endpoints: Vec<Pos2> = [wire.points.first(), wire.points.last()]
            .into_iter()
            .flatten()
            .copied()
            .collect();
        for ep in endpoints {
            for other in wires {
                if std::ptr::eq(wire as *const _, other as *const _) {
                    continue;
                }
                for seg in other.points.windows(2) {
                    let (a, b) = (seg[0], seg[1]);
                    if ep.distance(a) > SNAP
                        && ep.distance(b) > SNAP
                        && point_touches_wire_segment(ep, a, b)
                    {
                        if let Some(nid) = find_node_id_in(&nodes, ep) {
                            if !junctions.iter().any(|j: &Junction| j.node_id == nid) {
                                junctions.push(Junction {
                                    node_id: nid,
                                    position: ep,
                                    explicit: false,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // ── 4. Build WireSegments ─────────────────────────────────────────────────
    let mut next_seg_id = 1u64;
    let mut segments: Vec<WireSegment> = Vec::new();

    for wire in wires {
        if wire.points.len() < 2 {
            continue;
        }

        // Cumulative arc-length parameter for each polyline point.
        let mut cum: Vec<f32> = vec![0.0; wire.points.len()];
        for i in 1..wire.points.len() {
            cum[i] = cum[i - 1] + wire.points[i - 1].distance(wire.points[i]);
        }

        // Find all nodes that lie on this wire.
        let mut on_wire: Vec<(f32, NodeId)> = nodes
            .iter()
            .filter_map(|n| {
                polyline_param(&wire.points, n.position, SNAP).map(|t| (t, n.id))
            })
            .collect();
        on_wire.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        on_wire.dedup_by_key(|(_, id)| *id);

        // Emit a WireSegment for each consecutive pair of nodes.
        for window in on_wire.windows(2) {
            let (t_from, from_id) = window[0];
            let (t_to, to_id) = window[1];
            if from_id == to_id {
                continue;
            }

            let mut pts: Vec<Pos2> = Vec::new();
            if let Some(n) = nodes.iter().find(|n| n.id == from_id) {
                pts.push(n.position);
            }
            for (i, &pt) in wire.points.iter().enumerate() {
                let t_pt = cum[i];
                if t_pt > t_from + SNAP * 0.5 && t_pt < t_to - SNAP * 0.5 {
                    pts.push(pt);
                }
            }
            if let Some(n) = nodes.iter().find(|n| n.id == to_id) {
                pts.push(n.position);
            }
            if pts.len() < 2 {
                continue;
            }

            segments.push(WireSegment {
                id: next_seg_id,
                from_node: from_id,
                to_node: to_id,
                points: pts,
                source_wire_id: wire.id,
            });
            next_seg_id += 1;
        }
    }

    // ── 5. Build PinConnections ───────────────────────────────────────────────
    let mut pin_connections: Vec<PinConnection> = Vec::new();
    for comp in components {
        for pin in component_pin_defs(comp) {
            if let Some(nid) = find_node_id_in(&nodes, pin.pos) {
                pin_connections.push(PinConnection {
                    component_id: comp.id,
                    pin_name: pin.label.to_string(),
                    node_id: nid,
                });
            }
        }
    }

    // ── 6. Union-find → nets ──────────────────────────────────────────────────
    let n_nodes = nodes.len();
    let mut uf = Uf::new(n_nodes);

    for seg in &segments {
        if let (Some(ai), Some(bi)) = (
            node_idx_by_id(&nodes, seg.from_node),
            node_idx_by_id(&nodes, seg.to_node),
        ) {
            uf.union(ai, bi);
        }
    }

    // Merge Ground / NetLabel nodes.
    let mut label_groups: HashMap<String, Vec<usize>> = HashMap::new();
    for comp in components {
        let key = match comp.kind {
            ComponentKind::Ground => Some("gnd".to_string()),
            ComponentKind::NetLabel => {
                let v = comp.value.trim().to_ascii_lowercase();
                if v.is_empty() { None } else { Some(v) }
            }
            _ => None,
        };
        if let Some(k) = key {
            for pin in component_pin_defs(comp) {
                if let Some(idx) = find_node_idx_in(&nodes, pin.pos) {
                    label_groups.entry(k.clone()).or_default().push(idx);
                }
            }
        }
    }
    for idxs in label_groups.values() {
        for pair in idxs.windows(2) {
            uf.union(pair[0], pair[1]);
        }
    }

    // Group node indices by UF root.
    let mut root_to_node_indices: std::collections::BTreeMap<usize, Vec<usize>> =
        std::collections::BTreeMap::new();
    for i in 0..n_nodes {
        root_to_node_indices.entry(uf.find(i)).or_default().push(i);
    }

    // Net name hints.
    let mut root_to_name: HashMap<usize, String> = HashMap::new();
    for comp in components {
        let hint: Option<String> = match comp.kind {
            ComponentKind::Ground => Some("GND".to_string()),
            ComponentKind::NetLabel => {
                let v = comp.value.trim().to_string();
                if v.is_empty() { None } else { Some(v) }
            }
            _ => None,
        };
        let Some(hint_name) = hint else { continue };
        for pin in component_pin_defs(comp) {
            if let Some(idx) = find_node_idx_in(&nodes, pin.pos) {
                let root = uf.find(idx);
                root_to_name.entry(root).or_insert_with(|| hint_name.clone());
            }
        }
    }

    let mut net_id_counter = 0u64;
    let mut gen_name_counter = 1u32;
    let mut nets: Vec<SchematicNet> = Vec::new();
    let mut root_to_net_id: HashMap<usize, u64> = HashMap::new();

    for (&root, node_indices) in &root_to_node_indices {
        let net_id = net_id_counter;
        net_id_counter += 1;
        root_to_net_id.insert(root, net_id);

        let name = root_to_name.get(&root).cloned().unwrap_or_else(|| {
            let n = format!("NET_{gen_name_counter:03}");
            gen_name_counter += 1;
            n
        });

        let net_nodes: Vec<NodeId> = node_indices.iter().map(|&i| nodes[i].id).collect();
        let net_pins: Vec<PinConnection> = pin_connections
            .iter()
            .filter(|pc| net_nodes.contains(&pc.node_id))
            .cloned()
            .collect();
        let net_segments: Vec<u64> = segments
            .iter()
            .filter(|s| net_nodes.contains(&s.from_node) || net_nodes.contains(&s.to_node))
            .map(|s| s.id)
            .collect();

        nets.push(SchematicNet {
            id: net_id,
            name,
            nodes: net_nodes,
            pins: net_pins,
            segments: net_segments,
        });
    }

    // ── 7. Build Branches ─────────────────────────────────────────────────────
    let mut branches: Vec<Branch> = Vec::new();
    let mut branch_counter = 1u64;

    for comp in components {
        let pins = component_pin_defs(comp);
        if pins.len() < 2 {
            continue;
        }
        let from_node = pins.first().and_then(|p| find_node_id_in(&nodes, p.pos));
        let to_node = pins.get(1).and_then(|p| find_node_id_in(&nodes, p.pos));
        if let (Some(a), Some(b)) = (from_node, to_node) {
            if a != b {
                branches.push(Branch {
                    id: branch_counter,
                    kind: BranchKind::Component(comp.id),
                    from_node: a,
                    to_node: b,
                });
                branch_counter += 1;
            }
        }
    }

    SchematicGraph {
        nodes,
        junctions,
        segments,
        nets,
        pin_connections,
        branches,
    }
}
