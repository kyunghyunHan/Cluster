use crate::engine::mna;
use crate::engine::validation::ErcViolation;
use crate::model::{Component, Wire};
use std::collections::{HashMap, HashSet};

#[derive(Default, Clone)]
pub(crate) struct Simulation {
    pub(crate) closed: bool,
    pub(crate) shorted: bool,
    pub(crate) energized_components: HashSet<u64>,
    pub(crate) energized_wires: HashSet<u64>,
    pub(crate) summary: String,
    #[allow(dead_code)]
    pub(crate) details: Vec<String>,
    pub(crate) voltage: Option<f32>,
    pub(crate) resistance: Option<f32>,
    pub(crate) current: Option<f32>,
    pub(crate) component_warnings: HashMap<u64, String>,
    pub(crate) dc: Option<mna::DcResult>,
    pub(crate) erc: Vec<ErcViolation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Conductance {
    Open,
    Conductor,
    Load,
}

pub(crate) fn analyze_circuit(components: &[Component], wires: &[Wire]) -> Simulation {
    crate::analyze_circuit(components, wires)
}
