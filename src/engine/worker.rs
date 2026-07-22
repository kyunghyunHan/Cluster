use crate::engine::ngspice::CancellationToken;
use crate::model::cad::CadNet;
use crate::model::{Component, NetlistAnnotations, SavedCircuit, Wire};
use crate::pcb::board::Board;
use crate::pcb::drc::{DrcViolation, run_drc_with_nets};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};

pub(crate) enum AnalysisRequest {
    Schematic {
        components: Vec<Component>,
        wires: Vec<Wire>,
        annotations: Box<NetlistAnnotations>,
        ac_frequency_hz: f64,
        backend: crate::engine::backend::BackendKind,
        revision_key: crate::ui::app::SimulationRevisionKey,
        ac_key: u32,
    },
    FullDrc {
        board: Box<Board>,
        nets: Vec<CadNet>,
    },
    Autosave {
        saved: Box<SavedCircuit>,
        path: PathBuf,
    },
}

pub(crate) enum AnalysisPayload {
    Schematic(Box<SchematicAnalysis>),
    FullDrc(Vec<DrcViolation>),
    Autosave(Result<PathBuf, String>),
}

pub(crate) struct SchematicAnalysis {
    pub(crate) connectivity: crate::model::CanonicalConnectivity,
    pub(crate) simulation: crate::engine::simulation::Simulation,
    pub(crate) connectivity_ms: f64,
    pub(crate) simulation_ms: f64,
    pub(crate) erc_ms: f64,
    pub(crate) revision_key: crate::ui::app::SimulationRevisionKey,
    pub(crate) ac_key: u32,
}

pub(crate) struct AnalysisResult {
    pub(crate) document_revision: u64,
    pub(crate) payload: AnalysisPayload,
}

struct Job {
    document_revision: u64,
    request: AnalysisRequest,
    cancellation: CancellationToken,
}

#[derive(Default)]
struct WorkerCache {
    erc_topology_revision: Option<u64>,
    erc_topology: Vec<crate::engine::validation::ErcViolation>,
}

pub(crate) struct BoundedAnalysisWorker {
    jobs: SyncSender<Job>,
    results: Receiver<AnalysisResult>,
    schematic: CancellationToken,
    drc: CancellationToken,
    autosave: CancellationToken,
    pending_autosave: Option<Job>,
    failure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorkerSubmitError {
    QueueFull,
    Disconnected,
    StartupFailed(String),
}

impl BoundedAnalysisWorker {
    pub(crate) fn new() -> Self {
        let (jobs_tx, jobs_rx) = sync_channel::<Job>(2);
        let (results_tx, results_rx) = sync_channel::<AnalysisResult>(2);
        let worker_thread = std::thread::Builder::new()
            .name("cluster-analysis".to_string())
            .spawn(move || {
                let mut cache = WorkerCache::default();
                while let Ok(job) = jobs_rx.recv() {
                    if job.cancellation.is_cancelled() {
                        continue;
                    }
                    let payload = execute(job.request, &job.cancellation, &mut cache);
                    if !job.cancellation.is_cancelled() {
                        let _ = results_tx.send(AnalysisResult {
                            document_revision: job.document_revision,
                            payload,
                        });
                    }
                }
            });
        let failure = worker_thread
            .err()
            .map(|error| format!("Analysis worker failed to start: {error}"));
        Self {
            jobs: jobs_tx,
            results: results_rx,
            schematic: CancellationToken::default(),
            drc: CancellationToken::default(),
            autosave: CancellationToken::default(),
            pending_autosave: None,
            failure,
        }
    }

    pub(crate) fn submit(
        &mut self,
        document_revision: u64,
        request: AnalysisRequest,
    ) -> Result<(), WorkerSubmitError> {
        self.flush_pending_autosave();
        if let Some(error) = &self.failure {
            return Err(WorkerSubmitError::StartupFailed(error.clone()));
        }
        let active = match &request {
            AnalysisRequest::Schematic { .. } => &mut self.schematic,
            AnalysisRequest::FullDrc { .. } => &mut self.drc,
            AnalysisRequest::Autosave { .. } => &mut self.autosave,
        };
        active.cancel();
        let cancellation = CancellationToken::default();
        match self.jobs.try_send(Job {
            document_revision,
            request,
            cancellation: cancellation.clone(),
        }) {
            Ok(()) => {
                *active = cancellation;
                Ok(())
            }
            Err(TrySendError::Full(job))
                if matches!(&job.request, AnalysisRequest::Autosave { .. }) =>
            {
                *active = cancellation;
                self.pending_autosave = Some(job);
                Ok(())
            }
            Err(TrySendError::Full(_)) => Err(WorkerSubmitError::QueueFull),
            Err(TrySendError::Disconnected(_)) => Err(WorkerSubmitError::Disconnected),
        }
    }

    pub(crate) fn try_recv(&mut self) -> Option<AnalysisResult> {
        self.flush_pending_autosave();
        self.results.try_recv().ok()
    }

    pub(crate) fn take_failure(&mut self) -> Option<String> {
        self.failure.take()
    }

    fn flush_pending_autosave(&mut self) {
        let Some(job) = self.pending_autosave.take() else {
            return;
        };
        if job.cancellation.is_cancelled() {
            return;
        }
        match self.jobs.try_send(job) {
            Ok(()) => {}
            Err(TrySendError::Full(job)) => self.pending_autosave = Some(job),
            Err(TrySendError::Disconnected(job)) => {
                self.pending_autosave = Some(job);
                self.failure = Some(
                    "Analysis worker disconnected; pending autosave was retained.".to_string(),
                );
            }
        }
    }
}

fn execute(
    request: AnalysisRequest,
    cancellation: &CancellationToken,
    cache: &mut WorkerCache,
) -> AnalysisPayload {
    match request {
        AnalysisRequest::Schematic {
            components,
            wires,
            annotations,
            ac_frequency_hz,
            backend,
            revision_key,
            ac_key,
        } => {
            let started = std::time::Instant::now();
            let connectivity =
                crate::engine::netlist::build_canonical_connectivity_with_annotations(
                    &components,
                    &wires,
                    &annotations,
                );
            let connectivity_ms = started.elapsed().as_secs_f64() * 1_000.0;
            if cancellation.is_cancelled() {
                return AnalysisPayload::Schematic(Box::new(SchematicAnalysis {
                    connectivity,
                    simulation: crate::engine::simulation::Simulation::default(),
                    connectivity_ms,
                    simulation_ms: 0.0,
                    erc_ms: 0.0,
                    revision_key,
                    ac_key,
                }));
            }
            let started = std::time::Instant::now();
            let mut simulation = crate::ui::app::analyze_circuit_with_connectivity_and_cancellation(
                &components,
                &wires,
                &connectivity,
                Some(cancellation),
            );
            simulation.ac = crate::engine::mna::solve_ac_with_connectivity(
                &components,
                &wires,
                ac_frequency_hz,
                &connectivity,
            );
            if !cancellation.is_cancelled() {
                simulation.transient = match backend {
                    crate::engine::backend::BackendKind::InternalMna => {
                        crate::engine::transient::solve_transient_with_netlist(
                            &components,
                            &connectivity.netlist,
                        )
                    }
                    crate::engine::backend::BackendKind::NgSpice => {
                        use crate::engine::backend::SimulationBackend;
                        let backend = crate::engine::backend::NgSpiceBackend {
                            config: crate::engine::ngspice::NgspiceConfig::default(),
                        };
                        let circuit = crate::engine::backend::SimulationCircuit {
                            components: &components,
                            wires: &wires,
                            netlist: &connectivity.netlist,
                            document_revision: revision_key.connectivity,
                        };
                        match backend.transient(
                            &circuit,
                            &crate::engine::backend::TransientRequest {
                                duration_s: 0.02,
                                maximum_samples: 1_000,
                            },
                            cancellation,
                        ) {
                            Ok(transient) => Some(transient),
                            Err(error) => {
                                simulation
                                    .details
                                    .push(format!("ngspice transient failed: {error:?}"));
                                None
                            }
                        }
                    }
                };
            }
            let simulation_ms = started.elapsed().as_secs_f64() * 1_000.0;
            let started = std::time::Instant::now();
            if !cancellation.is_cancelled() {
                use crate::engine::erc::ErcDependency;
                if cache.erc_topology_revision != Some(revision_key.topology) {
                    cache.erc_topology = crate::engine::validation::validate_beginner_rules_for(
                        &connectivity.netlist,
                        &[ErcDependency::Topology],
                    );
                    cache.erc_topology_revision = Some(revision_key.topology);
                }
                simulation.erc.clone_from(&cache.erc_topology);
                simulation
                    .erc
                    .extend(crate::engine::validation::validate_beginner_rules_for(
                        &connectivity.netlist,
                        &[ErcDependency::Values],
                    ));
                let mut dynamic = Vec::new();
                crate::ui::app::append_dynamic_erc(&components, &wires, &simulation, &mut dynamic);
                simulation.erc.extend(dynamic);
            }
            let erc_ms = started.elapsed().as_secs_f64() * 1_000.0;
            AnalysisPayload::Schematic(Box::new(SchematicAnalysis {
                connectivity,
                simulation,
                connectivity_ms,
                simulation_ms,
                erc_ms,
                revision_key,
                ac_key,
            }))
        }
        AnalysisRequest::FullDrc { mut board, nets } => {
            board.rebuild_entity_index();
            AnalysisPayload::FullDrc(run_drc_with_nets(&board, &nets))
        }
        AnalysisRequest::Autosave { saved, path } => {
            let result = serde_json::to_string_pretty(&saved)
                .map_err(|error| error.to_string())
                .and_then(|json| {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|error| format!("Create {}: {error}", parent.display()))?;
                    }
                    let text = path
                        .to_str()
                        .ok_or_else(|| format!("Path is not valid UTF-8: {}", path.display()))?;
                    crate::storage::save::write_with_backup(text, &json)?;
                    Ok(path)
                });
            AnalysisPayload::Autosave(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Counters;

    fn empty_autosave(revision: u64) -> AnalysisRequest {
        AnalysisRequest::Autosave {
            saved: Box::new(SavedCircuit {
                schema_version: 4,
                next_id: revision + 1,
                counters: Counters::default(),
                components: Vec::new(),
                wires: Vec::new(),
                junction_dots: Vec::new(),
                no_connect_markers: Vec::new(),
                pages: Vec::new(),
                current_page: 0,
            }),
            path: PathBuf::from(format!("autosave-{revision}.json")),
        }
    }

    #[test]
    fn full_queue_retains_latest_autosave_until_worker_has_capacity() {
        let (jobs, jobs_rx) = sync_channel::<Job>(0);
        let (_results_tx, results) = sync_channel::<AnalysisResult>(1);
        let mut worker = BoundedAnalysisWorker {
            jobs,
            results,
            schematic: CancellationToken::default(),
            drc: CancellationToken::default(),
            autosave: CancellationToken::default(),
            pending_autosave: None,
            failure: None,
        };

        assert!(worker.submit(1, empty_autosave(1)).is_ok());
        assert_eq!(
            worker
                .pending_autosave
                .as_ref()
                .map(|job| job.document_revision),
            Some(1)
        );
        assert!(worker.submit(2, empty_autosave(2)).is_ok());
        assert_eq!(
            worker
                .pending_autosave
                .as_ref()
                .map(|job| job.document_revision),
            Some(2)
        );

        drop(jobs_rx);
        worker.flush_pending_autosave();
        assert!(worker.pending_autosave.is_some());
        assert!(worker.take_failure().is_some());
    }
}
