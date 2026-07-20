//! Optional ngspice backend for accurate DC operating-point simulation.
//!
//! Cluster's internal MNA solver is educational — it uses simplified companion
//! models and cannot handle subcircuit models, MOSFET I-V curves, or AC
//! analysis.  This module adds an **optional** ngspice integration:
//!
//! 1. `is_ngspice_available()` — detect whether ngspice is installed.
//! 2. `export_ngspice_netlist()` — write a complete `.cir` file.
//! 3. `run_ngspice()` — invoke the `ngspice` binary and capture output.
//! 4. `parse_operating_point()` — extract node voltages and branch currents
//!    from ngspice's operating-point (`.op`) output text.
//! 5. `NgspiceResult` — maps results back to schematic component IDs.
//!
//! When ngspice is not available the internal MNA solver is used as a preview.
//! Unsupported components are reported in [`NgspiceResult::unsupported`].

use std::collections::HashMap;
use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::engine::transient::{TransientKind, TransientResult, TransientSample};
use crate::model::{CircuitNetlist, Component, ComponentKind, Wire};

// ─── Result types ─────────────────────────────────────────────────────────────

/// Outcome of an ngspice operating-point run mapped to schematic elements.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // Optional backend result; UI integration is feature work.
pub(crate) struct NgspiceResult {
    /// Net name → solved voltage (V).
    pub(crate) node_voltages: HashMap<String, f64>,
    /// Component reference designator → branch current (A).
    pub(crate) branch_currents: HashMap<String, f64>,
    /// Components that could not be included in the ngspice netlist.
    pub(crate) unsupported: Vec<NgspiceUnsupported>,
    /// Raw operating-point text section from ngspice stdout.
    pub(crate) raw_output: String,
    pub(crate) stderr: String,
    pub(crate) document_revision: u64,
}

/// A component that was excluded from the ngspice netlist.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct NgspiceUnsupported {
    pub(crate) component_id: u64,
    pub(crate) reference: String,
    pub(crate) reason: &'static str,
}

/// Errors that can occur when running ngspice.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum NgspiceError {
    /// The `ngspice` binary was not found on PATH.
    NotInstalled,
    /// ngspice returned a non-zero exit code.
    SimulationFailed(String),
    /// The output could not be written or the process could not be started.
    IoError(std::io::Error),
    /// No operating-point data was found in the output.
    NoResults,
    Cancelled,
    TimedOut(Duration),
}

impl std::fmt::Display for NgspiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NgspiceError::NotInstalled => {
                write!(
                    f,
                    "ngspice is not installed. Install it with your package manager (e.g. `brew install ngspice`, `apt install ngspice`) to enable accurate simulation."
                )
            }
            NgspiceError::SimulationFailed(msg) => write!(f, "ngspice simulation failed: {msg}"),
            NgspiceError::IoError(e) => write!(f, "I/O error: {e}"),
            NgspiceError::NoResults => {
                write!(f, "ngspice ran but produced no operating-point results")
            }
            NgspiceError::Cancelled => write!(f, "ngspice simulation was cancelled"),
            NgspiceError::TimedOut(duration) => {
                write!(
                    f,
                    "ngspice exceeded the {:.1}s timeout",
                    duration.as_secs_f32()
                )
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NgspiceConfig {
    pub(crate) executable: PathBuf,
    pub(crate) timeout: Duration,
}

impl Default for NgspiceConfig {
    fn default() -> Self {
        Self {
            executable: PathBuf::from("ngspice"),
            timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl From<std::io::Error> for NgspiceError {
    fn from(e: std::io::Error) -> Self {
        NgspiceError::IoError(e)
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Returns `true` if an `ngspice` binary is reachable on PATH.
#[allow(dead_code)]
pub(crate) fn is_ngspice_available() -> bool {
    std::process::Command::new("ngspice")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Export a SPICE `.cir` netlist compatible with ngspice's operating-point
/// analysis (`.op` command).
///
/// Components with no SPICE model equivalent are listed in the returned
/// `Vec<NgspiceUnsupported>` and omitted from the netlist.
#[allow(dead_code)]
pub(crate) fn export_ngspice_netlist(
    components: &[Component],
    _wires: &[Wire],
    netlist: &CircuitNetlist,
) -> (String, Vec<NgspiceUnsupported>) {
    let mut out = String::new();
    let mut unsupported: Vec<NgspiceUnsupported> = Vec::new();

    out.push_str("* Cluster auto-generated ngspice netlist\n");
    out.push_str(".title Cluster schematic\n\n");

    // Map net_id → SPICE node name
    let node_name = |net_id: usize| -> String {
        let net = netlist.nets.iter().find(|n| n.id == net_id);
        match net {
            Some(n) if n.name.eq_ignore_ascii_case("GND") => "0".to_string(),
            Some(n) => spice_node_name(&n.name),
            None => format!("net{net_id}"),
        }
    };

    let pin_node = |comp_id: u64, pin_name: &str| -> Option<String> {
        netlist
            .pins
            .iter()
            .find(|p| p.component_id == comp_id && p.pin_name == pin_name)
            .map(|p| node_name(p.net_id))
    };

    let first_two_nodes = |comp_id: u64| -> Option<(String, String)> {
        let mut pins = netlist.pins.iter().filter(|p| p.component_id == comp_id);
        let a = pins.next().map(|p| node_name(p.net_id))?;
        let b = pins.next().map(|p| node_name(p.net_id))?;
        Some((a, b))
    };

    let mut seen_ids: std::collections::HashSet<u64> = std::collections::HashSet::new();

    for comp in components {
        if !seen_ids.insert(comp.id) {
            continue;
        }
        let label = spice_ref(&comp.label);

        match comp.kind {
            ComponentKind::Resistor => {
                if let Some((a, b)) = first_two_nodes(comp.id) {
                    let r = crate::parse_metric_value(&comp.value, "ohm").unwrap_or(1_000.0);
                    out.push_str(&format!("R{label} {a} {b} {r:.6}\n"));
                }
            }
            ComponentKind::Capacitor => {
                if let Some((a, b)) = first_two_nodes(comp.id) {
                    let c = crate::parse_metric_value(&comp.value, "f").unwrap_or(100e-9);
                    out.push_str(&format!("C{label} {a} {b} {c:.6e}\n"));
                }
            }
            ComponentKind::Inductor => {
                if let Some((a, b)) = first_two_nodes(comp.id) {
                    let l = crate::parse_metric_value(&comp.value, "h").unwrap_or(1e-3);
                    out.push_str(&format!("L{label} {a} {b} {l:.6e}\n"));
                }
            }
            ComponentKind::Diode => {
                if let Some((a, k)) = first_two_nodes(comp.id) {
                    out.push_str(&format!("D{label} {a} {k} DGEN\n"));
                }
            }
            ComponentKind::Led => {
                if let Some((a, k)) = first_two_nodes(comp.id) {
                    out.push_str(&format!("D{label} {a} {k} DLED\n"));
                }
            }
            ComponentKind::ZenerDiode => {
                let vz = crate::parse_metric_value(&comp.value, "v").unwrap_or(5.1);
                if let Some((a, k)) = first_two_nodes(comp.id) {
                    out.push_str(&format!("D{label} {a} {k} DZENER\n"));
                    out.push_str(&format!(
                        ".model DZENER{label} D(BV={vz:.2} IBV=1m Is=1e-14)\n"
                    ));
                }
            }
            ComponentKind::SchottkyDiode => {
                if let Some((a, k)) = first_two_nodes(comp.id) {
                    out.push_str(&format!("D{label} {a} {k} DSCH\n"));
                }
            }
            ComponentKind::VSource | ComponentKind::Battery => {
                let v = crate::parse_metric_value(&comp.value, "v").unwrap_or(5.0);
                if let (Some(pos), Some(neg)) = (pin_node(comp.id, "+"), pin_node(comp.id, "-")) {
                    out.push_str(&format!("V{label} {pos} {neg} DC {v:.4}\n"));
                }
            }
            ComponentKind::ISource => {
                let i = crate::parse_metric_value(&comp.value, "a").unwrap_or(0.01);
                if let Some((pos, neg)) = first_two_nodes(comp.id) {
                    out.push_str(&format!("I{label} {pos} {neg} DC {i:.6e}\n"));
                }
            }
            ComponentKind::Ground => {
                // GND is node 0; no element needed
            }
            ComponentKind::Switch | ComponentKind::PushButton | ComponentKind::SlideSwitch => {
                let closed = !comp.value.to_lowercase().contains("open")
                    && !comp.value.to_lowercase().contains("off");
                if let Some((a, b)) = first_two_nodes(comp.id) {
                    let r = if closed { 0.001 } else { 1e9 };
                    out.push_str(&format!("R{label}_sw {a} {b} {r:.6e}\n"));
                }
            }
            ComponentKind::Fuse => {
                if let Some((a, b)) = first_two_nodes(comp.id) {
                    out.push_str(&format!("R{label}_fuse {a} {b} 0.05\n"));
                }
            }
            ComponentKind::NpnTransistor => {
                let (b_n, c_n, e_n) = (
                    pin_node(comp.id, "B"),
                    pin_node(comp.id, "C"),
                    pin_node(comp.id, "E"),
                );
                if let (Some(b), Some(c), Some(e)) = (b_n, c_n, e_n) {
                    out.push_str(&format!("Q{label} {c} {b} {e} NPN_GEN\n"));
                }
            }
            ComponentKind::PnpTransistor => {
                let (b_n, c_n, e_n) = (
                    pin_node(comp.id, "B"),
                    pin_node(comp.id, "C"),
                    pin_node(comp.id, "E"),
                );
                if let (Some(b), Some(c), Some(e)) = (b_n, c_n, e_n) {
                    out.push_str(&format!("Q{label} {c} {b} {e} PNP_GEN\n"));
                }
            }
            ComponentKind::Nmosfet => {
                let (g, d, s) = (
                    pin_node(comp.id, "G"),
                    pin_node(comp.id, "D"),
                    pin_node(comp.id, "S"),
                );
                if let (Some(g), Some(d), Some(s)) = (g, d, s) {
                    out.push_str(&format!("M{label} {d} {g} {s} {s} NMOS_GEN\n"));
                }
            }
            ComponentKind::Pmosfet => {
                let (g, d, s) = (
                    pin_node(comp.id, "G"),
                    pin_node(comp.id, "D"),
                    pin_node(comp.id, "S"),
                );
                if let (Some(g), Some(d), Some(s)) = (g, d, s) {
                    out.push_str(&format!("M{label} {d} {g} {s} {s} PMOS_GEN\n"));
                }
            }
            ComponentKind::Voltmeter => {
                if let Some((a, b)) = first_two_nodes(comp.id) {
                    out.push_str(&format!("R{label}_vm {a} {b} 1MEG\n"));
                }
            }
            ComponentKind::Ammeter => {
                if let Some((a, b)) = first_two_nodes(comp.id) {
                    out.push_str(&format!("V{label}_am {a} {b} DC 0\n"));
                }
            }
            ComponentKind::Potentiometer => {
                if let Some((a, b)) = first_two_nodes(comp.id) {
                    let r = crate::parse_metric_value(&comp.value, "ohm").unwrap_or(10_000.0) * 0.5;
                    out.push_str(&format!("R{label} {a} {b} {r:.2}\n"));
                }
            }
            ComponentKind::Thermistor => {
                if let Some((a, b)) = first_two_nodes(comp.id) {
                    let r = crate::parse_metric_value(&comp.value, "ohm").unwrap_or(10_000.0);
                    out.push_str(&format!("R{label} {a} {b} {r:.2}\n"));
                }
            }
            ComponentKind::Buzzer => {
                if let Some((a, b)) = first_two_nodes(comp.id) {
                    out.push_str(&format!("R{label} {a} {b} 150\n"));
                }
            }
            ComponentKind::Lamp => {
                if let Some((a, b)) = first_two_nodes(comp.id) {
                    let rated_v = crate::parse_metric_value(&comp.value, "v").unwrap_or(12.0);
                    let r = (rated_v * rated_v) / 40.0;
                    out.push_str(&format!("R{label} {a} {b} {r:.2}\n"));
                }
            }
            ComponentKind::DcMotor => {
                if let Some((a, b)) = first_two_nodes(comp.id) {
                    out.push_str(&format!("R{label} {a} {b} 5\n"));
                }
            }
            ComponentKind::Relay => {
                if let (Some(cp), Some(cn)) =
                    (pin_node(comp.id, "COIL+"), pin_node(comp.id, "COIL-"))
                {
                    out.push_str(&format!("R{label}_coil {cp} {cn} 100\n"));
                }
                unsupported.push(NgspiceUnsupported {
                    component_id: comp.id,
                    reference: comp.label.clone(),
                    reason: "Relay contact switching is not modelled in the ngspice DC netlist",
                });
            }
            ComponentKind::NetLabel => {
                // NetLabels are handled by the netlist builder (same-name = same net)
            }
            // ── Unsupported in ngspice DC ────────────────────────────────────
            kind => {
                unsupported.push(NgspiceUnsupported {
                    component_id: comp.id,
                    reference: comp.label.clone(),
                    reason: unsupported_reason(kind),
                });
            }
        }
    }

    // ── Built-in models ───────────────────────────────────────────────────────
    out.push_str("\n* --- Built-in models ---\n");
    out.push_str(".model DGEN  D(Is=1e-14 N=1.8 Rs=8)\n");
    out.push_str(".model DLED  D(Is=1e-20 N=2.0 Rs=20 Vfwd=2.0)\n");
    out.push_str(".model DSCH  D(Is=1e-8  N=1.2 Rs=4)\n");
    out.push_str(".model NPN_GEN NPN(Is=1e-14 Bf=100 Vaf=100 Rb=10)\n");
    out.push_str(".model PNP_GEN PNP(Is=1e-14 Bf=100 Vaf=100 Rb=10)\n");
    out.push_str(".model NMOS_GEN NMOS(Vto=2 Kp=100m Gamma=0 Lambda=0)\n");
    out.push_str(".model PMOS_GEN PMOS(Vto=-2 Kp=100m Gamma=0 Lambda=0)\n");
    out.push_str("\n.op\n.end\n");

    (out, unsupported)
}

/// Write the netlist to a temporary file and invoke `ngspice -b -o`.
///
/// Returns `Ok(NgspiceResult)` on success.  The temporary file is cleaned up
/// after the run.
#[allow(dead_code)]
pub(crate) fn run_ngspice(netlist_text: &str) -> Result<NgspiceResult, NgspiceError> {
    run_ngspice_configured(
        netlist_text,
        0,
        &NgspiceConfig::default(),
        &CancellationToken::default(),
    )
}

pub(crate) fn run_ngspice_configured(
    netlist_text: &str,
    document_revision: u64,
    config: &NgspiceConfig,
    cancellation: &CancellationToken,
) -> Result<NgspiceResult, NgspiceError> {
    let (combined, stderr) =
        run_ngspice_raw_configured(netlist_text, document_revision, config, cancellation)?;
    let mut result = parse_operating_point(&combined).ok_or(NgspiceError::NoResults)?;
    result.stderr = stderr;
    result.document_revision = document_revision;
    Ok(result)
}

fn run_ngspice_raw_configured(
    netlist_text: &str,
    _document_revision: u64,
    config: &NgspiceConfig,
    cancellation: &CancellationToken,
) -> Result<(String, String), NgspiceError> {
    if cancellation.is_cancelled() {
        return Err(NgspiceError::Cancelled);
    }
    static RUN_ID: AtomicU64 = AtomicU64::new(1);
    let run_id = RUN_ID.fetch_add(1, Ordering::Relaxed);
    let work_dir =
        std::env::temp_dir().join(format!("cluster-ngspice-{}-{run_id}", std::process::id()));
    std::fs::create_dir(&work_dir)?;
    let cleanup = WorkDirGuard(work_dir.clone());
    let cir_path = work_dir.join("input.cir");
    let out_path = work_dir.join("results.out");

    {
        let mut f = std::fs::File::create(&cir_path)?;
        f.write_all(netlist_text.as_bytes())?;
        f.sync_all()?;
    }

    let mut child = std::process::Command::new(&config.executable)
        .args(["-b", "-o"])
        .arg(&out_path)
        .arg(&cir_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                NgspiceError::NotInstalled
            } else {
                NgspiceError::IoError(error)
            }
        })?;
    let started = Instant::now();
    loop {
        if cancellation.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(NgspiceError::Cancelled);
        }
        if started.elapsed() >= config.timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(NgspiceError::TimedOut(config.timeout));
        }
        if child.try_wait()?.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    // Read output file if it exists
    let out_text = std::fs::read_to_string(&out_path).unwrap_or_default();

    if !output.status.success() && out_text.is_empty() && stdout.is_empty() {
        return Err(NgspiceError::SimulationFailed(stderr));
    }

    let combined = format!("{stdout}\n{out_text}");
    drop(cleanup);
    Ok((combined, stderr))
}

pub(crate) fn export_ngspice_transient_netlist(
    components: &[Component],
    wires: &[Wire],
    netlist: &CircuitNetlist,
    duration_s: f64,
    maximum_samples: usize,
) -> (String, Vec<NgspiceUnsupported>) {
    let (operating_point, unsupported) = export_ngspice_netlist(components, wires, netlist);
    let target = netlist
        .pins
        .iter()
        .find(|pin| {
            components.iter().any(|component| {
                component.id == pin.component_id && component.kind == ComponentKind::Capacitor
            })
        })
        .and_then(|pin| netlist.nets.iter().find(|net| net.id == pin.net_id))
        .map(|net| {
            if net.name.eq_ignore_ascii_case("GND") {
                "0".to_string()
            } else {
                spice_node_name(&net.name)
            }
        })
        .unwrap_or_else(|| "0".to_string());
    let duration_s = duration_s.clamp(1.0e-6, 1.0e3);
    let samples = maximum_samples.clamp(2, 100_000);
    let step = duration_s / (samples - 1) as f64;
    let mut transient = operating_point.replace("\n.op\n.end\n", "\n");
    transient.push_str(&format!(
        ".tran {step:.9e} {duration_s:.9e}\n.print tran time v({target})\n.end\n"
    ));
    (transient, unsupported)
}

pub(crate) fn run_ngspice_transient_configured(
    netlist_text: &str,
    document_revision: u64,
    config: &NgspiceConfig,
    cancellation: &CancellationToken,
) -> Result<TransientResult, NgspiceError> {
    let (output, _stderr) =
        run_ngspice_raw_configured(netlist_text, document_revision, config, cancellation)?;
    parse_transient_output(&output).ok_or(NgspiceError::NoResults)
}

pub(crate) fn parse_transient_output(output: &str) -> Option<TransientResult> {
    let mut samples = Vec::new();
    for line in output.lines() {
        let values = line
            .split_whitespace()
            .filter_map(|value| value.parse::<f64>().ok())
            .collect::<Vec<_>>();
        let (time, voltage) = match values.as_slice() {
            [time, voltage] => (*time, *voltage),
            [_index, time, voltage, ..] => (*time, *voltage),
            _ => continue,
        };
        if time.is_finite() && voltage.is_finite() && time >= 0.0 {
            samples.push(TransientSample {
                t_s: time,
                v_cap: voltage,
                source_v: voltage,
            });
        }
    }
    (samples.len() >= 2).then(|| TransientResult {
        kind: TransientKind::RcStep,
        summary: format!("ngspice transient: {} samples", samples.len()),
        samples,
        limitations: vec!["Imported from the selected ngspice node.".to_string()],
    })
}

struct WorkDirGuard(PathBuf);

impl Drop for WorkDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Parse node voltages and branch currents from ngspice `.op` output text.
///
/// ngspice prints operating-point results in sections like:
///
/// ```text
///   Node                     Voltage
///   ----                     -------
///   v(net001)                 3.3000
///   ...
///
///   Source                   Current
///   ------                   -------
///   v1#branch               -0.0120
/// ```
pub(crate) fn parse_operating_point(output: &str) -> Option<NgspiceResult> {
    let mut result = NgspiceResult {
        raw_output: output.to_string(),
        ..Default::default()
    };

    let mut in_voltage_section = false;
    let mut in_current_section = false;

    for line in output.lines() {
        let l = line.trim();

        if l.to_lowercase().contains("node") && l.to_lowercase().contains("voltage") {
            in_voltage_section = true;
            in_current_section = false;
            continue;
        }
        if l.to_lowercase().contains("source") && l.to_lowercase().contains("current") {
            in_current_section = true;
            in_voltage_section = false;
            continue;
        }
        if l.starts_with("---") || l.is_empty() {
            continue;
        }
        // Stop at the next analysis section
        if l.starts_with('.') {
            in_voltage_section = false;
            in_current_section = false;
            continue;
        }

        if in_voltage_section {
            // Lines look like:  v(node_name)    3.3000
            // or                node_name       3.3000
            let parts: Vec<&str> = l.split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts[0]
                    .trim_start_matches("v(")
                    .trim_end_matches(')')
                    .to_uppercase();
                if let Ok(v) = parts[1].parse::<f64>() {
                    result.node_voltages.insert(name, v);
                }
            }
        } else if in_current_section {
            // Lines look like:  v1#branch    -0.0120
            let parts: Vec<&str> = l.split_whitespace().collect();
            if parts.len() >= 2 {
                let name_raw = parts[0].to_lowercase();
                // strip leading 'v' and trailing '#branch'
                let name = name_raw
                    .trim_start_matches('v')
                    .trim_end_matches("#branch")
                    .to_uppercase();
                if let Ok(i) = parts[1].parse::<f64>() {
                    result.branch_currents.insert(name, i);
                }
            }
        }
    }

    if result.node_voltages.is_empty() && result.branch_currents.is_empty() {
        return None;
    }
    Some(result)
}

// ─── Private helpers ──────────────────────────────────────────────────────────

fn spice_node_name(net_name: &str) -> String {
    // SPICE node names must start with a letter and contain only [A-Za-z0-9_]
    let cleaned: String = net_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty()
        || !cleaned
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic())
            .unwrap_or(false)
    {
        format!("N_{cleaned}")
    } else {
        cleaned
    }
}

fn spice_ref(label: &str) -> String {
    label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn unsupported_reason(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Timer555 => "555 timer — use a behavioural subcircuit model",
        ComponentKind::LogicNot
        | ComponentKind::LogicAnd
        | ComponentKind::LogicOr
        | ComponentKind::LogicNand
        | ComponentKind::LogicNor
        | ComponentKind::LogicXor => "Digital gate — no analogue SPICE model",
        ComponentKind::Esp32
        | ComponentKind::Esp32S3
        | ComponentKind::Esp32C3
        | ComponentKind::ArduinoUno
        | ComponentKind::RaspberryPiPico
        | ComponentKind::Stm32BluePill
        | ComponentKind::Stm32Nucleo64 => {
            "MCU — no SPICE model; model power draw as a resistive load manually"
        }
        ComponentKind::Oled | ComponentKind::Sensor => "I²C module — no SPICE model",
        ComponentKind::OpAmp => "Op-amp — add a subcircuit model (.lib) for accurate results",
        ComponentKind::Crystal => "Crystal — not relevant in DC operating-point",
        ComponentKind::Transformer => "Transformer — no DC operating-point model",
        ComponentKind::Optocoupler => "Optocoupler — use a subcircuit model",
        ComponentKind::MotorDriver => "Motor driver IC — no SPICE model",
        ComponentKind::VoltageRef => "Voltage reference — model as an ideal voltage source",
        ComponentKind::Servo => "Servo — model coil as a resistive load manually",
        ComponentKind::Breadboard | ComponentKind::TextNote | ComponentKind::NetLabel => {
            "Annotation — skipped"
        }
        ComponentKind::Dht11 | ComponentKind::Dht22 => "Digital sensor — no SPICE model",
        ComponentKind::HcSr04 => "Ultrasonic sensor — no SPICE model",
        ComponentKind::NeoPixel => "WS2812 — no SPICE model",
        ComponentKind::PirSensor => "PIR sensor — no SPICE model",
        ComponentKind::Display7Seg => "7-segment display — model LEDs individually",
        ComponentKind::GenericIc => "Generic IC — provide a subcircuit model (.lib)",
        _ => "Not supported in ngspice DC netlist",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_op_voltage_and_current() {
        let output = r#"
        Node                     Voltage
        ----                     -------
        v(net001)                 3.3000
        v(net002)                 1.6500

        Source                   Current
        ------                   -------
        v1#branch               -0.01200
        "#;
        let result = parse_operating_point(output).expect("should parse");
        assert!(
            (result.node_voltages["NET001"] - 3.3).abs() < 1e-6,
            "expected 3.3 V on NET001"
        );
        assert!(
            (result.branch_currents["1"] - (-0.012)).abs() < 1e-9,
            "expected -12 mA on V1"
        );
    }

    #[test]
    fn spice_node_name_sanitizes() {
        assert_eq!(spice_node_name("VCC"), "VCC");
        assert_eq!(spice_node_name("NET+3V3"), "NET_3V3");
        assert_eq!(spice_node_name("123abc"), "N_123abc");
    }

    #[test]
    fn cancelled_run_does_not_start_a_process_or_create_files() {
        let cancellation = CancellationToken::default();
        cancellation.cancel();
        let config = NgspiceConfig {
            executable: std::path::PathBuf::from("definitely-not-an-installed-ngspice"),
            timeout: Duration::from_secs(1),
        };
        assert!(matches!(
            run_ngspice_configured(".end", 42, &config, &cancellation),
            Err(NgspiceError::Cancelled)
        ));
    }

    #[test]
    fn transient_table_imports_time_and_voltage_samples() {
        let result = parse_transient_output(
            "Index   time          v(out)\n0 0.000000e+00 0.0\n1 1.000000e-03 3.2\n2 2.000000e-03 4.4\n",
        )
        .expect("transient result");
        assert_eq!(result.samples.len(), 3);
        assert!((result.samples[2].v_cap - 4.4).abs() < 1.0e-9);
    }
}
