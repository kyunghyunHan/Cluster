//! Net-map builder and component-entry structs used when constructing the MNA.

use egui::Pos2;

use crate::{CircuitNodes, UnionFind};

// ── Net assignment (position → net_root index) ────────────────────────────────

pub(super) struct NetMap {
    pub(super) nodes: CircuitNodes,
    pub(super) uf: UnionFind,
}

impl NetMap {
    pub(super) fn new() -> Self {
        NetMap {
            nodes: CircuitNodes::default(),
            uf: UnionFind::default(),
        }
    }

    pub(super) fn reg(&mut self, pos: Pos2) -> usize {
        let idx = self.nodes.node_for(pos);
        self.uf.ensure(idx);
        idx
    }

    pub(super) fn join(&mut self, a: usize, b: usize) {
        self.uf.union(a, b);
    }

    pub(super) fn root_of(&mut self, pos: Pos2) -> Option<usize> {
        let idx = self.nodes.find_existing(pos)?;
        Some(self.uf.find(idx))
    }

    pub(super) fn root_of_idx(&mut self, idx: usize) -> usize {
        self.uf.find(idx)
    }
}

// ── Component entries ─────────────────────────────────────────────────────────

pub(super) struct ResEntry {
    pub(super) id: u64,
    pub(super) a: usize,
    pub(super) b: usize,
    pub(super) r: f64,
}

pub(super) struct VsEntry {
    pub(super) id: u64,
    pub(super) pos: usize,
    pub(super) neg: usize,
    pub(super) v: f64,
}

pub(super) struct IsEntry {
    pub(super) id: u64,
    pub(super) pos: usize,
    pub(super) neg: usize,
    pub(super) i: f64,
}

/// Diode companion model: Vf (voltage source) in series with Rb (bulk resistance).
pub(super) struct DiodeEntry {
    pub(super) id: u64,
    pub(super) anode: usize,
    pub(super) cathode: usize,
    /// Forward voltage drop (V)
    pub(super) vf: f64,
    /// Bulk series resistance (Ω)
    pub(super) rb: f64,
}

pub(super) struct MosEntry {
    pub(super) id: u64,
    pub(super) gate: usize,
    pub(super) drain: usize,
    pub(super) source: usize,
    pub(super) pmos: bool,
    pub(super) vth: f64,
    pub(super) r_on: f64,
    pub(super) r_off: f64,
}

/// BJT companion model: linearised VBE diode + CCCS for Ic = hFE·Ib.
pub(super) struct BjtEntry {
    pub(super) id: u64,
    pub(super) b: usize,
    pub(super) c: usize,
    pub(super) e: usize,
    /// Base-emitter forward voltage (V)
    pub(super) vbe: f64,
    /// Base-emitter bulk resistance (Ω)
    pub(super) rb_be: f64,
    /// DC current gain
    pub(super) h_fe: f64,
    /// true = NPN (Vbe: B+, E-), false = PNP (Vbe: E+, B-)
    pub(super) npn: bool,
}
