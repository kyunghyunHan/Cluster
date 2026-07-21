use crate::app::{Selection, Tool};

// Re-export utilities moved to sub-modules.
// These keep `crate::X` paths working in engine/, export/, and in the local
// canvas/drawing code that still lives in main.rs.
pub(crate) use crate::engine::parse_metric_value;
pub(crate) use crate::model::{
    component_pin_defs, component_pins, component_size, distance_to_segment,
    point_touches_wire_segment, rotate_point,
};

use crate::engine::mna;
#[cfg(test)]
use crate::engine::netlist::build_circuit_netlist;
use crate::engine::simulation::{Conductance, Simulation, SimulationStatus};
use crate::engine::validation::{
    ErcRule, ErcSeverity, ErcViolation, pin_is_controller_scl, pin_is_controller_sda,
    pin_is_i2c_named, pin_is_microcontroller_gpio, validate_beginner_rules,
};
use crate::model::*;
use crate::ui::bottom_dock::{
    BottomDockAction, BottomDockModel, BottomDockTab, PageTabsAction, render_bottom_dock,
    render_page_tabs,
};
use crate::ui::breadboard::{BreadboardAction, build_breadboard_guide, render_breadboard_view};
use crate::ui::canvas::current_flow::{
    CurrentFlowCache, CurrentFlowSettings, FlowCacheKey, FlowRenderInput, render_current_flow,
};
use crate::ui::canvas::interaction::{SmartWireTone, assess_pin_pair};
use crate::ui::canvas::{
    CanvasView, draw_grid, draw_probe_card, hit_test, hit_test_component, hit_test_wire,
    hit_test_wire_control_point, selection_summary,
};
use crate::ui::canvas_overlay::draw_simulation_legend;
use crate::ui::left_palette::{
    PaletteAction, PaletteTemplate, render_parts_palette, selected_part,
};
use crate::ui::right_inspector::{InspectorTab, render_inspector_header};
use crate::ui::status_bar::{StatusBarModel, render_status_bar};
use crate::ui::top_toolbar::{TopToolbarAction, TopToolbarModel, render_top_toolbar};
use crate::ui::validation_panel::ValidationPanelAction;
use eframe::egui;
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

pub(crate) fn application_data_dir() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(std::path::PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(|home| std::path::PathBuf::from(home).join("Library/Application Support"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".local/share"))
        });
    base.unwrap_or_else(std::env::temp_dir).join("Cluster")
}

pub(crate) fn default_save_path() -> std::path::PathBuf {
    application_data_dir().join("cluster_circuit.json")
}
pub(crate) fn autorecover_path(document_id: u64) -> std::path::PathBuf {
    application_data_dir()
        .join("recovery")
        .join(format!("document-{document_id}.json"))
}

// Tool, AlignDir, Selection are defined in app/state.rs and imported above.

mod canvas_helpers;
mod energize;
mod overlays;
mod symbols;
#[cfg(test)]
mod tests;
mod util;
mod widgets;

pub(crate) use canvas_helpers::*;
pub(crate) use energize::*;
pub(crate) use overlays::*;
pub(crate) use symbols::*;
pub(crate) use util::*;
pub(crate) use widgets::*;

pub(crate) struct WorkspaceState {
    pub(crate) show_help: bool,
    pub(crate) bottom_dock_tab: BottomDockTab,
    pub(crate) bottom_dock_open: bool,
    pub(crate) workspace: Workspace,
    pub(crate) show_performance_overlay: bool,
    /// Set by the command dispatcher when a semantic change needs another frame.
    pub(crate) repaint_requested: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Workspace {
    #[default]
    Schematic,
    Breadboard,
    Pcb,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SimulationRunState {
    Stopped,
    Dirty,
    Solving,
    Valid,
    Warning,
    Failed,
}

impl SimulationRunState {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Stopped => "Stopped",
            Self::Dirty => "Stale",
            Self::Solving => "Solving",
            Self::Valid => "Valid",
            Self::Warning => "Warning",
            Self::Failed => "Failed",
        }
    }
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            show_help: false,
            bottom_dock_tab: BottomDockTab::Erc,
            bottom_dock_open: true,
            workspace: Workspace::Schematic,
            show_performance_overlay: false,
            repaint_requested: false,
        }
    }
}

pub(crate) struct CanvasState {
    pub(crate) rect: Rect,
    pub(crate) cursor_world_pos: Option<Pos2>,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            rect: Rect::EVERYTHING,
            cursor_world_pos: None,
        }
    }
}

#[derive(Default)]
pub(crate) struct PaletteState {
    pub(crate) filter: String,
}

pub(crate) struct SimulationUiState {
    pub(crate) show_voltage_labels: bool,
    pub(crate) show_dc_overlay: bool,
    pub(crate) show_oscilloscope: bool,
    pub(crate) ac_freq_hz: f32,
    pub(crate) current_flow: CurrentFlowSettings,
    pub(crate) backend: crate::engine::backend::BackendKind,
    pub(crate) flow_started_at: Option<f64>,
    pub(crate) last_simulation_enabled: bool,
}

impl Default for SimulationUiState {
    fn default() -> Self {
        Self {
            show_voltage_labels: false,
            show_dc_overlay: true,
            show_oscilloscope: false,
            ac_freq_hz: 1000.0,
            current_flow: CurrentFlowSettings::default(),
            backend: crate::engine::backend::BackendKind::InternalMna,
            flow_started_at: None,
            last_simulation_enabled: true,
        }
    }
}

#[derive(Default)]
pub(crate) struct InspectorState {
    pub(crate) active_tab: InspectorTab,
}

#[derive(Default)]
pub(crate) struct BreadboardUiState {
    pub(crate) open: bool,
}

#[derive(Default)]
pub(crate) struct PcbUiState {
    pub(crate) ratsnest_count: usize,
    pub(crate) last_sync_revision: u64,
    pub(crate) selected_drc_index: Option<usize>,
    pub(crate) workspace: crate::ui::pcb_workspace::PcbWorkspaceState,
}

pub(crate) struct HistoryEntry {
    pub(crate) delta: crate::editor::delta::DocumentDelta,
    pub(crate) description: &'static str,
    pub(crate) merge_key: Option<crate::commands::CommandMergeKey>,
    pub(crate) created_at: std::time::Instant,
    pub(crate) memory_cost: usize,
}

pub(crate) struct PendingHistory {
    pub(crate) snapshot: CircuitSnapshot,
    pub(crate) description: &'static str,
    pub(crate) merge_key: Option<crate::commands::CommandMergeKey>,
}

#[derive(Default)]
pub(crate) struct HistoryState {
    pub(crate) undo: std::collections::VecDeque<HistoryEntry>,
    pub(crate) redo: std::collections::VecDeque<HistoryEntry>,
    pub(crate) pending: Option<PendingHistory>,
    pub(crate) undo_memory_bytes: usize,
    pub(crate) dirty: bool,
}

/// Transient editing state and command history. This is never serialized into
/// the project document.
#[derive(Default)]
pub(crate) struct EditorState {
    pub(crate) document: crate::app::EditorDocumentState,
    pub(crate) pending_custom_part: Option<String>,
    pub(crate) clipboard: Vec<Component>,
    pub(crate) clipboard_wires: Vec<Wire>,
    pub(crate) rect_select_start: Option<Pos2>,
    pub(crate) history: HistoryState,
}

impl std::ops::Deref for EditorState {
    type Target = crate::app::EditorDocumentState;

    fn deref(&self) -> &Self::Target {
        &self.document
    }
}

impl std::ops::DerefMut for EditorState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.document
    }
}

#[derive(Default, Clone, Copy)]
pub(crate) struct DirtyFlags {
    pub(crate) geometry_dirty: bool,
    pub(crate) connectivity_dirty: bool,
    pub(crate) validation_dirty: bool,
    pub(crate) simulation_dirty: bool,
    pub(crate) pcb_sync_dirty: bool,
    pub(crate) pcb_drc_dirty: bool,
}

type ConnectedPinsCache = Option<(u64, Arc<Vec<(i32, i32)>>)>;

/// Derived analysis data. None of these fields are serialized as user data.
pub(crate) struct AnalysisState {
    pub(crate) circuit_revision: u64,
    pub(crate) revisions: DocumentRevisions,
    pub(crate) dirty_flags: DirtyFlags,
    pub(crate) cached_connectivity: Option<(u64, Arc<CanonicalConnectivity>)>,
    pub(crate) cached_netlist: Option<(u64, Arc<CircuitNetlist>)>,
    pub(crate) cached_simulation: Option<(SimulationRevisionKey, u32, Arc<Simulation>)>,
    pub(crate) simulation_revision: u64,
    pub(crate) cached_connected_pins: ConnectedPinsCache,
    pub(crate) current_flow_cache: CurrentFlowCache,
    pub(crate) schematic_entity_index: crate::model::SchematicEntityIndex,
    pub(crate) schematic_entity_revision: u64,
    pub(crate) schematic_spatial_index: crate::ui::canvas::spatial_index::SchematicSpatialIndex,
    pub(crate) schematic_spatial_revision: u64,
    pub(crate) pcb_cad: Option<crate::model::cad::CadProjectData>,
    pub(crate) pcb_drc: Vec<crate::pcb::drc::DrcViolation>,
    pub(crate) pcb_ratsnest_by_net: std::collections::HashMap<usize, usize>,
    pub(crate) worker: crate::engine::worker::BoundedAnalysisWorker,
    pub(crate) pending_schematic: Option<(u64, SimulationRevisionKey, u32)>,
    pub(crate) pending_full_drc_revision: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SimulationRevisionKey {
    pub(crate) connectivity: u64,
    pub(crate) topology: u64,
    pub(crate) parameters: u64,
    pub(crate) electrical: u64,
}

impl Default for AnalysisState {
    fn default() -> Self {
        Self {
            circuit_revision: 1,
            revisions: DocumentRevisions {
                persistence: 1,
                schematic_geometry: 1,
                schematic_connectivity: 1,
                electrical_parameters: 1,
                simulation_topology: 1,
                simulation_parameters: 1,
                board_topology: 1,
                board_geometry: 1,
                board_rules: 1,
                visual: 1,
            },
            dirty_flags: DirtyFlags::default(),
            cached_connectivity: None,
            cached_netlist: None,
            cached_simulation: None,
            simulation_revision: 0,
            cached_connected_pins: None,
            current_flow_cache: CurrentFlowCache::default(),
            schematic_entity_index: Default::default(),
            schematic_entity_revision: 0,
            schematic_spatial_index: Default::default(),
            schematic_spatial_revision: 0,
            pcb_cad: None,
            pcb_drc: Vec::new(),
            pcb_ratsnest_by_net: std::collections::HashMap::new(),
            worker: crate::engine::worker::BoundedAnalysisWorker::new(),
            pending_schematic: None,
            pending_full_drc_revision: None,
        }
    }
}

pub(crate) struct CircuitApp {
    pub(crate) document: ProjectDocument,
    pub(crate) editor: EditorState,
    pub(crate) analysis: AnalysisState,
    pub(crate) grid: f32,
    pub(crate) snap: bool,
    pub(crate) orthogonal_wires: bool,
    pub(crate) show_pins: bool,
    pub(crate) show_grid: bool,
    pub(crate) simulate: bool,
    pub(crate) simulation_run_state: SimulationRunState,
    pub(crate) status: String,
    pub(crate) workspace_state: WorkspaceState,
    pub(crate) canvas: CanvasState,
    pub(crate) palette_ui: PaletteState,
    pub(crate) simulation_ui: SimulationUiState,
    pub(crate) inspector_ui: InspectorState,
    pub(crate) breadboard_ui: BreadboardUiState,
    pub(crate) pcb_ui: PcbUiState,
    // View
    pub(crate) zoom: f32,
    pub(crate) pan: Vec2,
    // Net highlighting: wire ID hovered in select mode → highlight whole net
    pub(crate) hovered_net_wire: Option<u64>,
    // Cache of which wire IDs share the same net as hovered wire
    pub(crate) highlighted_net_wires: HashSet<u64>,
    pub(crate) last_autorecover_revision: u64,
    pub(crate) document_id: u64,
    // ── Find dialog ─────────────────────────────────────────────────────
    pub(crate) show_find: bool,
    pub(crate) find_query: String,
    pub(crate) find_results: Vec<u64>, // component IDs matching query
    pub(crate) find_result_idx: usize,
    // ── Deferred canvas fit (set after demo load, applied once canvas rect is known) ──
    pub(crate) pending_fit: bool,
    // ── Inline value editing: (component_id, edited_text) ───────────────
    pub(crate) inline_edit: Option<(u64, String)>,
    // ── Right-click context menu: (screen_pos, target component ID) ──────
    pub(crate) context_menu: Option<(egui::Pos2, u64)>,
    // ── PNG screenshot pending ────────────────────────────────────────────
    pub(crate) screenshot_pending: bool,
    pub(crate) automated_capture_path: Option<String>,
    pub(crate) automated_capture_requested: bool,
    pub(crate) performance: PerformanceStats,
}

#[derive(Debug, Default)]
pub(crate) struct MetricSummary {
    pub(crate) latest_ms: f64,
    pub(crate) samples: VecDeque<f64>,
    pub(crate) invocations: u64,
}

impl MetricSummary {
    const WINDOW: usize = 120;

    pub(crate) fn record(&mut self, duration_ms: f64) {
        self.latest_ms = duration_ms;
        self.invocations = self.invocations.saturating_add(1);
        if self.samples.len() == Self::WINDOW {
            self.samples.pop_front();
        }
        self.samples.push_back(duration_ms);
    }

    pub(crate) fn percentile(&self, percentile: f64) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let mut sorted = self.samples.iter().copied().collect::<Vec<_>>();
        sorted.sort_by(f64::total_cmp);
        let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
        sorted[index.min(sorted.len() - 1)]
    }

    pub(crate) fn maximum(&self) -> f64 {
        self.samples.iter().copied().fold(0.0, f64::max)
    }
}

#[derive(Debug, Default)]
pub(crate) struct PerformanceStats {
    pub(crate) frame: MetricSummary,
    pub(crate) mna_ms: f64,
    pub(crate) erc_ms: f64,
    pub(crate) netlist_ms: f64,
    pub(crate) simulation_cache_hit: bool,
    pub(crate) simulation_cache_hits: u64,
    pub(crate) simulation_cache_misses: u64,
    pub(crate) netlist_cache_hit: bool,
    pub(crate) netlist_cache_hits: u64,
    pub(crate) netlist_cache_misses: u64,
    pub(crate) flow_cache_hit: bool,
    pub(crate) rendered_components: usize,
    pub(crate) rendered_wire_segments: usize,
    pub(crate) flow_particles: usize,
    pub(crate) visible_flow_wires: usize,
}

// Transitional field-access bridge while call sites migrate to explicit
// `document` borrowing. The owned data lives only in ProjectDocument.
impl std::ops::Deref for CircuitApp {
    type Target = ProjectDocument;

    fn deref(&self) -> &Self::Target {
        &self.document
    }
}

impl std::ops::DerefMut for CircuitApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.document
    }
}

impl CircuitApp {
    pub(crate) fn new() -> Self {
        // Load user part definitions before any circuit file is opened so
        // saved circuits that reference them restore with full pin data.
        let (_loaded, _notes) =
            load_custom_parts_dir(std::path::Path::new(crate::model::CUSTOM_PARTS_DIR));
        Self {
            document: ProjectDocument::default(),
            editor: EditorState::default(),
            analysis: AnalysisState::default(),
            grid: 20.0,
            snap: true,
            orthogonal_wires: true,
            show_pins: true,
            show_grid: true,
            simulate: true,
            simulation_run_state: SimulationRunState::Dirty,
            status: String::new(),
            workspace_state: WorkspaceState::default(),
            canvas: CanvasState::default(),
            palette_ui: PaletteState::default(),
            simulation_ui: SimulationUiState::default(),
            inspector_ui: InspectorState::default(),
            breadboard_ui: BreadboardUiState::default(),
            pcb_ui: PcbUiState::default(),
            zoom: 1.0,
            pan: Vec2::ZERO,
            hovered_net_wire: None,
            highlighted_net_wires: HashSet::new(),
            last_autorecover_revision: 0,
            document_id: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |duration| duration.as_nanos() as u64),
            show_find: false,
            find_query: String::new(),
            find_results: Vec::new(),
            find_result_idx: 0,
            pending_fit: false,
            inline_edit: None,
            context_menu: None,
            screenshot_pending: false,
            automated_capture_path: None,
            automated_capture_requested: false,
            performance: PerformanceStats::default(),
        }
    }

    fn handle_top_toolbar_action(&mut self, action: TopToolbarAction, ctx: &egui::Context) {
        match action {
            TopToolbarAction::SelectTool => {
                self.editor.tool = Tool::Select;
                self.editor.draft_wire.clear();
            }
            TopToolbarAction::WireTool => {
                self.editor.tool = Tool::Wire;
                self.editor.draft_wire.clear();
            }
            TopToolbarAction::Undo => self.undo(),
            TopToolbarAction::Redo => self.redo(),
            TopToolbarAction::Rotate => self.rotate_selected(),
            TopToolbarAction::Duplicate => self.duplicate_selected(),
            TopToolbarAction::Delete => self.delete_selected(),
            TopToolbarAction::Align(dir) => self.align_selected(dir),
            TopToolbarAction::Distribute { vertical } => self.distribute_selected(vertical),
            TopToolbarAction::ToggleFind => self.show_find = !self.show_find,
            TopToolbarAction::ZoomOut => self.zoom_by(1.0 / 1.25_f32),
            TopToolbarAction::ZoomIn => self.zoom_by(1.25_f32),
            TopToolbarAction::ZoomFit => self.zoom_to_fit(),
            TopToolbarAction::SaveJson => self.save_circuit_json(),
            TopToolbarAction::LoadJson => self.load_circuit_json(),
            TopToolbarAction::RecoverAutosave => self.recover_autosave(),
            TopToolbarAction::TidyWires => {
                self.execute_editor_command(crate::commands::EditorCommand::Wiring(
                    crate::commands::wiring::WiringCommand::Tidy { wire_id: None },
                ));
            }
            TopToolbarAction::ExportSvg => self.export_svg(),
            TopToolbarAction::ExportPng => self.export_png(ctx),
            TopToolbarAction::ExportCir => self.export_spice_netlist(),
            TopToolbarAction::ExportNetlistText => self.export_netlist_text(),
            TopToolbarAction::ExportBomCsv => self.export_bom_csv(),
            TopToolbarAction::ExportArduinoCode => self.export_arduino_code(),
            TopToolbarAction::BlankSchematic => {
                self.reset_canvas();
                self.status = "Blank schematic ready.".to_string();
            }
            TopToolbarAction::AddPage => self.add_page(),
            TopToolbarAction::RemoveCurrentPage => self.remove_current_page(),
            TopToolbarAction::Help => self.workspace_state.show_help = true,
        }
    }

    fn load_palette_template(&mut self, template: PaletteTemplate) {
        match template {
            PaletteTemplate::Esp32Led => self.load_button_toggle_led_demo(),
            PaletteTemplate::Esp32Oled => self.load_esp32_oled_demo(),
            PaletteTemplate::Esp32Button => self.load_esp32_button_debounce_demo(),
            PaletteTemplate::ArduinoLed => self.load_arduino_led_demo(),
        }
    }

    fn zoom_by(&mut self, factor: f32) {
        let canvas_center = self.canvas.rect.center();
        let world_center = (canvas_center.to_vec2() - self.pan) / self.zoom;
        self.zoom = (self.zoom * factor).clamp(0.05, 8.0);
        self.pan = canvas_center.to_vec2() - world_center * self.zoom;
    }

    fn handle_validation_panel_action(&mut self, action: ValidationPanelAction) {
        match action {
            ValidationPanelAction::SelectComponent(id) => {
                self.editor.selected = Some(Selection::Component(id));
                self.highlighted_net_wires.clear();
                self.hovered_net_wire = None;
                if let Some(comp) = self.components.iter().find(|component| component.id == id) {
                    let canvas_center = self.canvas.rect.center();
                    self.pan = canvas_center.to_vec2() - comp.pos.to_vec2() * self.zoom;
                }
            }
            ValidationPanelAction::SelectWire(id) => {
                self.editor.selected = Some(Selection::Wire(id));
                self.hovered_net_wire = Some(id);
                self.highlighted_net_wires = self.same_net_wires(id);
                if let Some(wire) = self.wires.iter().find(|wire| wire.id == id) {
                    let canvas_center = self.canvas.rect.center();
                    self.pan = canvas_center.to_vec2() - wire_midpoint(wire).to_vec2() * self.zoom;
                }
            }
            ValidationPanelAction::ApplyAutoFix(auto_fix) => {
                self.apply_erc_auto_fix(auto_fix);
            }
        }
    }
}

impl CircuitApp {
    /// Render one complete application frame. Kept separate from the native
    /// frame wrapper so the exact same update path can run in offscreen
    /// performance and regression tests.
    pub(crate) fn update_ui(&mut self, ctx: &egui::Context) {
        let frame_started = std::time::Instant::now();
        self.poll_analysis_worker();
        if self.automated_capture_path.is_some() && !self.automated_capture_requested {
            self.automated_capture_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.screenshot_pending = true;
        }
        if std::mem::take(&mut self.workspace_state.repaint_requested) {
            ctx.request_repaint();
        }
        apply_app_style(ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "Cluster Circuits{}",
            if self.editor.history.dirty { " *" } else { "" }
        )));

        // ── Handle screenshot events ──────────────────────────────────────
        if self.screenshot_pending {
            let screenshot = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = screenshot {
                let path = self
                    .automated_capture_path
                    .as_deref()
                    .unwrap_or("cluster_circuit.png");
                let pixels: Vec<u8> = image
                    .pixels
                    .iter()
                    .flat_map(|color| [color.r(), color.g(), color.b(), color.a()])
                    .collect();
                match write_png(path, image.width(), image.height(), &pixels) {
                    Ok(()) => self.status = format!("Saved {path}."),
                    Err(error) => self.status = format!("PNG export failed: {error}"),
                }
                self.screenshot_pending = false;
                if self.automated_capture_path.is_some() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        let now = ctx.input(|input| input.time);
        if self.simulate != self.simulation_ui.last_simulation_enabled {
            self.simulation_ui.last_simulation_enabled = self.simulate;
            self.simulation_ui.flow_started_at = self.simulate.then_some(now);
        }
        let simulation = self.simulation_for_frame();
        let toolbar_simulation_summary = format!(
            "{} · {}",
            self.simulation_run_state.label(),
            simulation.summary
        );
        let inspector_netlist = self.current_netlist();
        let pcb_summary = self.pcb_dock_summary();

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            let toolbar_action = render_top_toolbar(
                ui,
                TopToolbarModel {
                    tool: self.editor.tool,
                    zoom: self.zoom,
                    simulation_summary: &toolbar_simulation_summary,
                    snap: &mut self.snap,
                    orthogonal_wires: &mut self.orthogonal_wires,
                    show_pins: &mut self.show_pins,
                    show_grid: &mut self.show_grid,
                    simulate: &mut self.simulate,
                    show_breadboard_view: &mut self.breadboard_ui.open,
                    show_voltage_labels: &mut self.simulation_ui.show_voltage_labels,
                    show_dc_overlay: &mut self.simulation_ui.show_dc_overlay,
                    show_oscilloscope: &mut self.simulation_ui.show_oscilloscope,
                    grid: &mut self.grid,
                    ac_freq_hz: &mut self.simulation_ui.ac_freq_hz,
                    current_flow: &mut self.simulation_ui.current_flow,
                    simulation_backend: &mut self.simulation_ui.backend,
                    show_performance_overlay: &mut self.workspace_state.show_performance_overlay,
                },
            );
            if let Some(action) = toolbar_action {
                self.handle_top_toolbar_action(action, ctx);
            }
            if self.simulate != self.simulation_ui.last_simulation_enabled {
                self.simulation_ui.last_simulation_enabled = self.simulate;
                self.simulation_ui.flow_started_at = self.simulate.then_some(now);
            }
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Workspace")
                        .size(10.5)
                        .color(crate::ui::theme::TEXT_MUTED),
                );
                for (workspace, label) in [
                    (Workspace::Schematic, "Schematic"),
                    (Workspace::Breadboard, "Breadboard"),
                    (Workspace::Pcb, "PCB"),
                    (Workspace::Code, "Code"),
                ] {
                    if ui
                        .selectable_label(self.workspace_state.workspace == workspace, label)
                        .clicked()
                    {
                        self.workspace_state.workspace = workspace;
                        match workspace {
                            Workspace::Schematic => {}
                            Workspace::Breadboard => self.breadboard_ui.open = true,
                            Workspace::Pcb => {
                                self.workspace_state.bottom_dock_open = false;
                                self.workspace_state.bottom_dock_tab = BottomDockTab::Pcb;
                            }
                            Workspace::Code => {
                                self.workspace_state.bottom_dock_open = true;
                                self.workspace_state.bottom_dock_tab = BottomDockTab::Logs;
                                self.status =
                                    "Code workspace: export an Arduino starter sketch from Export."
                                        .to_string();
                            }
                        }
                    }
                }
            });
            if !self.status.is_empty() {
                ui.label(
                    egui::RichText::new(&self.status)
                        .size(12.0)
                        .color(Color32::from_rgb(210, 218, 226)),
                );
            }
            ui.add_space(4.0);
        });

        egui::SidePanel::left("palette")
            .default_width(180.0)
            .width_range(160.0..=260.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("Parts")
                        .size(14.0)
                        .strong()
                        .color(Color32::from_rgb(220, 228, 236)),
                );
                ui.separator();
                ui.add_sized(
                    Vec2::new(ui.available_width(), 20.0),
                    egui::TextEdit::singleline(&mut self.palette_ui.filter)
                        .hint_text("Filter parts...")
                        .text_color(Color32::from_rgb(210, 218, 226)),
                );
                ui.add_space(2.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let filter = self.palette_ui.filter.clone();
                        let selected_custom = if self.editor.tool == Tool::Place(ComponentKind::Custom) {
                            self.editor.pending_custom_part.clone()
                        } else {
                            None
                        };
                        if let Some(action) = render_parts_palette(
                            ui,
                            &filter,
                            selected_part(self.editor.tool),
                            selected_custom.as_deref(),
                        ) {
                            match action {
                                PaletteAction::PlacePart { kind, label } => {
                                    self.editor.tool = Tool::Place(kind);
                                    self.editor.pending_custom_part = None;
                                    self.editor.draft_wire.clear();
                                    self.status = format!("Placing {label}. Click the canvas.");
                                }
                                PaletteAction::PlaceCustomPart { part_id } => {
                                    let name = custom_part(&part_id)
                                        .map(|def| def.name)
                                        .unwrap_or_else(|| part_id.clone());
                                    self.editor.pending_custom_part = Some(part_id);
                                    self.editor.tool = Tool::Place(ComponentKind::Custom);
                                    self.editor.draft_wire.clear();
                                    self.status = format!("Placing {name}. Click the canvas.");
                                }
                                PaletteAction::ReloadCustomParts => {
                                    self.reload_custom_parts();
                                }
                                PaletteAction::CreateSamplePart => {
                                    self.create_sample_custom_part();
                                }
                                PaletteAction::LoadTemplate(template) => {
                                    self.load_palette_template(template);
                                }
                            }
                        }

                        palette_section(ui, "Lessons: Current Flows", SectionMode::Open, |ui| {
                            if palette_action(ui, "LED Circuit").clicked() {
                                self.load_led_demo();
                            }
                            if palette_action(ui, "Switch + LED").clicked() {
                                self.load_switch_led_demo();
                            }
                            if palette_action(ui, "Parallel LEDs").clicked() {
                                self.load_parallel_led_demo();
                            }
                            if palette_action(ui, "Fused Lamp").clicked() {
                                self.load_lamp_demo();
                            }
                            if palette_action(ui, "Ohm Meter LED").clicked() {
                                self.load_ohms_law_meter_demo();
                            }
                            if palette_action(ui, "Transistor Switch").clicked() {
                                self.load_transistor_switch_demo();
                            }
                            if palette_action(ui, "Relay + Motor").clicked() {
                                self.load_motor_relay_demo();
                            }
                        });

                        palette_section(ui, "Lessons: Find The Problem", SectionMode::Open, |ui| {
                            if palette_action(ui, "Open Switch LED").clicked() {
                                self.load_open_switch_led_demo();
                            }
                            if palette_action(ui, "Capacitor Blocks DC").clicked() {
                                self.load_capacitor_dc_block_demo();
                            }
                            if palette_action(ui, "Missing Return Wire").clicked() {
                                self.load_missing_return_demo();
                            }
                            if palette_action(ui, "Reversed LED Warning").clicked() {
                                self.load_reversed_led_warning_demo();
                            }
                            if palette_action(ui, "Short Circuit Warning").clicked() {
                                self.load_short_circuit_lesson_demo();
                            }
                            if palette_action(ui, "GPIO Direct Motor").clicked() {
                                self.load_direct_gpio_motor_warning_demo();
                            }
                        });

                        palette_section(ui, "Examples: MCU Modules", SectionMode::Open, |ui| {
                            if palette_action(ui, "🔘 Button → LED Toggle").clicked() {
                                self.load_button_toggle_led_demo();
                            }
                            if palette_action(ui, "ESP32 Button Debounce").clicked() {
                                self.load_esp32_button_debounce_demo();
                            }
                            if palette_action(ui, "Voltage Divider").clicked() {
                                self.load_voltage_divider_demo();
                            }
                            if palette_action(ui, "Logic Inverter").clicked() {
                                self.load_logic_demo();
                            }
                            if palette_action(ui, "ESP32 + OLED").clicked() {
                                self.load_esp32_oled_demo();
                            }
                            if palette_action(ui, "ESP32 + Sensor").clicked() {
                                self.load_esp32_sensor_demo();
                            }
                            if palette_action(ui, "Arduino + LED").clicked() {
                                self.load_arduino_led_demo();
                            }
                            if palette_action(ui, "Arduino + OLED").clicked() {
                                self.load_arduino_oled_demo();
                            }
                            if palette_action(ui, "ESP32 + Motor Driver").clicked() {
                                self.load_motor_driver_demo();
                            }
                            if palette_action(ui, "Blank").clicked() {
                                self.reset_canvas();
                                self.status = "Blank schematic ready.".to_string();
                            }
                        });

                        palette_section(ui, "Circuit", SectionMode::Open, |ui| {
                            metric_row(ui, "Parts", self.components.len().to_string());
                            metric_row(ui, "Wires", self.wires.len().to_string());
                            metric_row(ui, "Pages", self.pages.len().to_string());
                            if compact_button(ui, "Breadboard View").clicked() {
                                self.breadboard_ui.open = true;
                            }
                            if self.simulate {
                                ui.add_space(4.0);
                                status_pill(ui, &simulation.summary, simulation_tone(&simulation));
                                metric_row(ui, "Confidence", simulation_status_label(simulation.status));
                                if !simulation.explanation.is_empty() {
                                    ui.label(
                                        egui::RichText::new(&simulation.explanation)
                                            .size(10.5)
                                            .color(Color32::from_rgb(156, 166, 176)),
                                    );
                                }
                                ui.add_space(4.0);
                                // DC operating point from MNA
                                if let Some(dc) = &simulation.dc {
                                    section_title(ui, "DC Operating Point");
                                    if let Some(voltage) = simulation.voltage {
                                        metric_row(ui, "Source", format!("{:.2} V", voltage));
                                    }
                                    if let Some(resistance) = simulation.resistance {
                                        metric_row(ui, "Load R", format_resistance(resistance));
                                    }
                                    if let Some(current) = simulation.current {
                                        metric_row(ui, "Loop I", format_current(current));
                                    }
                                    // Show top net voltages from MNA
                                    let mut net_v: Vec<f64> =
                                        dc.net_voltages.values().copied().collect();
                                    net_v.sort_by(|a, b| b.total_cmp(a));
                                    net_v.dedup();
                                    if !net_v.is_empty() {
                                        if let Some(&max_voltage) = net_v.first() {
                                            dc_metric_row(
                                                ui,
                                                "Max node V",
                                                &mna::format_voltage(max_voltage),
                                            );
                                        }
                                        if net_v.len() > 1
                                            && let Some(&min_voltage) = net_v.last() {
                                                dc_metric_row(
                                                    ui,
                                                    "Min node V",
                                                    &mna::format_voltage(min_voltage),
                                                );
                                            }
                                    }
                                    dc_metric_row(
                                        ui,
                                        "KCL residual",
                                        &mna::format_current(dc.max_kcl_residual),
                                    );
                                } else if let Some(error) = &simulation.dc_error {
                                    metric_row(ui, "DC solver", error.to_string());
                                } else {
                                    if let Some(voltage) = simulation.voltage {
                                        metric_row(ui, "Voltage", format!("{:.2} V", voltage));
                                    }
                                    if let Some(resistance) = simulation.resistance {
                                        metric_row(ui, "Resistance", format_resistance(resistance));
                                    }
                                    if let Some(current) = simulation.current {
                                        metric_row(ui, "Current", format_current(current));
                                    }
                                }
                            }
                        });

                        palette_section(
                            ui,
                            "Simulation Limits",
                            SectionMode::Collapsed,
                            |ui| {
                                ui.label("Educational DC operating-point solver.");
                                metric_row(ui, "Capacitors", "Open in DC");
                                metric_row(ui, "Inductors", "Short in DC");
                                metric_row(ui, "Transient", "Not available");
                                metric_row(ui, "PWM / startup", "Not simulated");
                                ui.label(
                                    egui::RichText::new(
                                        "Symbolic parts are checked by ERC but do not generate physical current.",
                                    )
                                    .size(10.5)
                                    .color(Color32::from_rgb(150, 160, 170)),
                                );
                            },
                        );

                        // ── Power Budget Panel ───────────────────────────────────
                        if self.simulate
                            && let Some(dc) = &simulation.dc
                                && !dc.component_power.is_empty() {
                                    palette_section(ui, "Power Budget", SectionMode::Collapsed, |ui| {
                                        let dissipated_power: f64 = dc.component_power
                                            .iter()
                                            .filter(|(id, _)| {
                                                dc.component_power_role.get(id)
                                                    == Some(&mna::ComponentPowerRole::Dissipating)
                                            })
                                            .map(|(_, power)| *power)
                                            .sum();
                                        let supplied_power: f64 = dc.component_power
                                            .iter()
                                            .filter(|(id, _)| {
                                                dc.component_power_role.get(id)
                                                    == Some(&mna::ComponentPowerRole::Supplying)
                                            })
                                            .map(|(_, power)| *power)
                                            .sum();
                                        let comp_id_map: std::collections::HashMap<u64, String> =
                                            self.components.iter().map(|c| (c.id, c.label.clone())).collect();

                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new("Dissipated")
                                                .size(11.0).strong()
                                                .color(Color32::from_rgb(255, 200, 80)));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                ui.label(egui::RichText::new(mna::format_power(dissipated_power))
                                                    .size(11.0).monospace()
                                                    .color(Color32::from_rgb(255, 200, 80)));
                                            });
                                        });
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new("Supplied")
                                                .size(10.5)
                                                .color(Color32::from_rgb(130, 190, 255)));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                ui.label(egui::RichText::new(mna::format_power(supplied_power))
                                                    .size(10.5).monospace()
                                                    .color(Color32::from_rgb(130, 190, 255)));
                                            });
                                        });
                                        ui.separator();

                                        let mut powers: Vec<(String, f64)> = dc.component_power
                                            .iter()
                                            .filter(|(id, power)| {
                                                **power > 1e-9
                                                    && dc.component_power_role.get(id)
                                                        == Some(&mna::ComponentPowerRole::Dissipating)
                                            })
                                            .map(|(&id, &p)| {
                                                let label = comp_id_map.get(&id)
                                                    .cloned()
                                                    .unwrap_or_else(|| format!("#{}", id));
                                                (label, p)
                                            })
                                            .collect();
                                        powers.sort_by(|a, b| b.1.total_cmp(&a.1));

                                        for (label, power) in &powers {
                                            let frac = if dissipated_power > 1e-12 {
                                                (power / dissipated_power).clamp(0.0, 1.0) as f32
                                            } else { 0.0 };
                                            ui.horizontal(|ui| {
                                                ui.add_sized(Vec2::new(55.0, 14.0),
                                                    egui::Label::new(egui::RichText::new(label)
                                                        .size(10.5).monospace()
                                                        .color(Color32::from_rgb(200, 210, 220))));
                                                let (bar_rect, _) = ui.allocate_exact_size(
                                                    Vec2::new(60.0, 11.0), egui::Sense::hover());
                                                ui.painter().rect_filled(bar_rect, 2.0,
                                                    Color32::from_rgba_unmultiplied(40, 50, 60, 200));
                                                let filled = egui::Rect::from_min_size(
                                                    bar_rect.min, Vec2::new(60.0 * frac, 11.0));
                                                let heat = Color32::from_rgb(
                                                    (120.0 + 135.0 * frac) as u8,
                                                    (200.0_f32 - 130.0 * frac) as u8,
                                                    40,
                                                );
                                                ui.painter().rect_filled(filled, 2.0, heat);
                                                ui.label(egui::RichText::new(mna::format_power(*power))
                                                    .size(10.0).monospace()
                                                    .color(Color32::from_rgb(230, 210, 140)));
                                            });
                                        }
                                    });
                                }

                        palette_section(ui, "Shortcuts", SectionMode::Collapsed, |ui| {
                            metric_row(ui, "W", "wire tool");
                            metric_row(ui, "S", "select tool");
                            metric_row(ui, "F", "zoom to fit");
                            metric_row(ui, "R", "rotate");
                            metric_row(ui, "Del", "delete");
                            metric_row(ui, "Enter", "finish wire");
                            metric_row(ui, "Esc", "cancel / select");
                            metric_row(ui, "Q", "place resistor");
                            metric_row(ui, "A", "place capacitor");
                            metric_row(ui, "I", "place inductor");
                            metric_row(ui, "D", "place diode");
                            metric_row(ui, "Z", "place zener");
                            metric_row(ui, "E", "place LED");
                            metric_row(ui, "N", "place NPN BJT");
                            metric_row(ui, "P", "place PNP BJT");
                            metric_row(ui, "B", "place battery");
                            metric_row(ui, "G", "place ground");
                        });
                    });
            });

        egui::SidePanel::right("inspector")
            .default_width(248.0)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some(report) = lesson_report(&self.components, &simulation) {
                    render_lesson_report(ui, &report);
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(4.0);
                }
                render_inspector_header(ui, &mut self.inspector_ui.active_tab);
                match self.editor.selected {
                    Some(Selection::Component(id)) => {
                        let mut inspector_changed = false;
                        let mut inspector_status: Option<String> = None;
                        let mut edited_properties = None;
                        if let Some(mut component) =
                            self.components.iter().find(|c| c.id == id).cloned()
                        {
                            let metadata = electrical_metadata(component.kind);
                            status_pill(
                                ui,
                                component_kind_label(component.kind),
                                StatusTone::Neutral,
                            );
                            ui.add_space(8.0);
                            if self.inspector_ui.active_tab == InspectorTab::Properties {
                            if edit_row(ui, "Label", &mut component.label)
                                || edit_row(ui, "Value", &mut component.value)
                            {
                                inspector_changed = true;
                            }
                            // ── Value quick-pick presets ──────────────────────────
                            match component.kind {
                                ComponentKind::Resistor => {
                                    ui.label(
                                        egui::RichText::new("Quick values:")
                                            .size(10.5)
                                            .color(Color32::from_rgb(150, 160, 170)),
                                    );
                                    ui.horizontal_wrapped(|ui| {
                                        for &v in &[
                                            "100", "220", "330", "470", "1k", "2.2k", "4.7k",
                                            "10k", "22k", "47k", "100k", "1M",
                                        ] {
                                            if ui.small_button(v).clicked() {
                                                component.value = v.to_string();
                                                inspector_changed = true;
                                            }
                                        }
                                    });
                                }
                                ComponentKind::Capacitor => {
                                    ui.label(
                                        egui::RichText::new("Quick values:")
                                            .size(10.5)
                                            .color(Color32::from_rgb(150, 160, 170)),
                                    );
                                    ui.horizontal_wrapped(|ui| {
                                        for &v in &[
                                            "10pF", "100pF", "1nF", "10nF", "100nF", "1uF", "10uF",
                                            "100uF", "1000uF",
                                        ] {
                                            if ui.small_button(v).clicked() {
                                                component.value = v.to_string();
                                                inspector_changed = true;
                                            }
                                        }
                                    });
                                }
                                ComponentKind::Led => {
                                    ui.label(
                                        egui::RichText::new("Color:")
                                            .size(10.5)
                                            .color(Color32::from_rgb(150, 160, 170)),
                                    );
                                    ui.horizontal_wrapped(|ui| {
                                        for &v in
                                            &["red", "green", "blue", "yellow", "white", "orange"]
                                        {
                                            if ui.small_button(v).clicked() {
                                                component.value = v.to_string();
                                                inspector_changed = true;
                                            }
                                        }
                                    });
                                }
                                ComponentKind::Battery | ComponentKind::VSource => {
                                    ui.label(
                                        egui::RichText::new("Voltage:")
                                            .size(10.5)
                                            .color(Color32::from_rgb(150, 160, 170)),
                                    );
                                    ui.horizontal_wrapped(|ui| {
                                        for &v in &["1.5V", "3.3V", "3.7V", "5V", "9V", "12V"] {
                                            if ui.small_button(v).clicked() {
                                                component.value = v.to_string();
                                                inspector_changed = true;
                                            }
                                        }
                                    });
                                }
                                _ => {}
                            }
                            if component_is_switch(component.kind) {
                                let mut closed =
                                    component_conductance(&component) != Conductance::Open;
                                if ui.checkbox(&mut closed, "Closed").changed() {
                                    component.value = if closed {
                                        "closed".to_string()
                                    } else {
                                        "open".to_string()
                                    };
                                    inspector_changed = true;
                                    inspector_status =
                                        Some(format!("{}: {}", component.label, component.value));
                                }
                            }
                            metric_row(ui, "Rotation", format!("{}°", component.rotation));
                            metric_row(
                                ui,
                                "Position",
                                format!("{:.0}, {:.0}", component.pos.x, component.pos.y),
                            );
                            }
                            if self.inspector_ui.active_tab == InspectorTab::Model {
                            ui.add_space(8.0);
                            section_title(ui, "Electrical Model");
                            metric_row(ui, "Model", metadata.model_name);
                            simulation_support_row(ui, "Simulation", metadata.simulation);
                            if let Some(warning) = metadata.simulation.warning() {
                                ui.label(
                                    egui::RichText::new(warning)
                                        .size(10.5)
                                        .color(Color32::from_rgb(230, 170, 90)),
                                );
                            }
                            if let Some(pin_count) = metadata.pin_count {
                                metric_row(ui, "Pins", pin_count.to_string());
                            }
                            if let Some((minimum, maximum)) = metadata.voltage_range {
                                metric_row(
                                    ui,
                                    "Voltage range",
                                    format!("{minimum:.1}V to {maximum:.1}V"),
                                );
                            }
                            if let Some(max_current) = metadata.max_current {
                                metric_row(
                                    ui,
                                    "Max current",
                                    mna::format_current(max_current as f64),
                                );
                            }
                            if metadata.needs_current_limit {
                                metric_row(ui, "Protection", "Series resistor required");
                            }
                            if metadata.needs_driver {
                                metric_row(ui, "Drive", "External driver required");
                            }
                            }
                            if self.inspector_ui.active_tab == InspectorTab::Simulation {
                            // ── DC operating-point results ─────────────────
                            if let Some(dc) = &simulation.dc {
                                let cid = component.id;
                                let show_v = dc.component_voltage.get(&cid).copied();
                                let show_i = dc.branch_current.get(&cid).copied();
                                let show_p = dc.component_power.get(&cid).copied();
                                if show_v.is_some() || show_i.is_some() {
                                    ui.add_space(8.0);
                                    section_title(ui, "DC Operating Point");
                                    if let Some(v) = show_v {
                                        dc_metric_row(ui, "Voltage", &mna::format_voltage(v));
                                    }
                                    if let Some(i) = show_i {
                                        dc_metric_row(ui, "Current", &mna::format_current(i));
                                    }
                                    if let Some(p) = show_p {
                                        dc_metric_row(ui, "Power", &mna::format_power(p));
                                    }
                                    if let Some(role) = dc.component_power_role.get(&cid) {
                                        metric_row(
                                            ui,
                                            "Power role",
                                            match role {
                                                mna::ComponentPowerRole::Dissipating => {
                                                    "Dissipating"
                                                }
                                                mna::ComponentPowerRole::Supplying => "Supplying",
                                                mna::ComponentPowerRole::Unknown => "Unknown",
                                            },
                                        );
                                    }
                                }
                            }

                            // ── AC impedance + time constant for reactive components ──
                            {
                                let f = self.simulation_ui.ac_freq_hz as f64;
                                let omega = 2.0 * std::f64::consts::PI * f;
                                match component.kind {
                                    ComponentKind::Capacitor => {
                                        let c =
                                            mna::parse_si_value(&component.value).unwrap_or(1e-6);
                                        if c > 0.0 {
                                            let z = 1.0 / (omega * c);
                                            ui.add_space(8.0);
                                            section_title(ui, "AC / Transient");
                                            dc_metric_row(
                                                ui,
                                                &format!("Xc @ {:.0}Hz", f),
                                                &format_resistance(z as f32),
                                            );
                                            // RC time constant: needs a series resistor — show C value hint
                                            dc_metric_row(
                                                ui,
                                                "τ = RC",
                                                &format!("{}×R", mna::format_si(c, "F")),
                                            );
                                            dc_metric_row(
                                                ui,
                                                "f_cutoff = 1/(2πRC)",
                                                "depends on R",
                                            );
                                        }
                                    }
                                    ComponentKind::Inductor => {
                                        let l =
                                            mna::parse_si_value(&component.value).unwrap_or(1e-3);
                                        let z = omega * l;
                                        ui.add_space(8.0);
                                        section_title(ui, "AC / Transient");
                                        dc_metric_row(
                                            ui,
                                            &format!("Xl @ {:.0}Hz", f),
                                            &format_resistance(z as f32),
                                        );
                                        dc_metric_row(
                                            ui,
                                            "τ = L/R",
                                            &format!("{}÷R", mna::format_si(l, "H")),
                                        );
                                    }
                                    _ => {}
                                }
                            }

                            let component_pins = inspector_netlist
                                .pins
                                .iter()
                                .filter(|pin| pin.component_id == component.id)
                                .collect::<Vec<_>>();
                            if !component_pins.is_empty() {
                                ui.add_space(8.0);
                                section_title(ui, "Pins");
                                for pin in component_pins {
                                    let net_name = inspector_netlist
                                        .nets
                                        .iter()
                                        .find(|net| net.id == pin.net_id)
                                        .map(|net| net.name.as_str())
                                        .unwrap_or("UNKNOWN");
                                    let voltage = simulation.dc.as_ref().and_then(|dc| {
                                        inspector_netlist
                                            .wire_nets
                                            .iter()
                                            .find(|(_, net_id)| **net_id == pin.net_id)
                                            .and_then(|(wire_id, _)| dc.wire_voltage.get(wire_id))
                                    });
                                    let value = voltage.map_or_else(
                                        || format!("{net_name} / {:?}", pin.electrical_type),
                                        |voltage| {
                                            format!(
                                                "{net_name} / {}",
                                                mna::format_voltage(*voltage)
                                            )
                                        },
                                    );
                                    metric_row(ui, &pin.pin_name, value);
                                }
                            }
                            }
                            if let Some(warning) = simulation.component_warnings.get(&component.id)
                            {
                                ui.add_space(6.0);
                                egui::Frame::NONE
                                    .fill(Color32::from_rgb(58, 28, 24))
                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(160, 64, 54)))
                                    .corner_radius(egui::CornerRadius::same(5))
                                    .inner_margin(egui::Margin::symmetric(8, 5))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(warning)
                                                .size(11.0)
                                                .color(Color32::from_rgb(255, 120, 100)),
                                        );
                                    });
                            }
                            if inspector_changed {
                                edited_properties = Some((component.label, component.value));
                            }
                        }
                        if let Some((label, value)) = edited_properties {
                            self.execute_editor_command(
                                crate::commands::EditorCommand::Properties(
                                    crate::commands::properties::PropertiesCommand::SetComponentProperties {
                                        component_id: id,
                                        label,
                                        value,
                                    },
                                ),
                            );
                        }
                        if let Some(status) = inspector_status {
                            self.status = status;
                        }
                    }
                    Some(Selection::Wire(id)) => {
                        if let Some(wire) = self.wires.iter().find(|w| w.id == id) {
                            status_pill(ui, "Wire / Net", StatusTone::Neutral);
                            ui.add_space(8.0);
                            if self.inspector_ui.active_tab == InspectorTab::Properties {
                                metric_row(ui, "Points", wire.points.len().to_string());
                                metric_row(ui, "Length", format!("{:.0}px", wire_length(wire)));
                            }
                            if let Some(net_id) = inspector_netlist.wire_nets.get(&wire.id) {
                                let net_name = inspector_netlist
                                    .nets
                                    .iter()
                                    .find(|net| net.id == *net_id)
                                    .map(|net| net.name.as_str())
                                    .unwrap_or("UNKNOWN");
                                metric_row(ui, "Net", net_name);
                                if self.inspector_ui.active_tab == InspectorTab::Properties {
                                    let connected = inspector_netlist
                                        .pins
                                        .iter()
                                        .filter(|pin| pin.net_id == *net_id)
                                        .map(|pin| format!("{}.{}", pin.component_label, pin.pin_name))
                                        .collect::<Vec<_>>();
                                    metric_row(ui, "Connected pins", if connected.is_empty() { "none".to_string() } else { connected.join(", ") });
                                }
                            }
                            if self.inspector_ui.active_tab == InspectorTab::Properties {
                                metric_row(ui, "Status", if inspector_netlist.floating_wires.contains(&wire.id) { "Floating" } else if inspector_netlist.isolated_wires.contains(&wire.id) { "Open / one-pin connection" } else { "Connected" });
                            }
                            if self.inspector_ui.active_tab == InspectorTab::Simulation
                                && let Some(dc) = &simulation.dc {
                                if let Some(&wv) = dc.wire_voltage.get(&wire.id) {
                                    ui.add_space(8.0);
                                    section_title(ui, "DC Wire");
                                    dc_metric_row(ui, "Voltage", &mna::format_voltage(wv));
                                }
                                if dc.wire_current_known.contains(&wire.id)
                                    && let Some(&current) = dc.wire_current.get(&wire.id)
                                {
                                    let direction = if current < 0.0 {
                                        "end -> start"
                                    } else {
                                        "start -> end"
                                    };
                                    dc_metric_row(
                                        ui,
                                        "Current",
                                        &mna::format_current(current.abs()),
                                    );
                                    metric_row(ui, "Direction", direction);
                                    metric_row(
                                        ui,
                                        "Animation",
                                        if !self.simulate {
                                            "Simulation off"
                                        } else if !self.simulation_ui.current_flow.enabled {
                                            "Disabled"
                                        } else if current.abs()
                                            < self
                                                .simulation_ui
                                                .current_flow
                                                .minimum_visible_current_a
                                        {
                                            "Below display threshold"
                                        } else {
                                            "Active"
                                        },
                                    );
                                } else if dc.wire_current.contains_key(&wire.id) {
                                    metric_row(ui, "Segment current", "Current varies by segment");
                                    metric_row(ui, "Direction", "Unavailable");
                                    metric_row(ui, "Animation", "Skipped for safety");
                                }
                                if let (Some(voltage), Some(current)) = (
                                    dc.wire_voltage.get(&wire.id),
                                    dc.wire_current.get(&wire.id).filter(|_| dc.wire_current_known.contains(&wire.id)),
                                ) {
                                    dc_metric_row(ui, "Power estimate", &mna::format_power((voltage * current).abs()));
                                }
                            } else if self.inspector_ui.active_tab == InspectorTab::Simulation {
                                metric_row(ui, "Voltage", "Unavailable");
                                metric_row(ui, "Segment current", "Unavailable");
                                metric_row(ui, "Animation", if self.simulate { "No valid DC result" } else { "Simulation off" });
                            }
                            if self.inspector_ui.active_tab == InspectorTab::Model {
                                metric_row(ui, "Element", "Ideal zero-resistance conductor");
                                metric_row(ui, "DC model", "Equipotential net segment");
                                ui.label(egui::RichText::new("Current direction is reported only when the solver can prove one signed current for every segment.").size(10.5).color(crate::ui::theme::TEXT_SECONDARY));
                            }
                        }
                    }
                    None => {
                        ui.label(
                            egui::RichText::new("Nothing selected")
                                .color(Color32::from_rgb(145, 154, 164)),
                        );
                    }
                }
            });

        // ── Page tabs (bottom strip above status bar) ────────────────────────
        egui::TopBottomPanel::bottom("page_tabs").show(ctx, |ui| {
            let page_names = self
                .pages
                .iter()
                .map(|page| page.name.clone())
                .collect::<Vec<_>>();
            if let Some(action) = render_page_tabs(ui, &page_names, self.current_page) {
                match action {
                    PageTabsAction::SwitchTo(idx) => self.switch_page(idx),
                    PageTabsAction::RenameDefault(idx) => {
                        self.pages[idx].name = format!("Page {}", idx + 1);
                    }
                    PageTabsAction::AddPage => self.add_page(),
                }
            }
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            let active_tool = match self.editor.tool {
                Tool::Select => "Select".to_string(),
                Tool::Wire => "Wire".to_string(),
                Tool::Place(kind) => format!("Place {}", component_kind_label(kind)),
            };
            let simulation_text = format!(
                "{}{}",
                simulation.summary,
                match simulation_warning_count(&simulation) {
                    0 => String::new(),
                    count => format!(" / {count} warning(s)"),
                }
            );
            let page_name = self
                .pages
                .get(self.current_page)
                .map(|page| page.name.as_str())
                .unwrap_or("Page");
            render_status_bar(
                ui,
                StatusBarModel {
                    active_tool,
                    grid: self.grid,
                    zoom: self.zoom,
                    snap: self.snap,
                    simulation_text,
                    simulation_color: simulation_text_color(&simulation),
                    selection: selection_summary(
                        self.editor.selected,
                        &self.components,
                        &self.wires,
                    ),
                    component_count: self.components.len(),
                    wire_count: self.wires.len(),
                    cursor_world: self.canvas.cursor_world_pos,
                    dirty: self.editor.history.dirty,
                    page_name,
                },
            );
        });

        if self.workspace_state.bottom_dock_open {
            egui::TopBottomPanel::bottom("bottom_dock")
                .default_height(190.0)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Analysis").strong().size(11.0));
                        if ui.small_button("Collapse").clicked() {
                            self.workspace_state.bottom_dock_open = false;
                        }
                    });
                    if let Some(action) = render_bottom_dock(
                        ui,
                        BottomDockModel {
                            active_tab: self.workspace_state.bottom_dock_tab,
                            violations: &simulation.erc,
                            has_components: !self.components.is_empty(),
                            simulation: &simulation,
                            netlist: &inspector_netlist,
                            breadboard_enabled: self.breadboard_ui.open,
                            pcb: &pcb_summary,
                            status: &self.status,
                        },
                    ) {
                        match action {
                            BottomDockAction::SetTab(tab) => {
                                self.workspace_state.bottom_dock_tab = tab
                            }
                            BottomDockAction::Validation(validation_action) => {
                                self.handle_validation_panel_action(validation_action);
                            }
                            BottomDockAction::OpenBreadboard => {
                                self.breadboard_ui.open = true;
                                self.workspace_state.bottom_dock_tab = BottomDockTab::Breadboard;
                            }
                            BottomDockAction::UpdatePcb => {
                                self.update_pcb_from_schematic();
                                self.workspace_state.bottom_dock_tab = BottomDockTab::Pcb;
                            }
                            BottomDockAction::AutoPlacePcb => {
                                self.auto_place_pcb_footprints();
                                self.workspace_state.bottom_dock_tab = BottomDockTab::Pcb;
                            }
                            BottomDockAction::FitPcbBoard => {
                                self.fit_pcb_board_to_contents();
                                self.workspace_state.bottom_dock_tab = BottomDockTab::Pcb;
                            }
                            BottomDockAction::RoutePcbRatsnest => {
                                self.route_pcb_ratsnest();
                                self.workspace_state.bottom_dock_tab = BottomDockTab::Pcb;
                            }
                            BottomDockAction::SelectPcbDrc(index) => {
                                self.select_pcb_drc_violation(index);
                                self.workspace_state.bottom_dock_tab = BottomDockTab::Pcb;
                            }
                            BottomDockAction::SavePcbProject => {
                                self.save_project_folder();
                                self.workspace_state.bottom_dock_tab = BottomDockTab::Pcb;
                            }
                            BottomDockAction::LoadPcbProject => {
                                self.load_project_folder();
                                self.workspace_state.bottom_dock_tab = BottomDockTab::Pcb;
                            }
                            BottomDockAction::ExportPcbFabrication => {
                                self.export_pcb_fabrication_files();
                                self.workspace_state.bottom_dock_tab = BottomDockTab::Pcb;
                            }
                        }
                    }
                });
        } else {
            egui::TopBottomPanel::bottom("bottom_dock_collapsed")
                .exact_height(26.0)
                .show(ctx, |ui| {
                    if ui.small_button("Show analysis").clicked() {
                        self.workspace_state.bottom_dock_open = true;
                    }
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.workspace_state.workspace == Workspace::Pcb {
                let nets = self
                    .analysis
                    .pcb_cad
                    .as_ref()
                    .map(|cad| cad.nets.clone())
                    .unwrap_or_default();
                let commands = crate::ui::pcb_workspace::render_pcb_workspace(
                    ui,
                    &self.document.board,
                    &nets,
                    &mut self.pcb_ui.workspace,
                );
                for command in commands {
                    self.execute_editor_command(crate::commands::EditorCommand::Pcb(command));
                }
                if self.analysis.dirty_flags.pcb_drc_dirty
                    && let Some(cad) = self.analysis.pcb_cad.clone()
                {
                    self.schedule_full_pcb_analysis(&cad);
                }
                return;
            }
            let available = ui.available_size();
            let (response, painter) = ui.allocate_painter(available, Sense::click_and_drag());
            let rect = response.rect;
            self.canvas.rect = rect;

            // Deferred zoom-to-fit: apply now that canvas rect is real
            if self.pending_fit && rect.width() > 1.0 && rect.is_finite() {
                self.zoom_to_fit_silent();
                self.pending_fit = false;
                // Schedule one more repaint so the new view renders
                ctx.request_repaint();
            }

            let show_flow = flow_overlay_enabled(&simulation, self.simulate)
                && self.simulation_ui.current_flow.enabled;

            // ── View transform ──────────────────────────────────────────
            let view = CanvasView {
                zoom: self.zoom,
                pan: self.pan,
                origin: rect.min,
            };

            // Scroll-wheel zoom (centered on cursor)
            let scroll_delta = ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll_delta != 0.0
                && rect.contains(ctx.input(|i| i.pointer.hover_pos().unwrap_or_default()))
            {
                let factor = if scroll_delta > 0.0 {
                    1.08_f32
                } else {
                    1.0 / 1.08
                };
                let new_zoom = (self.zoom * factor).clamp(0.2, 5.0);
                if let Some(cursor) = ctx.input(|i| i.pointer.hover_pos()) {
                    // Keep the world point under the cursor fixed
                    let world_under = view.to_world(cursor);
                    let new_view = CanvasView {
                        zoom: new_zoom,
                        pan: self.pan,
                        origin: rect.min,
                    };
                    let new_screen = new_view.to_screen(world_under);
                    self.pan += cursor - new_screen;
                }
                self.zoom = new_zoom;
            }

            // Middle-mouse, Alt+drag, or Space+drag to pan
            let space_held = ctx.input(|i| i.key_down(egui::Key::Space));
            if space_held
                && ctx
                    .input(|i| i.pointer.hover_pos())
                    .is_some_and(|p| rect.contains(p))
            {
                ctx.set_cursor_icon(egui::CursorIcon::Grab);
            }
            let panning = ctx.input(|i| i.pointer.middle_down())
                || (response.dragged() && ctx.input(|i| i.modifiers.alt))
                || (response.dragged() && space_held);
            if panning {
                self.pan += ctx.input(|i| i.pointer.delta());
                ctx.request_repaint();
            }

            // Recompute view after possible pan/zoom changes this frame
            let view = CanvasView {
                zoom: self.zoom,
                pan: self.pan,
                origin: rect.min,
            };

            // ── Draw ─────────────────────────────────────────────────────
            if self.show_grid {
                draw_grid(&painter, rect, self.grid, view);
            } else {
                painter.rect_filled(rect, 0.0, crate::ui::theme::CLUSTER_THEME.canvas_background);
            }
            if self.components.is_empty() && self.wires.is_empty() {
                draw_empty_canvas_hint(&painter, rect);
                let start_rect = Rect::from_center_size(rect.center(), Vec2::new(360.0, 190.0));
                let mut start_action = None;
                ui.scope_builder(egui::UiBuilder::new().max_rect(start_rect), |ui| {
                    crate::ui::theme::panel_frame().show(ui, |ui| {
                        ui.heading("Start a circuit");
                        ui.label(
                            egui::RichText::new(
                                "Choose a proven example or begin with an empty schematic.",
                            )
                            .size(11.0)
                            .color(crate::ui::theme::TEXT_SECONDARY),
                        );
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            for (label, action) in [
                                ("New blank", 0),
                                ("ESP32 + LED", 1),
                                ("ESP32 + OLED", 2),
                                ("Arduino + LED", 3),
                                ("Open circuit", 4),
                                ("Recover autosave", 5),
                            ] {
                                if ui
                                    .add_sized(Vec2::new(105.0, 26.0), egui::Button::new(label))
                                    .clicked()
                                {
                                    start_action = Some(action);
                                }
                            }
                        });
                    });
                });
                match start_action {
                    Some(0) => self.status = "Blank schematic ready.".to_string(),
                    Some(1) => self.load_button_toggle_led_demo(),
                    Some(2) => self.load_esp32_oled_demo(),
                    Some(3) => self.load_arduino_led_demo(),
                    Some(4) => self.load_open_switch_led_demo(),
                    Some(5) => self.recover_autosave(),
                    _ => {}
                }
            }

            let dc_imax = simulation
                .dc
                .as_ref()
                .map(|dc| {
                    dc.branch_current
                        .values()
                        .map(|v| v.abs())
                        .fold(0.0_f64, f64::max)
                })
                .unwrap_or(1.0);
            if self.analysis.schematic_spatial_revision
                != self.analysis.revisions.schematic_geometry
            {
                self.analysis
                    .schematic_spatial_index
                    .sync(&self.document.wires);
                self.analysis.schematic_spatial_revision =
                    self.analysis.revisions.schematic_geometry;
            }
            let world_padding = 24.0 / self.zoom.max(0.01);
            let world_min = view.to_world(rect.min) - Vec2::splat(world_padding);
            let world_max = view.to_world(rect.max) + Vec2::splat(world_padding);
            let indexed_visible_wires = self
                .analysis
                .schematic_spatial_index
                .query_rect(world_min, world_max);
            self.performance.rendered_wire_segments = 0;
            for wire in &self.document.wires {
                if !indexed_visible_wires.contains(&wire.id) {
                    continue;
                }
                let visible = wire
                    .points
                    .iter()
                    .any(|point| rect.expand(24.0).contains(view.to_screen(*point)))
                    || wire.points.windows(2).any(|segment| {
                        Rect::from_two_pos(view.to_screen(segment[0]), view.to_screen(segment[1]))
                            .expand(4.0)
                            .intersects(rect)
                    });
                if !visible {
                    continue;
                }
                self.performance.rendered_wire_segments += wire.points.len().saturating_sub(1);
                let energized = simulation.energized_wires.contains(&wire.id);
                let net_highlighted = self.highlighted_net_wires.contains(&wire.id)
                    && !self.highlighted_net_wires.is_empty();
                let dc_v = simulation
                    .dc
                    .as_ref()
                    .and_then(|dc| dc.wire_voltage.get(&wire.id).copied());
                let dc_i = simulation
                    .dc
                    .as_ref()
                    .filter(|dc| dc.wire_current_known.contains(&wire.id))
                    .and_then(|dc| dc.wire_current.get(&wire.id).copied());
                let dc_vmax = simulation.dc.as_ref().map(|dc| dc.vmax).unwrap_or(1.0);
                let open_wire = inspector_netlist.floating_wires.contains(&wire.id)
                    || inspector_netlist.isolated_wires.contains(&wire.id);
                draw_wire(
                    &painter,
                    wire,
                    self.editor.selected == Some(Selection::Wire(wire.id)),
                    energized,
                    simulation.shorted && energized,
                    false,
                    0.0,
                    net_highlighted,
                    dc_v,
                    dc_i,
                    dc_vmax,
                    dc_imax,
                    self.simulation_ui.show_voltage_labels && simulation.dc.is_some(),
                    open_wire,
                    view,
                );
            }

            let flow_rebuilt = self.analysis.current_flow_cache.rebuild_if_needed(
                FlowCacheKey {
                    geometry_revision: self.analysis.revisions.schematic_geometry,
                    simulation_revision: self.analysis.simulation_revision,
                },
                &self.document.wires,
                simulation
                    .dc
                    .as_ref()
                    .filter(|_| show_flow && !simulation.shorted),
            );
            self.performance.flow_cache_hit = !flow_rebuilt;
            let flow_stats = if show_flow {
                render_current_flow(
                    &self.analysis.current_flow_cache,
                    FlowRenderInput {
                        painter: &painter,
                        viewport: rect,
                        view,
                        time_seconds: ctx.input(|i| i.time),
                        settings: &self.simulation_ui.current_flow,
                        selected_wire: match self.editor.selected {
                            Some(Selection::Wire(id)) => Some(id),
                            _ => None,
                        },
                        highlighted_wires: &self.highlighted_net_wires,
                        startup_progress: self.simulation_ui.flow_started_at.map_or(
                            1.0,
                            |started| {
                                ((now - started)
                                    / (crate::ui::theme::SIMULATION_START_MS as f64 / 1000.0))
                                    .clamp(0.0, 1.0) as f32
                            },
                        ),
                    },
                )
            } else {
                crate::ui::canvas::current_flow::FlowRenderStats::default()
            };
            self.performance.flow_particles = flow_stats.particle_count;
            self.performance.visible_flow_wires = flow_stats.visible_wire_count;
            if flow_stats.needs_repaint {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                    1.0 / self.simulation_ui.current_flow.quality.fps().min(30) as f64,
                ));
            }

            // Compute connected pins for unconnected-pin rendering. This can be
            // expensive on larger circuits, so it is cached by circuit revision.
            let connected_pins = self.current_connected_pins();

            self.performance.rendered_components = 0;
            for component in &self.document.components {
                let cid = component.id;
                let size = component_size(component) * view.zoom;
                if !Rect::from_center_size(
                    view.to_screen(component.pos),
                    size.max(Vec2::splat(12.0)),
                )
                .expand(24.0)
                .intersects(rect)
                {
                    continue;
                }
                self.performance.rendered_components += 1;
                let dc_v = simulation
                    .dc
                    .as_ref()
                    .and_then(|dc| dc.component_voltage.get(&cid).copied());
                let dc_i = simulation
                    .dc
                    .as_ref()
                    .and_then(|dc| dc.branch_current.get(&cid).copied());
                draw_component(
                    &painter,
                    component,
                    self.editor.selected == Some(Selection::Component(cid)),
                    self.show_pins && self.zoom >= 0.38,
                    simulation.energized_components.contains(&cid),
                    &connected_pins,
                    view,
                    dc_v,
                    dc_i,
                    self.simulation_ui.show_dc_overlay && self.simulate && self.zoom >= 0.48,
                );
            }

            // Multi-select highlight boxes
            for comp in &self.components {
                if self.editor.multi_selected.contains(&comp.id) {
                    let sc = view.to_screen(comp.pos);
                    let sz = component_size(comp) * view.zoom;
                    let rot = ((comp.rotation % 360) + 360) % 360;
                    let eff = if rot == 90 || rot == 270 {
                        Vec2::new(sz.y, sz.x)
                    } else {
                        sz
                    };
                    let sb = Rect::from_center_size(sc, eff);
                    painter.rect_stroke(
                        sb.expand(8.0),
                        4.0,
                        Stroke::new(1.5_f32, Color32::from_rgb(110, 170, 220)),
                        StrokeKind::Outside,
                    );
                }
            }

            // Rectangle selection preview
            if let (Some(start), Some(end)) = (self.editor.rect_select_start, self.canvas.cursor_world_pos)
            {
                let ss = view.to_screen(start);
                let se = view.to_screen(end);
                let sel_rect = Rect::from_two_pos(ss, se);
                painter.rect_filled(
                    sel_rect,
                    0.0,
                    Color32::from_rgba_unmultiplied(100, 178, 255, 18),
                );
                painter.rect_stroke(
                    sel_rect,
                    0.0,
                    Stroke::new(1.0_f32, Color32::from_rgb(100, 178, 255)),
                    StrokeKind::Outside,
                );
            }

            draw_junctions(&painter, &self.wires, view);

            // ── Node voltage circles at wire junctions ─────────────────
            if self.simulation_ui.show_dc_overlay
                && self.simulate
                && let Some(dc) = &simulation.dc
            {
                draw_node_voltage_indicators(&painter, &self.wires, dc, view, dc.vmax);
            }

            // ── Simulation summary overlay (top-right of canvas) ───────
            if self.simulate && !self.components.is_empty() {
                draw_sim_summary(&painter, rect, &simulation);
            }
            draw_simulation_legend(&painter, rect, self.simulate, simulation.dc.is_some());

            draw_title_block(&painter, rect, &self.components, &self.wires, &simulation);

            // ── Minimap (bottom-right corner) ────────────────────────────
            if !self.components.is_empty() {
                draw_minimap(&painter, rect, &self.components, &self.wires, view);
            }
            if cfg!(debug_assertions) && self.workspace_state.show_performance_overlay {
                let fps = ctx.input(|input| 1.0 / input.stable_dt.max(1.0 / 240.0));
                let lines = [
                    format!(
                        "FPS {fps:.0}  ·  frame {:.2}/{:.2}/{:.2} ms (p50/p95/max)",
                        self.performance.frame.percentile(0.5),
                        self.performance.frame.percentile(0.95),
                        self.performance.frame.maximum()
                    ),
                    format!(
                        "Components {}/{}  ·  wire segments {}/{}",
                        self.performance.rendered_components,
                        self.components.len(),
                        self.performance.rendered_wire_segments,
                        self.wires
                            .iter()
                            .map(|wire| wire.points.len().saturating_sub(1))
                            .sum::<usize>()
                    ),
                    format!(
                        "MNA {:.2} ms [{}]  ·  ERC {:.2} ms",
                        self.performance.mna_ms,
                        if self.performance.simulation_cache_hit {
                            "hit"
                        } else {
                            "miss"
                        },
                        self.performance.erc_ms
                    ),
                    format!(
                        "Netlist {:.2} ms [{}]  ·  flow cache {}",
                        self.performance.netlist_ms,
                        if self.performance.netlist_cache_hit {
                            "hit"
                        } else {
                            "miss"
                        },
                        if self.performance.flow_cache_hit {
                            "hit"
                        } else {
                            "miss"
                        }
                    ),
                    format!(
                        "Flow {} wire(s)  ·  {} particle(s)",
                        self.performance.visible_flow_wires, self.performance.flow_particles
                    ),
                ];
                let overlay =
                    Rect::from_min_size(rect.min + Vec2::new(10.0, 10.0), Vec2::new(390.0, 96.0));
                painter.rect_filled(
                    overlay,
                    3.0,
                    Color32::from_rgba_unmultiplied(10, 14, 20, 225),
                );
                for (index, line) in lines.iter().enumerate() {
                    painter.text(
                        overlay.min + Vec2::new(8.0, 7.0 + index as f32 * 14.0),
                        Align2::LEFT_TOP,
                        line,
                        egui::FontId::monospace(10.0),
                        crate::ui::theme::TEXT_SECONDARY,
                    );
                }
            }

            // ── Hover / cursor helpers ───────────────────────────────────
            let hover_pos = ui.input(|i| i.pointer.hover_pos());
            let pointer_in_rect = hover_pos.filter(|pos| rect.contains(*pos));

            self.canvas.cursor_world_pos = None;
            self.editor.snap_target = None;
            if let Some(raw_hover) = pointer_in_rect {
                let world_hover = view.to_world(raw_hover);
                self.canvas.cursor_world_pos = Some(world_hover);
                let mut world_pos = snap_pos(world_hover, rect, self.grid, self.snap);
                let in_wire_mode = self.editor.tool == Tool::Wire;
                let in_select_mode = self.editor.tool == Tool::Select;

                // Ghost preview for placement mode
                if let Tool::Place(place_kind) = self.editor.tool {
                    let ghost_pos = world_pos;
                    let ghost_screen = view.to_screen(ghost_pos);
                    let ghost_size = {
                        let dummy = Component {
                            id: 0,
                            kind: place_kind,
                            pos: ghost_pos,
                            rotation: 0,
                            label: String::new(),
                            value: String::new(),
                            part_id: self.editor.pending_custom_part.clone(),
                        };
                        component_size(&dummy) * view.zoom
                    };
                    let ghost_rect = Rect::from_center_size(ghost_screen, ghost_size);
                    // Translucent crosshair
                    let ghost_col = Color32::from_rgba_unmultiplied(80, 200, 140, 90);
                    let ghost_stroke = Stroke::new(1.6_f32, ghost_col);
                    painter.rect_stroke(
                        ghost_rect.expand(4.0),
                        4.0,
                        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(60, 180, 120, 55)),
                        StrokeKind::Middle,
                    );
                    // Crosshair at snap point
                    let cr = 6.0_f32;
                    painter.line_segment(
                        [ghost_screen - Vec2::X * cr, ghost_screen + Vec2::X * cr],
                        ghost_stroke,
                    );
                    painter.line_segment(
                        [ghost_screen - Vec2::Y * cr, ghost_screen + Vec2::Y * cr],
                        ghost_stroke,
                    );
                    // Kind label
                    painter.text(
                        ghost_screen + Vec2::new(0.0, ghost_size.y * 0.5 + 12.0),
                        Align2::CENTER_CENTER,
                        component_kind_label(place_kind),
                        egui::FontId::proportional(11.0),
                        Color32::from_rgba_unmultiplied(100, 220, 160, 180),
                    );
                }

                if in_wire_mode || in_select_mode {
                    if let Some(snapped) =
                        snap_to_nearest_connection(world_pos, &self.components, &self.wires)
                    {
                        world_pos = snapped;
                        self.editor.snap_target = Some(snapped);
                    }
                    // Check if we're snapping to a specific pin
                    let snap_pin = nearest_pin_at(world_pos, &self.components, 10.0);
                    if let Some((pin_label, comp_label)) = &snap_pin {
                        // Bright highlighted pin snap indicator
                        let sp = view.to_screen(world_pos);
                        painter.circle_stroke(
                            sp,
                            view.scale_f(10.0),
                            Stroke::new(2.5_f32, Color32::from_rgb(50, 255, 120)),
                        );
                        painter.circle_filled(
                            sp,
                            view.scale_f(4.0),
                            Color32::from_rgb(50, 255, 120),
                        );
                        // Pin label tooltip
                        let label_text = format!("{comp_label}.{pin_label}");
                        let label_pos = sp + egui::Vec2::new(12.0, -10.0);
                        painter.text(
                            label_pos,
                            Align2::LEFT_TOP,
                            &label_text,
                            egui::FontId::proportional(11.0),
                            Color32::from_rgb(80, 255, 140),
                        );
                    } else if is_connection_point(world_pos, &self.components, &self.wires) {
                        // Wire endpoint snap indicator
                        let sp = view.to_screen(world_pos);
                        painter.circle_stroke(
                            sp,
                            view.scale_f(8.0),
                            Stroke::new(2.0_f32, Color32::from_rgb(100, 240, 160)),
                        );
                        painter.circle_filled(
                            sp,
                            view.scale_f(3.5),
                            Color32::from_rgb(100, 240, 160),
                        );
                    } else if is_on_wire_segment(world_pos, &self.wires) {
                        // Mid-segment T-junction preview (cyan ring)
                        let sp = view.to_screen(world_pos);
                        painter.circle_stroke(
                            sp,
                            view.scale_f(7.0),
                            Stroke::new(1.8_f32, Color32::from_rgb(80, 200, 255)),
                        );
                        painter.circle_filled(
                            sp,
                            view.scale_f(2.5),
                            Color32::from_rgb(80, 200, 255),
                        );
                    }
                }

                if in_wire_mode && !self.editor.draft_wire.is_empty() {
                    let source_pin = self.editor.draft_wire.first().and_then(|start| {
                        self.components
                            .iter()
                            .flat_map(component_pin_defs)
                            .min_by(|a, b| {
                                a.pos.distance(*start).total_cmp(&b.pos.distance(*start))
                            })
                            .filter(|pin| pin.pos.distance(*start) <= 10.0)
                    });
                    if let Some(source_pin) = source_pin.as_ref() {
                        for target in self.components.iter().flat_map(component_pin_defs) {
                            if target.pos.distance(source_pin.pos) <= 1.0 {
                                continue;
                            }
                            let (tone, _) = assess_pin_pair(source_pin, &target);
                            let color = match tone {
                                SmartWireTone::Compatible => crate::ui::theme::OK,
                                SmartWireTone::Neutral => crate::ui::theme::TEXT_MUTED,
                                SmartWireTone::Suspicious => crate::ui::theme::WARNING,
                            };
                            let screen = view.to_screen(target.pos);
                            if rect.contains(screen) {
                                painter.circle_stroke(
                                    screen,
                                    5.0,
                                    Stroke::new(
                                        if tone == SmartWireTone::Compatible {
                                            1.8_f32
                                        } else {
                                            1.0_f32
                                        },
                                        color,
                                    ),
                                );
                            }
                        }
                        if let Some(target_pin) = self
                            .components
                            .iter()
                            .flat_map(component_pin_defs)
                            .min_by(|a, b| {
                                a.pos
                                    .distance(world_pos)
                                    .total_cmp(&b.pos.distance(world_pos))
                            })
                            .filter(|pin| pin.pos.distance(world_pos) <= 10.0)
                        {
                            let (tone, explanation) = assess_pin_pair(source_pin, &target_pin);
                            let color = match tone {
                                SmartWireTone::Compatible => crate::ui::theme::OK,
                                SmartWireTone::Neutral => crate::ui::theme::TEXT_SECONDARY,
                                SmartWireTone::Suspicious => crate::ui::theme::WARNING,
                            };
                            let pos = raw_hover + Vec2::new(14.0, 18.0);
                            painter.text(
                                pos,
                                Align2::LEFT_TOP,
                                explanation,
                                egui::FontId::proportional(10.5),
                                color,
                            );
                        }
                    }
                    let preview =
                        preview_wire_points(&self.editor.draft_wire, world_pos, self.orthogonal_wires);
                    let screen_preview: Vec<Pos2> =
                        preview.iter().map(|&p| view.to_screen(p)).collect();
                    draw_wire_preview(&painter, &screen_preview);
                }

                // Net highlight: update hovered wire in select mode
                if in_select_mode {
                    let hov_wire = hit_test_wire(world_hover, &self.wires).and_then(|s| {
                        if let Selection::Wire(id) = s {
                            Some(id)
                        } else {
                            None
                        }
                    });
                    if hov_wire != self.hovered_net_wire {
                        self.hovered_net_wire = hov_wire;
                        self.highlighted_net_wires = hov_wire
                            .map(|id| self.same_net_wires(id))
                            .unwrap_or_default();
                        if hov_wire.is_some() {
                            ctx.request_repaint();
                        }
                    }
                } else {
                    self.hovered_net_wire = None;
                    self.highlighted_net_wires.clear();
                }

                // Component hover tooltip + glow ring
                if in_select_mode {
                    if let Some(Selection::Component(hov_id)) =
                        hit_test_component(world_hover, &self.components)
                        && let Some(comp) = self.components.iter().find(|c| c.id == hov_id)
                    {
                        // Glow ring around hovered component
                        let bounds = component_bounds(comp);
                        let screen_center = view.to_screen(bounds.center());
                        let glow_r =
                            (bounds.width().max(bounds.height()) * 0.55 * view.zoom).max(18.0);
                        for i in 0..3 {
                            let alpha = 22 - i * 7;
                            painter.circle_stroke(
                                screen_center,
                                glow_r + i as f32 * 4.0,
                                Stroke::new(
                                    2.5 - i as f32 * 0.5,
                                    Color32::from_rgba_unmultiplied(80, 200, 255, alpha),
                                ),
                            );
                        }

                        // Build tooltip: base info + DC sim measurements
                        let base = if component_is_switch(comp.kind) {
                            let state = if comp.value.to_lowercase().contains("open") {
                                "OPEN"
                            } else {
                                "CLOSED"
                            };
                            format!("{} [{}]  Click to toggle", comp.label, state)
                        } else if let Some(vl) = canvas_value_label(comp) {
                            format!(
                                "{}  {}  {}",
                                comp.label,
                                component_kind_label(comp.kind),
                                vl
                            )
                        } else {
                            format!("{}  {}", comp.label, component_kind_label(comp.kind))
                        };
                        let mut dc_line = String::new();
                        if let Some(dc) = &simulation.dc {
                            let mut parts: Vec<String> = Vec::new();
                            if let Some(&v) = dc.component_voltage.get(&hov_id)
                                && v.abs() > 1e-9
                            {
                                parts.push(mna::format_voltage(v));
                            }
                            if let Some(&i) = dc.branch_current.get(&hov_id)
                                && i.abs() > 1e-12
                            {
                                parts.push(mna::format_current(i));
                            }
                            if let Some(&p) = dc.component_power.get(&hov_id)
                                && p.abs() > 1e-12
                            {
                                parts.push(mna::format_si(p, "W"));
                            }
                            if !parts.is_empty() {
                                dc_line = parts.join("  ·  ");
                            }
                        }
                        let tip_lines: Vec<String> = if dc_line.is_empty() {
                            vec![base]
                        } else {
                            vec![base, dc_line]
                        };
                        draw_probe_card(
                            &painter,
                            rect,
                            raw_hover,
                            &tip_lines,
                            crate::ui::theme::ACCENT,
                        );
                    }
                    // Wire net tooltip
                    if let Some(wid) = self.hovered_net_wire {
                        let net_size = self.highlighted_net_wires.len();
                        let dc_v = simulation
                            .dc
                            .as_ref()
                            .and_then(|dc| dc.wire_voltage.get(&wid).copied());
                        let dc_i = simulation
                            .dc
                            .as_ref()
                            .filter(|dc| dc.wire_current_known.contains(&wid))
                            .and_then(|dc| dc.wire_current.get(&wid).copied());
                        let direction = dc_i.map(|current| if current < 0.0 { "←" } else { "→" });
                        let tip_text = match (dc_v, dc_i, direction) {
                            (Some(v), Some(i), Some(direction)) => format!(
                                "Net  {}  ·  {} {}  ·  {} wire(s)",
                                mna::format_voltage(v),
                                direction,
                                mna::format_current(i.abs()),
                                net_size
                            ),
                            (Some(v), _, _) => {
                                format!("Net  {}  ·  {} wire(s)", mna::format_voltage(v), net_size)
                            }
                            _ => format!("Net  ·  {} wire(s)", net_size),
                        };
                        let tip_col = if dc_v.is_some() {
                            Color32::from_rgb(120, 220, 255)
                        } else {
                            Color32::from_rgb(140, 210, 255)
                        };
                        draw_probe_card(&painter, rect, raw_hover, &[tip_text], tip_col);
                    }
                }
            } else {
                // Pointer left canvas — clear net highlight
                if self.hovered_net_wire.is_some() {
                    self.hovered_net_wire = None;
                    self.highlighted_net_wires.clear();
                }
            }

            // ── Click / drag interactions (all in world space) ───────────
            if response.clicked_by(egui::PointerButton::Primary)
                && !panning
                && let Some(raw_pos) = pointer_in_rect
            {
                let world_raw = view.to_world(raw_pos);
                let world_pos = snap_pos(world_raw, rect, self.grid, self.snap);
                match self.editor.tool {
                    Tool::Select => {
                        let ctrl = ctx.input(|i| i.modifiers.command);
                        if let Some(sel) = hit_test(world_raw, &self.components, &self.wires) {
                            // Toggle switch/button on single click
                            if let Selection::Component(cid) = sel {
                                let comp_kind =
                                    self.components.iter().find(|c| c.id == cid).map(|c| c.kind);
                                if let Some(kind) = comp_kind
                                    && component_is_switch(kind)
                                {
                                    self.execute_editor_command(
                                        crate::commands::EditorCommand::Properties(
                                            crate::commands::properties::PropertiesCommand::ToggleSwitch {
                                                component_id: cid,
                                            },
                                        ),
                                    );
                                    ctx.request_repaint();
                                    self.editor.selected = Some(Selection::Component(cid));
                                }
                            }
                            // Ctrl+click toggles multi-select; plain click sets primary selection
                            if ctrl {
                                if let Selection::Component(cid) = sel {
                                    if self.editor.multi_selected.contains(&cid) {
                                        self.editor.multi_selected.remove(&cid);
                                    } else {
                                        self.editor.multi_selected.insert(cid);
                                    }
                                }
                            } else {
                                self.editor.selected = Some(sel);
                                self.editor.multi_selected.clear();
                            }
                        } else if !ctrl {
                            self.editor.selected = None;
                            self.editor.multi_selected.clear();
                        }
                    }
                    Tool::Place(kind) => {
                        if kind == ComponentKind::Custom {
                            if let Some(part_id) = self.editor.pending_custom_part.clone() {
                                self.add_custom_component(&part_id, world_pos);
                            } else {
                                self.editor.tool = Tool::Select;
                                self.status =
                                    "No custom part selected. Pick one from My Parts.".to_string();
                            }
                        } else {
                            self.add_component(kind, world_pos);
                        }
                    }
                    Tool::Wire => {
                        let wp =
                            snap_to_nearest_connection(world_pos, &self.components, &self.wires)
                                .unwrap_or(world_pos);
                        let already_started = !self.editor.draft_wire.is_empty();
                        let landed = is_connection_point(wp, &self.components, &self.wires);
                        self.push_wire_point(wp);
                        if already_started && landed && self.editor.draft_wire.len() >= 2 {
                            let points = std::mem::take(&mut self.editor.draft_wire);
                            self.add_wire(points);
                            if self.editor.wire_from_select {
                                self.editor.tool = Tool::Select;
                                self.editor.wire_from_select = false;
                            }
                        }
                    }
                }
            }

            if response.clicked_by(egui::PointerButton::Secondary) {
                if self.editor.tool == Tool::Wire {
                    if !self.editor.draft_wire.is_empty() {
                        let points = std::mem::take(&mut self.editor.draft_wire);
                        self.add_wire(points);
                    }
                    if self.editor.wire_from_select {
                        self.editor.tool = Tool::Select;
                        self.editor.wire_from_select = false;
                    }
                } else if self.editor.tool == Tool::Select {
                    // Open context menu on component right-click
                    if let Some(raw_pos) = pointer_in_rect {
                        let world = view.to_world(raw_pos);
                        if let Some(Selection::Component(cid)) =
                            hit_test_component(world, &self.components)
                        {
                            self.editor.selected = Some(Selection::Component(cid));
                            self.context_menu = Some((raw_pos, cid));
                        } else {
                            self.context_menu = None;
                        }
                    }
                }
            }

            // Close context menu on left click elsewhere
            if response.clicked_by(egui::PointerButton::Primary) {
                self.context_menu = None;
            }

            // Draw context menu
            if let Some((menu_pos, menu_cid)) = self.context_menu {
                let menu_w = 148.0;
                let menu_h = 130.0;
                let _menu_rect = Rect::from_min_size(menu_pos, Vec2::new(menu_w, menu_h));
                // Adjust so it doesn't go off screen
                let clamped_min = Pos2::new(
                    menu_pos.x.min(rect.right() - menu_w - 4.0),
                    menu_pos.y.min(rect.bottom() - menu_h - 4.0),
                );
                let menu_rect = Rect::from_min_size(clamped_min, Vec2::new(menu_w, menu_h));

                painter.rect_filled(menu_rect, 5.0, Color32::from_rgb(20, 26, 34));
                painter.rect_stroke(
                    menu_rect,
                    5.0,
                    Stroke::new(1.0_f32, Color32::from_rgb(55, 68, 82)),
                    StrokeKind::Outside,
                );

                let items: &[(&str, &str)] = &[
                    ("R  ", "Rotate 90°"),
                    ("D  ", "Duplicate"),
                    ("E  ", "Edit value"),
                    ("Del", "Delete"),
                    ("W  ", "Wire from pin"),
                ];
                let item_h = 24.0;
                let mut action: Option<u8> = None;
                for (idx, (key, label)) in items.iter().enumerate() {
                    let item_rect = Rect::from_min_size(
                        clamped_min + Vec2::new(0.0, idx as f32 * item_h + 4.0),
                        Vec2::new(menu_w, item_h),
                    );
                    let hovered = item_rect
                        .contains(ctx.input(|i| i.pointer.hover_pos().unwrap_or_default()));
                    if hovered {
                        painter.rect_filled(item_rect, 3.0, Color32::from_rgb(38, 50, 62));
                    }
                    painter.text(
                        item_rect.min + Vec2::new(8.0, 4.0),
                        Align2::LEFT_TOP,
                        label,
                        egui::FontId::proportional(12.0),
                        Color32::from_rgb(210, 220, 232),
                    );
                    painter.text(
                        item_rect.min + Vec2::new(menu_w - 30.0, 4.0),
                        Align2::LEFT_TOP,
                        key,
                        egui::FontId::monospace(10.0),
                        Color32::from_rgb(110, 140, 160),
                    );
                    if hovered && response.clicked_by(egui::PointerButton::Primary) {
                        action = Some(idx as u8);
                    }
                }
                if let Some(act) = action {
                    self.context_menu = None;
                    self.editor.selected = Some(Selection::Component(menu_cid));
                    match act {
                        0 => {
                            self.rotate_selected();
                        }
                        1 => {
                            self.duplicate_selected();
                        }
                        2 => {
                            if let Some(comp) = self.components.iter().find(|c| c.id == menu_cid) {
                                self.inline_edit = Some((menu_cid, comp.value.clone()));
                            }
                        }
                        3 => {
                            self.delete_selected();
                        }
                        4 => {
                            self.editor.tool = Tool::Wire;
                            self.editor.wire_from_select = true;
                        }
                        _ => {}
                    }
                }
                ctx.request_repaint();
            }

            if response.double_clicked() && self.editor.tool == Tool::Wire && self.editor.draft_wire.len() >= 2 {
                let points = std::mem::take(&mut self.editor.draft_wire);
                self.add_wire(points);
                if self.editor.wire_from_select {
                    self.editor.tool = Tool::Select;
                    self.editor.wire_from_select = false;
                }
            }

            // Double-click component in Select mode → open inline value editor
            if response.double_clicked()
                && self.editor.tool == Tool::Select
                && self.inline_edit.is_none()
                && let Some(raw_pos) = pointer_in_rect
            {
                let world = view.to_world(raw_pos);
                if let Some(Selection::Component(cid)) = hit_test_component(world, &self.components)
                    && let Some(comp) = self.components.iter().find(|c| c.id == cid)
                {
                    self.inline_edit = Some((cid, comp.value.clone()));
                }
            }

            // Render inline edit popup
            if let Some((edit_id, ref mut edit_text)) = self.inline_edit {
                if let Some(comp) = self
                    .document
                    .components
                    .iter()
                    .find(|c| c.id == edit_id)
                {
                    let sp = view.to_screen(comp.pos);
                    let popup_rect = Rect::from_center_size(
                        sp + Vec2::new(0.0, -component_size(comp).y * view.zoom * 0.7),
                        Vec2::new(120.0, 26.0),
                    );
                    painter.rect_filled(popup_rect, 4.0, Color32::from_rgb(22, 27, 34));
                    painter.rect_stroke(
                        popup_rect,
                        4.0,
                        Stroke::new(1.5_f32, Color32::from_rgb(80, 180, 120)),
                        StrokeKind::Outside,
                    );
                    let text_pos = popup_rect.min + Vec2::new(6.0, 5.0);
                    painter.text(
                        text_pos,
                        Align2::LEFT_TOP,
                        format!("{}: {}_", comp.label, edit_text),
                        egui::FontId::monospace(12.0),
                        Color32::from_rgb(160, 240, 180),
                    );
                }
                // Keyboard input for inline edit
                ctx.input(|i| {
                    for event in &i.events {
                        match event {
                            egui::Event::Text(ch) => edit_text.push_str(ch),
                            egui::Event::Key {
                                key: egui::Key::Backspace,
                                pressed: true,
                                ..
                            } => {
                                edit_text.pop();
                            }
                            _ => {}
                        }
                    }
                });
                let commit = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                let cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                if commit {
                    let new_val = edit_text.clone();
                    let label = self
                        .components
                        .iter()
                        .find(|c| c.id == edit_id)
                        .map(|c| c.label.clone())
                        .unwrap_or_default();
                    self.execute_editor_command(crate::commands::EditorCommand::Properties(
                        crate::commands::properties::PropertiesCommand::SetComponentValue {
                            component_id: edit_id,
                            value: new_val,
                        },
                    ));
                    self.status = format!("{} value updated.", label);
                    self.inline_edit = None;
                } else if cancel {
                    self.inline_edit = None;
                }
            }

            if response.drag_started()
                && !panning
                && self.editor.tool == Tool::Select
                && let Some(pos) = pointer_in_rect
            {
                let world = view.to_world(pos);
                if let Some((wire_id, point_index)) =
                    hit_test_wire_control_point(world, &self.wires)
                {
                    self.record_history();
                    self.editor.drag = Some(DragState::WirePoint {
                        wire_id,
                        point_index,
                    });
                    self.editor.selected = Some(Selection::Wire(wire_id));
                } else if hit_test_wire(world, &self.wires).is_some() {
                    self.execute_editor_command(crate::commands::EditorCommand::Wiring(
                        crate::commands::wiring::WiringCommand::InsertControlPoint {
                            position: world,
                        },
                    ));
                } else if let Some(Selection::Component(id)) =
                    hit_test_component(world, &self.components)
                {
                    self.record_history();
                    if let Some(component) = self.components.iter().find(|c| c.id == id) {
                        self.editor.drag = Some(DragState::Component {
                            id,
                            offset: world - component.pos,
                        });
                        self.editor.selected = Some(Selection::Component(id));
                        // Ensure dragged component is in multi_selected if multi_selected is active
                        if !self.editor.multi_selected.is_empty() {
                            self.editor.multi_selected.insert(id);
                        }
                    }
                } else {
                    // Empty area — start rectangle selection
                    let ctrl = ctx.input(|i| i.modifiers.command);
                    self.editor.rect_select_start = Some(world);
                    if !ctrl {
                        self.editor.selected = None;
                        self.editor.multi_selected.clear();
                    }
                }
            }

            if response.dragged()
                && !panning
                && let (Some(drag), Some(pos)) = (self.editor.drag.clone(), pointer_in_rect)
            {
                let world = view.to_world(pos);
                let mut data_changed = false;
                let force_connection_snap = ctx.input(|i| i.modifiers.ctrl || i.modifiers.command);
                match drag {
                    DragState::Component { id, offset } => {
                        let snapped = snap_pos(world, rect, self.grid, self.snap);
                        let in_multi =
                            self.editor.multi_selected.len() > 1 && self.editor.multi_selected.contains(&id);
                        if in_multi {
                            let old_pos =
                                self.components.iter().find(|c| c.id == id).map(|c| c.pos);
                            if let Some(old_pos) = old_pos {
                                let mut delta = snapped - offset - old_pos;
                                let ids = self.editor.multi_selected.clone();
                                let old_pins = self
                                    .components
                                    .iter()
                                    .filter(|component| ids.contains(&component.id))
                                    .flat_map(component_pins)
                                    .collect::<Vec<_>>();
                                if force_connection_snap
                                    && let Some(adjust) = snap_delta_for_moved_components(
                                        &self.components,
                                        &self.wires,
                                        &ids,
                                        delta,
                                        &old_pins,
                                    )
                                {
                                    delta += adjust;
                                }
                                data_changed = self
                                    .execute_continuous_editor_command(
                                        crate::commands::EditorCommand::Component(
                                            crate::commands::component::ComponentCommand::Move {
                                                component_ids: ids,
                                                delta,
                                            },
                                        ),
                                    )
                                    .persistence_changed;
                            }
                        } else if let Some(index) = self.components.iter().position(|c| c.id == id)
                        {
                            let old_pins = component_pins(&self.components[index]);
                            let mut new_pos = snapped - offset;
                            if force_connection_snap {
                                let ids = HashSet::from([id]);
                                if let Some(adjust) = snap_delta_for_moved_components(
                                    &self.components,
                                    &self.wires,
                                    &ids,
                                    new_pos - self.components[index].pos,
                                    &old_pins,
                                ) {
                                    new_pos += adjust;
                                }
                            }
                            let delta = new_pos - self.components[index].pos;
                            data_changed = self
                                .execute_continuous_editor_command(
                                    crate::commands::EditorCommand::Component(
                                        crate::commands::component::ComponentCommand::Move {
                                            component_ids: HashSet::from([id]),
                                            delta,
                                        },
                                    ),
                                )
                                .persistence_changed;
                        }
                    }
                    DragState::WirePoint {
                        wire_id,
                        point_index,
                    } => {
                        let mut snapped = snap_pos(world, rect, self.grid, self.snap);
                        if force_connection_snap
                            && let Some(connection) =
                                snap_to_nearest_connection(snapped, &self.components, &self.wires)
                        {
                            snapped = connection;
                        }
                        data_changed = self
                            .execute_continuous_editor_command(
                                crate::commands::EditorCommand::Wiring(
                                    crate::commands::wiring::WiringCommand::MoveControlPoint {
                                        wire_id,
                                        point_index,
                                        position: snapped,
                                    },
                                ),
                            )
                            .persistence_changed;
                    }
                }
                let _ = data_changed;
            }

            let primary_down = ctx.input(|i| i.pointer.primary_down());
            if !primary_down {
                if self.editor.drag.is_some() {
                    self.finish_history_transaction();
                }
                self.editor.drag = None;
                if let Some(start) = self.editor.rect_select_start.take()
                    && let Some(end) = self.canvas.cursor_world_pos
                    && start.distance(end) > 4.0
                {
                    let sel = Rect::from_two_pos(start, end);
                    let selected_ids = self
                        .document
                        .components
                        .iter()
                        .filter(|component| sel.contains(component.pos))
                        .map(|component| component.id)
                        .collect::<Vec<_>>();
                    self.editor.multi_selected.extend(selected_ids);
                    self.status = format!("{} component(s) selected.", self.editor.multi_selected.len());
                }
            }
        });

        // ── Keyboard shortcuts ────────────────────────────────────────────
        let backspace = ctx.input(|i| i.key_pressed(egui::Key::Backspace));
        // Backspace during wire drawing removes the last placed point
        if backspace && self.editor.tool == Tool::Wire && !self.editor.draft_wire.is_empty() {
            self.editor.draft_wire.pop();
            if self.orthogonal_wires && self.editor.draft_wire.len() >= 2 {
                // pop the auto-inserted L-bend corner too
                self.editor.draft_wire.pop();
            }
            self.status = if self.editor.draft_wire.is_empty() {
                "Wire cancelled.".to_string()
            } else {
                "Wire point removed.".to_string()
            };
        } else if ctx.input(|i| i.key_pressed(egui::Key::Delete)) || backspace {
            self.delete_selected();
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            // Hierarchical Esc: find dialog → multi-select → single select → wire draft → select tool
            if self.show_find {
                self.show_find = false;
            } else if !self.editor.draft_wire.is_empty() {
                self.editor.draft_wire.clear();
                self.editor.wire_from_select = false;
            } else if !self.editor.multi_selected.is_empty() {
                self.editor.multi_selected.clear();
                self.editor.selected = None;
                self.editor.rect_select_start = None;
            } else if self.editor.selected.is_some() {
                self.editor.selected = None;
            } else {
                self.editor.tool = Tool::Select;
                self.editor.wire_from_select = false;
            }
        }

        if ctx.input(|i| i.key_pressed(egui::Key::R)) {
            self.rotate_selected();
        }

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z)) {
            self.undo();
        }

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Y)) {
            self.redo();
        }

        if ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::Z)) {
            self.redo();
        }

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::D)) {
            self.duplicate_selected();
        }

        // Ctrl+C — copy selected component(s) + internal wires
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::C)) {
            self.editor.clipboard.clear();
            self.editor.clipboard_wires.clear();
            let ids: Vec<u64> = if !self.editor.multi_selected.is_empty() {
                self.editor.multi_selected.iter().copied().collect()
            } else if let Some(Selection::Component(id)) = self.editor.selected {
                vec![id]
            } else {
                Vec::new()
            };
            if !ids.is_empty() {
                self.editor.clipboard = self
                    .components
                    .iter()
                    .filter(|c| ids.contains(&c.id))
                    .cloned()
                    .collect();
                // Copy wires whose BOTH endpoints lie within copied component pins
                let pin_positions: HashSet<(i32, i32)> = self
                    .editor
                    .clipboard
                    .iter()
                    .flat_map(component_pin_defs)
                    .map(|p| (p.pos.x.round() as i32, p.pos.y.round() as i32))
                    .collect();
                self.editor.clipboard_wires = self
                    .wires
                    .iter()
                    .filter(|w| {
                        let key_first = w
                            .points
                            .first()
                            .map(|p| (p.x.round() as i32, p.y.round() as i32));
                        let key_last = w
                            .points
                            .last()
                            .map(|p| (p.x.round() as i32, p.y.round() as i32));
                        key_first.is_some_and(|k| pin_positions.contains(&k))
                            && key_last.is_some_and(|k| pin_positions.contains(&k))
                    })
                    .cloned()
                    .collect();
                self.status = format!(
                    "Copied {} component(s) + {} wire(s).",
                    self.editor.clipboard.len(),
                    self.editor.clipboard_wires.len()
                );
            }
        }

        // Ctrl+V — paste clipboard with offset
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::V)) {
            if self.editor.clipboard.is_empty() {
                self.status = "Clipboard empty. Ctrl+C to copy first.".to_string();
            } else {
                let offset = Vec2::new(self.grid * 3.0, self.grid * 3.0);
                self.execute_editor_command(crate::commands::EditorCommand::Component(
                    crate::commands::component::ComponentCommand::Paste {
                        components: self.editor.clipboard.clone(),
                        wires: self.editor.clipboard_wires.clone(),
                        offset,
                    },
                ));
            }
        }

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::A)) {
            self.editor.multi_selected = self.components.iter().map(|c| c.id).collect();
            self.status = format!(
                "Selected all {} component(s).",
                self.editor.multi_selected.len()
            );
        }

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            self.save_circuit_json();
        }

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::O)) {
            self.load_circuit_json();
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Enter))
            && self.editor.tool == Tool::Wire
            && self.editor.draft_wire.len() >= 2
        {
            let points = std::mem::take(&mut self.editor.draft_wire);
            self.add_wire(points);
        }

        // Home / Ctrl+0 — reset view
        if ctx.input(|i| i.key_pressed(egui::Key::Home))
            || ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Num0))
        {
            self.zoom = 1.0;
            self.pan = Vec2::ZERO;
            self.status = "View reset.".to_string();
        }

        // T — tidy selected wire; Ctrl+T — tidy all wires
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::T)) {
            self.execute_editor_command(crate::commands::EditorCommand::Wiring(
                crate::commands::wiring::WiringCommand::Tidy { wire_id: None },
            ));
        } else if ctx.input(|i| i.key_pressed(egui::Key::T))
            && let Some(Selection::Wire(id)) = self.editor.selected
            && self.wires.iter().any(|w| w.id == id)
        {
            self.execute_editor_command(crate::commands::EditorCommand::Wiring(
                crate::commands::wiring::WiringCommand::Tidy { wire_id: Some(id) },
            ));
        }

        // Ctrl+A — Select all components
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::A)) {
            self.editor.multi_selected = self.components.iter().map(|c| c.id).collect();
            self.editor.selected = None;
            self.status = format!(
                "{} component(s) selected.",
                self.editor.multi_selected.len()
            );
        }

        // Ctrl+D — Duplicate selected
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::D)) {
            self.duplicate_selected();
        }

        // Ctrl+F — Find
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
            self.show_find = !self.show_find;
            if self.show_find {
                self.find_query.clear();
                self.find_results.clear();
            }
        }

        // W — Wire tool
        if ctx.input(|i| !i.modifiers.any() && i.key_pressed(egui::Key::W)) {
            self.editor.tool = Tool::Wire;
            self.editor.draft_wire.clear();
            self.status = "Wire tool.".to_string();
        }

        // S — Select tool
        if ctx.input(|i| !i.modifiers.any() && i.key_pressed(egui::Key::S)) {
            self.editor.tool = Tool::Select;
            self.editor.draft_wire.clear();
            self.editor.wire_from_select = false;
            self.status = "Select tool.".to_string();
        }

        // F — Zoom to fit
        if ctx.input(|i| !i.modifiers.any() && i.key_pressed(egui::Key::F)) {
            self.zoom_to_fit();
        }

        // ? — Toggle shortcuts help
        if ctx.input(|i| !i.modifiers.any() && i.key_pressed(egui::Key::Questionmark)) {
            self.workspace_state.show_help = !self.workspace_state.show_help;
        }

        // Space — toggle simulation on/off (when not dragging/panning)
        if ctx.input(|i| !i.modifiers.any() && i.key_pressed(egui::Key::Space))
            && self.editor.drag.is_none()
            && self.editor.tool != Tool::Wire
        {
            self.simulate = !self.simulate;
            self.status = if self.simulate {
                "Simulation ON.".to_string()
            } else {
                "Simulation OFF.".to_string()
            };
        }

        // Quick-place shortcuts
        let place_shortcuts: &[(egui::Key, ComponentKind, &str)] = &[
            (egui::Key::Q, ComponentKind::Resistor, "Resistor"),
            (egui::Key::A, ComponentKind::Capacitor, "Capacitor"),
            (egui::Key::I, ComponentKind::Inductor, "Inductor"),
            (egui::Key::D, ComponentKind::Diode, "Diode"),
            (egui::Key::Z, ComponentKind::ZenerDiode, "Zener"),
            (egui::Key::E, ComponentKind::Led, "LED"),
            (egui::Key::N, ComponentKind::NpnTransistor, "NPN BJT"),
            (egui::Key::P, ComponentKind::PnpTransistor, "PNP BJT"),
            (egui::Key::B, ComponentKind::Battery, "Battery"),
            (egui::Key::G, ComponentKind::Ground, "Ground"),
            (egui::Key::S, ComponentKind::Switch, "Switch"),
            (egui::Key::V, ComponentKind::Voltmeter, "Voltmeter"),
            (egui::Key::M, ComponentKind::Ammeter, "Ammeter"),
        ];
        for &(key, kind, name) in place_shortcuts {
            if ctx.input(|i| !i.modifiers.any() && i.key_pressed(key)) {
                if self.editor.tool == Tool::Place(kind) {
                    self.editor.tool = Tool::Select;
                    self.status = "Select tool.".to_string();
                } else {
                    self.editor.tool = Tool::Place(kind);
                    self.editor.draft_wire.clear();
                    self.status = format!("Placing {}. Click the canvas.", name);
                }
            }
        }

        if self.editor.drag.is_none() && !ctx.input(|i| i.pointer.primary_down()) {
            self.flush_autorecover_if_needed();
        }

        // ── Find dialog (floating overlay) ──────────────────────────────────
        // ── Keyboard shortcuts help dialog ───────────────────────────────────
        if self.workspace_state.show_help {
            let mut open = self.workspace_state.show_help;
            egui::Window::new("⌨  Keyboard Shortcuts")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_pos(egui::Pos2::new(200.0, 120.0))
                .default_width(380.0)
                .show(ctx, |ui| {
                    let row = |ui: &mut egui::Ui, key: &str, desc: &str| {
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                Vec2::new(110.0, 18.0),
                                egui::Label::new(
                                    egui::RichText::new(key)
                                        .monospace()
                                        .size(11.5)
                                        .color(Color32::from_rgb(220, 200, 100)),
                                ),
                            );
                            ui.label(
                                egui::RichText::new(desc)
                                    .size(11.5)
                                    .color(Color32::from_rgb(200, 208, 218)),
                            );
                        });
                    };
                    ui.columns(2, |cols| {
                        let ui = &mut cols[0];
                        ui.label(
                            egui::RichText::new("Tools")
                                .strong()
                                .color(Color32::from_rgb(120, 200, 160)),
                        );
                        row(ui, "W", "Wire tool");
                        row(ui, "Esc", "Select / cancel");
                        row(ui, "R", "Rotate component");
                        row(ui, "Delete", "Delete selected");
                        row(ui, "Ctrl+Z", "Undo");
                        row(ui, "Ctrl+Y", "Redo");
                        row(ui, "Ctrl+C", "Copy");
                        row(ui, "Ctrl+V", "Paste");
                        row(ui, "Ctrl+A", "Select all");
                        row(ui, "Ctrl+D", "Duplicate");
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("View")
                                .strong()
                                .color(Color32::from_rgb(120, 200, 160)),
                        );
                        row(ui, "F", "Zoom to fit");
                        row(ui, "Home", "Reset view");
                        row(ui, "Space+drag", "Pan");
                        row(ui, "Scroll", "Zoom");

                        let ui = &mut cols[1];
                        ui.label(
                            egui::RichText::new("File")
                                .strong()
                                .color(Color32::from_rgb(120, 200, 160)),
                        );
                        row(ui, "Ctrl+S", "Save");
                        row(ui, "Ctrl+O", "Load");
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Quick Place")
                                .strong()
                                .color(Color32::from_rgb(120, 200, 160)),
                        );
                        row(ui, "Q", "Resistor");
                        row(ui, "A", "Capacitor");
                        row(ui, "I", "Inductor");
                        row(ui, "E", "LED");
                        row(ui, "D", "Diode");
                        row(ui, "Z", "Zener Diode");
                        row(ui, "G", "Ground");
                        row(ui, "B", "Battery");
                        row(ui, "S", "Switch");
                        row(ui, "V", "Voltmeter");
                        row(ui, "M", "Ammeter");
                        row(ui, "N", "NPN BJT");
                        row(ui, "P", "PNP BJT");
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Simulation")
                                .strong()
                                .color(Color32::from_rgb(120, 200, 160)),
                        );
                        row(ui, "Space", "Toggle sim on/off");
                        row(ui, "Click btn", "Toggle button on/off");
                        row(ui, "Dbl-click", "Edit component value");
                        row(ui, "?", "This help");
                    });
                    ui.add_space(4.0);
                    ui.separator();
                    ui.label(
                        egui::RichText::new("Press ? or close to dismiss")
                            .size(10.5)
                            .color(Color32::from_rgb(120, 130, 140))
                            .italics(),
                    );
                });
            self.workspace_state.show_help = open;
        }

        // ── DC / AC Analysis panel ──────────────────────────────────────────
        if self.simulation_ui.show_oscilloscope {
            let mut open = self.simulation_ui.show_oscilloscope;
            let dc_result = simulation.dc.clone();
            let ac_result = simulation.ac.clone();
            let id_to_label: std::collections::HashMap<u64, String> = self
                .components
                .iter()
                .map(|c| {
                    (
                        c.id,
                        format!("{} ({})", c.label, component_kind_label(c.kind)),
                    )
                })
                .collect();

            // Build wire → net name map: prefer NetLabel values on same net, else "Net#id"
            let wire_net_names: std::collections::HashMap<u64, String> = {
                let mut m: std::collections::HashMap<u64, String> = self
                    .wires
                    .iter()
                    .map(|w| (w.id, format!("Net#{}", w.id)))
                    .collect();
                // Find NetLabel components and match their pin position to wires
                for comp in &self.components {
                    if comp.kind == ComponentKind::NetLabel && !comp.value.is_empty() {
                        let pin_positions: Vec<Pos2> = component_pin_defs(comp)
                            .into_iter()
                            .map(|p| p.pos)
                            .collect();
                        for wire in &self.wires {
                            let touches = pin_positions.iter().any(|&pp| {
                                wire.points.iter().any(|&wp| wp.distance(pp) < 6.0)
                                    || wire
                                        .points
                                        .windows(2)
                                        .any(|seg| distance_to_segment(pp, seg[0], seg[1]) < 3.0)
                            });
                            if touches {
                                m.insert(wire.id, comp.value.clone());
                            }
                        }
                    }
                }
                m
            };

            egui::Window::new("DC / AC Analysis")
                .open(&mut open)
                .collapsible(true)
                .resizable(true)
                .default_pos(egui::Pos2::new(60.0, 120.0))
                .default_size(Vec2::new(480.0, 480.0))
                .show(ctx, |ui| {
                    if dc_result.is_none() && ac_result.is_none() {
                        ui.add_space(16.0);
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                egui::RichText::new("Enable Live Simulation to see data")
                                    .size(14.0)
                                    .color(Color32::from_rgb(140, 150, 160))
                                    .italics(),
                            );
                        });
                    } else {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            if let Some(dc) = &dc_result {
                                // ── DC Component Voltages ──────────────────────
                                ui.label(
                                    egui::RichText::new("DC  ·  Component Voltages")
                                        .strong()
                                        .color(Color32::from_rgb(120, 200, 160)),
                                );
                                ui.separator();
                                let mut comp_voltages: Vec<(String, f64)> = dc
                                    .component_voltage
                                    .iter()
                                    .filter_map(|(&id, &v)| {
                                        id_to_label.get(&id).map(|l| (l.clone(), v))
                                    })
                                    .collect();
                                comp_voltages.sort_by(|a, b| a.0.cmp(&b.0));
                                if comp_voltages.is_empty() {
                                    ui.label(
                                        egui::RichText::new("No data")
                                            .color(Color32::from_rgb(120, 120, 120))
                                            .italics(),
                                    );
                                } else {
                                    let max_v = comp_voltages
                                        .iter()
                                        .map(|(_, v)| v.abs())
                                        .fold(0.001_f64, f64::max);
                                    osc_bar_rows(ui, &comp_voltages, max_v, true);
                                }
                                ui.add_space(10.0);

                                // ── DC Branch Currents ─────────────────────────
                                ui.label(
                                    egui::RichText::new("DC  ·  Branch Currents")
                                        .strong()
                                        .color(Color32::from_rgb(120, 180, 255)),
                                );
                                ui.separator();
                                let mut comp_currents: Vec<(String, f64)> = dc
                                    .branch_current
                                    .iter()
                                    .filter_map(|(&id, &v)| {
                                        id_to_label.get(&id).map(|l| (l.clone(), v))
                                    })
                                    .collect();
                                comp_currents.sort_by(|a, b| a.0.cmp(&b.0));
                                if comp_currents.is_empty() {
                                    ui.label(
                                        egui::RichText::new("No data")
                                            .color(Color32::from_rgb(120, 120, 120))
                                            .italics(),
                                    );
                                } else {
                                    let max_i = comp_currents
                                        .iter()
                                        .map(|(_, v)| v.abs())
                                        .fold(0.001_f64, f64::max);
                                    osc_bar_rows(ui, &comp_currents, max_i, false);
                                }
                                ui.add_space(10.0);

                                // ── DC Wire / Net Voltages ─────────────────────
                                ui.label(
                                    egui::RichText::new("DC  ·  Net Voltages")
                                        .strong()
                                        .color(Color32::from_rgb(200, 160, 255)),
                                );
                                ui.separator();
                                let mut wire_rows: Vec<(String, f64)> = dc
                                    .wire_voltage
                                    .iter()
                                    .filter_map(|(&id, &v)| {
                                        wire_net_names.get(&id).map(|name| (name.clone(), v))
                                    })
                                    .collect();
                                // Deduplicate by net name (keep highest absolute voltage)
                                wire_rows.sort_by(|a, b| a.0.cmp(&b.0));
                                wire_rows.dedup_by(|b, a| {
                                    if a.0 == b.0 {
                                        if b.1.abs() > a.1.abs() {
                                            a.1 = b.1;
                                        }
                                        true
                                    } else {
                                        false
                                    }
                                });
                                if wire_rows.is_empty() {
                                    ui.label(
                                        egui::RichText::new("No data")
                                            .color(Color32::from_rgb(120, 120, 120))
                                            .italics(),
                                    );
                                } else {
                                    let max_v = wire_rows
                                        .iter()
                                        .map(|(_, v)| v.abs())
                                        .fold(0.001_f64, f64::max);
                                    osc_bar_rows(ui, &wire_rows, max_v, true);
                                }
                            }

                            // ── AC section ────────────────────────────────────
                            if let Some(ac) = &ac_result {
                                ui.add_space(14.0);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "AC  ·  {:.0} Hz  ·  Net Voltage |V|",
                                        self.simulation_ui.ac_freq_hz
                                    ))
                                    .strong()
                                    .color(Color32::from_rgb(255, 200, 100)),
                                );
                                ui.separator();
                                let mut ac_rows: Vec<(String, f64)> = ac
                                    .wire_voltage_mag
                                    .iter()
                                    .filter_map(|(&id, &mag)| {
                                        wire_net_names.get(&id).map(|name| (name.clone(), mag))
                                    })
                                    .collect();
                                ac_rows.sort_by(|a, b| a.0.cmp(&b.0));
                                ac_rows.dedup_by(|b, a| {
                                    if a.0 == b.0 {
                                        if b.1 > a.1 {
                                            a.1 = b.1;
                                        }
                                        true
                                    } else {
                                        false
                                    }
                                });
                                if ac_rows.is_empty() {
                                    ui.label(
                                        egui::RichText::new("No AC data (add C, L, or AC source)")
                                            .color(Color32::from_rgb(120, 120, 120))
                                            .italics(),
                                    );
                                } else {
                                    let max_v =
                                        ac_rows.iter().map(|(_, v)| *v).fold(0.001_f64, f64::max);
                                    osc_bar_rows(ui, &ac_rows, max_v, true);
                                }

                                // Component impedances
                                if !ac.component_impedance.is_empty() {
                                    ui.add_space(10.0);
                                    ui.label(
                                        egui::RichText::new("AC  ·  Impedances |Z|")
                                            .strong()
                                            .color(Color32::from_rgb(255, 165, 80)),
                                    );
                                    ui.separator();
                                    let mut z_rows: Vec<(String, String)> = ac
                                        .component_impedance
                                        .iter()
                                        .filter_map(|(&id, &z)| {
                                            id_to_label
                                                .get(&id)
                                                .map(|l| (l.clone(), format_resistance(z as f32)))
                                        })
                                        .collect();
                                    z_rows.sort_by(|a, b| a.0.cmp(&b.0));
                                    for (lbl, z) in &z_rows {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new(lbl)
                                                    .size(11.0)
                                                    .color(Color32::from_rgb(200, 210, 220)),
                                            );
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    ui.label(
                                                        egui::RichText::new(z)
                                                            .size(11.0)
                                                            .monospace()
                                                            .color(Color32::from_rgb(
                                                                255, 200, 100,
                                                            )),
                                                    );
                                                },
                                            );
                                        });
                                    }
                                }
                            }
                        }); // ScrollArea
                    }
                });
            self.simulation_ui.show_oscilloscope = open;
        }

        // ── Breadboard View / wiring assistant ─────────────────────────────
        if self.breadboard_ui.open {
            let mut open = self.breadboard_ui.open;
            let guide = build_breadboard_guide(&self.components, &inspector_netlist);
            let mut breadboard_action: Option<BreadboardAction> = None;
            egui::Window::new("Breadboard View")
                .open(&mut open)
                .collapsible(true)
                .resizable(true)
                .default_pos(egui::Pos2::new(80.0, 120.0))
                .default_size(Vec2::new(520.0, 420.0))
                .show(ctx, |ui| {
                    breadboard_action = render_breadboard_view(ui, &guide);
                });
            if let Some(action) = breadboard_action {
                match action {
                    BreadboardAction::Select(route) => {
                        self.select_breadboard_route(&inspector_netlist, route);
                    }
                    BreadboardAction::AddJumper(route) => {
                        self.connect_breadboard_route(route);
                    }
                }
            }
            self.breadboard_ui.open = open;
        }

        if self.show_find {
            egui::Window::new("Find Component")
                .collapsible(false)
                .resizable(false)
                .default_pos(egui::Pos2::new(300.0, 80.0))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let response = ui.add_sized(
                            Vec2::new(200.0, 22.0),
                            egui::TextEdit::singleline(&mut self.find_query)
                                .hint_text("Label or value..."),
                        );
                        if response.changed() {
                            let q = self.find_query.to_lowercase();
                            self.find_results = self
                                .components
                                .iter()
                                .filter(|c| {
                                    c.label.to_lowercase().contains(&q)
                                        || c.value.to_lowercase().contains(&q)
                                        || component_kind_label(c.kind).to_lowercase().contains(&q)
                                })
                                .map(|c| c.id)
                                .collect();
                            self.find_result_idx = 0;
                        }
                        let go_prev = ui.small_button("↑").clicked()
                            || ctx.input(|i| i.key_pressed(egui::Key::ArrowUp));
                        let go_next = ui.small_button("↓").clicked()
                            || ctx.input(|i| i.key_pressed(egui::Key::ArrowDown));
                        if go_prev && !self.find_results.is_empty() {
                            self.find_result_idx = self
                                .find_result_idx
                                .checked_sub(1)
                                .unwrap_or(self.find_results.len() - 1);
                        }
                        if go_next && !self.find_results.is_empty() {
                            self.find_result_idx =
                                (self.find_result_idx + 1) % self.find_results.len();
                        }
                        if ui.small_button("✕").clicked() {
                            self.show_find = false;
                        }
                    });

                    if !self.find_results.is_empty() {
                        let cur_id = self.find_results[self.find_result_idx];
                        self.editor.selected = Some(Selection::Component(cur_id));
                        // Center canvas on the found component
                        if let Some(comp) = self.components.iter().find(|c| c.id == cur_id) {
                            let canvas_center = self.canvas.rect.center().to_vec2();
                            self.pan = canvas_center - comp.pos.to_vec2() * self.zoom;
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "{}/{}",
                                self.find_result_idx + 1,
                                self.find_results.len()
                            ))
                            .size(11.0)
                            .color(Color32::from_rgb(140, 200, 160)),
                        );
                    } else if !self.find_query.is_empty() {
                        ui.label(
                            egui::RichText::new("No results")
                                .size(11.0)
                                .color(Color32::from_rgb(200, 100, 90)),
                        );
                    }
                });
        }
        self.performance
            .frame
            .record(frame_started.elapsed().as_secs_f64() * 1_000.0);
    }
}

impl eframe::App for CircuitApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_ui(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.flush_autorecover_if_needed();
    }
}
