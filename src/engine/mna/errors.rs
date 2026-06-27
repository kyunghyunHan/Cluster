//! Simulation error types for the internal MNA solver.
//!
//! The internal solver is educational — it uses simplified linearised companion
//! models and Gaussian elimination.  For production-accurate results, use the
//! ngspice backend (see `engine::ngspice`).

/// Errors that can occur during a DC operating-point solve.
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
                "The DC equations cannot be solved, usually because a node is floating or only \
                 ideal parts constrain it."
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

/// Whether a component is actively dissipating or supplying power.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentPowerRole {
    Dissipating,
    Supplying,
    Unknown,
}
