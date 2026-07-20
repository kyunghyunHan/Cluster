use crate::engine::mna;
use crate::engine::transient::TransientResult;
use crate::engine::validation::ErcViolation;
use crate::model::{Component, Wire};
use std::collections::{HashMap, HashSet};

#[derive(Default, Clone)]
pub(crate) struct Simulation {
    pub(crate) status: SimulationStatus,
    pub(crate) closed: bool,
    pub(crate) shorted: bool,
    pub(crate) energized_components: HashSet<u64>,
    pub(crate) energized_wires: HashSet<u64>,
    pub(crate) summary: String,
    pub(crate) explanation: String,
    #[allow(dead_code)]
    pub(crate) details: Vec<String>,
    pub(crate) voltage: Option<f32>,
    pub(crate) resistance: Option<f32>,
    pub(crate) current: Option<f32>,
    pub(crate) component_warnings: HashMap<u64, String>,
    pub(crate) dc: Option<mna::DcResult>,
    pub(crate) dc_error: Option<mna::SimulationError>,
    pub(crate) ac: Option<mna::AcResult>,
    pub(crate) transient: Option<TransientResult>,
    pub(crate) erc: Vec<ErcViolation>,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SimulationStatus {
    Ok,
    #[default]
    Warning,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Conductance {
    Open,
    Conductor,
    Load,
}

#[allow(dead_code)] // Compatibility facade; runtime passes cached connectivity explicitly.
pub(crate) fn analyze_circuit(components: &[Component], wires: &[Wire]) -> Simulation {
    crate::analyze_circuit(components, wires)
}

#[allow(dead_code)]
pub(crate) fn analyze_circuit_with_connectivity(
    components: &[Component],
    wires: &[Wire],
    connectivity: &crate::model::CanonicalConnectivity,
) -> Simulation {
    crate::ui::app::analyze_circuit_with_connectivity(components, wires, connectivity)
}
