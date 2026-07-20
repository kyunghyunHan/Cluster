//! DC operating-point solver.
//!
//! **Educational accuracy only** — uses linearised companion models and
//! Gaussian elimination.  For production accuracy use the ngspice backend.
//!
//! # Current visualisation contract
//! - `branch_current` is authoritative for each component.
//! - `wire_current` is only valid when `wire_current_known` contains that wire.
//! - Wires at T-junctions carry different currents on each segment; displaying
//!   a single value there is physically misleading, so only unambiguous wires
//!   appear in `wire_current_known`.

use std::collections::{HashMap, HashSet};

use egui::Pos2;

use crate::{
    CanonicalConnectivity, CircuitPin, Component, ComponentKind, PinRole, Wire, WireSegmentId,
    component_pin_defs, parse_metric_value, point_touches_wire_segment,
};

use super::errors::{ComponentPowerRole, SimulationError};
use super::matrix::{Mna, validate_voltage_sources};
use super::models::{BjtEntry, DiodeEntry, IsEntry, MosEntry, NetMap, ResEntry, VsEntry};

// ── Public result type ────────────────────────────────────────────────────────

#[allow(dead_code)] // Graph aliases are retained for result-format compatibility.
#[derive(Default, Clone, Debug)]
pub struct DcResult {
    /// net_root → voltage in Volts (GND net = 0.0 V)
    pub net_voltages: HashMap<usize, f64>,
    /// NetId → voltage in Volts. New graph-oriented alias for `net_voltages`.
    pub node_voltages: HashMap<usize, f64>,
    /// component_id → voltage across the component (V_a − V_b)
    pub component_voltage: HashMap<u64, f64>,
    /// component_id → conventional current through the component (A).
    ///
    /// **This is the authoritative current source for visualisation.**
    pub branch_current: HashMap<u64, f64>,
    /// ComponentBranchId/component_id → conventional current through the component (A).
    pub component_currents: HashMap<u64, f64>,
    /// component_id → power dissipated (W, always ≥ 0)
    pub component_power: HashMap<u64, f64>,
    /// wire_id → net voltage (V)
    pub wire_voltage: HashMap<u64, f64>,
    /// wire_id → net root index
    pub wire_net_root: HashMap<u64, usize>,
    /// wire_id → conventional current along the wire (A).
    ///
    /// **Only valid when `wire_current_known` contains this wire ID.**
    pub wire_current: HashMap<u64, f64>,
    /// Wires whose single displayed current is unambiguous (no mid-wire branch).
    pub wire_current_known: HashSet<u64>,
    /// WireSegmentId → conventional current through that solved segment (A).
    pub wire_segment_currents: HashMap<WireSegmentId, f64>,
    /// component_id → power role
    pub component_power_role: HashMap<u64, ComponentPowerRole>,
    /// Maximum absolute KCL residual across solved non-ground nodes (A)
    pub max_kcl_residual: f64,
    /// Maximum absolute voltage seen anywhere in the circuit (for display scaling)
    pub vmax: f64,
    /// Number of nonlinear state iterations used for diode/LED/MOSFET companion states.
    pub nonlinear_iterations: usize,
    /// True when piecewise-linear device states stopped changing before the iteration limit.
    pub nonlinear_converged: bool,
}

// ── Entry points ─────────────────────────────────────────────────────────────

/// Attempt a DC operating-point solve.
/// Returns `None` when the circuit has no GND, is open, or the matrix is
/// singular (floating sub-network, etc.).
#[cfg(test)]
pub fn solve_dc(components: &[Component], wires: &[Wire]) -> Option<DcResult> {
    solve_dc_detailed(components, wires).ok()
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn solve_dc_detailed(
    components: &[Component],
    wires: &[Wire],
) -> Result<DcResult, SimulationError> {
    let connectivity = crate::engine::netlist::build_canonical_connectivity(components, wires);
    solve_dc_detailed_with_connectivity(components, wires, &connectivity)
}

pub fn solve_dc_detailed_with_connectivity(
    components: &[Component],
    wires: &[Wire],
    connectivity: &CanonicalConnectivity,
) -> Result<DcResult, SimulationError> {
    solve_dc_detailed_with_connectivity_and_cancellation(components, wires, connectivity, None)
}

pub(crate) fn solve_dc_detailed_with_cancellation(
    components: &[Component],
    wires: &[Wire],
    cancellation: &crate::engine::ngspice::CancellationToken,
) -> Result<DcResult, SimulationError> {
    let connectivity = crate::engine::netlist::build_canonical_connectivity(components, wires);
    solve_dc_detailed_with_connectivity_and_cancellation(
        components,
        wires,
        &connectivity,
        Some(cancellation),
    )
}

pub(crate) fn solve_dc_detailed_with_connectivity_and_cancellation(
    components: &[Component],
    wires: &[Wire],
    connectivity: &CanonicalConnectivity,
    cancellation: Option<&crate::engine::ngspice::CancellationToken>,
) -> Result<DcResult, SimulationError> {
    // ── 1. Build net map ──────────────────────────────────────────────────
    let mut nm = NetMap::from_connectivity(components, wires, connectivity);

    for comp in components {
        if cancellation.is_some_and(|token| token.is_cancelled()) {
            return Err(SimulationError::Cancelled);
        }
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
    // ── 2. Identify GND roots ─────────────────────────────────────────────
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

    // ── 3. Assign MNA node numbers (GND=0, others 1..N) ──────────────────
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
    let num_nodes = next_node - 1;

    let mna_node = |pos: Pos2, nm: &mut NetMap, mna_of: &HashMap<usize, usize>| -> Option<usize> {
        let root = nm.root_of(pos)?;
        mna_of.get(&root).copied()
    };

    // ── 4. Build component entries ────────────────────────────────────────
    let mut res: Vec<ResEntry> = Vec::new();
    let mut vs: Vec<VsEntry> = Vec::new();
    let mut is_src: Vec<IsEntry> = Vec::new();
    let mut diode_entries: Vec<DiodeEntry> = Vec::new();
    let mut bjt_entries: Vec<BjtEntry> = Vec::new();
    let mut mos_entries: Vec<MosEntry> = Vec::new();

    for comp in components {
        let pins = component_pin_defs(comp);
        let p0 = pins.first().and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
        let p1 = pins.get(1).and_then(|p| mna_node(p.pos, &mut nm, &mna_of));

        match comp.kind {
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
                let rated_v = parse_metric_value(&comp.value, "v").unwrap_or(12.0) as f64;
                let r = (rated_v * rated_v) / 40.0;
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
                let in_n = pins
                    .iter()
                    .find(|p| p.label == "IN")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                let out_n = pins
                    .iter()
                    .find(|p| p.label == "OUT")
                    .and_then(|p| mna_node(p.pos, &mut nm, &mna_of));
                if let (Some(a), Some(b)) = (in_n, out_n) {
                    res.push(ResEntry {
                        id: comp.id,
                        a,
                        b,
                        r: 0.5,
                    });
                }
            }
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
            ComponentKind::Diode => {
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
                let vz = parse_metric_value(&comp.value, "v").unwrap_or(5.1) as f64;
                if let (Some(a), Some(k)) = (p0, p1) {
                    diode_entries.push(DiodeEntry {
                        id: comp.id,
                        anode: k,
                        cathode: a,
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
            // Components not modelled in DC MNA — ignored (open circuit).
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
            | ComponentKind::Stm32BluePill
            | ComponentKind::Stm32Nucleo64
            | ComponentKind::NetLabel
            | ComponentKind::Timer555
            | ComponentKind::Crystal
            | ComponentKind::Transformer
            | ComponentKind::Display7Seg
            | ComponentKind::VoltageRef
            | ComponentKind::MotorDriver
            | ComponentKind::Optocoupler
            | ComponentKind::GenericIc
            | ComponentKind::TextNote
            | ComponentKind::Dht11
            | ComponentKind::Dht22
            | ComponentKind::HcSr04
            | ComponentKind::Servo
            | ComponentKind::Oled
            | ComponentKind::Sensor
            | ComponentKind::NeoPixel
            | ComponentKind::PirSensor
            | ComponentKind::Custom => {}
        }
    }

    // ── 5b. Expand BJTs: VBE diode + CCCS for Ic = hFE·Ib ───────────────
    let bjt_start_node = num_nodes + 1;
    let bjt_vs_start = vs.len();
    for (bi, bjt) in bjt_entries.iter().enumerate() {
        let mid = bjt_start_node + bi;
        if bjt.npn {
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

    // ── 6. Build MNA matrix ───────────────────────────────────────────────
    let m = vs.len();
    if total_nodes == 0 {
        return Err(SimulationError::FloatingNode);
    }
    let solve_with_states = |diode_on: &[bool],
                             mos_on: &[bool]|
     -> Result<super::matrix::SolveSolution, SimulationError> {
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
        mat.solve_with_cancellation(cancellation)
    };

    let mut diode_states = vec![false; diode_entries.len()];
    let mut mos_states = vec![false; mos_entries.len()];
    let mut solution = solve_with_states(&diode_states, &mos_states)?;
    let has_iterated_nonlinear_devices = !diode_entries.is_empty() || !mos_entries.is_empty();
    let mut nonlinear_iterations = 0usize;
    let mut nonlinear_converged = !has_iterated_nonlinear_devices;

    if has_iterated_nonlinear_devices {
        for iteration in 1..=24 {
            if cancellation.is_some_and(|token| token.is_cancelled()) {
                return Err(SimulationError::Cancelled);
            }
            let previous_x = solution.x.clone();
            let voltage = |mna_idx: usize| -> f64 {
                if mna_idx == 0 {
                    0.0
                } else {
                    previous_x.get(mna_idx - 1).copied().unwrap_or(0.0)
                }
            };
            let next_diode_states = diode_entries
                .iter()
                .enumerate()
                .map(|(index, d)| {
                    let vd = voltage(d.anode) - voltage(d.cathode);
                    if vd >= d.vf {
                        true
                    } else if vd <= d.vf - 0.025 {
                        false
                    } else {
                        diode_states.get(index).copied().unwrap_or(false)
                    }
                })
                .collect::<Vec<_>>();
            let next_mos_states = mos_entries
                .iter()
                .enumerate()
                .map(|(index, mos)| {
                    let drive = if mos.pmos {
                        voltage(mos.source) - voltage(mos.gate)
                    } else {
                        voltage(mos.gate) - voltage(mos.source)
                    };
                    if drive >= mos.vth {
                        true
                    } else if drive <= mos.vth - 0.050 {
                        false
                    } else {
                        mos_states.get(index).copied().unwrap_or(false)
                    }
                })
                .collect::<Vec<_>>();

            nonlinear_iterations = iteration;
            if next_diode_states == diode_states && next_mos_states == mos_states {
                nonlinear_converged = true;
                break;
            }
            diode_states = next_diode_states;
            mos_states = next_mos_states;
            solution = solve_with_states(&diode_states, &mos_states)?;
        }
    }
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

    for de in &diode_entries {
        let va = vnode(de.anode);
        let vk = vnode(de.cathode);
        let vd = va - vk;
        component_voltage.insert(de.id, vd);
        let index = diode_entries
            .iter()
            .position(|c| c.id == de.id)
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

    for (bi, bjt) in bjt_entries.iter().enumerate() {
        let k_vbe = bjt_vs_start + bi;
        let ib = x.get(total_nodes + k_vbe).copied().unwrap_or(0.0);
        let ic = bjt.h_fe * ib;
        let vce = vnode(bjt.c) - vnode(bjt.e);
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

    let mut wire_voltage: HashMap<u64, f64> = HashMap::new();
    let mut wire_net_root: HashMap<u64, usize> = HashMap::new();
    for wire in wires {
        let wire_root = wire.points.first().and_then(|&pt| nm.root_of(pt));
        if let Some(root) = wire_root {
            wire_net_root.insert(wire.id, root);
            if let Some(&mna_idx) = mna_of.get(&root) {
                wire_voltage.insert(wire.id, vnode(mna_idx));
            }
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
                    .any(|seg| point_touches_wire_segment(pin.pos, seg[0], seg[1]))
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
                candidates.push(terminal_current * toward_pin_sign);
            }
        }
        let representative = candidates
            .iter()
            .copied()
            .max_by(|a, b| a.abs().total_cmp(&b.abs()))
            .unwrap_or(0.0);
        wire_current.insert(wire.id, representative);
        if !candidates.is_empty() {
            let tolerance = representative.abs().max(1.0) * 1.0e-9;
            if candidates
                .iter()
                .all(|c| (*c - representative).abs() <= tolerance)
            {
                wire_current_known.insert(wire.id);
            }
        }
    }

    let mut wire_segment_currents = HashMap::new();
    for wire in wires {
        if wire_current_known.contains(&wire.id)
            && let Some(&current) = wire_current.get(&wire.id)
        {
            for (segment_index, _) in wire.points.windows(2).enumerate() {
                let segment_id = WireSegmentId::new(wire.id, segment_index);
                wire_segment_currents.insert(segment_id, current);
            }
        }
    }

    let vmax = net_voltages
        .values()
        .map(|v| v.abs())
        .fold(0.0_f64, f64::max);

    Ok(DcResult {
        node_voltages: net_voltages.clone(),
        net_voltages,
        component_voltage,
        component_currents: branch_current.clone(),
        branch_current,
        component_power,
        wire_voltage,
        wire_net_root,
        wire_current,
        wire_current_known,
        wire_segment_currents,
        component_power_role,
        max_kcl_residual: solution.max_kcl_residual,
        vmax: vmax.max(0.1),
        nonlinear_iterations,
        nonlinear_converged,
    })
}

// ── Wire geometry helpers ─────────────────────────────────────────────────────

pub(super) fn terminal_current_into_component(
    kind: ComponentKind,
    pin: &CircuitPin,
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

pub(super) fn wire_polyline_length(points: &[Pos2]) -> f32 {
    points.windows(2).map(|s| s[0].distance(s[1])).sum()
}

pub(super) fn distance_along_wire(points: &[Pos2], point: Pos2) -> Option<f32> {
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
        if best.is_none_or(|(bd, _)| distance < bd) {
            best = Some((distance, along));
        }
        traveled += a.distance(b);
    }
    best.filter(|(d, _)| *d <= 1.0).map(|(_, along)| along)
}
