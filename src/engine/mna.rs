//! Modified Nodal Analysis – DC operating-point solver.
#![allow(dead_code)]
//!
//! Builds the MNA matrix from the schematic, solves with Gaussian elimination
//! (partial pivoting), and returns node voltages + branch currents.
//! Nonlinear elements (diodes, transistors) use a single-iteration linearised
//! companion model adequate for educational / first-pass analysis.

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
    /// Maximum absolute voltage seen anywhere in the circuit (for display scaling)
    pub vmax: f64,
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

    /// Solve A·x = z with Gaussian elimination + partial pivoting.
    fn solve(self) -> Option<Vec<f64>> {
        let sz = self.n + self.m;
        if sz == 0 {
            return Some(vec![]);
        }
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
                return None; // singular
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
                return None;
            }
            x[i] /= a[i][i];
        }
        Some(x)
    }
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

// ─────────────────────────────────────────────────────────────────────────────
//  Main entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Attempt a DC operating-point solve on the circuit.
/// Returns `None` when the circuit has no GND, is open, or the matrix is
/// singular (floating sub-network, etc.).
pub fn solve_dc(components: &[Component], wires: &[Wire]) -> Option<DcResult> {
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
        return None;
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
            // ── Transistors (linearised as resistors when in active region) ──
            ComponentKind::NpnTransistor | ComponentKind::PnpTransistor => {
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
                        r: 50.0,
                    });
                }
            }
            ComponentKind::Nmosfet | ComponentKind::Pmosfet => {
                let d_n = pins
                    .iter()
                    .find(|p| p.label == "D")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                let s_n = pins
                    .iter()
                    .find(|p| p.label == "S")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                if let (Some(d), Some(s)) = (d_n, s_n) {
                    res.push(ResEntry {
                        id: comp.id,
                        a: d,
                        b: s,
                        r: 20.0,
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
                if pos_n != neg_n {
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
                // Forward mode only (simplified): Vf = 0.65 V, Rb = 5 Ω
                let _vz = parse_metric_value(&comp.value, "v").unwrap_or(5.1) as f64;
                if let (Some(a), Some(k)) = (p0, p1) {
                    diode_entries.push(DiodeEntry {
                        id: comp.id,
                        anode: a,
                        cathode: k,
                        vf: 0.65,
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
            ComponentKind::Timer555 => {
                let vcc_n = pins
                    .iter()
                    .find(|p| p.label == "VCC")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                let gnd_n = pins
                    .iter()
                    .find(|p| p.label == "GND" || p.role == PinRole::Ground)
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                if let (Some(v), Some(g)) = (vcc_n, gnd_n) {
                    res.push(ResEntry {
                        id: comp.id,
                        a: v,
                        b: g,
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
            | ComponentKind::GenericIc => {}
        }
    }

    // ── 5. Expand diodes into series (Vf + Rb) with virtual intermediate nodes ──
    // Each diode needs one intermediate node (num_nodes + diode_index).
    // Virtual nodes are appended after the normal non-GND nodes.
    let diode_start_node = num_nodes + 1; // 1-indexed virtual node for diode 0
    for (di, de) in diode_entries.iter().enumerate() {
        let mid = diode_start_node + di; // MNA node for anode side of Rb
        // anode → [Vf source] → mid → [Rb] → cathode
        vs.push(VsEntry {
            id: de.id,
            pos: de.anode,
            neg: mid,
            v: de.vf,
        });
        res.push(ResEntry {
            id: de.id,
            a: mid,
            b: de.cathode,
            r: de.rb,
        });
    }
    let total_nodes = num_nodes + diode_entries.len(); // total non-GND nodes incl. virtual

    // ── 6. Build MNA matrix ──────────────────────────────────────────────
    let m = vs.len();
    if total_nodes == 0 {
        return None;
    }
    let mut mat = Mna::new(total_nodes, m);

    for re in &res {
        mat.stamp_r(re.a, re.b, re.r);
    }
    for (k, v_src) in vs.iter().enumerate() {
        mat.stamp_vs(k, v_src.pos, v_src.neg, v_src.v);
    }
    for i_src in &is_src {
        mat.stamp_is(i_src.pos, i_src.neg, i_src.i);
    }

    // ── 6. Solve ─────────────────────────────────────────────────────────
    let x = mat.solve()?;

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

    for re in &res {
        let va = vnode(re.a);
        let vb = vnode(re.b);
        let vd = va - vb;
        let i_r = vd / re.r;
        component_voltage.entry(re.id).or_insert(vd);
        branch_current.entry(re.id).or_insert(i_r);
        component_power.entry(re.id).or_insert((vd * i_r).abs());
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
    }

    // For diodes specifically: override with the anode→cathode total voltage drop
    for de in &diode_entries {
        let va = vnode(de.anode);
        let vk = vnode(de.cathode);
        let vd = va - vk;
        // current through diode = current through its VS (the k-th VS entry)
        // We need to find which VS entry corresponds to this diode.
        // VS entries for diodes were pushed starting at the original vs.len() position.
        // They're paired: VS at index (original_vs_count + di), R at the same di.
        // Since we use entry().or_insert above, the VS current is already stored.
        // Just update the voltage to the full anode-cathode drop:
        component_voltage.insert(de.id, vd);
        if let Some(i) = branch_current.get(&de.id) {
            component_power.insert(de.id, (vd * i).abs());
        }
    }

    for i_src in &is_src {
        let va = vnode(i_src.pos);
        let vb = vnode(i_src.neg);
        let vd = va - vb;
        component_voltage.insert(i_src.id, vd);
        branch_current.insert(i_src.id, i_src.i);
        component_power.insert(i_src.id, (vd * i_src.i).abs());
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

    let vmax = net_voltages
        .values()
        .map(|v| v.abs())
        .fold(0.0_f64, f64::max);

    Some(DcResult {
        net_voltages,
        component_voltage,
        branch_current,
        component_power,
        wire_voltage,
        vmax: vmax.max(0.1),
    })
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
