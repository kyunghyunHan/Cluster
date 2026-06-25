//! Modified Nodal Analysis – DC operating-point solver.
#![allow(dead_code)]
//!
//! Builds the MNA matrix from the schematic, solves with Gaussian elimination
//! (partial pivoting), and returns node voltages + branch currents.
//! Diodes and MOSFETs use a two-stage operating-point classification before
//! the final linear solve; BJTs use a linearised educational companion model.

use egui::Pos2;
use std::collections::{HashMap, HashSet};

use crate::{
    CircuitNodes, Component, ComponentKind, PinRole, UnionFind, Wire, component_pin_defs,
    parse_metric_value, point_touches_wire_segment, wire_contact_points,
};

// ─────────────────────────────────────────────────────────────────────────────
//  Public result type
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone, Debug)]
pub struct DcResult {
    /// net_root → voltage in Volts (GND net = 0.0 V)
    pub net_voltages: HashMap<usize, f64>,
    /// component_id → voltage across the component (V_a − V_b)
    pub component_voltage: HashMap<u64, f64>,
    /// component_id → conventional current through the component (A)
    pub branch_current: HashMap<u64, f64>,
    /// component_id → power dissipated (W, always ≥ 0)
    pub component_power: HashMap<u64, f64>,
    /// wire_id → representative voltage on that wire (V)
    pub wire_voltage: HashMap<u64, f64>,
    /// wire_id → conventional current along the stored wire point order (A)
    pub wire_current: HashMap<u64, f64>,
    /// Wires whose single displayed current is valid across the whole polyline.
    pub wire_current_known: HashSet<u64>,
    /// component_id → whether the component dissipates or supplies power
    pub component_power_role: HashMap<u64, ComponentPowerRole>,
    /// Maximum absolute KCL residual across solved non-ground nodes (A)
    pub max_kcl_residual: f64,
    /// Maximum absolute voltage seen anywhere in the circuit (for display scaling)
    pub vmax: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentPowerRole {
    Dissipating,
    Supplying,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimulationError {
    NoGround,
    SingularMatrix,
    FloatingNode,
    VoltageSourceConflict,
    VoltageSourceLoop,
    ShortCircuit,
    UnsupportedComponent,
}

impl std::fmt::Display for SimulationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            SimulationError::NoGround => "No GND reference",
            SimulationError::SingularMatrix => "Singular circuit matrix",
            SimulationError::FloatingNode => "Floating node or empty DC network",
            SimulationError::VoltageSourceConflict => "Conflicting ideal voltage sources",
            SimulationError::VoltageSourceLoop => "Ideal voltage source loop",
            SimulationError::ShortCircuit => "Ideal source short circuit",
            SimulationError::UnsupportedComponent => "Unsupported component model",
        };
        f.write_str(message)
    }
}

impl SimulationError {
    pub(crate) fn beginner_explanation(&self) -> &'static str {
        match self {
            SimulationError::NoGround => {
                "Add exactly one clear GND/reference path before solving voltages."
            }
            SimulationError::SingularMatrix => {
                "The DC equations cannot be solved, usually because a node is floating or only ideal parts constrain it."
            }
            SimulationError::FloatingNode => {
                "At least one voltage island has no DC path back to GND."
            }
            SimulationError::VoltageSourceConflict => {
                "Two ideal voltage sources force incompatible voltages on the same nodes."
            }
            SimulationError::VoltageSourceLoop => {
                "A loop of ideal voltage sources has no resistance, so current is undefined."
            }
            SimulationError::ShortCircuit => {
                "A source is effectively connected across a near-zero resistance path."
            }
            SimulationError::UnsupportedComponent => {
                "One or more parts need a SPICE model or are only checked by ERC in Cluster."
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  SI / SPICE value parser
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a value string with optional SI suffix and unit into a plain f64.
///
/// Examples: `"10k"` → 10 000.0,  `"100nF"` → 100e-9,  `"3.3V"` → 3.3,
///           `"10mA"` → 0.01,  `"1Meg"` → 1 000 000.0
pub fn parse_si_value(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // Strip trailing unit letters that carry no scale information.
    let s = strip_unit(s);
    if s.is_empty() {
        return None;
    }
    // Locate the boundary between the numeric part and the SI prefix.
    let num_end = numeric_end(s);
    if num_end == 0 {
        return None;
    }
    let base: f64 = s[..num_end].parse().ok()?;
    let sfx = s[num_end..].trim().to_lowercase();
    let mult: f64 = match sfx.as_str() {
        "t" => 1e12,
        "g" => 1e9,
        "meg" | "mega" => 1e6,
        "k" => 1e3,
        "" => 1.0,
        "m" => 1e-3,
        "u" | "µ" | "μ" => 1e-6,
        "n" => 1e-9,
        "p" => 1e-12,
        "f" => 1e-15,
        _ => return None,
    };
    Some(base * mult)
}

fn numeric_end(s: &str) -> usize {
    let mut end = 0usize;
    let mut dot = false;
    let mut exp = false;
    for (i, c) in s.char_indices() {
        if c.is_ascii_digit() {
            end = i + 1;
        } else if c == '.' && !dot {
            dot = true;
            end = i + 1;
        } else if (c == 'e' || c == 'E') && end > 0 && !exp {
            exp = true;
            end = i + 1;
        } else if (c == '+' || c == '-') && exp && end == i {
            end = i + 1;
        } else {
            break;
        }
    }
    end
}

fn strip_unit(s: &str) -> &str {
    if let Some(stripped) = s.strip_suffix('Ω') {
        return stripped.trim_end();
    }
    let up = s.to_uppercase();
    for unit in &["OHMS", "OHM", "HZ", "VAC", "VDC", "AC", "DC"] {
        if up.ends_with(unit) && s.len() > unit.len() {
            return s[..s.len() - unit.len()].trim_end();
        }
    }
    if let Some(last) = s.chars().last() {
        if matches!(last.to_ascii_uppercase(), 'V' | 'A' | 'W' | 'F' | 'H') {
            let cut = s[..s.len() - last.len_utf8()].trim_end();
            // Don't strip if remaining part ends with 'e'/'E' (sci-notation).
            if !cut.is_empty() && !cut.ends_with(['e', 'E']) {
                return cut;
            }
        }
    }
    s
}

// ─────────────────────────────────────────────────────────────────────────────
//  MNA matrix
// ─────────────────────────────────────────────────────────────────────────────

struct Mna {
    /// non-GND node count N
    n: usize,
    /// voltage-source count M
    m: usize,
    /// (N+M) × (N+M) system matrix
    a: Vec<Vec<f64>>,
    /// RHS vector length N+M
    z: Vec<f64>,
}

impl Mna {
    fn new(n: usize, m: usize) -> Self {
        let sz = n + m;
        Mna {
            n,
            m,
            a: vec![vec![0.0; sz]; sz],
            z: vec![0.0; sz],
        }
    }

    /// Stamp a resistor with resistance `r` (Ω) between MNA nodes `a` and `b`.
    /// Node 0 = GND (reference); use 0 for ground-connected terminals.
    fn stamp_r(&mut self, a: usize, b: usize, r: f64) {
        if r < 1e-18 {
            return;
        }
        let g = 1.0 / r;
        if a > 0 {
            self.a[a - 1][a - 1] += g;
        }
        if b > 0 {
            self.a[b - 1][b - 1] += g;
        }
        if a > 0 && b > 0 {
            self.a[a - 1][b - 1] -= g;
            self.a[b - 1][a - 1] -= g;
        }
    }

    /// Stamp an ideal voltage source  V_pos − V_neg = `v`.
    /// `k` is the 0-based voltage-source index.
    fn stamp_vs(&mut self, k: usize, pos: usize, neg: usize, v: f64) {
        let ki = self.n + k;
        if pos > 0 {
            self.a[ki][pos - 1] += 1.0;
            self.a[pos - 1][ki] += 1.0;
        }
        if neg > 0 {
            self.a[ki][neg - 1] -= 1.0;
            self.a[neg - 1][ki] -= 1.0;
        }
        self.z[ki] += v;
    }

    /// Stamp an independent current source flowing from `neg` into `pos`.
    fn stamp_is(&mut self, pos: usize, neg: usize, i: f64) {
        if pos > 0 {
            self.z[pos - 1] += i;
        }
        if neg > 0 {
            self.z[neg - 1] -= i;
        }
    }

    /// Stamp a current-controlled current source (CCCS).
    /// Output current = `gain` × I_k (branch current of VS index `k_vs`).
    /// Conventional output current flows INTO `pos` and OUT OF `neg`.
    fn stamp_cccs(&mut self, pos: usize, neg: usize, k_vs: usize, gain: f64) {
        let ki = self.n + k_vs;
        if pos > 0 {
            self.a[pos - 1][ki] += gain;
        }
        if neg > 0 {
            self.a[neg - 1][ki] -= gain;
        }
    }

    /// Stamp a voltage-controlled current source (VCCS / transconductance).
    /// Output current = `gm` × (V_ctrl_p − V_ctrl_n), flows INTO `pos`, OUT OF `neg`.
    fn stamp_vccs(&mut self, pos: usize, neg: usize, ctrl_p: usize, ctrl_n: usize, gm: f64) {
        if pos > 0 {
            if ctrl_p > 0 {
                self.a[pos - 1][ctrl_p - 1] += gm;
            }
            if ctrl_n > 0 {
                self.a[pos - 1][ctrl_n - 1] -= gm;
            }
        }
        if neg > 0 {
            if ctrl_p > 0 {
                self.a[neg - 1][ctrl_p - 1] -= gm;
            }
            if ctrl_n > 0 {
                self.a[neg - 1][ctrl_n - 1] += gm;
            }
        }
    }

    /// Solve A·x = z with Gaussian elimination + partial pivoting.
    fn solve(self) -> Result<SolveSolution, SimulationError> {
        let sz = self.n + self.m;
        if sz == 0 {
            return Ok(SolveSolution {
                x: vec![],
                max_kcl_residual: 0.0,
            });
        }
        let original_a = self.a.clone();
        let original_z = self.z.clone();
        let mut a = self.a;
        let mut b = self.z;

        for col in 0..sz {
            // Partial pivot
            let mut prow = col;
            for row in (col + 1)..sz {
                if a[row][col].abs() > a[prow][col].abs() {
                    prow = row;
                }
            }
            if a[prow][col].abs() < 1e-12 {
                return Err(SimulationError::SingularMatrix);
            }
            a.swap(col, prow);
            b.swap(col, prow);

            let piv = a[col][col];
            for row in (col + 1)..sz {
                let f = a[row][col] / piv;
                if f.abs() < 1e-18 {
                    continue;
                }
                b[row] -= f * b[col];
                for j in col..sz {
                    a[row][j] -= f * a[col][j];
                }
            }
        }

        let mut x = vec![0.0; sz];
        for i in (0..sz).rev() {
            x[i] = b[i];
            for j in (i + 1)..sz {
                x[i] -= a[i][j] * x[j];
            }
            if a[i][i].abs() < 1e-12 {
                return Err(SimulationError::SingularMatrix);
            }
            x[i] /= a[i][i];
        }
        let max_kcl_residual = original_a
            .iter()
            .take(self.n)
            .zip(&original_z)
            .map(|(row, rhs)| (row.iter().zip(&x).map(|(a, x)| a * x).sum::<f64>() - rhs).abs())
            .fold(0.0_f64, f64::max);
        Ok(SolveSolution {
            x,
            max_kcl_residual,
        })
    }
}

struct SolveSolution {
    x: Vec<f64>,
    max_kcl_residual: f64,
}

fn validate_voltage_sources(vs: &[VsEntry]) -> Result<(), SimulationError> {
    let mut constraints: HashMap<(usize, usize), f64> = HashMap::new();
    for source in vs {
        if source.pos == source.neg {
            if source.v.abs() > 1.0e-12 {
                return Err(SimulationError::VoltageSourceConflict);
            }
            continue;
        }

        let key = if source.pos < source.neg {
            (source.pos, source.neg)
        } else {
            (source.neg, source.pos)
        };
        let signed_v = if source.pos < source.neg {
            source.v
        } else {
            -source.v
        };
        if let Some(existing) = constraints.get(&key) {
            if (existing - signed_v).abs() <= 1.0e-9 {
                return Err(SimulationError::VoltageSourceLoop);
            }
            return Err(SimulationError::VoltageSourceConflict);
        } else {
            constraints.insert(key, signed_v);
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
//  Net assignment  (position → net_root index)
// ─────────────────────────────────────────────────────────────────────────────

struct NetMap {
    nodes: CircuitNodes,
    uf: UnionFind,
}

impl NetMap {
    fn new() -> Self {
        NetMap {
            nodes: CircuitNodes::default(),
            uf: UnionFind::default(),
        }
    }

    fn reg(&mut self, pos: Pos2) -> usize {
        let idx = self.nodes.node_for(pos);
        self.uf.ensure(idx);
        idx
    }

    fn join(&mut self, a: usize, b: usize) {
        self.uf.union(a, b);
    }

    fn root_of(&mut self, pos: Pos2) -> Option<usize> {
        let idx = self.nodes.find_existing(pos)?;
        Some(self.uf.find(idx))
    }

    fn root_of_idx(&mut self, idx: usize) -> usize {
        self.uf.find(idx)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Component entries used while building the MNA
// ─────────────────────────────────────────────────────────────────────────────

struct ResEntry {
    id: u64,
    a: usize,
    b: usize,
    r: f64,
}

struct VsEntry {
    id: u64,
    pos: usize,
    neg: usize,
    v: f64,
}

struct IsEntry {
    id: u64,
    pos: usize,
    neg: usize,
    i: f64,
}

/// Diode companion model: Vf (voltage source) in series with Rb (bulk resistance).
/// Requires an intermediate virtual node.
struct DiodeEntry {
    id: u64,
    anode: usize,   // MNA node for anode
    cathode: usize, // MNA node for cathode
    vf: f64,        // forward voltage drop (V)
    rb: f64,        // bulk series resistance (Ω)
}

struct MosEntry {
    id: u64,
    gate: usize,
    drain: usize,
    source: usize,
    pmos: bool,
    vth: f64,
    r_on: f64,
    r_off: f64,
}

/// BJT companion model: VBE diode + CCCS for collector current.
struct BjtEntry {
    id: u64,
    b: usize,   // base MNA node
    c: usize,   // collector MNA node
    e: usize,   // emitter MNA node
    vbe: f64,   // base-emitter forward voltage (V)
    rb_be: f64, // base-emitter bulk resistance (Ω)
    h_fe: f64,  // DC current gain
    /// true = NPN (Vbe: B+, E-), false = PNP (Vbe: E+, B-)
    npn: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
//  Main entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Attempt a DC operating-point solve on the circuit.
/// Returns `None` when the circuit has no GND, is open, or the matrix is
/// singular (floating sub-network, etc.).
pub fn solve_dc(components: &[Component], wires: &[Wire]) -> Option<DcResult> {
    solve_dc_detailed(components, wires).ok()
}

pub fn solve_dc_detailed(
    components: &[Component],
    wires: &[Wire],
) -> Result<DcResult, SimulationError> {
    // ── 1. Build net map ─────────────────────────────────────────────────
    let mut nm = NetMap::new();

    for wire in wires {
        let indices: Vec<usize> = wire.points.iter().map(|&p| nm.reg(p)).collect();
        for w in indices.windows(2) {
            nm.join(w[0], w[1]);
        }
    }
    for comp in components {
        for pin in component_pin_defs(comp) {
            nm.reg(pin.pos);
        }
        // Connect pin pairs that are shorted internally (closed switches / inductors)
        let pins = component_pin_defs(comp);
        let shorted = match comp.kind {
            ComponentKind::Inductor => true,
            ComponentKind::Switch | ComponentKind::PushButton | ComponentKind::SlideSwitch => {
                let v = comp.value.to_lowercase();
                !v.contains("open") && !v.contains("off")
            }
            _ => false,
        };
        if shorted && pins.len() >= 2 {
            let ia = nm.nodes.find_existing(pins[0].pos).unwrap_or(0);
            let ib = nm.nodes.find_existing(pins[1].pos).unwrap_or(0);
            nm.uf.ensure(ia);
            nm.uf.ensure(ib);
            nm.join(ia, ib);
        }
    }
    for contact in wire_contact_points(components, wires) {
        let contact_node = nm.reg(contact);
        for wire in wires {
            for segment in wire.points.windows(2) {
                if point_touches_wire_segment(contact, segment[0], segment[1]) {
                    let a = nm.reg(segment[0]);
                    let b = nm.reg(segment[1]);
                    nm.join(contact_node, a);
                    nm.join(contact_node, b);
                }
            }
        }
    }

    // ── 1b. Merge nets connected by NetLabel: same label → same net ──────
    {
        let mut label_to_nodes: HashMap<String, Vec<usize>> = HashMap::new();
        for comp in components {
            if comp.kind == ComponentKind::NetLabel {
                let label = comp.value.trim().to_ascii_lowercase();
                if label.is_empty() {
                    continue;
                }
                for pin in component_pin_defs(comp) {
                    let idx = nm.reg(pin.pos);
                    label_to_nodes.entry(label.clone()).or_default().push(idx);
                }
            }
        }
        for (_, nodes) in &label_to_nodes {
            for w in nodes.windows(2) {
                nm.join(w[0], w[1]);
            }
        }
    }

    // ── 2. Identify GND roots ────────────────────────────────────────────
    let mut gnd_roots: HashSet<usize> = HashSet::new();
    for comp in components {
        let mark_gnd = |pos: Pos2, nm: &mut NetMap, gnd_roots: &mut HashSet<usize>| {
            if let Some(root) = nm.root_of(pos) {
                gnd_roots.insert(root);
            }
        };
        match comp.kind {
            ComponentKind::Ground => {
                for pin in component_pin_defs(comp) {
                    mark_gnd(pin.pos, &mut nm, &mut gnd_roots);
                }
            }
            ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource => {
                for pin in component_pin_defs(comp) {
                    if pin.role == PinRole::Ground {
                        mark_gnd(pin.pos, &mut nm, &mut gnd_roots);
                    }
                }
            }
            _ => {}
        }
    }
    if gnd_roots.is_empty() {
        return Err(SimulationError::NoGround);
    }

    // ── 3. Assign MNA node numbers (GND=0, others 1..N) ─────────────────
    let node_count = nm.nodes.positions.len();
    let all_roots: HashSet<usize> = (0..node_count).map(|i| nm.uf.find(i)).collect();

    let mut mna_of: HashMap<usize, usize> = HashMap::new(); // net_root → MNA node
    let mut next_node = 1usize;
    for &root in &all_roots {
        if gnd_roots.contains(&root) {
            mna_of.insert(root, 0);
        } else {
            mna_of.insert(root, next_node);
            next_node += 1;
        }
    }
    let num_nodes = next_node - 1; // N (non-GND nodes)

    // Helper closure: MNA node for a pin position.
    let mna_node = |pos: Pos2, nm: &mut NetMap, mna_of: &HashMap<usize, usize>| -> Option<usize> {
        let root = nm.root_of(pos)?;
        mna_of.get(&root).copied()
    };

    // ── 4. Build component entries ───────────────────────────────────────
    let mut res: Vec<ResEntry> = Vec::new();
    let mut vs: Vec<VsEntry> = Vec::new();
    let mut is_src: Vec<IsEntry> = Vec::new();
    let mut diode_entries: Vec<DiodeEntry> = Vec::new();
    let mut bjt_entries: Vec<BjtEntry> = Vec::new();
    let mut mos_entries: Vec<MosEntry> = Vec::new();

    for comp in components {
        let pins = component_pin_defs(comp);
        let p0 = pins.get(0).and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
        let p1 = pins.get(1).and_then(|p| mna_node(p.pos, &mut nm, &mna_of));

        match comp.kind {
            // ── Resistive two-terminal ────────────────────────────────
            ComponentKind::Resistor => {
                let r = parse_metric_value(&comp.value, "ohm").unwrap_or(1_000.0) as f64;
                if let (Some(a), Some(b)) = (p0, p1) {
                    res.push(ResEntry {
                        id: comp.id,
                        a,
                        b,
                        r,
                    });
                }
            }
            ComponentKind::Potentiometer => {
                let r = parse_metric_value(&comp.value, "ohm").unwrap_or(10_000.0) as f64 * 0.5;
                if let (Some(a), Some(b)) = (p0, p1) {
                    res.push(ResEntry {
                        id: comp.id,
                        a,
                        b,
                        r,
                    });
                }
            }
            ComponentKind::Fuse => {
                if let (Some(a), Some(b)) = (p0, p1) {
                    res.push(ResEntry {
                        id: comp.id,
                        a,
                        b,
                        r: 0.05,
                    });
                }
            }
            ComponentKind::Lamp => {
                // Model as resistance: P = V²/R, assume rated V from value
                let rated_v = parse_metric_value(&comp.value, "v").unwrap_or(12.0) as f64;
                let r = (rated_v * rated_v) / 40.0; // assume ~40 W filament
                if let (Some(a), Some(b)) = (p0, p1) {
                    res.push(ResEntry {
                        id: comp.id,
                        a,
                        b,
                        r,
                    });
                }
            }
            ComponentKind::DcMotor => {
                // Winding resistance + back-EMF not modelled; use 5 Ω stub
                if let (Some(a), Some(b)) = (p0, p1) {
                    res.push(ResEntry {
                        id: comp.id,
                        a,
                        b,
                        r: 5.0,
                    });
                }
            }
            ComponentKind::Relay => {
                // Coil modelled as 100 Ω
                let coil_p = pins
                    .iter()
                    .find(|p| p.label.contains("COIL+"))
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                let coil_n = pins
                    .iter()
                    .find(|p| p.label.contains("COIL-"))
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                if let (Some(a), Some(b)) = (coil_p, coil_n) {
                    res.push(ResEntry {
                        id: comp.id,
                        a,
                        b,
                        r: 100.0,
                    });
                }
            }
            ComponentKind::VoltageReg => {
                // Very simplified: pass-through with 0.5 Ω drop (ignore regulation)
                let in_n = pins
                    .iter()
                    .find(|p| p.label == "IN")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                let out_n = pins
                    .iter()
                    .find(|p| p.label == "OUT")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                let gnd_n = pins
                    .iter()
                    .find(|p| p.role == PinRole::Ground)
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                if let (Some(a), Some(b)) = (in_n, out_n) {
                    res.push(ResEntry {
                        id: comp.id,
                        a,
                        b,
                        r: 0.5,
                    });
                }
                // GND pin: stamp small resistance to GND
                if let (Some(o), Some(g)) = (out_n, gnd_n) {
                    // current path gnd pin → gnd ref (regulation stub)
                    let _ = (o, g);
                }
            }
            // ── BJT transistors: VBE diode + CCCS for Ic = hFE·Ib ──────────
            ComponentKind::NpnTransistor => {
                let b_n = pins
                    .iter()
                    .find(|p| p.label == "B")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                let c_n = pins
                    .iter()
                    .find(|p| p.label == "C")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                let e_n = pins
                    .iter()
                    .find(|p| p.label == "E")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                if let (Some(b), Some(c), Some(e)) = (b_n, c_n, e_n) {
                    bjt_entries.push(BjtEntry {
                        id: comp.id,
                        b,
                        c,
                        e,
                        vbe: 0.65,
                        rb_be: 10.0,
                        h_fe: 100.0,
                        npn: true,
                    });
                    // Leakage Rce to prevent open-circuit singularity
                    res.push(ResEntry {
                        id: comp.id,
                        a: c,
                        b: e,
                        r: 100_000.0,
                    });
                }
            }
            ComponentKind::PnpTransistor => {
                let b_n = pins
                    .iter()
                    .find(|p| p.label == "B")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                let c_n = pins
                    .iter()
                    .find(|p| p.label == "C")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                let e_n = pins
                    .iter()
                    .find(|p| p.label == "E")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                if let (Some(b), Some(c), Some(e)) = (b_n, c_n, e_n) {
                    bjt_entries.push(BjtEntry {
                        id: comp.id,
                        b,
                        c,
                        e,
                        vbe: 0.65,
                        rb_be: 10.0,
                        h_fe: 100.0,
                        npn: false,
                    });
                    res.push(ResEntry {
                        id: comp.id,
                        a: e,
                        b: c,
                        r: 100_000.0,
                    });
                }
            }
            // ── MOSFETs: VCCS (transconductance) model + leakage Rds ────────
            ComponentKind::Nmosfet => {
                let g_n = pins
                    .iter()
                    .find(|p| p.label == "G")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                let d_n = pins
                    .iter()
                    .find(|p| p.label == "D")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                let s_n = pins
                    .iter()
                    .find(|p| p.label == "S")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                if let (Some(g), Some(d), Some(s)) = (g_n, d_n, s_n) {
                    mos_entries.push(MosEntry {
                        id: comp.id,
                        gate: g,
                        drain: d,
                        source: s,
                        pmos: false,
                        vth: 2.0,
                        r_on: 1.0,
                        r_off: 1.0e9,
                    });
                }
            }
            ComponentKind::Pmosfet => {
                let g_n = pins
                    .iter()
                    .find(|p| p.label == "G")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                let d_n = pins
                    .iter()
                    .find(|p| p.label == "D")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                let s_n = pins
                    .iter()
                    .find(|p| p.label == "S")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                if let (Some(g), Some(d), Some(s)) = (g_n, d_n, s_n) {
                    mos_entries.push(MosEntry {
                        id: comp.id,
                        gate: g,
                        drain: d,
                        source: s,
                        pmos: true,
                        vth: 2.0,
                        r_on: 1.0,
                        r_off: 1.0e9,
                    });
                }
            }
            // ── Voltage sources ───────────────────────────────────────
            ComponentKind::VSource | ComponentKind::Battery => {
                let vv = parse_metric_value(&comp.value, "v").unwrap_or(
                    if comp.kind == ComponentKind::Battery {
                        9.0
                    } else {
                        5.0
                    },
                ) as f64;
                let pos_n = pins
                    .iter()
                    .find(|p| p.role == PinRole::Positive || p.label == "+")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of))
                    .unwrap_or(0);
                let neg_n = pins
                    .iter()
                    .find(|p| p.role == PinRole::Ground || p.label == "-")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of))
                    .unwrap_or(0);
                if pos_n == neg_n {
                    if vv.abs() > 1.0e-12 {
                        return Err(SimulationError::VoltageSourceConflict);
                    }
                } else {
                    vs.push(VsEntry {
                        id: comp.id,
                        pos: pos_n,
                        neg: neg_n,
                        v: vv,
                    });
                }
            }
            ComponentKind::ISource => {
                let iv = parse_metric_value(&comp.value, "a").unwrap_or(0.01) as f64;
                let pos_n = pins
                    .iter()
                    .find(|p| p.role == PinRole::Positive)
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of))
                    .unwrap_or(0);
                let neg_n = pins
                    .iter()
                    .find(|p| p.role == PinRole::Ground)
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of))
                    .unwrap_or(0);
                is_src.push(IsEntry {
                    id: comp.id,
                    pos: pos_n,
                    neg: neg_n,
                    i: iv,
                });
            }
            // ── Diodes (linearised Thevenin companion model) ─────────
            // Strategy: model each diode as an ideal voltage source Vf
            // in series with a small bulk resistance Rb.
            // Series connection requires an intermediate node per diode.
            // We allocate virtual node indices beyond the normal net range.
            ComponentKind::Diode => {
                // Vf ≈ 0.65 V, Rb ≈ 8 Ω  (1N4148 at ~20 mA)
                if let (Some(a), Some(k)) = (p0, p1) {
                    diode_entries.push(DiodeEntry {
                        id: comp.id,
                        anode: a,
                        cathode: k,
                        vf: 0.65,
                        rb: 8.0,
                    });
                }
            }
            ComponentKind::Led => {
                // Vf ≈ 2.0 V, Rb ≈ 20 Ω  (typical red LED)
                if let (Some(a), Some(k)) = (p0, p1) {
                    diode_entries.push(DiodeEntry {
                        id: comp.id,
                        anode: a,
                        cathode: k,
                        vf: 2.0,
                        rb: 20.0,
                    });
                }
            }
            ComponentKind::ZenerDiode => {
                // Reverse-breakdown (typical Zener use): model as reverse voltage source Vz
                // with small bulk resistance. Cathode is the + side in breakdown.
                let vz = parse_metric_value(&comp.value, "v").unwrap_or(5.1) as f64;
                if let (Some(a), Some(k)) = (p0, p1) {
                    // Polarity reversed vs normal diode: cathode(k) → anode(a) = Vz
                    diode_entries.push(DiodeEntry {
                        id: comp.id,
                        anode: k,   // cathode acts as "anode" in reverse model
                        cathode: a, // anode acts as "cathode"
                        vf: vz,
                        rb: 5.0,
                    });
                }
            }
            ComponentKind::Thermistor => {
                let r = parse_metric_value(&comp.value, "ohm").unwrap_or(10_000.0) as f64;
                if let (Some(a), Some(b)) = (p0, p1) {
                    res.push(ResEntry {
                        id: comp.id,
                        a,
                        b,
                        r,
                    });
                }
            }
            ComponentKind::Varistor => {
                if let (Some(a), Some(b)) = (p0, p1) {
                    res.push(ResEntry {
                        id: comp.id,
                        a,
                        b,
                        r: 100.0,
                    });
                }
            }
            ComponentKind::SchottkyDiode => {
                // Vf ≈ 0.3 V, Rb ≈ 4 Ω
                if let (Some(a), Some(k)) = (p0, p1) {
                    diode_entries.push(DiodeEntry {
                        id: comp.id,
                        anode: a,
                        cathode: k,
                        vf: 0.30,
                        rb: 4.0,
                    });
                }
            }
            ComponentKind::TvsDiode => {
                // Treated as open in forward operation; clamp in transient not modelled in DC
                if let (Some(a), Some(b)) = (p0, p1) {
                    res.push(ResEntry {
                        id: comp.id,
                        a,
                        b,
                        r: 1000.0,
                    });
                }
            }
            ComponentKind::Phototransistor => {
                let c_n = pins
                    .iter()
                    .find(|p| p.label == "C")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                let e_n = pins
                    .iter()
                    .find(|p| p.label == "E")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                if let (Some(c), Some(e)) = (c_n, e_n) {
                    res.push(ResEntry {
                        id: comp.id,
                        a: c,
                        b: e,
                        r: 500.0,
                    });
                }
            }
            // ── Capacitor: open in DC; Inductor: short (handled above) ─
            ComponentKind::Capacitor
            | ComponentKind::Inductor
            | ComponentKind::Switch
            | ComponentKind::PushButton
            | ComponentKind::SlideSwitch
            | ComponentKind::Ground
            | ComponentKind::OpAmp
            | ComponentKind::LogicNot
            | ComponentKind::LogicAnd
            | ComponentKind::LogicOr
            | ComponentKind::LogicNand
            | ComponentKind::LogicNor
            | ComponentKind::LogicXor
            | ComponentKind::Breadboard
            | ComponentKind::Esp32
            | ComponentKind::Esp32S3
            | ComponentKind::Esp32C3
            | ComponentKind::ArduinoUno
            | ComponentKind::RaspberryPiPico
            | ComponentKind::Servo
            | ComponentKind::Oled
            | ComponentKind::Sensor
            | ComponentKind::NetLabel
            | ComponentKind::Crystal
            | ComponentKind::Transformer
            | ComponentKind::Display7Seg
            | ComponentKind::VoltageRef
            | ComponentKind::MotorDriver
            | ComponentKind::Optocoupler
            | ComponentKind::GenericIc
            | ComponentKind::Timer555
            | ComponentKind::TextNote => {}
            // ── Voltmeter: ideal → 1 MΩ probe (barely loads circuit) ────────
            ComponentKind::Voltmeter => {
                if let (Some(a), Some(b)) = (p0, p1) {
                    res.push(ResEntry {
                        id: comp.id,
                        a,
                        b,
                        r: 1_000_000.0,
                    });
                }
            }
            // ── Ammeter: ideal → 1 mΩ series resistor ────────────────────
            ComponentKind::Ammeter => {
                if let (Some(a), Some(b)) = (p0, p1) {
                    res.push(ResEntry {
                        id: comp.id,
                        a,
                        b,
                        r: 0.001,
                    });
                }
            }
            // ── Buzzer: resistive load (typical ~150 Ω) ──────────────────
            ComponentKind::Buzzer => {
                if let (Some(a), Some(b)) = (p0, p1) {
                    res.push(ResEntry {
                        id: comp.id,
                        a,
                        b,
                        r: 150.0,
                    });
                }
            }
            // ── New sensors: not simulated in DC MNA ─────────────────────
            ComponentKind::Dht11
            | ComponentKind::Dht22
            | ComponentKind::HcSr04
            | ComponentKind::NeoPixel
            | ComponentKind::PirSensor => {}
        }
    }

    // ── 5b. Expand BJTs: VBE diode + CCCS for Ic = hFE·Ib ──────────────
    // Each BJT needs one intermediate node (for the VBE diode series resistor).
    let bjt_start_node = num_nodes + 1;
    // Record the index of the first BJT VS entry (used for CCCS stamp below)
    let bjt_vs_start = vs.len();
    for (bi, bjt) in bjt_entries.iter().enumerate() {
        let mid = bjt_start_node + bi;
        if bjt.npn {
            // NPN: Vbe between B(+) and mid(-), then Rb_be from mid to E
            vs.push(VsEntry {
                id: bjt.id,
                pos: bjt.b,
                neg: mid,
                v: bjt.vbe,
            });
            res.push(ResEntry {
                id: bjt.id,
                a: mid,
                b: bjt.e,
                r: bjt.rb_be,
            });
        } else {
            // PNP: Veb between E(+) and mid(-), then Rb_eb from mid to B
            vs.push(VsEntry {
                id: bjt.id,
                pos: bjt.e,
                neg: mid,
                v: bjt.vbe,
            });
            res.push(ResEntry {
                id: bjt.id,
                a: mid,
                b: bjt.b,
                r: bjt.rb_be,
            });
        }
    }
    validate_voltage_sources(&vs)?;
    let total_nodes = num_nodes + bjt_entries.len();

    // ── 6. Build MNA matrix ──────────────────────────────────────────────
    let m = vs.len();
    if total_nodes == 0 {
        return Err(SimulationError::FloatingNode);
    }
    let solve_with_states =
        |diode_on: &[bool], mos_on: &[bool]| -> Result<SolveSolution, SimulationError> {
            let mut mat = Mna::new(total_nodes, m);
            for re in &res {
                mat.stamp_r(re.a, re.b, re.r);
            }
            for (index, diode) in diode_entries.iter().enumerate() {
                if diode_on.get(index).copied().unwrap_or(false) {
                    mat.stamp_r(diode.anode, diode.cathode, diode.rb);
                    mat.stamp_is(diode.anode, diode.cathode, diode.vf / diode.rb);
                } else {
                    mat.stamp_r(diode.anode, diode.cathode, 1.0e9);
                }
            }
            for (index, mos) in mos_entries.iter().enumerate() {
                mat.stamp_r(
                    mos.drain,
                    mos.source,
                    if mos_on.get(index).copied().unwrap_or(false) {
                        mos.r_on
                    } else {
                        mos.r_off
                    },
                );
            }
            for (k, v_src) in vs.iter().enumerate() {
                mat.stamp_vs(k, v_src.pos, v_src.neg, v_src.v);
            }
            for (bi, bjt) in bjt_entries.iter().enumerate() {
                let k_vbe = bjt_vs_start + bi;
                if bjt.npn {
                    mat.stamp_cccs(bjt.c, bjt.e, k_vbe, bjt.h_fe);
                } else {
                    mat.stamp_cccs(bjt.c, bjt.e, k_vbe, -bjt.h_fe);
                }
            }
            for i_src in &is_src {
                mat.stamp_is(i_src.pos, i_src.neg, i_src.i);
            }
            mat.solve()
        };

    let initial_diode_states = vec![false; diode_entries.len()];
    let initial_mos_states = vec![false; mos_entries.len()];
    let initial_solution = solve_with_states(&initial_diode_states, &initial_mos_states)?;
    let initial = &initial_solution.x;
    let initial_voltage = |mna_idx: usize| -> f64 {
        if mna_idx == 0 {
            0.0
        } else {
            initial.get(mna_idx - 1).copied().unwrap_or(0.0)
        }
    };
    let diode_states = diode_entries
        .iter()
        .map(|diode| initial_voltage(diode.anode) - initial_voltage(diode.cathode) >= diode.vf)
        .collect::<Vec<_>>();
    let mos_states = mos_entries
        .iter()
        .map(|mos| {
            if mos.pmos {
                initial_voltage(mos.source) - initial_voltage(mos.gate) >= mos.vth
            } else {
                initial_voltage(mos.gate) - initial_voltage(mos.source) >= mos.vth
            }
        })
        .collect::<Vec<_>>();
    let solution = solve_with_states(&diode_states, &mos_states)?;
    let x = &solution.x;

    // ── 8. Decode results ─────────────────────────────────────────────────
    let vnode = |mna_idx: usize| -> f64 {
        if mna_idx == 0 {
            0.0
        } else {
            x.get(mna_idx - 1).copied().unwrap_or(0.0)
        }
    };

    let mut net_voltages: HashMap<usize, f64> = HashMap::new();
    for (&root, &mna_idx) in &mna_of {
        net_voltages.insert(root, vnode(mna_idx));
    }

    let mut component_voltage: HashMap<u64, f64> = HashMap::new();
    let mut branch_current: HashMap<u64, f64> = HashMap::new();
    let mut component_power: HashMap<u64, f64> = HashMap::new();
    let mut component_power_role: HashMap<u64, ComponentPowerRole> = HashMap::new();

    for re in &res {
        let va = vnode(re.a);
        let vb = vnode(re.b);
        let vd = va - vb;
        let i_r = vd / re.r;
        component_voltage.entry(re.id).or_insert(vd);
        branch_current.entry(re.id).or_insert(i_r);
        component_power.entry(re.id).or_insert((vd * i_r).abs());
        component_power_role
            .entry(re.id)
            .or_insert(ComponentPowerRole::Dissipating);
    }

    for (k, v_src) in vs.iter().enumerate() {
        let i_vs = x.get(total_nodes + k).copied().unwrap_or(0.0);
        let va = vnode(v_src.pos);
        let vb = vnode(v_src.neg);
        let vd = va - vb;
        // Don't overwrite an already-set entry from a better measurement
        // (diode VS entry is set after res entry — use abs voltage across anode→cathode for diodes)
        component_voltage.entry(v_src.id).or_insert(vd);
        branch_current.entry(v_src.id).or_insert(i_vs);
        component_power.entry(v_src.id).or_insert((vd * i_vs).abs());
        component_power_role.insert(
            v_src.id,
            if vd * i_vs < -1e-12 {
                ComponentPowerRole::Supplying
            } else if vd * i_vs > 1e-12 {
                ComponentPowerRole::Dissipating
            } else {
                ComponentPowerRole::Unknown
            },
        );
    }

    // For diodes: override with the anode→cathode total voltage drop
    for de in &diode_entries {
        let va = vnode(de.anode);
        let vk = vnode(de.cathode);
        let vd = va - vk;
        component_voltage.insert(de.id, vd);
        let index = diode_entries
            .iter()
            .position(|candidate| candidate.id == de.id)
            .unwrap_or(0);
        let current = if diode_states.get(index).copied().unwrap_or(false) {
            ((vd - de.vf) / de.rb).max(0.0)
        } else {
            vd / 1.0e9
        };
        branch_current.insert(de.id, current);
        component_power.insert(de.id, (vd * current).abs());
        component_power_role.insert(de.id, ComponentPowerRole::Dissipating);
    }

    for (index, mos) in mos_entries.iter().enumerate() {
        let vd = vnode(mos.drain) - vnode(mos.source);
        let resistance = if mos_states.get(index).copied().unwrap_or(false) {
            mos.r_on
        } else {
            mos.r_off
        };
        let current = vd / resistance;
        component_voltage.insert(mos.id, vd);
        branch_current.insert(mos.id, current);
        component_power.insert(mos.id, (vd * current).abs());
        component_power_role.insert(mos.id, ComponentPowerRole::Dissipating);
    }

    // For BJTs: override with Vce voltage and total Ic current
    for (bi, bjt) in bjt_entries.iter().enumerate() {
        let k_vbe = bjt_vs_start + bi;
        let ib = x.get(total_nodes + k_vbe).copied().unwrap_or(0.0);
        let ic = bjt.h_fe * ib;
        let vc = vnode(bjt.c);
        let ve = vnode(bjt.e);
        let vce = vc - ve;
        component_voltage.insert(bjt.id, vce);
        branch_current.insert(bjt.id, ic.abs());
        component_power.insert(bjt.id, (vce * ic).abs());
        component_power_role.insert(bjt.id, ComponentPowerRole::Dissipating);
    }

    for i_src in &is_src {
        let va = vnode(i_src.pos);
        let vb = vnode(i_src.neg);
        let vd = va - vb;
        component_voltage.insert(i_src.id, vd);
        branch_current.insert(i_src.id, i_src.i);
        component_power.insert(i_src.id, (vd * i_src.i).abs());
        component_power_role.insert(
            i_src.id,
            if -vd * i_src.i < -1e-12 {
                ComponentPowerRole::Supplying
            } else if -vd * i_src.i > 1e-12 {
                ComponentPowerRole::Dissipating
            } else {
                ComponentPowerRole::Unknown
            },
        );
    }

    // Wire voltages: average of MNA voltages at all wire-point nodes
    let mut wire_voltage: HashMap<u64, f64> = HashMap::new();
    for wire in wires {
        let mut sum = 0.0;
        let mut cnt = 0usize;
        for &pt in &wire.points {
            if let Some(root) = nm.root_of(pt) {
                if let Some(&mna_idx) = mna_of.get(&root) {
                    sum += vnode(mna_idx);
                    cnt += 1;
                }
            }
        }
        if cnt > 0 {
            wire_voltage.insert(wire.id, sum / cnt as f64);
        }
    }

    let mut wire_current = HashMap::new();
    let mut wire_current_known = HashSet::new();
    for wire in wires {
        let mut candidates = Vec::new();
        for component in components {
            let Some(&current) = branch_current.get(&component.id) else {
                continue;
            };
            let pins = component_pin_defs(component);
            for (pin_index, pin) in pins.iter().enumerate() {
                if !wire
                    .points
                    .windows(2)
                    .any(|segment| point_touches_wire_segment(pin.pos, segment[0], segment[1]))
                {
                    continue;
                }
                let terminal_current =
                    terminal_current_into_component(component.kind, pin, pin_index, current);
                let Some(distance) = distance_along_wire(&wire.points, pin.pos) else {
                    continue;
                };
                let toward_pin_sign = if distance <= wire_polyline_length(&wire.points) * 0.5 {
                    -1.0
                } else {
                    1.0
                };
                let candidate = terminal_current * toward_pin_sign;
                candidates.push(candidate);
            }
        }

        let representative = candidates
            .iter()
            .copied()
            .max_by(|a, b| a.abs().total_cmp(&b.abs()))
            .unwrap_or(0.0);
        wire_current.insert(wire.id, representative);

        // A polyline can only have one meaningful displayed current when every
        // attached terminal agrees. Mid-wire branches can carry different
        // currents on either side, so suppress their direction rather than
        // showing a physically false net-wide value.
        if !candidates.is_empty() {
            let tolerance = representative.abs().max(1.0) * 1.0e-9;
            if candidates
                .iter()
                .all(|candidate| (*candidate - representative).abs() <= tolerance)
            {
                wire_current_known.insert(wire.id);
            }
        }
    }

    let vmax = net_voltages
        .values()
        .map(|v| v.abs())
        .fold(0.0_f64, f64::max);

    Ok(DcResult {
        net_voltages,
        component_voltage,
        branch_current,
        component_power,
        wire_voltage,
        wire_current,
        wire_current_known,
        component_power_role,
        max_kcl_residual: solution.max_kcl_residual,
        vmax: vmax.max(0.1),
    })
}

fn terminal_current_into_component(
    kind: ComponentKind,
    pin: &crate::CircuitPin,
    pin_index: usize,
    branch_current: f64,
) -> f64 {
    match kind {
        ComponentKind::VSource | ComponentKind::Battery => {
            if pin.role == PinRole::Positive || pin.label == "+" {
                branch_current
            } else {
                -branch_current
            }
        }
        ComponentKind::ISource => {
            if pin.role == PinRole::Positive || pin.label == "+" {
                -branch_current
            } else {
                branch_current
            }
        }
        ComponentKind::Diode
        | ComponentKind::Led
        | ComponentKind::SchottkyDiode
        | ComponentKind::ZenerDiode
        | ComponentKind::TvsDiode => {
            if pin.label == "A" {
                branch_current
            } else {
                -branch_current
            }
        }
        ComponentKind::Nmosfet | ComponentKind::Pmosfet => match pin.label {
            "D" => branch_current,
            "S" => -branch_current,
            _ => 0.0,
        },
        _ => {
            if pin_index == 0 {
                branch_current
            } else if pin_index == 1 {
                -branch_current
            } else {
                0.0
            }
        }
    }
}

fn wire_polyline_length(points: &[Pos2]) -> f32 {
    points
        .windows(2)
        .map(|segment| segment[0].distance(segment[1]))
        .sum()
}

fn distance_along_wire(points: &[Pos2], point: Pos2) -> Option<f32> {
    let mut traveled = 0.0;
    let mut best: Option<(f32, f32)> = None;
    for segment in points.windows(2) {
        let a = segment[0];
        let b = segment[1];
        let ab = b - a;
        let length_sq = ab.length_sq();
        if length_sq <= f32::EPSILON {
            continue;
        }
        let t = ((point - a).dot(ab) / length_sq).clamp(0.0, 1.0);
        let projection = a + ab * t;
        let distance = projection.distance(point);
        let along = traveled + a.distance(b) * t;
        if best.is_none_or(|(best_distance, _)| distance < best_distance) {
            best = Some((distance, along));
        }
        traveled += a.distance(b);
    }
    best.filter(|(distance, _)| *distance <= 1.0)
        .map(|(_, along)| along)
}

// ─────────────────────────────────────────────────────────────────────────────
//  Display helpers
// ─────────────────────────────────────────────────────────────────────────────

pub fn format_voltage(v: f64) -> String {
    if v.abs() >= 1000.0 {
        format!("{:.1}kV", v / 1000.0)
    } else if v.abs() >= 1.0 {
        format!("{:.3}V", v)
    } else if v.abs() >= 0.001 {
        format!("{:.1}mV", v * 1000.0)
    } else {
        format!("{:.1}µV", v * 1_000_000.0)
    }
}

pub fn format_current(i: f64) -> String {
    let a = i.abs();
    if a >= 1.0 {
        format!("{:.3}A", i)
    } else if a >= 0.001 {
        format!("{:.2}mA", i * 1000.0)
    } else if a >= 1e-6 {
        format!("{:.2}µA", i * 1_000_000.0)
    } else {
        format!("{:.2}nA", i * 1e9)
    }
}

/// Format a value with SI prefix and unit (e.g. 1e-6 F → "1.00µF")
pub fn format_si(val: f64, unit: &str) -> String {
    let a = val.abs();
    if a >= 1.0 {
        format!("{:.3}{}", val, unit)
    } else if a >= 1e-3 {
        format!("{:.3}m{}", val * 1e3, unit)
    } else if a >= 1e-6 {
        format!("{:.3}µ{}", val * 1e6, unit)
    } else if a >= 1e-9 {
        format!("{:.3}n{}", val * 1e9, unit)
    } else {
        format!("{:.3}p{}", val * 1e12, unit)
    }
}

pub fn format_power(w: f64) -> String {
    if w >= 1.0 {
        format!("{:.3}W", w)
    } else if w >= 0.001 {
        format!("{:.2}mW", w * 1000.0)
    } else {
        format!("{:.2}µW", w * 1_000_000.0)
    }
}

/// Map a voltage to a display colour gradient:
/// GND (0 V) → steel-blue, low → cyan, mid → green, high → orange, very-high → red.
pub fn voltage_color(v: f64, vmax: f64) -> egui::Color32 {
    use egui::Color32;
    if vmax < 0.001 {
        return Color32::from_rgb(80, 120, 160);
    }
    let t = (v / vmax).clamp(-1.0, 1.0);
    if t < 0.0 {
        // Negative voltage: blue-purple
        let s = (-t) as f32;
        return Color32::from_rgb(
            (80.0 + s * 120.0) as u8,
            (80.0 - s * 60.0) as u8,
            (200.0 + s * 55.0) as u8,
        );
    }
    // Positive: gradient  blue → cyan → green → yellow → orange → red
    let s = t as f32;
    if s < 0.25 {
        let u = s / 0.25;
        Color32::from_rgb(
            (40.0 + u * 20.0) as u8,
            (180.0 + u * 60.0) as u8,
            (220.0 - u * 80.0) as u8,
        )
    } else if s < 0.5 {
        let u = (s - 0.25) / 0.25;
        Color32::from_rgb(
            (60.0 + u * 130.0) as u8,
            (240.0 - u * 30.0) as u8,
            (140.0 - u * 100.0) as u8,
        )
    } else if s < 0.75 {
        let u = (s - 0.5) / 0.25;
        Color32::from_rgb(
            (190.0 + u * 60.0) as u8,
            (210.0 - u * 100.0) as u8,
            (40.0 - u * 30.0) as u8,
        )
    } else {
        let u = (s - 0.75) / 0.25;
        Color32::from_rgb(
            (250.0 - u * 20.0) as u8,
            (110.0 - u * 100.0) as u8,
            (10.0) as u8,
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  AC operating-point solver (small-signal, single frequency)
// ─────────────────────────────────────────────────────────────────────────────

/// Per-node AC result at a given frequency.
#[derive(Default, Clone, Debug)]
pub struct AcResult {
    /// net_root → complex voltage (re, im) in Volts
    pub node_voltages: HashMap<usize, (f64, f64)>,
    /// wire_id → |V| magnitude
    pub wire_voltage_mag: HashMap<u64, f64>,
    /// wire_id → phase angle in degrees
    pub wire_voltage_phase: HashMap<u64, f64>,
    /// component_id → impedance magnitude |Z| in Ω
    pub component_impedance: HashMap<u64, f64>,
    /// Maximum voltage magnitude for display scaling
    pub vmax: f64,
}

/// Solve the AC small-signal operating point at `freq_hz`.
/// Returns `None` when the circuit has no GND or the matrix is singular.
/// Capacitors and inductors are modelled as complex admittances; resistors as
/// real admittances. Voltage sources are treated as ideal (zero impedance).
pub fn solve_ac(components: &[Component], wires: &[Wire], freq_hz: f64) -> Option<AcResult> {
    if freq_hz <= 0.0 {
        return None;
    }
    let omega = 2.0 * std::f64::consts::PI * freq_hz;

    // ── Build net map (same as DC) ────────────────────────────────────────
    let mut nm = NetMap::new();
    for wire in wires {
        let indices: Vec<usize> = wire.points.iter().map(|&p| nm.reg(p)).collect();
        for w in indices.windows(2) {
            nm.join(w[0], w[1]);
        }
    }
    for comp in components {
        for pin in component_pin_defs(comp) {
            nm.reg(pin.pos);
        }
    }
    for contact in wire_contact_points(components, wires) {
        let ci = nm.reg(contact);
        for wire in wires {
            for seg in wire.points.windows(2) {
                if point_touches_wire_segment(contact, seg[0], seg[1]) {
                    let a = nm.reg(seg[0]);
                    let b = nm.reg(seg[1]);
                    nm.join(ci, a);
                    nm.join(ci, b);
                }
            }
        }
    }
    // NetLabel merging
    {
        let mut label_nodes: HashMap<String, Vec<usize>> = HashMap::new();
        for comp in components {
            if comp.kind == ComponentKind::NetLabel {
                let lbl = comp.value.trim().to_ascii_lowercase();
                if lbl.is_empty() {
                    continue;
                }
                for pin in component_pin_defs(comp) {
                    let idx = nm.reg(pin.pos);
                    label_nodes.entry(lbl.clone()).or_default().push(idx);
                }
            }
        }
        for (_, nodes) in &label_nodes {
            for w in nodes.windows(2) {
                nm.join(w[0], w[1]);
            }
        }
    }

    // ── GND detection ────────────────────────────────────────────────────
    let mut gnd_roots: HashSet<usize> = HashSet::new();
    for comp in components {
        match comp.kind {
            ComponentKind::Ground => {
                for pin in component_pin_defs(comp) {
                    if let Some(r) = nm.root_of(pin.pos) {
                        gnd_roots.insert(r);
                    }
                }
            }
            ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource => {
                for pin in component_pin_defs(comp) {
                    if pin.role == PinRole::Ground {
                        if let Some(r) = nm.root_of(pin.pos) {
                            gnd_roots.insert(r);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if gnd_roots.is_empty() {
        return None;
    }

    // ── Assign MNA node numbers ───────────────────────────────────────────
    let node_count = nm.nodes.positions.len();
    let all_roots: HashSet<usize> = (0..node_count).map(|i| nm.uf.find(i)).collect();
    let mut mna_of: HashMap<usize, usize> = HashMap::new();
    let mut next_node = 1usize;
    for &root in &all_roots {
        if gnd_roots.contains(&root) {
            mna_of.insert(root, 0);
        } else {
            mna_of.insert(root, next_node);
            next_node += 1;
        }
    }
    let n = next_node - 1;
    if n == 0 {
        return None;
    }

    let mna_node_ac =
        |pos: Pos2, nm: &mut NetMap, mna_of: &HashMap<usize, usize>| -> Option<usize> {
            nm.root_of(pos).and_then(|r| mna_of.get(&r).copied())
        };

    // ── Complex admittance matrix (re + j*im stored as flat 2D) ──────────
    // We build a 2n × 2n real system by interleaving real/imag rows:
    //   row 2k   = real part of KCL at node k+1
    //   row 2k+1 = imag part of KCL at node k+1
    // plus VS rows for ideal voltage sources (real only, imaginary = 0).
    // For simplicity, voltage sources are stamped as ideal (Thevenin with 0 impedance).

    // Count VS entries (same as DC logic)
    let mut vs_count = 0usize;
    for comp in components {
        match comp.kind {
            ComponentKind::VSource | ComponentKind::Battery => {
                vs_count += 1;
            }
            _ => {}
        }
    }
    let sz = 2 * n + vs_count;
    if sz == 0 {
        return None;
    }

    let mut a_mat = vec![vec![0.0f64; sz]; sz];
    let mut z_vec = vec![0.0f64; sz];

    // Helper: stamp complex admittance (g_re + j*g_im) between nodes a and b
    let stamp_y = |a_mat: &mut Vec<Vec<f64>>, a: usize, b: usize, g_re: f64, g_im: f64| {
        if a > 0 {
            a_mat[2 * (a - 1)][2 * (a - 1)] += g_re;
            a_mat[2 * (a - 1) + 1][2 * (a - 1) + 1] += g_re;
            a_mat[2 * (a - 1)][2 * (a - 1) + 1] -= g_im;
            a_mat[2 * (a - 1) + 1][2 * (a - 1)] += g_im;
        }
        if b > 0 {
            a_mat[2 * (b - 1)][2 * (b - 1)] += g_re;
            a_mat[2 * (b - 1) + 1][2 * (b - 1) + 1] += g_re;
            a_mat[2 * (b - 1)][2 * (b - 1) + 1] -= g_im;
            a_mat[2 * (b - 1) + 1][2 * (b - 1)] += g_im;
        }
        if a > 0 && b > 0 {
            a_mat[2 * (a - 1)][2 * (b - 1)] -= g_re;
            a_mat[2 * (a - 1) + 1][2 * (b - 1) + 1] -= g_re;
            a_mat[2 * (a - 1)][2 * (b - 1) + 1] += g_im;
            a_mat[2 * (a - 1) + 1][2 * (b - 1)] -= g_im;

            a_mat[2 * (b - 1)][2 * (a - 1)] -= g_re;
            a_mat[2 * (b - 1) + 1][2 * (a - 1) + 1] -= g_re;
            a_mat[2 * (b - 1)][2 * (a - 1) + 1] += g_im;
            a_mat[2 * (b - 1) + 1][2 * (a - 1)] -= g_im;
        }
    };

    // Helper: stamp VS row (index k, zero-based) for V_pos - V_neg = v (real)
    let stamp_vs_ac = |a_mat: &mut Vec<Vec<f64>>,
                       z_vec: &mut Vec<f64>,
                       k: usize,
                       pos: usize,
                       neg: usize,
                       v: f64| {
        let ki = 2 * n + k;
        if pos > 0 {
            a_mat[ki][2 * (pos - 1)] += 1.0;
            a_mat[2 * (pos - 1)][ki] += 1.0;
        }
        if neg > 0 {
            a_mat[ki][2 * (neg - 1)] -= 1.0;
            a_mat[2 * (neg - 1)][ki] -= 1.0;
        }
        z_vec[ki] += v;
    };

    let mut vs_k = 0usize;
    let mut comp_impedance: HashMap<u64, f64> = HashMap::new();

    for comp in components {
        let pins = component_pin_defs(comp);
        let p0 = pins
            .get(0)
            .and_then(|p| mna_node_ac(p.pos, &mut nm, &mna_of));
        let p1 = pins
            .get(1)
            .and_then(|p| mna_node_ac(p.pos, &mut nm, &mna_of));

        match comp.kind {
            ComponentKind::Resistor
            | ComponentKind::Potentiometer
            | ComponentKind::Thermistor
            | ComponentKind::Varistor => {
                let r = parse_metric_value(&comp.value, "ohm").unwrap_or(10_000.0) as f64;
                if let (Some(a), Some(b)) = (p0, p1) {
                    stamp_y(&mut a_mat, a, b, 1.0 / r, 0.0);
                    comp_impedance.insert(comp.id, r);
                }
            }
            ComponentKind::Capacitor => {
                let c = parse_si_value(&comp.value).unwrap_or(100e-9);
                if c > 0.0 {
                    // Y_c = jωC  →  G_re=0, G_im=ωC
                    if let (Some(a), Some(b)) = (p0, p1) {
                        stamp_y(&mut a_mat, a, b, 0.0, omega * c);
                        comp_impedance.insert(comp.id, 1.0 / (omega * c));
                    }
                }
            }
            ComponentKind::Inductor => {
                let l = parse_si_value(&comp.value).unwrap_or(10e-6);
                if l > 0.0 {
                    // Y_l = 1/(jωL) = -j/(ωL)  →  G_re=0, G_im=-1/(ωL)
                    if let (Some(a), Some(b)) = (p0, p1) {
                        stamp_y(&mut a_mat, a, b, 0.0, -1.0 / (omega * l));
                        comp_impedance.insert(comp.id, omega * l);
                    }
                }
            }
            ComponentKind::VSource | ComponentKind::Battery => {
                let v = parse_metric_value(&comp.value, "v").unwrap_or(5.0) as f64;
                let pos_n = pins
                    .iter()
                    .find(|p| p.role == PinRole::Positive || p.label == "+")
                    .and_then(|p| mna_node_ac(p.pos, &mut nm, &mna_of))
                    .unwrap_or(0);
                let neg_n = pins
                    .iter()
                    .find(|p| p.role == PinRole::Ground || p.label == "-")
                    .and_then(|p| mna_node_ac(p.pos, &mut nm, &mna_of))
                    .unwrap_or(0);
                stamp_vs_ac(&mut a_mat, &mut z_vec, vs_k, pos_n, neg_n, v);
                vs_k += 1;
            }
            // Treat other components as resistive stubs
            ComponentKind::Fuse | ComponentKind::Lamp | ComponentKind::DcMotor => {
                if let (Some(a), Some(b)) = (p0, p1) {
                    stamp_y(&mut a_mat, a, b, 1.0 / 10.0, 0.0);
                }
            }
            ComponentKind::Diode
            | ComponentKind::Led
            | ComponentKind::SchottkyDiode
            | ComponentKind::ZenerDiode => {
                if let (Some(a), Some(b)) = (p0, p1) {
                    stamp_y(&mut a_mat, a, b, 1.0 / 20.0, 0.0);
                }
            }
            ComponentKind::Voltmeter => {
                if let (Some(a), Some(b)) = (p0, p1) {
                    stamp_y(&mut a_mat, a, b, 1.0 / 1_000_000.0, 0.0);
                }
            }
            ComponentKind::Ammeter => {
                if let (Some(a), Some(b)) = (p0, p1) {
                    stamp_y(&mut a_mat, a, b, 1.0 / 0.001, 0.0);
                }
            }
            _ => {}
        }
    }

    // ── Gaussian elimination on the real-expanded system ─────────────────
    let n_eq = sz;
    for col in 0..n_eq {
        let mut prow = col;
        for row in (col + 1)..n_eq {
            if a_mat[row][col].abs() > a_mat[prow][col].abs() {
                prow = row;
            }
        }
        if a_mat[prow][col].abs() < 1e-14 {
            return None;
        }
        a_mat.swap(col, prow);
        z_vec.swap(col, prow);
        let piv = a_mat[col][col];
        for row in (col + 1)..n_eq {
            let f = a_mat[row][col] / piv;
            if f.abs() < 1e-20 {
                continue;
            }
            z_vec[row] -= f * z_vec[col];
            for j in col..n_eq {
                a_mat[row][j] -= f * a_mat[col][j];
            }
        }
    }
    let mut x = vec![0.0f64; n_eq];
    for i in (0..n_eq).rev() {
        x[i] = z_vec[i];
        for j in (i + 1)..n_eq {
            x[i] -= a_mat[i][j] * x[j];
        }
        if a_mat[i][i].abs() < 1e-14 {
            return None;
        }
        x[i] /= a_mat[i][i];
    }

    // ── Extract node voltages (re, im) ────────────────────────────────────
    let vnode_c = |mna_idx: usize| -> (f64, f64) {
        if mna_idx == 0 {
            (0.0, 0.0)
        } else {
            let re = x.get(2 * (mna_idx - 1)).copied().unwrap_or(0.0);
            let im = x.get(2 * (mna_idx - 1) + 1).copied().unwrap_or(0.0);
            (re, im)
        }
    };

    let mut node_voltages: HashMap<usize, (f64, f64)> = HashMap::new();
    for (&root, &mna_idx) in &mna_of {
        node_voltages.insert(root, vnode_c(mna_idx));
    }

    // ── Wire voltage magnitudes ───────────────────────────────────────────
    let mut wire_voltage_mag: HashMap<u64, f64> = HashMap::new();
    let mut wire_voltage_phase: HashMap<u64, f64> = HashMap::new();
    for wire in wires {
        let mut sum_re = 0.0;
        let mut sum_im = 0.0;
        let mut cnt = 0usize;
        for &pt in &wire.points {
            if let Some(root) = nm.root_of(pt) {
                if let Some(&mna_idx) = mna_of.get(&root) {
                    let (re, im) = vnode_c(mna_idx);
                    sum_re += re;
                    sum_im += im;
                    cnt += 1;
                }
            }
        }
        if cnt > 0 {
            let re = sum_re / cnt as f64;
            let im = sum_im / cnt as f64;
            let mag = (re * re + im * im).sqrt();
            let phase = im.atan2(re).to_degrees();
            wire_voltage_mag.insert(wire.id, mag);
            wire_voltage_phase.insert(wire.id, phase);
        }
    }

    let vmax = node_voltages
        .values()
        .map(|(re, im)| (re * re + im * im).sqrt())
        .fold(0.1_f64, f64::max);

    Some(AcResult {
        node_voltages,
        wire_voltage_mag,
        wire_voltage_phase,
        component_impedance: comp_impedance,
        vmax,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
//  DC solver tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;

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

    // Helper: multi-point wire.
    fn lseg(id: u64, points: Vec<Pos2>) -> Wire {
        Wire { id, points }
    }

    // Build an L-shaped wire from point a to b via a corner at (b.x, a.y).
    fn l_wire(id: u64, a: Pos2, b: Pos2) -> Wire {
        let corner = Pos2::new(b.x, a.y);
        Wire {
            id,
            points: vec![a, corner, b],
        }
    }

    // ── Single resistor across a battery ─────────────────────────────────
    // Circuit: BAT(9V) +→ R(1kΩ) → GND
    // Layout uses L-shaped wires to avoid collinear T-junction false positives.
    // Expected: I ≈ 9 mA
    #[test]
    fn single_resistor_load() {
        // Battery at (0, 0):  + at (32,0),  - at (-32,0)
        // Resistor at (200, 0): A at (164,0), B at (236,0)
        // Ground at (0, 120):  pin at (0, 100)
        let bat = comp(1, ComponentKind::Battery, Pos2::new(0.0, 0.0), "BAT1", "9V");
        let r = comp(
            2,
            ComponentKind::Resistor,
            Pos2::new(200.0, 0.0),
            "R1",
            "1k",
        );
        let gnd = comp(
            3,
            ComponentKind::Ground,
            Pos2::new(0.0, 120.0),
            "GND1",
            "0V",
        );

        let bat_pins = component_pin_defs(&bat);
        let r_pins = component_pin_defs(&r);
        let gnd_pins = component_pin_defs(&gnd);

        let bat_p = bat_pins
            .iter()
            .find(|p| p.role == PinRole::Positive)
            .unwrap()
            .pos; // (32, 0)
        let bat_n = bat_pins
            .iter()
            .find(|p| p.role == PinRole::Ground)
            .unwrap()
            .pos; // (-32, 0)
        let r_a = r_pins.iter().find(|p| p.label == "A").unwrap().pos; // (164, 0)
        let r_b = r_pins.iter().find(|p| p.label == "B").unwrap().pos; // (236, 0)
        let gnd_p = gnd_pins[0].pos; // (0, 100)

        // Use L-shaped wires so no endpoint lies collinear on another segment.
        // W10: bat+ (32,0) → up to (32,-40) → right to (164,-40) → down to r_a (164,0)
        // W11: r_b (236,0) → down to (236,60) → left to (-32,60) → up to bat- (-32,0)
        // W12: bat- (-32,0) → diagonal to gnd (0,100)   [not collinear with W10/W11]
        let wires = vec![
            lseg(
                10,
                vec![
                    bat_p,
                    Pos2::new(bat_p.x, -40.0),
                    Pos2::new(r_a.x, -40.0),
                    r_a,
                ],
            ),
            lseg(
                11,
                vec![r_b, Pos2::new(r_b.x, 60.0), Pos2::new(bat_n.x, 60.0), bat_n],
            ),
            lseg(12, vec![bat_n, gnd_p]),
        ];

        let result = solve_dc(&[bat, r, gnd], &wires);
        assert!(
            result.is_some(),
            "Should converge for simple resistive circuit"
        );
        let dc = result.unwrap();

        // Battery branch current ≈ 9 mA (9V / 1kΩ).
        let bat_i = dc.branch_current.get(&1).copied().unwrap_or(0.0);
        assert!(
            (bat_i.abs() - 0.009).abs() < 0.001,
            "Expected ~9 mA, got {bat_i:.4} A"
        );
        assert!(
            dc.wire_current
                .values()
                .any(|current| current.abs() > 0.008),
            "Wire current should be derived from solved branch current"
        );
    }

    #[test]
    fn five_volts_across_one_kilohm_matches_ohms_law_and_power() {
        let bat = comp(1, ComponentKind::Battery, Pos2::new(0.0, 0.0), "BAT1", "5V");
        let resistor = comp(
            2,
            ComponentKind::Resistor,
            Pos2::new(200.0, 0.0),
            "R1",
            "1k",
        );
        let bat_pins = component_pin_defs(&bat);
        let resistor_pins = component_pin_defs(&resistor);
        let bat_p = bat_pins.iter().find(|pin| pin.label == "+").unwrap().pos;
        let bat_n = bat_pins.iter().find(|pin| pin.label == "-").unwrap().pos;
        let r_a = resistor_pins
            .iter()
            .find(|pin| pin.label == "A")
            .unwrap()
            .pos;
        let r_b = resistor_pins
            .iter()
            .find(|pin| pin.label == "B")
            .unwrap()
            .pos;
        let wires = vec![
            lseg(
                10,
                vec![
                    bat_p,
                    Pos2::new(bat_p.x, -40.0),
                    Pos2::new(r_a.x, -40.0),
                    r_a,
                ],
            ),
            lseg(
                11,
                vec![r_b, Pos2::new(r_b.x, 40.0), Pos2::new(bat_n.x, 40.0), bat_n],
            ),
        ];

        let dc = solve_dc(&[bat, resistor], &wires).expect("resistive circuit should solve");
        let voltage = dc.component_voltage[&2];
        let current = dc.branch_current[&2];
        let power = dc.component_power[&2];
        assert!((voltage.abs() - 5.0).abs() < 1.0e-9);
        assert!((current.abs() - 0.005).abs() < 1.0e-9);
        assert!((power - 0.025).abs() < 1.0e-9);
        assert!((power - voltage.powi(2) / 1_000.0).abs() < 1.0e-9);
        assert!((power - current.powi(2) * 1_000.0).abs() < 1.0e-9);
    }

    #[test]
    fn open_source_wire_has_voltage_but_zero_current() {
        let bat = comp(1, ComponentKind::Battery, Pos2::new(0.0, 0.0), "BAT1", "5V");
        let pins = component_pin_defs(&bat);
        let positive = pins.iter().find(|pin| pin.label == "+").unwrap().pos;
        let wire = lseg(
            10,
            vec![positive, Pos2::new(positive.x + 120.0, positive.y)],
        );

        let dc =
            solve_dc(&[bat], &[wire]).expect("open voltage source should have an operating point");
        assert!((dc.wire_voltage[&10] - 5.0).abs() < 1.0e-9);
        assert!(dc.wire_current[&10].abs() < 1.0e-12);
        assert!(dc.wire_current_known.contains(&10));
        assert!(dc.branch_current[&1].abs() < 1.0e-12);
    }

    #[test]
    fn branched_polyline_does_not_claim_one_current_for_all_segments() {
        let bat = comp(1, ComponentKind::Battery, Pos2::new(0.0, 0.0), "BAT1", "5V");
        let r1 = comp(
            2,
            ComponentKind::Resistor,
            Pos2::new(300.0, 0.0),
            "R1",
            "1k",
        );
        let mut r2 = comp(
            3,
            ComponentKind::Resistor,
            Pos2::new(164.0, 36.0),
            "R2",
            "1k",
        );
        r2.rotation = 90;
        let bat_pins = component_pin_defs(&bat);
        let r1_pins = component_pin_defs(&r1);
        let r2_pins = component_pin_defs(&r2);
        let bat_p = bat_pins.iter().find(|pin| pin.label == "+").unwrap().pos;
        let bat_n = bat_pins.iter().find(|pin| pin.label == "-").unwrap().pos;
        let r1_a = r1_pins.iter().find(|pin| pin.label == "A").unwrap().pos;
        let r1_b = r1_pins.iter().find(|pin| pin.label == "B").unwrap().pos;
        let r2_a = r2_pins.iter().find(|pin| pin.label == "A").unwrap().pos;
        let r2_b = r2_pins.iter().find(|pin| pin.label == "B").unwrap().pos;
        let wires = vec![
            lseg(10, vec![bat_p, r2_a, r1_a]),
            lseg(11, vec![r1_b, Pos2::new(r1_b.x, 80.0), bat_n]),
            lseg(12, vec![r2_b, Pos2::new(r2_b.x, 120.0), bat_n]),
        ];

        let dc = solve_dc(&[bat, r1, r2], &wires).expect("parallel load should solve");

        assert!((dc.branch_current[&2].abs() - 0.005).abs() < 1.0e-9);
        assert!((dc.branch_current[&3].abs() - 0.005).abs() < 1.0e-9);
        assert!(
            !dc.wire_current_known.contains(&10),
            "A midpoint branch has different current on each side of one polyline"
        );
    }

    // ── No GND → solver returns None ─────────────────────────────────────

    #[test]
    fn no_gnd_returns_none() {
        let bat = comp(1, ComponentKind::Battery, Pos2::new(0.0, 0.0), "BAT1", "9V");
        let r = comp(
            2,
            ComponentKind::Resistor,
            Pos2::new(100.0, 0.0),
            "R1",
            "1k",
        );
        // No ground and no wires → battery negative isn't marked GND either.
        let result = solve_dc(&[bat, r], &[]);
        assert!(result.is_none(), "Circuit without GND must not converge");
    }

    // ── Open switch → no current path ────────────────────────────────────

    #[test]
    fn open_switch_blocks_current() {
        // If solve_dc returns Some, the resistor current must be ≈ 0.
        // If it returns None (singular), that's also acceptable.
        let bat = comp(1, ComponentKind::Battery, Pos2::new(0.0, 0.0), "BAT1", "9V");
        let sw = comp(
            2,
            ComponentKind::Switch,
            Pos2::new(0.0, -120.0),
            "SW1",
            "open",
        );
        let r = comp(
            3,
            ComponentKind::Resistor,
            Pos2::new(200.0, -120.0),
            "R1",
            "1k",
        );
        let gnd = comp(
            4,
            ComponentKind::Ground,
            Pos2::new(0.0, 120.0),
            "GND1",
            "0V",
        );

        let bat_pins = component_pin_defs(&bat);
        let sw_pins = component_pin_defs(&sw);
        let r_pins = component_pin_defs(&r);
        let gnd_pins = component_pin_defs(&gnd);

        let bat_p = bat_pins
            .iter()
            .find(|p| p.role == PinRole::Positive)
            .unwrap()
            .pos;
        let bat_n = bat_pins
            .iter()
            .find(|p| p.role == PinRole::Ground)
            .unwrap()
            .pos;

        let wires = vec![
            l_wire(10, bat_p, sw_pins[0].pos),
            l_wire(11, sw_pins[1].pos, r_pins[1].pos),
            l_wire(12, r_pins[0].pos, bat_n),
            lseg(13, vec![bat_n, gnd_pins[0].pos]),
        ];

        let result = solve_dc(&[bat, sw, r, gnd], &wires);
        if let Some(dc) = result {
            let r_i = dc.branch_current.get(&3).copied().unwrap_or(0.0);
            assert!(
                r_i.abs() < 1e-6,
                "Open switch should block current, got {r_i}"
            );
        }
        // None (singular matrix) is also acceptable for an open circuit.
    }

    #[test]
    fn reversed_led_has_only_leakage_current() {
        let bat = comp(1, ComponentKind::Battery, Pos2::new(0.0, 0.0), "BAT1", "9V");
        let led = comp(2, ComponentKind::Led, Pos2::new(180.0, 0.0), "LED1", "red");
        let bat_pins = component_pin_defs(&bat);
        let led_pins = component_pin_defs(&led);
        let bat_p = bat_pins.iter().find(|pin| pin.label == "+").unwrap().pos;
        let bat_n = bat_pins.iter().find(|pin| pin.label == "-").unwrap().pos;
        let anode = led_pins.iter().find(|pin| pin.label == "A").unwrap().pos;
        let cathode = led_pins.iter().find(|pin| pin.label == "B").unwrap().pos;
        let wires = vec![
            lseg(
                10,
                vec![
                    bat_p,
                    Pos2::new(bat_p.x, -40.0),
                    Pos2::new(cathode.x, -40.0),
                    cathode,
                ],
            ),
            lseg(
                11,
                vec![
                    anode,
                    Pos2::new(anode.x, 40.0),
                    Pos2::new(bat_n.x, 40.0),
                    bat_n,
                ],
            ),
        ];

        let dc = solve_dc(&[bat, led], &wires).expect("reverse-biased LED circuit should solve");
        let current = dc.branch_current.get(&2).copied().unwrap_or(0.0);
        assert!(
            current.abs() < 1.0e-6,
            "Reverse-biased LED should be nearly open, got {current} A"
        );
    }

    #[test]
    fn forward_led_current_matches_piecewise_model() {
        let bat = comp(1, ComponentKind::Battery, Pos2::new(0.0, 0.0), "BAT1", "5V");
        let resistor = comp(
            2,
            ComponentKind::Resistor,
            Pos2::new(160.0, -80.0),
            "R1",
            "330",
        );
        let led = comp(
            3,
            ComponentKind::Led,
            Pos2::new(320.0, -80.0),
            "LED1",
            "red",
        );
        let bat_pins = component_pin_defs(&bat);
        let resistor_pins = component_pin_defs(&resistor);
        let led_pins = component_pin_defs(&led);
        let bat_p = bat_pins.iter().find(|pin| pin.label == "+").unwrap().pos;
        let bat_n = bat_pins.iter().find(|pin| pin.label == "-").unwrap().pos;
        let r_a = resistor_pins
            .iter()
            .find(|pin| pin.label == "A")
            .unwrap()
            .pos;
        let r_b = resistor_pins
            .iter()
            .find(|pin| pin.label == "B")
            .unwrap()
            .pos;
        let led_a = led_pins.iter().find(|pin| pin.label == "A").unwrap().pos;
        let led_k = led_pins.iter().find(|pin| pin.label == "B").unwrap().pos;
        let wires = vec![
            lseg(
                10,
                vec![
                    bat_p,
                    Pos2::new(bat_p.x, -140.0),
                    Pos2::new(r_a.x, -140.0),
                    r_a,
                ],
            ),
            lseg(11, vec![r_b, led_a]),
            lseg(
                12,
                vec![
                    led_k,
                    Pos2::new(led_k.x, 60.0),
                    Pos2::new(bat_n.x, 60.0),
                    bat_n,
                ],
            ),
        ];

        let dc = solve_dc(&[bat, resistor, led], &wires).expect("forward LED circuit should solve");
        let current = dc.branch_current[&3].abs();
        let expected = (5.0 - 2.0) / (330.0 + 20.0);
        assert!(
            (current - expected).abs() < 0.0002,
            "Expected about {expected} A, got {current} A"
        );
    }

    fn mosfet_switch_circuit(gate_high: bool) -> (Vec<Component>, Vec<Wire>, u64) {
        let bat = comp(1, ComponentKind::Battery, Pos2::new(0.0, 0.0), "BAT1", "5V");
        let resistor = comp(
            2,
            ComponentKind::Resistor,
            Pos2::new(160.0, -100.0),
            "R1",
            "1k",
        );
        let mos = comp(
            3,
            ComponentKind::Nmosfet,
            Pos2::new(300.0, 0.0),
            "Q1",
            "2N7000",
        );
        let bat_pins = component_pin_defs(&bat);
        let resistor_pins = component_pin_defs(&resistor);
        let mos_pins = component_pin_defs(&mos);
        let bat_p = bat_pins.iter().find(|pin| pin.label == "+").unwrap().pos;
        let bat_n = bat_pins.iter().find(|pin| pin.label == "-").unwrap().pos;
        let r_a = resistor_pins
            .iter()
            .find(|pin| pin.label == "A")
            .unwrap()
            .pos;
        let r_b = resistor_pins
            .iter()
            .find(|pin| pin.label == "B")
            .unwrap()
            .pos;
        let gate = mos_pins.iter().find(|pin| pin.label == "G").unwrap().pos;
        let drain = mos_pins.iter().find(|pin| pin.label == "D").unwrap().pos;
        let source = mos_pins.iter().find(|pin| pin.label == "S").unwrap().pos;
        let gate_wire = if gate_high {
            lseg(
                13,
                vec![
                    bat_p,
                    Pos2::new(bat_p.x, -80.0),
                    Pos2::new(gate.x, -80.0),
                    gate,
                ],
            )
        } else {
            lseg(
                13,
                vec![
                    bat_n,
                    Pos2::new(bat_n.x, 80.0),
                    Pos2::new(gate.x, 80.0),
                    gate,
                ],
            )
        };
        (
            vec![bat, resistor, mos],
            vec![
                l_wire(10, bat_p, r_a),
                l_wire(11, r_b, drain),
                l_wire(12, source, bat_n),
                gate_wire,
            ],
            3,
        )
    }

    #[test]
    fn nmos_gate_low_is_off() {
        let (components, wires, mos_id) = mosfet_switch_circuit(false);
        let dc = solve_dc_detailed(&components, &wires)
            .expect("NMOS off circuit should solve with leakage resistance");
        let current = dc.branch_current.get(&mos_id).copied().unwrap_or(0.0);
        assert!(
            current.abs() < 1.0e-6,
            "NMOS should be OFF, got {current} A"
        );
    }

    #[test]
    fn nmos_gate_high_is_on() {
        let (components, wires, mos_id) = mosfet_switch_circuit(true);
        let dc = solve_dc(&components, &wires).expect("NMOS on circuit should solve");
        let current = dc.branch_current.get(&mos_id).copied().unwrap_or(0.0);
        assert!(
            (current.abs() - 0.005).abs() < 0.0005,
            "NMOS should conduct about 5 mA, got {current} A"
        );
    }

    // ── Voltage divider ──────────────────────────────────────────────────
    // 9V battery, R1=2kΩ, R2=1kΩ in series → V(mid) ≈ 3 V

    #[test]
    fn voltage_divider_mid_point() {
        // Positions chosen so no wire segment is collinear with another wire's endpoints.
        // Battery at (0, 0):   + at (32,0),   - at (-32,0)
        // R1 at (100, -80):    A at (64,-80),  B at (136,-80)  [2kΩ]
        // R2 at (220, -80):    A at (184,-80), B at (256,-80)  [1kΩ]
        // Ground at (0, 120):  pin at (0,100)
        let bat = comp(1, ComponentKind::Battery, Pos2::new(0.0, 0.0), "BAT1", "9V");
        let r1 = comp(
            2,
            ComponentKind::Resistor,
            Pos2::new(100.0, -80.0),
            "R1",
            "2k",
        );
        let r2 = comp(
            3,
            ComponentKind::Resistor,
            Pos2::new(220.0, -80.0),
            "R2",
            "1k",
        );
        let gnd = comp(
            4,
            ComponentKind::Ground,
            Pos2::new(0.0, 120.0),
            "GND1",
            "0V",
        );

        let bat_pins = component_pin_defs(&bat);
        let r1_pins = component_pin_defs(&r1);
        let r2_pins = component_pin_defs(&r2);
        let gnd_pins = component_pin_defs(&gnd);

        let bat_p = bat_pins
            .iter()
            .find(|p| p.role == PinRole::Positive)
            .unwrap()
            .pos; // (32,0)
        let bat_n = bat_pins
            .iter()
            .find(|p| p.role == PinRole::Ground)
            .unwrap()
            .pos; // (-32,0)
        let r1_a = r1_pins.iter().find(|p| p.label == "A").unwrap().pos; // (64,-80)
        let r1_b = r1_pins.iter().find(|p| p.label == "B").unwrap().pos; // (136,-80)
        let r2_a = r2_pins.iter().find(|p| p.label == "A").unwrap().pos; // (184,-80)
        let r2_b = r2_pins.iter().find(|p| p.label == "B").unwrap().pos; // (256,-80)
        let gnd_p = gnd_pins[0].pos; // (0,100)

        let wires = vec![
            // bat+ → r1_a via L-shape going above y=0
            lseg(
                10,
                vec![
                    bat_p,
                    Pos2::new(bat_p.x, -120.0),
                    Pos2::new(r1_a.x, -120.0),
                    r1_a,
                ],
            ),
            // r1_b → r2_a straight (same y, no other contacts at y=-80 in this range)
            lseg(11, vec![r1_b, r2_a]),
            // r2_b → bat_n via L-shape going below
            lseg(
                12,
                vec![
                    r2_b,
                    Pos2::new(r2_b.x, 60.0),
                    Pos2::new(bat_n.x, 60.0),
                    bat_n,
                ],
            ),
            // bat- → GND
            lseg(13, vec![bat_n, gnd_p]),
        ];

        let result = solve_dc(&[bat, r1, r2, gnd], &wires);
        assert!(result.is_some(), "Voltage divider should converge");
        let dc = result.unwrap();

        // V(R2) = 9V * R2/(R1+R2) = 9 * 1/3 = 3V
        let r2_v = dc.component_voltage.get(&3).copied().unwrap_or(-99.0);
        assert!(
            (r2_v - 3.0).abs() < 0.2,
            "Expected R2 voltage ≈ 3V, got {r2_v:.3}V"
        );
    }

    #[test]
    fn current_source_into_one_kilohm_sets_ohms_law_voltage() {
        let src = comp(1, ComponentKind::ISource, Pos2::new(0.0, 0.0), "I1", "10mA");
        let resistor = comp(
            2,
            ComponentKind::Resistor,
            Pos2::new(200.0, 0.0),
            "R1",
            "1k",
        );
        let src_pins = component_pin_defs(&src);
        let resistor_pins = component_pin_defs(&resistor);
        let src_p = src_pins.iter().find(|pin| pin.label == "+").unwrap().pos;
        let src_n = src_pins.iter().find(|pin| pin.label == "-").unwrap().pos;
        let r_a = resistor_pins
            .iter()
            .find(|pin| pin.label == "A")
            .unwrap()
            .pos;
        let r_b = resistor_pins
            .iter()
            .find(|pin| pin.label == "B")
            .unwrap()
            .pos;
        let wires = vec![
            lseg(10, vec![src_p, Pos2::new(src_p.x, -40.0), r_a]),
            lseg(11, vec![r_b, Pos2::new(r_b.x, 40.0), src_n]),
        ];

        let dc = solve_dc_detailed(&[src, resistor], &wires).unwrap();

        let voltage = dc.component_voltage[&2];
        let current = dc.branch_current[&2];
        let power = dc.component_power[&2];
        assert!(
            (voltage.abs() - 10.0).abs() < 1.0e-6,
            "expected 10V across 1k from 10mA source, got {voltage}V"
        );
        assert!(
            (current.abs() - 0.010).abs() < 1.0e-9,
            "expected 10mA through 1k, got {current}A"
        );
        assert!(
            (power - 0.100).abs() < 1.0e-6,
            "expected 100mW, got {power}W"
        );
    }

    #[test]
    fn capacitor_is_open_in_dc_operating_point() {
        let bat = comp(1, ComponentKind::Battery, Pos2::new(0.0, 0.0), "BAT1", "5V");
        let cap = comp(
            2,
            ComponentKind::Capacitor,
            Pos2::new(180.0, 0.0),
            "C1",
            "100nF",
        );
        let bat_pins = component_pin_defs(&bat);
        let cap_pins = component_pin_defs(&cap);
        let bat_p = bat_pins.iter().find(|pin| pin.label == "+").unwrap().pos;
        let bat_n = bat_pins.iter().find(|pin| pin.label == "-").unwrap().pos;
        let c_a = cap_pins.iter().find(|pin| pin.label == "A").unwrap().pos;
        let c_b = cap_pins.iter().find(|pin| pin.label == "B").unwrap().pos;
        let wires = vec![
            lseg(10, vec![bat_p, Pos2::new(bat_p.x, -40.0), c_a]),
            lseg(11, vec![c_b, Pos2::new(c_b.x, 40.0), bat_n]),
        ];

        let dc = solve_dc_detailed(&[bat, cap], &wires).unwrap();

        assert!(dc.branch_current.get(&2).is_none());
        assert!(dc.branch_current[&1].abs() < 1.0e-12);
    }

    #[test]
    fn inductor_is_short_in_dc_operating_point() {
        let bat = comp(1, ComponentKind::Battery, Pos2::new(0.0, 0.0), "BAT1", "5V");
        let inductor = comp(
            2,
            ComponentKind::Inductor,
            Pos2::new(160.0, -80.0),
            "L1",
            "10uH",
        );
        let resistor = comp(
            3,
            ComponentKind::Resistor,
            Pos2::new(320.0, -80.0),
            "R1",
            "1k",
        );
        let bat_pins = component_pin_defs(&bat);
        let ind_pins = component_pin_defs(&inductor);
        let resistor_pins = component_pin_defs(&resistor);
        let bat_p = bat_pins.iter().find(|pin| pin.label == "+").unwrap().pos;
        let bat_n = bat_pins.iter().find(|pin| pin.label == "-").unwrap().pos;
        let l_a = ind_pins.iter().find(|pin| pin.label == "A").unwrap().pos;
        let l_b = ind_pins.iter().find(|pin| pin.label == "B").unwrap().pos;
        let r_a = resistor_pins
            .iter()
            .find(|pin| pin.label == "A")
            .unwrap()
            .pos;
        let r_b = resistor_pins
            .iter()
            .find(|pin| pin.label == "B")
            .unwrap()
            .pos;
        let wires = vec![
            lseg(10, vec![bat_p, Pos2::new(bat_p.x, -140.0), l_a]),
            lseg(11, vec![l_b, r_a]),
            lseg(12, vec![r_b, Pos2::new(r_b.x, 60.0), bat_n]),
        ];

        let dc = solve_dc_detailed(&[bat, inductor, resistor], &wires).unwrap();

        assert!((dc.branch_current[&3].abs() - 0.005).abs() < 1.0e-9);
        assert!((dc.component_voltage[&3].abs() - 5.0).abs() < 1.0e-9);
    }

    #[test]
    fn conflicting_parallel_voltage_sources_are_reported() {
        let bat1 = comp(1, ComponentKind::Battery, Pos2::new(0.0, 0.0), "BAT1", "5V");
        let bat2 = comp(
            2,
            ComponentKind::Battery,
            Pos2::new(180.0, 0.0),
            "BAT2",
            "9V",
        );
        let b1_pins = component_pin_defs(&bat1);
        let b2_pins = component_pin_defs(&bat2);
        let b1_p = b1_pins.iter().find(|pin| pin.label == "+").unwrap().pos;
        let b1_n = b1_pins.iter().find(|pin| pin.label == "-").unwrap().pos;
        let b2_p = b2_pins.iter().find(|pin| pin.label == "+").unwrap().pos;
        let b2_n = b2_pins.iter().find(|pin| pin.label == "-").unwrap().pos;
        let wires = vec![lseg(10, vec![b1_p, b2_p]), lseg(11, vec![b1_n, b2_n])];

        assert!(matches!(
            solve_dc_detailed(&[bat1, bat2], &wires),
            Err(SimulationError::VoltageSourceConflict)
        ));
    }

    // ── SI value parser ───────────────────────────────────────────────────

    #[test]
    fn parse_si_value_handles_common_cases() {
        assert!((parse_si_value("10k").unwrap() - 10_000.0).abs() < 0.1);
        assert!((parse_si_value("1K").unwrap() - 1_000.0).abs() < 0.1);
        assert!((parse_si_value("10kΩ").unwrap() - 10_000.0).abs() < 0.1);
        assert!((parse_si_value("4.7k").unwrap() - 4_700.0).abs() < 0.1);
        // SPICE-compatible: bare M means milli; use Meg for mega.
        assert!((parse_si_value("1M").unwrap() - 0.001).abs() < 1e-12);
        assert!((parse_si_value("100nF").unwrap() - 100e-9).abs() < 1e-12);
        assert!((parse_si_value("100u").unwrap() - 100e-6).abs() < 1e-12);
        assert!((parse_si_value("100µ").unwrap() - 100e-6).abs() < 1e-12);
        assert!((parse_si_value("100μ").unwrap() - 100e-6).abs() < 1e-12);
        assert!((parse_si_value("10uF").unwrap() - 10e-6).abs() < 1e-12);
        assert!((parse_si_value("3.3V").unwrap() - 3.3).abs() < 0.001);
        assert!((parse_si_value("1Meg").unwrap() - 1_000_000.0).abs() < 1.0);
        assert!((parse_si_value("10mA").unwrap() - 0.01).abs() < 0.0001);
        assert!((parse_si_value("20mA").unwrap() - 0.02).abs() < 0.0001);
        assert!(parse_si_value("").is_none());
        assert!(parse_si_value("abc").is_none());
    }

    #[test]
    fn detailed_solver_reports_missing_ground() {
        let resistor = comp(
            1,
            ComponentKind::Resistor,
            Pos2::new(100.0, 100.0),
            "R1",
            "1k",
        );
        assert!(matches!(
            solve_dc_detailed(&[resistor], &[]),
            Err(SimulationError::NoGround)
        ));
    }

    #[test]
    fn solved_resistor_obeys_kcl_and_power_roles() {
        let bat = comp(1, ComponentKind::Battery, Pos2::new(0.0, 0.0), "BAT1", "5V");
        let resistor = comp(
            2,
            ComponentKind::Resistor,
            Pos2::new(200.0, 0.0),
            "R1",
            "1k",
        );
        let bat_pins = component_pin_defs(&bat);
        let resistor_pins = component_pin_defs(&resistor);
        let bat_p = bat_pins.iter().find(|pin| pin.label == "+").unwrap().pos;
        let bat_n = bat_pins.iter().find(|pin| pin.label == "-").unwrap().pos;
        let r_a = resistor_pins
            .iter()
            .find(|pin| pin.label == "A")
            .unwrap()
            .pos;
        let r_b = resistor_pins
            .iter()
            .find(|pin| pin.label == "B")
            .unwrap()
            .pos;
        let wires = vec![
            lseg(
                10,
                vec![
                    bat_p,
                    Pos2::new(bat_p.x, -40.0),
                    Pos2::new(r_a.x, -40.0),
                    r_a,
                ],
            ),
            lseg(
                11,
                vec![r_b, Pos2::new(r_b.x, 40.0), Pos2::new(bat_n.x, 40.0), bat_n],
            ),
        ];
        let dc = solve_dc_detailed(&[bat, resistor], &wires).unwrap();
        assert!(dc.max_kcl_residual < 1e-12);
        assert_eq!(dc.component_power_role[&2], ComponentPowerRole::Dissipating);
        assert_eq!(dc.component_power_role[&1], ComponentPowerRole::Supplying);
    }
}
