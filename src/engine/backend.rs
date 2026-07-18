//! Simulation backend contract shared by the educational solver and ngspice.
#![allow(dead_code)] // Backend selection UI is introduced incrementally; both implementations are complete entry points.

use crate::engine::mna;
use crate::engine::ngspice::{
    CancellationToken, NgspiceConfig, NgspiceError, NgspiceResult, export_ngspice_netlist,
    run_ngspice_configured,
};
use crate::engine::transient::{TransientResult, solve_transient_with_netlist};
use crate::model::{CircuitNetlist, Component, Wire};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackendKind {
    InternalMna,
    NgSpice,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BackendDescriptor {
    pub(crate) kind: BackendKind,
    pub(crate) name: &'static str,
    pub(crate) accuracy: &'static str,
    pub(crate) runs_out_of_process: bool,
}

pub(crate) struct SimulationCircuit<'a> {
    pub(crate) components: &'a [Component],
    pub(crate) wires: &'a [Wire],
    pub(crate) netlist: &'a CircuitNetlist,
    pub(crate) document_revision: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TransientRequest {
    pub(crate) duration_s: f64,
    pub(crate) maximum_samples: usize,
}

pub(crate) enum OperatingPointResult {
    Internal(Box<mna::DcResult>),
    NgSpice(Box<NgspiceResult>),
}

#[derive(Debug)]
pub(crate) enum SimulationBackendError {
    Cancelled,
    Internal(mna::SimulationError),
    NgSpice(NgspiceError),
    UnsupportedAnalysis(&'static str),
}

pub(crate) trait SimulationBackend {
    fn descriptor(&self) -> BackendDescriptor;

    fn operating_point(
        &self,
        circuit: &SimulationCircuit<'_>,
        cancellation: &CancellationToken,
    ) -> Result<OperatingPointResult, SimulationBackendError>;

    fn transient(
        &self,
        circuit: &SimulationCircuit<'_>,
        request: &TransientRequest,
        cancellation: &CancellationToken,
    ) -> Result<TransientResult, SimulationBackendError>;
}

pub(crate) struct InternalMnaBackend;

impl SimulationBackend for InternalMnaBackend {
    fn descriptor(&self) -> BackendDescriptor {
        BackendDescriptor {
            kind: BackendKind::InternalMna,
            name: "Internal educational MNA",
            accuracy: "Simplified DC and narrow RC/PWM transient preview",
            runs_out_of_process: false,
        }
    }

    fn operating_point(
        &self,
        circuit: &SimulationCircuit<'_>,
        cancellation: &CancellationToken,
    ) -> Result<OperatingPointResult, SimulationBackendError> {
        if cancellation.is_cancelled() {
            return Err(SimulationBackendError::Cancelled);
        }
        mna::solve_dc_detailed(circuit.components, circuit.wires)
            .map(Box::new)
            .map(OperatingPointResult::Internal)
            .map_err(SimulationBackendError::Internal)
    }

    fn transient(
        &self,
        circuit: &SimulationCircuit<'_>,
        request: &TransientRequest,
        cancellation: &CancellationToken,
    ) -> Result<TransientResult, SimulationBackendError> {
        if cancellation.is_cancelled() {
            return Err(SimulationBackendError::Cancelled);
        }
        let _requested_bounds = (request.duration_s, request.maximum_samples);
        solve_transient_with_netlist(circuit.components, circuit.netlist).ok_or(
            SimulationBackendError::UnsupportedAnalysis(
                "Internal transient supports only one-R/one-C step or PWM circuits",
            ),
        )
    }
}

pub(crate) struct NgSpiceBackend {
    pub(crate) config: NgspiceConfig,
}

impl SimulationBackend for NgSpiceBackend {
    fn descriptor(&self) -> BackendDescriptor {
        BackendDescriptor {
            kind: BackendKind::NgSpice,
            name: "ngspice",
            accuracy: "External SPICE models; symbol-only modules remain unsupported",
            runs_out_of_process: true,
        }
    }

    fn operating_point(
        &self,
        circuit: &SimulationCircuit<'_>,
        cancellation: &CancellationToken,
    ) -> Result<OperatingPointResult, SimulationBackendError> {
        let (netlist, unsupported) =
            export_ngspice_netlist(circuit.components, circuit.wires, circuit.netlist);
        let mut result = run_ngspice_configured(
            &netlist,
            circuit.document_revision,
            &self.config,
            cancellation,
        )
        .map_err(SimulationBackendError::NgSpice)?;
        result.unsupported = unsupported;
        Ok(OperatingPointResult::NgSpice(Box::new(result)))
    }

    fn transient(
        &self,
        _circuit: &SimulationCircuit<'_>,
        _request: &TransientRequest,
        _cancellation: &CancellationToken,
    ) -> Result<TransientResult, SimulationBackendError> {
        Err(SimulationBackendError::UnsupportedAnalysis(
            "ngspice transient import is not connected yet",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_internal_request_never_solves() {
        let token = CancellationToken::default();
        token.cancel();
        let netlist = CircuitNetlist::default();
        let circuit = SimulationCircuit {
            components: &[],
            wires: &[],
            netlist: &netlist,
            document_revision: 7,
        };
        assert!(matches!(
            InternalMnaBackend.operating_point(&circuit, &token),
            Err(SimulationBackendError::Cancelled)
        ));
    }
}
