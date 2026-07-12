//! AC small-signal solver at a single frequency.
//!
//! Capacitors and inductors are modelled as complex admittances.
//! Resistors as real admittances. Voltage sources are ideal (zero impedance).
//! **Educational accuracy only** — accurate AC sweep requires ngspice.

use std::collections::{HashMap, HashSet};

use egui::Pos2;

use crate::{
    Component, ComponentKind, PinRole, Wire, component_pin_defs, parse_metric_value,
    point_touches_wire_segment, wire_contact_points,
};

use super::display::parse_si_value;

use super::models::NetMap;

#[allow(dead_code)] // Graph-oriented AC fields are part of the solver result contract.
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
#[allow(clippy::needless_range_loop)] // Gaussian elimination requires indexed columns.
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
        for nodes in label_nodes.values() {
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
                    if pin.role == PinRole::Ground
                        && let Some(r) = nm.root_of(pin.pos)
                    {
                        gnd_roots.insert(r);
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
            .first()
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
            if let Some(root) = nm.root_of(pt)
                && let Some(&mna_idx) = mna_of.get(&root)
            {
                let (re, im) = vnode_c(mna_idx);
                sum_re += re;
                sum_im += im;
                cnt += 1;
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
