#![allow(
    dead_code,
    unused_imports,
    clippy::collapsible_else_if,
    clippy::collapsible_if,
    clippy::for_kv_map,
    clippy::get_first,
    clippy::if_same_then_else,
    clippy::iter_cloned_collect,
    clippy::needless_borrow,
    clippy::needless_range_loop,
    clippy::redundant_closure,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::wrong_self_convention
)]

mod app;
mod engine;
mod examples;
mod export;
mod model;
mod pcb;
mod storage;
mod ui;

use app::{AlignDir, Selection, Tool};

// Re-export utilities moved to sub-modules.
// These keep `crate::X` paths working in engine/, export/, and in the local
// canvas/drawing code that still lives in main.rs.
pub(crate) use engine::parse_metric_value;
pub(crate) use model::{
    component_pin_defs, component_pins, component_size, distance_to_segment,
    point_touches_wire_segment, rotate_point,
};

use eframe::egui;
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use engine::mna;
use engine::netlist::build_circuit_netlist;
use engine::simulation as simulation_engine;
use engine::simulation::{Conductance, Simulation, SimulationStatus};
use engine::validation::{
    ErcRule, ErcSeverity, ErcViolation, pin_is_controller_scl, pin_is_controller_sda,
    pin_is_i2c_named, pin_is_microcontroller_gpio, validate_beginner_rules,
};
use model::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use storage::save::write_with_backup;
use ui::bottom_dock::{
    BottomDockAction, BottomDockModel, BottomDockTab, PageTabsAction, render_bottom_dock,
    render_page_tabs,
};
use ui::breadboard::{BreadboardRoute, build_breadboard_guide, render_breadboard_view};
use ui::canvas_overlay::draw_simulation_legend;
use ui::left_palette::{PaletteAction, PaletteTemplate, render_parts_palette, selected_part};
use ui::right_inspector::render_inspector_header;
use ui::status_bar::{StatusBarModel, render_status_bar};
use ui::top_toolbar::{TopToolbarAction, TopToolbarModel, render_top_toolbar};
use ui::validation_panel::{ValidationPanelAction, render_validation_panel};

const SAVE_PATH: &str = "cluster_circuit.json";
const AUTORECOVER_PATH: &str = "cluster_autorecover.json";

// Tool, AlignDir, Selection are defined in app/state.rs and imported above.

struct UiState {
    show_help: bool,
    bottom_dock_tab: BottomDockTab,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            show_help: false,
            bottom_dock_tab: BottomDockTab::Erc,
        }
    }
}

struct CanvasState {
    rect: Rect,
    cursor_world_pos: Option<Pos2>,
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
struct PaletteState {
    filter: String,
}

struct SimulationUiState {
    show_voltage_labels: bool,
    show_dc_overlay: bool,
    show_oscilloscope: bool,
    ac_freq_hz: f32,
}

impl Default for SimulationUiState {
    fn default() -> Self {
        Self {
            show_voltage_labels: false,
            show_dc_overlay: true,
            show_oscilloscope: false,
            ac_freq_hz: 1000.0,
        }
    }
}

#[derive(Default)]
struct InspectorState;

#[derive(Default)]
struct BreadboardUiState {
    open: bool,
}

#[derive(Default)]
struct HistoryState {
    undo: Vec<CircuitSnapshot>,
    redo: Vec<CircuitSnapshot>,
    dirty: bool,
}

struct CircuitApp {
    components: Vec<Component>,
    wires: Vec<Wire>,
    tool: Tool,
    selected: Option<Selection>,
    drag: Option<DragState>,
    draft_wire: Vec<Pos2>,
    wire_from_select: bool,
    grid: f32,
    snap: bool,
    orthogonal_wires: bool,
    show_pins: bool,
    simulate: bool,
    status: String,
    next_id: u64,
    counters: Counters,
    history_state: HistoryState,
    ui_state: UiState,
    canvas: CanvasState,
    palette_ui: PaletteState,
    simulation_ui: SimulationUiState,
    inspector_ui: InspectorState,
    breadboard_ui: BreadboardUiState,
    // View
    zoom: f32,
    pan: Vec2,
    // Clipboard
    /// Multi-component clipboard (supports single and group copy)
    clipboard: Vec<Component>,
    /// Wires internal to the copied group (both endpoints in selection)
    clipboard_wires: Vec<Wire>,
    // Multi-select: component IDs
    multi_selected: HashSet<u64>,
    // World-space anchor for rectangle selection drag
    rect_select_start: Option<Pos2>,
    // Net highlighting: wire ID hovered in select mode → highlight whole net
    hovered_net_wire: Option<u64>,
    // Cache of which wire IDs share the same net as hovered wire
    highlighted_net_wires: HashSet<u64>,
    // Pin snap preview (world pos of pin we're about to snap to)
    snap_target: Option<Pos2>,
    circuit_revision: u64,
    cached_netlist: Option<(u64, CircuitNetlist)>,
    cached_simulation: Option<(u64, u32, Simulation)>,
    cached_connected_pins: Option<(u64, Vec<(i32, i32)>)>,
    last_autorecover_revision: u64,
    // ── Multi-page ──────────────────────────────────────────────────────
    /// All pages: (name, components, wires, next_id, counters)
    pages: Vec<(String, Vec<Component>, Vec<Wire>, u64, Counters)>,
    current_page: usize,
    // ── Find dialog ─────────────────────────────────────────────────────
    show_find: bool,
    find_query: String,
    find_results: Vec<u64>, // component IDs matching query
    find_result_idx: usize,
    // ── Deferred canvas fit (set after demo load, applied once canvas rect is known) ──
    pending_fit: bool,
    // ── Inline value editing: (component_id, edited_text) ───────────────
    inline_edit: Option<(u64, String)>,
    // ── Right-click context menu: (screen_pos, target component ID) ──────
    context_menu: Option<(egui::Pos2, u64)>,
    // ── PNG screenshot pending ────────────────────────────────────────────
    screenshot_pending: bool,
}

impl CircuitApp {
    fn new() -> Self {
        Self {
            components: Vec::new(),
            wires: Vec::new(),
            tool: Tool::Select,
            selected: None,
            drag: None,
            draft_wire: Vec::new(),
            wire_from_select: false,
            grid: 20.0,
            snap: true,
            orthogonal_wires: true,
            show_pins: true,
            simulate: true,
            status: String::new(),
            next_id: 1,
            counters: Counters::default(),
            history_state: HistoryState::default(),
            ui_state: UiState::default(),
            canvas: CanvasState::default(),
            palette_ui: PaletteState::default(),
            simulation_ui: SimulationUiState::default(),
            inspector_ui: InspectorState,
            breadboard_ui: BreadboardUiState::default(),
            zoom: 1.0,
            pan: Vec2::ZERO,
            clipboard: Vec::new(),
            clipboard_wires: Vec::new(),
            multi_selected: HashSet::new(),
            rect_select_start: None,
            hovered_net_wire: None,
            highlighted_net_wires: HashSet::new(),
            snap_target: None,
            circuit_revision: 1,
            cached_netlist: None,
            cached_simulation: None,
            cached_connected_pins: None,
            last_autorecover_revision: 0,
            pages: vec![(
                "Page 1".to_string(),
                Vec::new(),
                Vec::new(),
                1,
                Counters::default(),
            )],
            current_page: 0,
            show_find: false,
            find_query: String::new(),
            find_results: Vec::new(),
            find_result_idx: 0,
            pending_fit: false,
            inline_edit: None,
            context_menu: None,
            screenshot_pending: false,
        }
    }

    fn handle_top_toolbar_action(&mut self, action: TopToolbarAction, ctx: &egui::Context) {
        match action {
            TopToolbarAction::SelectTool => {
                self.tool = Tool::Select;
                self.draft_wire.clear();
            }
            TopToolbarAction::WireTool => {
                self.tool = Tool::Wire;
                self.draft_wire.clear();
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
                self.record_history();
                let count = self.wires.len();
                for wire in &mut self.wires {
                    tidy_wire_points(wire);
                }
                self.status = format!("Tidied {} wire(s).", count);
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
            TopToolbarAction::Help => self.ui_state.show_help = true,
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
                self.selected = Some(Selection::Component(id));
                self.highlighted_net_wires.clear();
                self.hovered_net_wire = None;
                if let Some(comp) = self.components.iter().find(|component| component.id == id) {
                    let canvas_center = self.canvas.rect.center();
                    self.pan = canvas_center.to_vec2() - comp.pos.to_vec2() * self.zoom;
                }
            }
            ValidationPanelAction::SelectWire(id) => {
                self.selected = Some(Selection::Wire(id));
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

impl eframe::App for CircuitApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_app_style(ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "Cluster Circuits{}",
            if self.history_state.dirty { " *" } else { "" }
        )));

        // ── Handle screenshot events ──────────────────────────────────────
        if self.screenshot_pending {
            ctx.input(|i| {
                for event in &i.events {
                    if let egui::Event::Screenshot { image, .. } = event {
                        let path = "cluster_circuit.png";
                        let pixels: Vec<u8> = image
                            .pixels
                            .iter()
                            .flat_map(|c| [c.r(), c.g(), c.b(), c.a()])
                            .collect();
                        match write_png(path, image.width(), image.height(), &pixels) {
                            Ok(()) => self.status = format!("Saved {path}."),
                            Err(e) => self.status = format!("PNG export failed: {e}"),
                        }
                        self.screenshot_pending = false;
                    }
                }
            });
        }

        let simulation = self.current_simulation();
        let inspector_netlist = self.current_netlist();

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            let toolbar_action = render_top_toolbar(
                ui,
                TopToolbarModel {
                    tool: self.tool,
                    zoom: self.zoom,
                    simulation_summary: &simulation.summary,
                    snap: &mut self.snap,
                    orthogonal_wires: &mut self.orthogonal_wires,
                    show_pins: &mut self.show_pins,
                    simulate: &mut self.simulate,
                    show_breadboard_view: &mut self.breadboard_ui.open,
                    show_voltage_labels: &mut self.simulation_ui.show_voltage_labels,
                    show_dc_overlay: &mut self.simulation_ui.show_dc_overlay,
                    show_oscilloscope: &mut self.simulation_ui.show_oscilloscope,
                    grid: &mut self.grid,
                    ac_freq_hz: &mut self.simulation_ui.ac_freq_hz,
                },
            );
            if let Some(action) = toolbar_action {
                self.handle_top_toolbar_action(action, ctx);
            }
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
                        if let Some(action) =
                            render_parts_palette(ui, &filter, selected_part(self.tool))
                        {
                            match action {
                                PaletteAction::PlacePart { kind, label } => {
                                    self.tool = Tool::Place(kind);
                                    self.draft_wire.clear();
                                    self.status = format!("Placing {label}. Click the canvas.");
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
                                    net_v.sort_by(|a, b| b.partial_cmp(a).unwrap());
                                    net_v.dedup();
                                    if !net_v.is_empty() {
                                        dc_metric_row(
                                            ui,
                                            "Max node V",
                                            &mna::format_voltage(*net_v.first().unwrap()),
                                        );
                                        if net_v.len() > 1 {
                                            dc_metric_row(
                                                ui,
                                                "Min node V",
                                                &mna::format_voltage(*net_v.last().unwrap()),
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
                        if self.simulate {
                            if let Some(dc) = &simulation.dc {
                                if !dc.component_power.is_empty() {
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
                                        powers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

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
                            }
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
                render_inspector_header(ui);
                match self.selected {
                    Some(Selection::Component(id)) => {
                        let mut inspector_changed = false;
                        let mut inspector_status: Option<String> = None;
                        if let Some(component) = self.components.iter_mut().find(|c| c.id == id) {
                            let metadata = electrical_metadata(component.kind);
                            status_pill(
                                ui,
                                component_kind_label(component.kind),
                                StatusTone::Neutral,
                            );
                            ui.add_space(8.0);
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
                                    component_conductance(component) != Conductance::Open;
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
                            if let Some(warning) = simulation.component_warnings.get(&component.id)
                            {
                                ui.add_space(6.0);
                                egui::Frame::NONE
                                    .fill(Color32::from_rgb(58, 28, 24))
                                    .stroke(Stroke::new(1.0, Color32::from_rgb(160, 64, 54)))
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
                        }
                        if inspector_changed {
                            self.mark_dirty();
                        }
                        if let Some(status) = inspector_status {
                            self.status = status;
                        }
                    }
                    Some(Selection::Wire(id)) => {
                        if let Some(wire) = self.wires.iter().find(|w| w.id == id) {
                            status_pill(ui, "Wire / Net", StatusTone::Neutral);
                            ui.add_space(8.0);
                            metric_row(ui, "Points", wire.points.len().to_string());
                            metric_row(ui, "Length", format!("{:.0}px", wire_length(wire)));
                            if let Some(net_id) = inspector_netlist.wire_nets.get(&wire.id) {
                                let net_name = inspector_netlist
                                    .nets
                                    .iter()
                                    .find(|net| net.id == *net_id)
                                    .map(|net| net.name.as_str())
                                    .unwrap_or("UNKNOWN");
                                metric_row(ui, "Net", net_name);
                                let connected = inspector_netlist
                                    .pins
                                    .iter()
                                    .filter(|pin| pin.net_id == *net_id)
                                    .map(|pin| format!("{}.{}", pin.component_label, pin.pin_name))
                                    .collect::<Vec<_>>();
                                metric_row(
                                    ui,
                                    "Connected pins",
                                    if connected.is_empty() {
                                        "none".to_string()
                                    } else {
                                        connected.join(", ")
                                    },
                                );
                            }
                            metric_row(
                                ui,
                                "Status",
                                if inspector_netlist.floating_wires.contains(&wire.id) {
                                    "Floating"
                                } else if inspector_netlist.isolated_wires.contains(&wire.id) {
                                    "Open / one-pin connection"
                                } else {
                                    "Connected"
                                },
                            );
                            if let Some(dc) = &simulation.dc {
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
                                } else if dc.wire_current.contains_key(&wire.id) {
                                    metric_row(ui, "Current", "varies at junction / unavailable");
                                }
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
                .map(|page| page.0.clone())
                .collect::<Vec<_>>();
            if let Some(action) = render_page_tabs(ui, &page_names, self.current_page) {
                match action {
                    PageTabsAction::SwitchTo(idx) => self.switch_page(idx),
                    PageTabsAction::RenameDefault(idx) => {
                        self.pages[idx].0 = format!("Page {}", idx + 1);
                    }
                    PageTabsAction::AddPage => self.add_page(),
                }
            }
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            let active_tool = match self.tool {
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
                .map(|page| page.0.as_str())
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
                    selection: selection_summary(self.selected, &self.components, &self.wires),
                    component_count: self.components.len(),
                    wire_count: self.wires.len(),
                    cursor_world: self.canvas.cursor_world_pos,
                    dirty: self.history_state.dirty,
                    page_name,
                },
            );
        });

        egui::TopBottomPanel::bottom("bottom_dock")
            .default_height(190.0)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some(action) = render_bottom_dock(
                    ui,
                    BottomDockModel {
                        active_tab: self.ui_state.bottom_dock_tab,
                        violations: &simulation.erc,
                        has_components: !self.components.is_empty(),
                        simulation: &simulation,
                        breadboard_enabled: self.breadboard_ui.open,
                        status: &self.status,
                    },
                ) {
                    match action {
                        BottomDockAction::SetTab(tab) => self.ui_state.bottom_dock_tab = tab,
                        BottomDockAction::Validation(validation_action) => {
                            self.handle_validation_panel_action(validation_action);
                        }
                        BottomDockAction::OpenBreadboard => {
                            self.breadboard_ui.open = true;
                            self.ui_state.bottom_dock_tab = BottomDockTab::Breadboard;
                        }
                    }
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
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

            // Flow arrows are only for valid load paths. A short is a fault, not
            // useful "current flow", so it gets red wire highlighting instead.
            let flow_speed = simulation
                .dc
                .as_ref()
                .map(|dc| {
                    let max_i = dc
                        .branch_current
                        .values()
                        .map(|v| v.abs())
                        .fold(0.0_f64, f64::max);
                    (max_i as f32 * 8000.0).clamp(60.0, 220.0)
                })
                .unwrap_or(110.0);
            let flow_phase = ctx.input(|i| i.time) as f32 * flow_speed;
            let show_flow = flow_overlay_enabled(&simulation, self.simulate);
            if show_flow {
                ctx.request_repaint();
            }

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
            if space_held {
                if ctx
                    .input(|i| i.pointer.hover_pos())
                    .is_some_and(|p| rect.contains(p))
                {
                    ctx.set_cursor_icon(egui::CursorIcon::Grab);
                }
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
            draw_grid(&painter, rect, self.grid, view);
            if self.components.is_empty() && self.wires.is_empty() {
                draw_empty_canvas_hint(&painter, rect);
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
            for wire in &self.wires {
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
                    self.selected == Some(Selection::Wire(wire.id)),
                    energized,
                    simulation.shorted && energized,
                    show_flow && dc_i.is_some_and(|current| current.abs() > 1e-9),
                    flow_phase,
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

            // Compute connected pins for unconnected-pin rendering. This can be
            // expensive on larger circuits, so it is cached by circuit revision.
            let connected_pins = self.current_connected_pins();

            for component in &self.components {
                let cid = component.id;
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
                    self.selected == Some(Selection::Component(cid)),
                    self.show_pins,
                    simulation.energized_components.contains(&cid),
                    &connected_pins,
                    view,
                    dc_v,
                    dc_i,
                    self.simulation_ui.show_dc_overlay && self.simulate,
                );
            }

            // Multi-select highlight boxes
            for comp in &self.components {
                if self.multi_selected.contains(&comp.id) {
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
                        Stroke::new(1.5, Color32::from_rgb(110, 170, 220)),
                        StrokeKind::Outside,
                    );
                }
            }

            // Rectangle selection preview
            if let (Some(start), Some(end)) = (self.rect_select_start, self.canvas.cursor_world_pos)
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
                    Stroke::new(1.0, Color32::from_rgb(100, 178, 255)),
                    StrokeKind::Outside,
                );
            }

            draw_junctions(&painter, &self.wires, view);

            // ── Node voltage circles at wire junctions ─────────────────
            if self.simulation_ui.show_dc_overlay && self.simulate {
                if let Some(dc) = &simulation.dc {
                    draw_node_voltage_indicators(&painter, &self.wires, dc, view, dc.vmax);
                }
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

            // ── Hover / cursor helpers ───────────────────────────────────
            let hover_pos = ui.input(|i| i.pointer.hover_pos());
            let pointer_in_rect = hover_pos.filter(|pos| rect.contains(*pos));

            self.canvas.cursor_world_pos = None;
            self.snap_target = None;
            if let Some(raw_hover) = pointer_in_rect {
                let world_hover = view.to_world(raw_hover);
                self.canvas.cursor_world_pos = Some(world_hover);
                let mut world_pos = snap_pos(world_hover, rect, self.grid, self.snap);
                let in_wire_mode = self.tool == Tool::Wire;
                let in_select_mode = self.tool == Tool::Select;

                // Ghost preview for placement mode
                if let Tool::Place(place_kind) = self.tool {
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
                        };
                        component_size(&dummy) * view.zoom
                    };
                    let ghost_rect = Rect::from_center_size(ghost_screen, ghost_size);
                    // Translucent crosshair
                    let ghost_col = Color32::from_rgba_unmultiplied(80, 200, 140, 90);
                    let ghost_stroke = Stroke::new(1.6, ghost_col);
                    painter.rect_stroke(
                        ghost_rect.expand(4.0),
                        4.0,
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(60, 180, 120, 55)),
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
                        self.snap_target = Some(snapped);
                    }
                    // Check if we're snapping to a specific pin
                    let snap_pin = nearest_pin_at(world_pos, &self.components, 10.0);
                    if let Some((pin_label, comp_label)) = &snap_pin {
                        // Bright highlighted pin snap indicator
                        let sp = view.to_screen(world_pos);
                        painter.circle_stroke(
                            sp,
                            view.scale_f(10.0),
                            Stroke::new(2.5, Color32::from_rgb(50, 255, 120)),
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
                            Stroke::new(2.0, Color32::from_rgb(100, 240, 160)),
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
                            Stroke::new(1.8, Color32::from_rgb(80, 200, 255)),
                        );
                        painter.circle_filled(
                            sp,
                            view.scale_f(2.5),
                            Color32::from_rgb(80, 200, 255),
                        );
                    }
                }

                if in_wire_mode && !self.draft_wire.is_empty() {
                    let preview =
                        preview_wire_points(&self.draft_wire, world_pos, self.orthogonal_wires);
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
                    {
                        if let Some(comp) = self.components.iter().find(|c| c.id == hov_id) {
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
                            let base = if comp.kind == ComponentKind::PushButton && self.simulate {
                                let state = if comp.value.to_lowercase().contains("open") {
                                    "OPEN"
                                } else {
                                    "CLOSED"
                                };
                                format!("{} [{}]  Click to toggle", comp.label, state)
                            } else if comp.kind == ComponentKind::Switch
                                || comp.kind == ComponentKind::SlideSwitch
                            {
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
                                if let Some(&v) = dc.component_voltage.get(&hov_id) {
                                    if v.abs() > 1e-9 {
                                        parts.push(mna::format_voltage(v));
                                    }
                                }
                                if let Some(&i) = dc.branch_current.get(&hov_id) {
                                    if i.abs() > 1e-12 {
                                        parts.push(mna::format_current(i));
                                    }
                                }
                                if let Some(&p) = dc.component_power.get(&hov_id) {
                                    if p.abs() > 1e-12 {
                                        parts.push(mna::format_si(p, "W"));
                                    }
                                }
                                if !parts.is_empty() {
                                    dc_line = parts.join("  ·  ");
                                }
                            }
                            let tip_lines: Vec<&str> = if dc_line.is_empty() {
                                vec![&base]
                            } else {
                                vec![&base, &dc_line]
                            };
                            let tip_w = tip_lines
                                .iter()
                                .map(|l| l.len() as f32 * 6.2 + 12.0)
                                .fold(0.0_f32, f32::max);
                            let tip_h = tip_lines.len() as f32 * 16.0 + 6.0;
                            let tip_pos = raw_hover + egui::Vec2::new(14.0, -8.0);
                            let bg = Rect::from_min_size(
                                tip_pos - egui::Vec2::new(4.0, 4.0),
                                egui::Vec2::new(tip_w, tip_h),
                            );
                            painter.rect_filled(
                                bg,
                                3.0,
                                Color32::from_rgba_unmultiplied(15, 20, 28, 230),
                            );
                            painter.rect_stroke(
                                bg,
                                3.0,
                                Stroke::new(1.0, Color32::from_rgb(55, 65, 80)),
                                StrokeKind::Outside,
                            );
                            for (i, line) in tip_lines.iter().enumerate() {
                                let lpos = tip_pos + egui::Vec2::new(0.0, i as f32 * 16.0);
                                let col = if i == 0 {
                                    Color32::from_rgb(210, 218, 226)
                                } else {
                                    Color32::from_rgb(90, 210, 255)
                                };
                                painter.text(
                                    lpos,
                                    Align2::LEFT_TOP,
                                    line,
                                    egui::FontId::proportional(11.0),
                                    col,
                                );
                            }
                        }
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
                        let tip_pos = raw_hover + egui::Vec2::new(14.0, -8.0);
                        let tip_w = tip_text.len() as f32 * 6.2 + 10.0;
                        let bg = Rect::from_min_size(
                            tip_pos - egui::Vec2::new(4.0, 4.0),
                            egui::Vec2::new(tip_w, 20.0),
                        );
                        painter.rect_filled(
                            bg,
                            3.0,
                            Color32::from_rgba_unmultiplied(15, 20, 28, 230),
                        );
                        painter.rect_stroke(
                            bg,
                            3.0,
                            Stroke::new(
                                1.0,
                                if dc_v.is_some() {
                                    Color32::from_rgb(50, 120, 200)
                                } else {
                                    Color32::from_rgb(55, 65, 78)
                                },
                            ),
                            StrokeKind::Outside,
                        );
                        painter.text(
                            tip_pos,
                            Align2::LEFT_TOP,
                            &tip_text,
                            egui::FontId::proportional(11.0),
                            tip_col,
                        );
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
                match self.tool {
                    Tool::Select => {
                        let ctrl = ctx.input(|i| i.modifiers.command);
                        if let Some(sel) = hit_test(world_raw, &self.components, &self.wires) {
                            // Toggle switch/button on single click
                            if let Selection::Component(cid) = sel {
                                let comp_kind =
                                    self.components.iter().find(|c| c.id == cid).map(|c| c.kind);
                                if let Some(kind) = comp_kind {
                                    if component_is_switch(kind) {
                                        if kind == ComponentKind::PushButton {
                                            // Toggle on each click (open ↔ closed)
                                            self.record_history();
                                            if let Some(comp) =
                                                self.components.iter_mut().find(|c| c.id == cid)
                                            {
                                                let open =
                                                    comp.value.to_lowercase().contains("open");
                                                comp.value = if open {
                                                    "closed".to_string()
                                                } else {
                                                    "open".to_string()
                                                };
                                                let state = if open {
                                                    "▶ CLOSED (ON)"
                                                } else {
                                                    "■ OPEN (OFF)"
                                                };
                                                self.status = format!("{} {}", comp.label, state);
                                            }
                                            self.invalidate_analysis_cache();
                                            ctx.request_repaint();
                                        } else {
                                            // Toggle switch / slide switch: full edit with history
                                            self.record_history();
                                            if let Some(comp) =
                                                self.components.iter_mut().find(|c| c.id == cid)
                                            {
                                                let open =
                                                    comp.value.to_lowercase().contains("open");
                                                comp.value = if open {
                                                    "closed".to_string()
                                                } else {
                                                    "open".to_string()
                                                };
                                                let state =
                                                    if open { "▶ CLOSED" } else { "■ OPEN" };
                                                self.status = format!("{} {state}", comp.label);
                                            }
                                            self.invalidate_analysis_cache();
                                            ctx.request_repaint();
                                        }
                                        self.selected = Some(Selection::Component(cid));
                                    }
                                }
                            }
                            // Ctrl+click toggles multi-select; plain click sets primary selection
                            if ctrl {
                                if let Selection::Component(cid) = sel {
                                    if self.multi_selected.contains(&cid) {
                                        self.multi_selected.remove(&cid);
                                    } else {
                                        self.multi_selected.insert(cid);
                                    }
                                }
                            } else {
                                self.selected = Some(sel);
                                self.multi_selected.clear();
                            }
                        } else if !ctrl {
                            self.selected = None;
                            self.multi_selected.clear();
                        }
                    }
                    Tool::Place(kind) => {
                        self.add_component(kind, world_pos);
                    }
                    Tool::Wire => {
                        let wp =
                            snap_to_nearest_connection(world_pos, &self.components, &self.wires)
                                .unwrap_or(world_pos);
                        let already_started = !self.draft_wire.is_empty();
                        let landed = is_connection_point(wp, &self.components, &self.wires);
                        self.push_wire_point(wp);
                        if already_started && landed && self.draft_wire.len() >= 2 {
                            let points = std::mem::take(&mut self.draft_wire);
                            self.add_wire(points);
                            if self.wire_from_select {
                                self.tool = Tool::Select;
                                self.wire_from_select = false;
                            }
                        }
                    }
                }
            }

            if response.clicked_by(egui::PointerButton::Secondary) {
                if self.tool == Tool::Wire {
                    if !self.draft_wire.is_empty() {
                        let points = std::mem::take(&mut self.draft_wire);
                        self.add_wire(points);
                    }
                    if self.wire_from_select {
                        self.tool = Tool::Select;
                        self.wire_from_select = false;
                    }
                } else if self.tool == Tool::Select {
                    // Open context menu on component right-click
                    if let Some(raw_pos) = pointer_in_rect {
                        let world = view.to_world(raw_pos);
                        if let Some(Selection::Component(cid)) =
                            hit_test_component(world, &self.components)
                        {
                            self.selected = Some(Selection::Component(cid));
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
                    Stroke::new(1.0, Color32::from_rgb(55, 68, 82)),
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
                    self.selected = Some(Selection::Component(menu_cid));
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
                            self.tool = Tool::Wire;
                            self.wire_from_select = true;
                        }
                        _ => {}
                    }
                }
                ctx.request_repaint();
            }

            if response.double_clicked() && self.tool == Tool::Wire && self.draft_wire.len() >= 2 {
                let points = std::mem::take(&mut self.draft_wire);
                self.add_wire(points);
                if self.wire_from_select {
                    self.tool = Tool::Select;
                    self.wire_from_select = false;
                }
            }

            // Double-click component in Select mode → open inline value editor
            if response.double_clicked() && self.tool == Tool::Select && self.inline_edit.is_none()
            {
                if let Some(raw_pos) = pointer_in_rect {
                    let world = view.to_world(raw_pos);
                    if let Some(Selection::Component(cid)) =
                        hit_test_component(world, &self.components)
                    {
                        if let Some(comp) = self.components.iter().find(|c| c.id == cid) {
                            self.inline_edit = Some((cid, comp.value.clone()));
                        }
                    }
                }
            }

            // Render inline edit popup
            if let Some((edit_id, ref mut edit_text)) = self.inline_edit {
                if let Some(comp) = self.components.iter().find(|c| c.id == edit_id) {
                    let sp = view.to_screen(comp.pos);
                    let popup_rect = Rect::from_center_size(
                        sp + Vec2::new(0.0, -component_size(comp).y * view.zoom * 0.7),
                        Vec2::new(120.0, 26.0),
                    );
                    painter.rect_filled(popup_rect, 4.0, Color32::from_rgb(22, 27, 34));
                    painter.rect_stroke(
                        popup_rect,
                        4.0,
                        Stroke::new(1.5, Color32::from_rgb(80, 180, 120)),
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
                    self.record_history();
                    if let Some(comp) = self.components.iter_mut().find(|c| c.id == edit_id) {
                        comp.value = new_val;
                    }
                    self.status = format!("{} value updated.", label);
                    self.invalidate_analysis_cache();
                    self.inline_edit = None;
                } else if cancel {
                    self.inline_edit = None;
                }
            }

            if response.drag_started()
                && !panning
                && self.tool == Tool::Select
                && let Some(pos) = pointer_in_rect
            {
                let world = view.to_world(pos);
                if let Some((wire_id, point_index)) =
                    hit_test_wire_control_point(world, &self.wires)
                {
                    self.record_history();
                    self.drag = Some(DragState::WirePoint {
                        wire_id,
                        point_index,
                    });
                    self.selected = Some(Selection::Wire(wire_id));
                } else if hit_test_wire(world, &self.wires).is_some() {
                    self.record_history();
                    if let Some((wire_id, point_index)) =
                        insert_wire_control_point(world, &mut self.wires)
                    {
                        self.drag = Some(DragState::WirePoint {
                            wire_id,
                            point_index,
                        });
                        self.selected = Some(Selection::Wire(wire_id));
                    }
                } else if let Some(Selection::Component(id)) =
                    hit_test_component(world, &self.components)
                {
                    self.record_history();
                    if let Some(component) = self.components.iter().find(|c| c.id == id) {
                        self.drag = Some(DragState::Component {
                            id,
                            offset: world - component.pos,
                        });
                        self.selected = Some(Selection::Component(id));
                        // Ensure dragged component is in multi_selected if multi_selected is active
                        if !self.multi_selected.is_empty() {
                            self.multi_selected.insert(id);
                        }
                    }
                } else {
                    // Empty area — start rectangle selection
                    let ctrl = ctx.input(|i| i.modifiers.command);
                    self.rect_select_start = Some(world);
                    if !ctrl {
                        self.selected = None;
                        self.multi_selected.clear();
                    }
                }
            }

            if response.dragged()
                && !panning
                && let (Some(drag), Some(pos)) = (self.drag.clone(), pointer_in_rect)
            {
                let world = view.to_world(pos);
                let mut data_changed = false;
                let force_connection_snap = ctx.input(|i| i.modifiers.ctrl || i.modifiers.command);
                match drag {
                    DragState::Component { id, offset } => {
                        let snapped = snap_pos(world, rect, self.grid, self.snap);
                        let in_multi =
                            self.multi_selected.len() > 1 && self.multi_selected.contains(&id);
                        if in_multi {
                            let old_pos =
                                self.components.iter().find(|c| c.id == id).map(|c| c.pos);
                            if let Some(old_pos) = old_pos {
                                let mut delta = snapped - offset - old_pos;
                                let ids = self.multi_selected.clone();
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
                                for comp in self.components.iter_mut() {
                                    if ids.contains(&comp.id) {
                                        comp.pos += delta;
                                        data_changed = true;
                                    }
                                }
                                let new_pins = self
                                    .components
                                    .iter()
                                    .filter(|component| ids.contains(&component.id))
                                    .flat_map(component_pins)
                                    .collect::<Vec<_>>();
                                move_attached_wire_endpoints(&mut self.wires, &old_pins, &new_pins);
                                for wire in self.wires.iter_mut() {
                                    if wire.points.len() > 2 {
                                        let first = wire.points[0];
                                        let last = *wire.points.last().unwrap();
                                        if old_pins.iter().any(|p| first.distance(*p) <= 20.0)
                                            || old_pins.iter().any(|p| last.distance(*p) <= 20.0)
                                        {
                                            tidy_wire_points(wire);
                                        }
                                    }
                                }
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
                            if self.components[index].pos.distance(new_pos) > 0.1 {
                                self.components[index].pos = new_pos;
                                data_changed = true;
                            }
                            let new_pins = component_pins(&self.components[index]);
                            move_attached_wire_endpoints(&mut self.wires, &old_pins, &new_pins);
                            for wire in self.wires.iter_mut() {
                                if wire.points.len() > 2 {
                                    let first = wire.points[0];
                                    let last = *wire.points.last().unwrap();
                                    if old_pins.iter().any(|p| first.distance(*p) <= 20.0)
                                        || old_pins.iter().any(|p| last.distance(*p) <= 20.0)
                                    {
                                        tidy_wire_points(wire);
                                    }
                                }
                            }
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
                        move_wire_control_point(&mut self.wires, wire_id, point_index, snapped);
                        data_changed = true;
                    }
                }
                if data_changed {
                    self.mark_dirty();
                }
            }

            let primary_down = ctx.input(|i| i.pointer.primary_down());
            if !primary_down {
                self.drag = None;
                if let Some(start) = self.rect_select_start.take() {
                    if let Some(end) = self.canvas.cursor_world_pos {
                        if start.distance(end) > 4.0 {
                            let sel = Rect::from_two_pos(start, end);
                            for comp in &self.components {
                                if sel.contains(comp.pos) {
                                    self.multi_selected.insert(comp.id);
                                }
                            }
                            self.status =
                                format!("{} component(s) selected.", self.multi_selected.len());
                        }
                    }
                }
            }
        });

        // ── Keyboard shortcuts ────────────────────────────────────────────
        let backspace = ctx.input(|i| i.key_pressed(egui::Key::Backspace));
        // Backspace during wire drawing removes the last placed point
        if backspace && self.tool == Tool::Wire && !self.draft_wire.is_empty() {
            self.draft_wire.pop();
            if self.orthogonal_wires && self.draft_wire.len() >= 2 {
                // pop the auto-inserted L-bend corner too
                self.draft_wire.pop();
            }
            self.status = if self.draft_wire.is_empty() {
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
            } else if !self.draft_wire.is_empty() {
                self.draft_wire.clear();
                self.wire_from_select = false;
            } else if !self.multi_selected.is_empty() {
                self.multi_selected.clear();
                self.selected = None;
                self.rect_select_start = None;
            } else if self.selected.is_some() {
                self.selected = None;
            } else {
                self.tool = Tool::Select;
                self.wire_from_select = false;
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
            self.clipboard.clear();
            self.clipboard_wires.clear();
            let ids: Vec<u64> = if !self.multi_selected.is_empty() {
                self.multi_selected.iter().copied().collect()
            } else if let Some(Selection::Component(id)) = self.selected {
                vec![id]
            } else {
                Vec::new()
            };
            if !ids.is_empty() {
                self.clipboard = self
                    .components
                    .iter()
                    .filter(|c| ids.contains(&c.id))
                    .cloned()
                    .collect();
                // Copy wires whose BOTH endpoints lie within copied component pins
                let pin_positions: HashSet<(i32, i32)> = self
                    .clipboard
                    .iter()
                    .flat_map(|c| component_pin_defs(c))
                    .map(|p| (p.pos.x.round() as i32, p.pos.y.round() as i32))
                    .collect();
                self.clipboard_wires = self
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
                    self.clipboard.len(),
                    self.clipboard_wires.len()
                );
            }
        }

        // Ctrl+V — paste clipboard with offset
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::V)) {
            if self.clipboard.is_empty() {
                self.status = "Clipboard empty. Ctrl+C to copy first.".to_string();
            } else {
                self.record_history();
                let offset = Vec2::new(self.grid * 3.0, self.grid * 3.0);
                // Map old IDs → new IDs for wire reconnection
                let mut id_map: HashMap<u64, u64> = HashMap::new();
                let mut new_ids = Vec::new();
                let srcs = self.clipboard.clone();
                for src in &srcs {
                    let new_id = self.next_id();
                    id_map.insert(src.id, new_id);
                    let new_label = self.next_label(src.kind);
                    self.components.push(Component {
                        id: new_id,
                        kind: src.kind,
                        pos: src.pos + offset,
                        rotation: src.rotation,
                        label: new_label,
                        value: src.value.clone(),
                    });
                    new_ids.push(new_id);
                }
                // Paste internal wires with offset
                for w in &self.clipboard_wires.clone() {
                    let new_wire_id = self.next_id();
                    let pts: Vec<Pos2> = w.points.iter().map(|&p| p + offset).collect();
                    self.wires.push(Wire {
                        id: new_wire_id,
                        points: pts,
                    });
                }
                self.multi_selected = new_ids.iter().copied().collect();
                self.selected = None;
                self.mark_dirty();
                self.status = format!(
                    "Pasted {} component(s) + {} wire(s).",
                    new_ids.len(),
                    self.clipboard_wires.len()
                );
            }
        }

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::A)) {
            self.multi_selected = self.components.iter().map(|c| c.id).collect();
            self.status = format!("Selected all {} component(s).", self.multi_selected.len());
        }

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            self.save_circuit_json();
        }

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::O)) {
            self.load_circuit_json();
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Enter))
            && self.tool == Tool::Wire
            && self.draft_wire.len() >= 2
        {
            let points = std::mem::take(&mut self.draft_wire);
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
            self.record_history();
            for wire in &mut self.wires {
                tidy_wire_points(wire);
            }
            self.status = format!("Tidied {} wire(s).", self.wires.len());
        } else if ctx.input(|i| i.key_pressed(egui::Key::T))
            && let Some(Selection::Wire(id)) = self.selected
            && self.wires.iter().any(|w| w.id == id)
        {
            self.record_history();
            if let Some(wire) = self.wires.iter_mut().find(|w| w.id == id) {
                tidy_wire_points(wire);
            }
            self.status = "Wire straightened.".to_string();
        }

        // Ctrl+A — Select all components
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::A)) {
            self.multi_selected = self.components.iter().map(|c| c.id).collect();
            self.selected = None;
            self.status = format!("{} component(s) selected.", self.multi_selected.len());
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
            self.tool = Tool::Wire;
            self.draft_wire.clear();
            self.status = "Wire tool.".to_string();
        }

        // S — Select tool
        if ctx.input(|i| !i.modifiers.any() && i.key_pressed(egui::Key::S)) {
            self.tool = Tool::Select;
            self.draft_wire.clear();
            self.wire_from_select = false;
            self.status = "Select tool.".to_string();
        }

        // F — Zoom to fit
        if ctx.input(|i| !i.modifiers.any() && i.key_pressed(egui::Key::F)) {
            self.zoom_to_fit();
        }

        // ? — Toggle shortcuts help
        if ctx.input(|i| !i.modifiers.any() && i.key_pressed(egui::Key::Questionmark)) {
            self.ui_state.show_help = !self.ui_state.show_help;
        }

        // Space — toggle simulation on/off (when not dragging/panning)
        if ctx.input(|i| !i.modifiers.any() && i.key_pressed(egui::Key::Space))
            && self.drag.is_none()
            && self.tool != Tool::Wire
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
                if self.tool == Tool::Place(kind) {
                    self.tool = Tool::Select;
                    self.status = "Select tool.".to_string();
                } else {
                    self.tool = Tool::Place(kind);
                    self.draft_wire.clear();
                    self.status = format!("Placing {}. Click the canvas.", name);
                }
            }
        }

        if self.drag.is_none() && !ctx.input(|i| i.pointer.primary_down()) {
            self.flush_autorecover_if_needed();
        }

        // ── Find dialog (floating overlay) ──────────────────────────────────
        // ── Keyboard shortcuts help dialog ───────────────────────────────────
        if self.ui_state.show_help {
            let mut open = self.ui_state.show_help;
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
            self.ui_state.show_help = open;
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
            let mut selected_route: Option<BreadboardRoute> = None;
            egui::Window::new("Breadboard View")
                .open(&mut open)
                .collapsible(true)
                .resizable(true)
                .default_pos(egui::Pos2::new(80.0, 120.0))
                .default_size(Vec2::new(520.0, 420.0))
                .show(ctx, |ui| {
                    selected_route = render_breadboard_view(ui, &guide);
                });
            if let Some(route) = selected_route {
                self.hovered_net_wire = None;
                self.highlighted_net_wires = inspector_netlist
                    .wire_nets
                    .iter()
                    .filter_map(|(wire_id, net_id)| (*net_id == route.net_id).then_some(*wire_id))
                    .collect();
                self.selected = Some(Selection::Component(route.from_component_id));
                self.status = format!(
                    "Breadboard route: {} {} -> {} {}.",
                    route.from_label, route.from_pin, route.to_label, route.to_pin
                );
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
                                .hint_text("Label or value…"),
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
                        self.selected = Some(Selection::Component(cur_id));
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
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.flush_autorecover_if_needed();
    }
}

/// Canvas coordinate transform: world ↔ screen.
///
/// World positions are the stored component/wire positions.
/// Screen positions are egui painter pixel positions.
/// At zoom=1 and pan=ZERO the two spaces are identical.
#[derive(Clone, Copy)]
struct CanvasView {
    zoom: f32,
    pan: Vec2,
    origin: Pos2, // canvas rect.min
}

impl CanvasView {
    fn to_screen(&self, world: Pos2) -> Pos2 {
        self.origin + (world - self.origin) * self.zoom + self.pan
    }

    fn to_world(&self, screen: Pos2) -> Pos2 {
        self.origin + ((screen - self.origin) - self.pan) / self.zoom
    }

    fn scale_f(&self, f: f32) -> f32 {
        (f * self.zoom).clamp(f * 0.4, f * 2.0)
    }
}

fn snap_pos(pos: Pos2, _rect: Rect, grid: f32, snap: bool) -> Pos2 {
    if snap {
        Pos2::new((pos.x / grid).round() * grid, (pos.y / grid).round() * grid)
    } else {
        pos
    }
}

fn hit_test(pos: Pos2, components: &[Component], wires: &[Wire]) -> Option<Selection> {
    if let Some(selection) = hit_test_component(pos, components) {
        return Some(selection);
    }
    if let Some(selection) = hit_test_wire(pos, wires) {
        return Some(selection);
    }
    None
}

fn selection_summary(
    selected: Option<Selection>,
    components: &[Component],
    wires: &[Wire],
) -> String {
    match selected {
        Some(Selection::Component(id)) => components
            .iter()
            .find(|component| component.id == id)
            .map(|component| format!("Selected: {}", component.label))
            .unwrap_or_else(|| "Selected: missing component".to_string()),
        Some(Selection::Wire(id)) => wires
            .iter()
            .find(|wire| wire.id == id)
            .map(|wire| format!("Selected: wire {:.0}px", wire_length(wire)))
            .unwrap_or_else(|| "Selected: missing wire".to_string()),
        None => "Selected: none".to_string(),
    }
}

fn hit_test_component(pos: Pos2, components: &[Component]) -> Option<Selection> {
    for component in components.iter().rev() {
        if component_bounds(component).contains(pos) {
            return Some(Selection::Component(component.id));
        }
    }
    None
}

fn hit_test_wire(pos: Pos2, wires: &[Wire]) -> Option<Selection> {
    let threshold = 10.0;
    for wire in wires.iter().rev() {
        for segment in wire.points.windows(2) {
            let a = segment[0];
            let b = segment[1];
            if distance_to_segment(pos, a, b) <= threshold {
                return Some(Selection::Wire(wire.id));
            }
        }
    }
    None
}

fn hit_test_wire_control_point(pos: Pos2, wires: &[Wire]) -> Option<(u64, usize)> {
    let threshold = 14.0;
    for wire in wires.iter().rev() {
        for (index, point) in wire.points.iter().enumerate() {
            if pos.distance(*point) <= threshold {
                return Some((wire.id, index));
            }
        }
    }
    None
}

fn insert_wire_control_point(pos: Pos2, wires: &mut [Wire]) -> Option<(u64, usize)> {
    let threshold = 10.0;
    for wire in wires.iter_mut().rev() {
        for index in 0..wire.points.len().saturating_sub(1) {
            let a = wire.points[index];
            let b = wire.points[index + 1];
            if distance_to_segment(pos, a, b) <= threshold {
                let horizontal = (a.y - b.y).abs() <= 0.5;
                let vertical = (a.x - b.x).abs() <= 0.5;
                let inserted = if horizontal {
                    Pos2::new(pos.x.clamp(a.x.min(b.x), a.x.max(b.x)), a.y)
                } else if vertical {
                    Pos2::new(a.x, pos.y.clamp(a.y.min(b.y), a.y.max(b.y)))
                } else {
                    closest_point_on_segment(pos, a, b)
                };
                wire.points.insert(index + 1, inserted);
                return Some((wire.id, index + 1));
            }
        }
    }
    None
}

fn move_wire_control_point(wires: &mut [Wire], wire_id: u64, point_index: usize, pos: Pos2) {
    let Some(wire) = wires.iter_mut().find(|wire| wire.id == wire_id) else {
        return;
    };
    if point_index >= wire.points.len() {
        return;
    }
    wire.points[point_index] = pos;
    let is_endpoint = point_index == 0 || point_index + 1 == wire.points.len();
    if !is_endpoint {
        straighten_neighbor_segments(wire, point_index);
    }
}

fn straighten_neighbor_segments(wire: &mut Wire, point_index: usize) {
    let point = wire.points[point_index];
    if point_index > 0 {
        let prev = wire.points[point_index - 1];
        let dx = (point.x - prev.x).abs();
        let dy = (point.y - prev.y).abs();
        if dx <= dy {
            wire.points[point_index - 1].x = point.x;
        } else {
            wire.points[point_index - 1].y = point.y;
        }
    }
    if point_index + 1 < wire.points.len() {
        let next = wire.points[point_index + 1];
        let dx = (point.x - next.x).abs();
        let dy = (point.y - next.y).abs();
        if dx <= dy {
            wire.points[point_index + 1].x = point.x;
        } else {
            wire.points[point_index + 1].y = point.y;
        }
    }
}

fn is_connection_point(pos: Pos2, components: &[Component], wires: &[Wire]) -> bool {
    for component in components {
        for pin in component_pin_defs(component) {
            if pin.pos.distance(pos) < 6.0 {
                return true;
            }
        }
    }
    for wire in wires {
        // Endpoints
        for &ep in wire.points.first().iter().chain(wire.points.last().iter()) {
            if ep.distance(pos) < 6.0 {
                return true;
            }
        }
        // Mid-segment: a point on a segment is also a valid connection target
        for seg in wire.points.windows(2) {
            if distance_to_segment(pos, seg[0], seg[1]) < 4.0 {
                return true;
            }
        }
    }
    false
}

/// Returns (pin_label, component_label) when pos is within `radius` of a pin.
fn nearest_pin_at(pos: Pos2, components: &[Component], radius: f32) -> Option<(String, String)> {
    let mut best_dist = radius;
    let mut result = None;
    for comp in components {
        for pin in component_pin_defs(comp) {
            let d = pin.pos.distance(pos);
            if d < best_dist {
                best_dist = d;
                result = Some((pin.label.to_string(), comp.label.clone()));
            }
        }
    }
    result
}

fn snap_to_nearest_connection(pos: Pos2, components: &[Component], wires: &[Wire]) -> Option<Pos2> {
    let mut best: Option<Pos2> = None;
    // Pins get priority (smaller threshold)
    let mut best_dist_pin = 30.0_f32;
    let mut best_dist_wire = 20.0_f32;

    // Component pins — highest priority
    for component in components {
        for pin in component_pin_defs(component) {
            let d = pin.pos.distance(pos);
            if d < best_dist_pin {
                best_dist_pin = d;
                best = Some(pin.pos);
            }
        }
    }

    // Wire endpoints and all intermediate points
    for wire in wires {
        for &pt in &wire.points {
            let d = pt.distance(pos);
            if d < best_dist_wire {
                best_dist_wire = d;
                // Only override if no pin is already closer
                if best.is_none() || d < best_dist_pin {
                    best = Some(pt);
                }
            }
        }
        // Also snap to the closest point on each segment (for T-junctions)
        for seg in wire.points.windows(2) {
            let snapped = closest_point_on_segment(pos, seg[0], seg[1]);
            let d = snapped.distance(pos);
            if d < best_dist_wire && best_dist_pin > d {
                best_dist_wire = d;
                best = Some(snapped);
            }
        }
    }

    best
}

fn snap_delta_for_moved_components(
    components: &[Component],
    wires: &[Wire],
    moved_ids: &HashSet<u64>,
    delta: Vec2,
    old_pins: &[Pos2],
) -> Option<Vec2> {
    let mut best_adjust = None;
    let mut best_dist = 28.0_f32;

    let moving_pins = components
        .iter()
        .filter(|component| moved_ids.contains(&component.id))
        .flat_map(|component| {
            let mut moved = component.clone();
            moved.pos += delta;
            component_pin_defs(&moved).into_iter().map(|pin| pin.pos)
        })
        .collect::<Vec<_>>();

    if moving_pins.is_empty() {
        return None;
    }

    for moving_pin in &moving_pins {
        for component in components {
            if moved_ids.contains(&component.id) {
                continue;
            }
            for target_pin in component_pin_defs(component) {
                let d = moving_pin.distance(target_pin.pos);
                if d < best_dist {
                    best_dist = d;
                    best_adjust = Some(target_pin.pos - *moving_pin);
                }
            }
        }

        for wire in wires {
            for &target in &wire.points {
                if old_pins
                    .iter()
                    .any(|old_pin| old_pin.distance(target) <= 1.0)
                {
                    continue;
                }
                let d = moving_pin.distance(target);
                if d < best_dist {
                    best_dist = d;
                    best_adjust = Some(target - *moving_pin);
                }
            }

            for segment in wire.points.windows(2) {
                if old_pins
                    .iter()
                    .any(|old_pin| point_touches_wire_segment(*old_pin, segment[0], segment[1]))
                {
                    continue;
                }
                let target = closest_point_on_segment(*moving_pin, segment[0], segment[1]);
                let d = moving_pin.distance(target);
                if d < best_dist {
                    best_dist = d;
                    best_adjust = Some(target - *moving_pin);
                }
            }
        }
    }

    best_adjust
}

fn is_on_wire_segment(pos: Pos2, wires: &[Wire]) -> bool {
    for wire in wires {
        for seg in wire.points.windows(2) {
            if distance_to_segment(pos, seg[0], seg[1]) < 2.5
                && pos.distance(seg[0]) > 2.0
                && pos.distance(seg[1]) > 2.0
            {
                return true;
            }
        }
    }
    false
}

fn closest_point_on_segment(p: Pos2, a: Pos2, b: Pos2) -> Pos2 {
    let ab = b - a;
    let ap = p - a;
    let ab_len_sq = ab.x * ab.x + ab.y * ab.y;
    if ab_len_sq == 0.0 {
        return a;
    }
    let t = ((ap.x * ab.x) + (ap.y * ab.y)) / ab_len_sq;
    a + ab * t.clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Copy)]
enum StatusTone {
    Neutral,
    Live,
    Warning,
    Error,
}

struct LessonReport {
    title: String,
    checks: Vec<LessonCheck>,
    next_action: String,
}

struct LessonCheck {
    label: String,
    passed: bool,
    detail: String,
}

fn apply_app_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = Color32::from_rgb(18, 21, 26);
    visuals.panel_fill = Color32::from_rgb(18, 21, 26);
    visuals.extreme_bg_color = Color32::from_rgb(12, 14, 18);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(31, 36, 43);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(43, 50, 59);
    visuals.widgets.active.bg_fill = Color32::from_rgb(46, 58, 68);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(52, 58, 66));
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.visuals = ctx.style().visuals.clone();
    ctx.set_style(style);
}

fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(11.0)
            .strong()
            .color(Color32::from_rgb(138, 149, 160)),
    );
}

fn compact_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(Color32::from_rgb(215, 222, 230)))
            .fill(Color32::from_rgb(31, 36, 43))
            .stroke(Stroke::new(1.0, Color32::from_rgb(56, 64, 74)))
            .min_size(Vec2::new(74.0, 26.0)),
    )
}

fn palette_action(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add_sized(
        Vec2::new(ui.available_width(), 22.0),
        egui::Button::new(
            egui::RichText::new(label)
                .size(10.5)
                .color(Color32::from_rgb(216, 224, 232)),
        )
        .fill(Color32::from_rgb(28, 33, 39))
        .stroke(Stroke::new(1.0, Color32::from_rgb(48, 56, 64))),
    )
}

/// Simulation-support badge row for the inspector: a compact colored pill
/// (not just plain text) so the confidence level of a component's model is
/// scannable at a glance, matching the same badge style used for wire/pin
/// status. `ExactDc` reads as neutral; every reduced-confidence level
/// (`ApproximateDc`, `DigitalOnly`, `SymbolOnly`, `Unsupported`) reads as a
/// warning, mirroring `SimulationSupport::needs_inspector_warning`.
fn simulation_support_row(ui: &mut egui::Ui, label: &str, support: SimulationSupport) {
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new(label)
                .size(11.0)
                .color(Color32::from_rgb(135, 146, 156)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let tone = if support.needs_inspector_warning() {
                StatusTone::Warning
            } else {
                StatusTone::Neutral
            };
            status_pill(ui, support.label(), tone);
        });
    });
}

fn status_pill(ui: &mut egui::Ui, text: &str, tone: StatusTone) {
    let (fill, stroke, color) = match tone {
        StatusTone::Neutral => (
            Color32::from_rgb(30, 36, 43),
            Color32::from_rgb(58, 68, 78),
            Color32::from_rgb(210, 218, 226),
        ),
        StatusTone::Live => (
            Color32::from_rgb(54, 42, 22),
            Color32::from_rgb(132, 92, 34),
            Color32::from_rgb(255, 198, 92),
        ),
        StatusTone::Warning => (
            Color32::from_rgb(54, 45, 22),
            Color32::from_rgb(150, 115, 38),
            Color32::from_rgb(255, 215, 110),
        ),
        StatusTone::Error => (
            Color32::from_rgb(58, 30, 30),
            Color32::from_rgb(142, 64, 58),
            Color32::from_rgb(255, 128, 112),
        ),
    };
    egui::Frame::NONE
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(11.0).strong().color(color));
        });
}

fn simulation_tone(simulation: &Simulation) -> StatusTone {
    let has_erc_error = simulation
        .erc
        .iter()
        .any(|e| e.severity == ErcSeverity::Error);
    if simulation.shorted || has_erc_error {
        StatusTone::Error
    } else {
        match simulation.status {
            SimulationStatus::Ok => StatusTone::Live,
            SimulationStatus::Warning => StatusTone::Warning,
            SimulationStatus::Failed => StatusTone::Error,
        }
    }
}

fn simulation_status_label(status: SimulationStatus) -> &'static str {
    match status {
        SimulationStatus::Ok => "OK",
        SimulationStatus::Warning => "Warning",
        SimulationStatus::Failed => "Failed",
    }
}

fn simulation_status_from_solver(
    shorted: bool,
    dc_error: Option<&mna::SimulationError>,
) -> SimulationStatus {
    if shorted {
        SimulationStatus::Failed
    } else if dc_error.is_some() {
        SimulationStatus::Warning
    } else {
        SimulationStatus::Ok
    }
}

fn simulation_text_color(simulation: &Simulation) -> Color32 {
    match simulation_tone(simulation) {
        StatusTone::Error => Color32::from_rgb(255, 128, 112),
        StatusTone::Live => Color32::from_rgb(255, 198, 92),
        StatusTone::Warning => Color32::from_rgb(255, 215, 110),
        StatusTone::Neutral => Color32::from_rgb(152, 162, 172),
    }
}

fn simulation_warning_count(simulation: &Simulation) -> usize {
    simulation
        .erc
        .iter()
        .filter(|e| matches!(e.severity, ErcSeverity::Error | ErcSeverity::Warning))
        .count()
}

fn flow_overlay_enabled(simulation: &Simulation, simulate_enabled: bool) -> bool {
    simulate_enabled && !simulation.shorted && !simulation.energized_wires.is_empty()
}

fn lesson_report(components: &[Component], simulation: &Simulation) -> Option<LessonReport> {
    let notes = components
        .iter()
        .filter(|component| component.kind == ComponentKind::TextNote)
        .map(|component| component.value.trim())
        .filter(|value| value.to_ascii_lowercase().contains("expect:"))
        .collect::<Vec<_>>();
    if notes.is_empty() {
        return None;
    }

    let joined = notes.join("\n");
    let lower = joined.to_ascii_lowercase();
    let mut checks = Vec::new();

    let erc_errors = simulation
        .erc
        .iter()
        .filter(|violation| violation.severity == ErcSeverity::Error)
        .count();
    let erc_warnings = simulation
        .erc
        .iter()
        .filter(|violation| violation.severity == ErcSeverity::Warning)
        .count();
    let led_ids = components
        .iter()
        .filter(|component| component.kind == ComponentKind::Led)
        .map(|component| component.id)
        .collect::<Vec<_>>();
    let motor_ids = components
        .iter()
        .filter(|component| component.kind == ComponentKind::DcMotor)
        .map(|component| component.id)
        .collect::<Vec<_>>();
    let capacitor_ids = components
        .iter()
        .filter(|component| component.kind == ComponentKind::Capacitor)
        .map(|component| component.id)
        .collect::<Vec<_>>();
    let energized_leds = led_ids
        .iter()
        .filter(|id| simulation.energized_components.contains(id))
        .count();
    let energized_motors = motor_ids
        .iter()
        .filter(|id| simulation.energized_components.contains(id))
        .count();
    let energized_caps = capacitor_ids
        .iter()
        .filter(|id| simulation.energized_components.contains(id))
        .count();

    let expects_on = lower.contains("expect: on") || lower.contains("expect: both on");
    let expects_off = lower.contains("expect: off") || lower.contains("open circuit");
    let expects_error = lower.contains("expect: error") || lower.contains("short circuit");
    let expects_warning = lower.contains("expect: warning") || lower.contains("warning");

    if expects_on {
        checks.push(LessonCheck {
            label: "Closed path".to_string(),
            passed: simulation.closed && !simulation.shorted && erc_errors == 0,
            detail: simulation.summary.clone(),
        });
        if !led_ids.is_empty() {
            checks.push(LessonCheck {
                label: "LED output".to_string(),
                passed: energized_leds == led_ids.len(),
                detail: format!("{energized_leds}/{} lit", led_ids.len()),
            });
        }
        if !motor_ids.is_empty() {
            checks.push(LessonCheck {
                label: "Motor output".to_string(),
                passed: energized_motors == motor_ids.len(),
                detail: format!("{energized_motors}/{} running", motor_ids.len()),
            });
        }
    }

    if expects_off {
        checks.push(LessonCheck {
            label: "No closed current path".to_string(),
            passed: !simulation.closed && !simulation.shorted,
            detail: simulation.summary.clone(),
        });
        if !led_ids.is_empty() {
            checks.push(LessonCheck {
                label: "LED stays off".to_string(),
                passed: energized_leds == 0,
                detail: format!("{energized_leds}/{} lit", led_ids.len()),
            });
        }
        if lower.contains("capacitor") && !capacitor_ids.is_empty() {
            checks.push(LessonCheck {
                label: "Capacitor blocks DC".to_string(),
                passed: energized_caps == 0,
                detail: format!("{energized_caps}/{} conducting", capacitor_ids.len()),
            });
        }
    }

    if expects_error {
        checks.push(LessonCheck {
            label: "Error detected".to_string(),
            passed: simulation.shorted || erc_errors > 0,
            detail: format!("{erc_errors} ERC error(s)"),
        });
    }

    if expects_warning {
        checks.push(LessonCheck {
            label: "Warning detected".to_string(),
            passed: erc_errors + erc_warnings > 0,
            detail: format!("{erc_errors} error(s), {erc_warnings} warning(s)"),
        });
        if lower.contains("gpio") && lower.contains("motor") {
            let gpio_motor_warning = simulation.erc.iter().any(|violation| {
                violation.message.contains("GPIO") && violation.message.contains("motor")
            });
            checks.push(LessonCheck {
                label: "GPIO motor rule".to_string(),
                passed: gpio_motor_warning,
                detail: "Use a driver, transistor, or relay".to_string(),
            });
            if !motor_ids.is_empty() {
                checks.push(LessonCheck {
                    label: "Motor stays off".to_string(),
                    passed: energized_motors == 0,
                    detail: format!("{energized_motors}/{} running", motor_ids.len()),
                });
            }
        }
    }

    if checks.is_empty() {
        checks.push(LessonCheck {
            label: "Simulation state".to_string(),
            passed: !simulation.shorted,
            detail: simulation.summary.clone(),
        });
    }

    let passed = checks.iter().filter(|check| check.passed).count();
    let total = checks.len();
    let next_action = if passed == total {
        "Matches the EXPECT note. Try editing one wire or value and watch this change.".to_string()
    } else if expects_off {
        "Find the unwanted live path or missing break, then compare orange highlights.".to_string()
    } else if expects_error || expects_warning {
        "Open the ERC panel and click the reported item to locate the fault.".to_string()
    } else {
        "Complete the source-load-return path, then verify the orange live path.".to_string()
    };

    Some(LessonReport {
        title: format!("Lesson Check {passed}/{total}"),
        checks,
        next_action,
    })
}

fn metric_row(ui: &mut egui::Ui, label: impl Into<String>, value: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new(label.into())
                .size(11.0)
                .color(Color32::from_rgb(135, 146, 156)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value.into())
                    .size(11.0)
                    .color(Color32::from_rgb(210, 218, 226)),
            );
        });
    });
}

fn render_lesson_report(ui: &mut egui::Ui, report: &LessonReport) {
    let all_passed = report.checks.iter().all(|check| check.passed);
    let (fill, stroke, title_color) = if all_passed {
        (
            Color32::from_rgb(20, 42, 34),
            Color32::from_rgb(58, 150, 105),
            Color32::from_rgb(150, 245, 185),
        )
    } else {
        (
            Color32::from_rgb(48, 36, 22),
            Color32::from_rgb(170, 115, 48),
            Color32::from_rgb(255, 205, 115),
        )
    };

    egui::Frame::NONE
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::symmetric(9, 7))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(&report.title)
                    .size(12.0)
                    .strong()
                    .color(title_color),
            );
            ui.add_space(4.0);
            for check in &report.checks {
                let mark = if check.passed { "PASS" } else { "CHECK" };
                let color = if check.passed {
                    Color32::from_rgb(150, 235, 180)
                } else {
                    Color32::from_rgb(255, 190, 115)
                };
                ui.horizontal(|ui| {
                    ui.add_sized(
                        Vec2::new(42.0, 16.0),
                        egui::Label::new(
                            egui::RichText::new(mark)
                                .size(10.0)
                                .monospace()
                                .strong()
                                .color(color),
                        ),
                    );
                    ui.label(
                        egui::RichText::new(&check.label)
                            .size(11.0)
                            .color(Color32::from_rgb(220, 228, 235)),
                    );
                });
                ui.label(
                    egui::RichText::new(&check.detail)
                        .size(10.5)
                        .color(Color32::from_rgb(150, 160, 170)),
                );
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(&report.next_action)
                    .size(11.0)
                    .color(Color32::from_rgb(205, 214, 222)),
            );
        });
}

fn dc_metric_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new(label)
                .size(11.0)
                .color(Color32::from_rgb(130, 200, 160)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value)
                    .size(12.0)
                    .strong()
                    .color(Color32::from_rgb(140, 230, 180)),
            );
        });
    });
}

fn edit_row(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    ui.label(
        egui::RichText::new(label)
            .size(11.0)
            .color(Color32::from_rgb(135, 146, 156)),
    );
    ui.add_sized(
        Vec2::new(ui.available_width(), 25.0),
        egui::TextEdit::singleline(value)
            .text_color(Color32::from_rgb(230, 235, 240))
            .background_color(Color32::from_rgb(12, 15, 19)),
    )
    .changed()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SectionMode {
    Open,
    Collapsed,
}

fn palette_section(
    ui: &mut egui::Ui,
    title: &str,
    mode: SectionMode,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    ui.add_space(3.0);
    egui::Frame::NONE
        .fill(Color32::from_rgb(23, 28, 35))
        .stroke(Stroke::new(1.0, Color32::from_rgb(58, 68, 80)))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(5, 3))
        .show(ui, |ui| {
            let title = egui::RichText::new(title.to_uppercase())
                .size(10.0)
                .strong()
                .color(Color32::from_rgb(190, 204, 218));
            match mode {
                SectionMode::Open => {
                    ui.label(title);
                    ui.add_space(3.0);
                    add_contents(ui);
                }
                SectionMode::Collapsed => {
                    egui::CollapsingHeader::new(title)
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.add_space(2.0);
                            add_contents(ui);
                        });
                }
            }
        });
}

fn push_unique_point(points: &mut Vec<Pos2>, pos: Pos2) {
    if points.last().is_some_and(|last| last.distance(pos) < 0.5) {
        return;
    }
    points.push(pos);
}

fn move_attached_wire_endpoints(wires: &mut [Wire], old_pins: &[Pos2], new_pins: &[Pos2]) {
    for wire in wires {
        if wire.points.is_empty() {
            continue;
        }

        if let Some(new_pos) = moved_pin_for_point(wire.points[0], old_pins, new_pins) {
            wire.points[0] = new_pos;
            keep_wire_end_orthogonal(wire, true);
        }

        let last_index = wire.points.len() - 1;
        if let Some(new_pos) = moved_pin_for_point(wire.points[last_index], old_pins, new_pins) {
            wire.points[last_index] = new_pos;
            keep_wire_end_orthogonal(wire, false);
        }
    }
}

fn moved_pin_for_point(point: Pos2, old_pins: &[Pos2], new_pins: &[Pos2]) -> Option<Pos2> {
    old_pins
        .iter()
        .zip(new_pins)
        .find(|(old_pin, _)| point.distance(**old_pin) <= 20.0)
        .map(|(_, &new_pin)| new_pin)
}

fn wire_path_pin_crossings(points: &[Pos2], pins: &[Pos2]) -> usize {
    pins.iter()
        .filter(|&&pin| {
            points
                .windows(2)
                .any(|segment| point_touches_wire_segment(pin, segment[0], segment[1]))
        })
        .count()
}

fn keep_wire_end_orthogonal(wire: &mut Wire, first: bool) {
    if wire.points.len() < 2 {
        return;
    }
    if wire.points.len() == 2 {
        let start = wire.points[0];
        let end = wire.points[1];
        if (start.x - end.x).abs() <= 0.5 || (start.y - end.y).abs() <= 0.5 {
            return;
        }
        let corner = Pos2::new(end.x, start.y);
        wire.points = simplify_wire(vec![start, corner, end]);
        return;
    }
    let (end_index, neighbor_index) = if first {
        (0, 1)
    } else {
        (wire.points.len() - 1, wire.points.len() - 2)
    };
    let end = wire.points[end_index];
    let neighbor = wire.points[neighbor_index];
    let dx = (end.x - neighbor.x).abs();
    let dy = (end.y - neighbor.y).abs();
    if dx <= dy {
        wire.points[neighbor_index].x = end.x;
    } else {
        wire.points[neighbor_index].y = end.y;
    }
}

fn simplify_wire(points: Vec<Pos2>) -> Vec<Pos2> {
    let mut deduped = Vec::new();
    for point in points {
        push_unique_point(&mut deduped, point);
    }

    let mut simplified: Vec<Pos2> = Vec::new();
    for point in deduped {
        simplified.push(point);
        while simplified.len() >= 3 {
            let len = simplified.len();
            let a = simplified[len - 3];
            let b = simplified[len - 2];
            let c = simplified[len - 1];
            let horizontal = (a.y - b.y).abs() < 0.5 && (b.y - c.y).abs() < 0.5;
            let vertical = (a.x - b.x).abs() < 0.5 && (b.x - c.x).abs() < 0.5;
            if horizontal || vertical {
                simplified.remove(len - 2);
            } else {
                break;
            }
        }
    }
    simplified
}

fn preview_wire_points(points: &[Pos2], cursor: Pos2, orthogonal: bool) -> Vec<Pos2> {
    let mut preview = points.to_vec();
    if orthogonal && let Some(&last) = preview.last() {
        let dx = (cursor.x - last.x).abs();
        let dy = (cursor.y - last.y).abs();
        if dx > 0.1 && dy > 0.1 {
            let corner = if dx >= dy {
                Pos2::new(cursor.x, last.y)
            } else {
                Pos2::new(last.x, cursor.y)
            };
            push_unique_point(&mut preview, corner);
        }
    }
    push_unique_point(&mut preview, cursor);
    preview
}

/// Replaces intermediate points with a minimal 1-bend orthogonal route.
fn tidy_wire_points(wire: &mut Wire) {
    if wire.points.len() < 2 {
        return;
    }
    let start = wire.points[0];
    let end = *wire.points.last().unwrap();
    let dx = (end.x - start.x).abs();
    let dy = (end.y - start.y).abs();
    let new_points = if dx < 0.5 || dy < 0.5 {
        // Already axis-aligned — straight line
        vec![start, end]
    } else {
        // One L-bend: pick whichever option has the shorter total length
        let corner_h = Pos2::new(end.x, start.y); // horizontal-first
        let corner_v = Pos2::new(start.x, end.y); // vertical-first
        let len_h = start.distance(corner_h) + corner_h.distance(end);
        let len_v = start.distance(corner_v) + corner_v.distance(end);
        let corner = if len_h <= len_v { corner_h } else { corner_v };
        vec![start, corner, end]
    };
    wire.points = simplify_wire(new_points);
}

fn wire_length(wire: &Wire) -> f32 {
    wire.points
        .windows(2)
        .map(|segment| segment[0].distance(segment[1]))
        .sum()
}

fn wire_midpoint(wire: &Wire) -> Pos2 {
    midpoint_of_polyline(&wire.points)
        .or_else(|| wire.points.first().copied())
        .unwrap_or(Pos2::ZERO)
}

fn analyze_circuit(components: &[Component], wires: &[Wire]) -> Simulation {
    let mut nodes = CircuitNodes::default();
    let mut graph: Vec<HashSet<usize>> = Vec::new();
    let mut wire_graph: Vec<HashSet<usize>> = Vec::new();
    let mut positive_nodes = Vec::new();
    let mut return_nodes = Vec::new();
    let mut component_edges = Vec::new();
    let mut powered_module_edges = Vec::new();
    let mut wire_edges = Vec::new();
    let mut component_warnings: HashMap<u64, String> = HashMap::new();

    for wire in wires {
        for segment in wire.points.windows(2) {
            let a = nodes.node_for(segment[0]);
            let b = nodes.node_for(segment[1]);
            connect(&mut graph, a, b);
            connect(&mut wire_graph, a, b);
            wire_edges.push((wire.id, a, b));
        }
    }
    for component in components {
        for pin in component_pin_defs(component) {
            nodes.node_for(pin.pos);
        }
    }
    connect_wire_contacts(&mut nodes, &mut graph, wires, components);
    connect_wire_contacts(&mut nodes, &mut wire_graph, wires, components);

    // Pass 1: sources/returns only. Component conductance is added after
    // wire-only reachability exists, so polarity checks can reject bad paths.
    for component in components {
        let pins = component_pin_defs(component);
        let pin_nodes: Vec<usize> = pins.iter().map(|pin| nodes.node_for(pin.pos)).collect();
        match component.kind {
            ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource => {
                for (pin, &node) in pins.iter().zip(&pin_nodes) {
                    match pin.role {
                        PinRole::Positive => positive_nodes.push(node),
                        PinRole::Ground => return_nodes.push(node),
                        _ => {}
                    }
                }
                if positive_nodes.is_empty() && return_nodes.is_empty() && pin_nodes.len() >= 2 {
                    return_nodes.push(pin_nodes[0]);
                    positive_nodes.push(pin_nodes[1]);
                }
            }
            ComponentKind::Ground => {
                for &node in &pin_nodes {
                    return_nodes.push(node);
                }
            }
            _ => {}
        }
    }

    // Wire-only reachability — used for polarity checking and short detection
    let wire_from_positive = reachable_nodes(&wire_graph, &positive_nodes);
    let wire_from_return = reachable_nodes(&wire_graph, &return_nodes);
    let (control_from_positive, control_from_return) = controlled_reachability_graph(
        components,
        &mut nodes,
        &graph,
        &wire_from_positive,
        &wire_from_return,
        &positive_nodes,
        &return_nodes,
    );

    // Pass 2: non-module conductor/load edges.
    for component in components {
        if matches!(
            component.kind,
            ComponentKind::VSource
                | ComponentKind::Battery
                | ComponentKind::ISource
                | ComponentKind::Ground
        ) || component_is_powered_module(component)
        {
            continue;
        }

        let conductance = component_conductance(component);
        if conductance == Conductance::Open {
            continue;
        }

        let pins = component_pin_defs(component);
        let pin_nodes: Vec<usize> = pins.iter().map(|pin| nodes.node_for(pin.pos)).collect();
        if pin_nodes.len() < 2 {
            continue;
        }

        if component_is_controlled_switch(component.kind)
            && !controlled_switch_is_enabled(
                component.kind,
                &pins,
                &pin_nodes,
                &control_from_positive,
                &control_from_return,
            )
        {
            let has_control_wire = pins.iter().zip(&pin_nodes).any(|(pin, &node)| {
                pin.role == PinRole::Control
                    && graph
                        .get(node)
                        .is_some_and(|neighbors| !neighbors.is_empty())
            });
            if has_control_wire {
                component_warnings.insert(
                    component.id,
                    "Control warning: transistor gate/base is not driven to an active level."
                        .to_string(),
                );
            } else {
                component_warnings.insert(
                    component.id,
                    "Control warning: transistor gate/base is open.".to_string(),
                );
            }
            continue;
        }

        if component_is_polarized_diode(component.kind)
            && diode_appears_reversed(&pins, &pin_nodes, &wire_from_positive, &wire_from_return)
        {
            component_warnings.insert(
                component.id,
                "Polarity warning: anode appears on return and cathode on source +.".to_string(),
            );
            continue;
        }

        let Some((a, b)) = conductive_terminal_nodes(component.kind, &pins, &pin_nodes) else {
            continue;
        };
        connect(&mut graph, a, b);
        component_edges.push((component.id, a, b, conductance == Conductance::Load));
        if component.kind == ComponentKind::Relay {
            let relay_positive_reach = reachable_nodes(&graph, &positive_nodes);
            let relay_return_reach = reachable_nodes(&graph, &return_nodes);
            if relay_coil_is_enabled(
                &pins,
                &pin_nodes,
                &relay_positive_reach,
                &relay_return_reach,
            ) && let Some((com, no)) = relay_contact_nodes(&pins, &pin_nodes)
            {
                connect(&mut graph, com, no);
                component_edges.push((component.id, com, no, false));
            }
        }
    }

    let external_graph = graph.clone();

    // Pass 3: powered modules — only connect if polarity is correct
    for component in components {
        if !component_is_powered_module(component) {
            continue;
        }
        let pins = component_pin_defs(component);
        let pin_nodes: Vec<usize> = pins.iter().map(|pin| nodes.node_for(pin.pos)).collect();

        let positives: Vec<usize> = pins
            .iter()
            .zip(&pin_nodes)
            .filter(|(pin, _)| pin.role == PinRole::Positive)
            .map(|(_, &node)| node)
            .collect();
        let grounds: Vec<usize> = pins
            .iter()
            .zip(&pin_nodes)
            .filter(|(pin, _)| pin.role == PinRole::Ground)
            .map(|(_, &node)| node)
            .collect();

        if positives.is_empty() || grounds.is_empty() {
            continue;
        }

        let vcc_on_positive = positives.iter().any(|&n| wire_from_positive.contains(&n));
        let gnd_on_return = grounds.iter().any(|&n| wire_from_return.contains(&n));

        if vcc_on_positive && gnd_on_return {
            for &pos in &positives {
                for &gnd in &grounds {
                    connect(&mut graph, pos, gnd);
                    powered_module_edges.push((component.id, pos, gnd));
                }
            }
        } else if !wire_from_positive.is_empty() && !wire_from_return.is_empty() {
            let vcc_on_return = positives.iter().any(|&n| wire_from_return.contains(&n));
            let gnd_on_positive = grounds.iter().any(|&n| wire_from_positive.contains(&n));
            if vcc_on_return || gnd_on_positive {
                component_warnings.insert(
                    component.id,
                    "Polarity reversed: swap VCC and GND connections.".to_string(),
                );
            }
        }
    }

    // Pass 4: modules powered by already-powered modules (e.g., OLED via ESP32's 3V3 output).
    // Collect positive/ground pin nodes from modules powered above, then check remaining modules.
    {
        let mut ext_positive = positive_nodes.clone();
        let mut ext_return = return_nodes.clone();
        for (powered_id, _, _) in &powered_module_edges {
            if let Some(c) = components.iter().find(|c| c.id == *powered_id) {
                for pin in component_pin_defs(c) {
                    let Some(n) = nodes.find_existing(pin.pos) else {
                        continue;
                    };
                    match pin.role {
                        PinRole::Positive => ext_positive.push(n),
                        PinRole::Ground => ext_return.push(n),
                        _ => {}
                    }
                }
            }
        }
        let ext_wire_pos = reachable_nodes(&wire_graph, &ext_positive);
        let ext_wire_ret = reachable_nodes(&wire_graph, &ext_return);

        for component in components {
            if !component_is_powered_module(component) {
                continue;
            }
            if powered_module_edges
                .iter()
                .any(|(id, _, _)| *id == component.id)
            {
                continue;
            }

            let pins = component_pin_defs(component);
            let positives: Vec<usize> = pins
                .iter()
                .filter(|p| p.role == PinRole::Positive)
                .filter_map(|p| nodes.find_existing(p.pos))
                .collect();
            let grounds: Vec<usize> = pins
                .iter()
                .filter(|p| p.role == PinRole::Ground)
                .filter_map(|p| nodes.find_existing(p.pos))
                .collect();

            if positives.is_empty() || grounds.is_empty() {
                continue;
            }

            let vcc_ok = positives.iter().any(|&n| ext_wire_pos.contains(&n));
            let gnd_ok = grounds.iter().any(|&n| ext_wire_ret.contains(&n));

            if vcc_ok && gnd_ok {
                for &pos in &positives {
                    for &gnd in &grounds {
                        connect(&mut graph, pos, gnd);
                        powered_module_edges.push((component.id, pos, gnd));
                    }
                }
            } else if !ext_wire_pos.is_empty() && !ext_wire_ret.is_empty() {
                let vcc_on_ret = positives.iter().any(|&n| ext_wire_ret.contains(&n));
                let gnd_on_pos = grounds.iter().any(|&n| ext_wire_pos.contains(&n));
                if vcc_on_ret || gnd_on_pos {
                    component_warnings.entry(component.id).or_insert_with(|| {
                        "Polarity reversed: swap VCC and GND connections.".to_string()
                    });
                }
            }
        }
    }

    let mut details = validate_i2c_links(components, &nodes, &wire_graph);

    if positive_nodes.is_empty() || return_nodes.is_empty() {
        details.push("Add a source/battery and GND return to run live simulation.".to_string());
        let (dc, dc_error) = match mna::solve_dc_detailed(components, wires) {
            Ok(dc) => (Some(dc), None),
            Err(error) => (None, Some(error)),
        };
        return Simulation {
            status: SimulationStatus::Warning,
            summary: "No source or return".to_string(),
            explanation:
                "Add a voltage/current source and a return path to GND before DC current can flow."
                    .to_string(),
            details,
            component_warnings,
            dc,
            dc_error,
            ..Simulation::default()
        };
    }

    let from_positive = reachable_nodes(&graph, &positive_nodes);
    let from_return = reachable_nodes(&graph, &return_nodes);
    let loop_nodes: HashSet<usize> = from_positive.intersection(&from_return).copied().collect();
    if loop_nodes.is_empty() {
        details.push("No closed path between source + and return/GND.".to_string());
        let (dc, dc_error) = match mna::solve_dc_detailed(components, wires) {
            Ok(dc) => (Some(dc), None),
            Err(error) => (None, Some(error)),
        };
        return Simulation {
            status: SimulationStatus::Warning,
            summary: "Open circuit".to_string(),
            explanation:
                "Voltage can exist on open nodes, but current is 0 A until a closed path reaches the return/GND node."
                    .to_string(),
            details,
            component_warnings,
            dc,
            dc_error,
            ..Simulation::default()
        };
    }

    let energized_component_edges: Vec<(u64, bool)> = component_edges
        .into_iter()
        .filter(|(_, a, b, _)| loop_nodes.contains(a) && loop_nodes.contains(b))
        .map(|(id, _, _, is_load)| (id, is_load))
        .collect();
    let energized_loads: HashSet<u64> = energized_component_edges
        .iter()
        .filter(|(_, is_load)| *is_load)
        .map(|(id, _)| *id)
        .chain(
            powered_module_edges
                .iter()
                .filter(|(_, a, b)| loop_nodes.contains(a) && loop_nodes.contains(b))
                .map(|(id, _, _)| *id),
        )
        .collect();

    let mut energized_components: HashSet<u64> = energized_component_edges
        .into_iter()
        .map(|(id, _)| id)
        .chain(
            powered_module_edges
                .into_iter()
                .filter(|(_, a, b)| loop_nodes.contains(a) && loop_nodes.contains(b))
                .map(|(id, _, _)| id),
        )
        .chain(
            components
                .iter()
                .filter(|component| {
                    matches!(
                        component.kind,
                        ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource
                    ) && component_pin_defs(component)
                        .iter()
                        .map(|pin| nodes.find_existing(pin.pos))
                        .all(|node| node.is_some_and(|node| loop_nodes.contains(&node)))
                })
                .map(|component| component.id),
        )
        .collect();

    let mut energized_wires: HashSet<u64> = wire_edges
        .into_iter()
        .filter(|(_, a, b)| loop_nodes.contains(a) && loop_nodes.contains(b))
        .map(|(id, _, _)| id)
        .collect();

    // Wire-only short detection reuses wire_from_positive/return already computed
    let direct_wire_short = wire_from_positive
        .intersection(&wire_from_return)
        .next()
        .is_some();
    let hard_direct_short = explicit_source_to_ground_wire_short(components, wires);
    // Short if + reaches return via bare wires, OR if the closed loop has no resistive/module load.
    let mut shorted = hard_direct_short || (direct_wire_short && energized_loads.is_empty());

    prune_uncontrolled_digital_output_paths(
        components,
        wires,
        &nodes,
        &external_graph,
        &return_nodes,
        &mut energized_components,
        &mut energized_wires,
    );
    mark_powered_digital_output_paths(
        components,
        wires,
        &nodes,
        &external_graph,
        &wire_graph,
        &return_nodes,
        &mut energized_components,
        &mut energized_wires,
    );

    // OLED / Sensor: always require I2C (SDA+SCL) to be wired to a controller.
    // The guard was previously `if !ctrl_sda.is_empty() || !ctrl_scl.is_empty()`,
    // but that skipped the check entirely when the controller's I2C pins had no wires,
    // letting OLED appear energized with wrong or missing connections.
    let (ctrl_sda, ctrl_scl) = collect_controller_i2c_nodes(components, &nodes);
    for component in components {
        if !matches!(component.kind, ComponentKind::Oled | ComponentKind::Sensor) {
            continue;
        }
        if !energized_components.contains(&component.id) {
            continue;
        }
        let mut sda_ok = false;
        let mut scl_ok = false;
        for pin in component_pin_defs(component) {
            let Some(node) = nodes.find_existing(pin.pos) else {
                continue;
            };
            let label = pin.label.to_lowercase();
            if label.contains("sda") {
                sda_ok = ctrl_sda
                    .iter()
                    .any(|&s| nodes_connected(&wire_graph, node, s));
            }
            if label.contains("scl") {
                scl_ok = ctrl_scl
                    .iter()
                    .any(|&s| nodes_connected(&wire_graph, node, s));
            }
        }
        if !sda_ok || !scl_ok {
            energized_components.remove(&component.id);
            component_warnings.entry(component.id).or_insert_with(|| {
                let msg = match (sda_ok, scl_ok) {
                    (false, false) => "SDA and SCL not connected — wire to an I2C controller.",
                    (false, true) => "SDA not connected — wire to controller SDA pin.",
                    (true, false) => "SCL not connected — wire to controller SCL pin.",
                    _ => unreachable!(),
                };
                msg.to_string()
            });
        }
    }

    if !hard_direct_short && has_energized_load_component(components, &energized_components) {
        shorted = false;
    }

    let voltage = estimate_loop_voltage(components, &nodes, &loop_nodes);
    let resistance = estimate_loop_resistance(components, &energized_loads);
    let current = match (voltage, resistance) {
        (Some(v), Some(r)) if r > 0.0 && !shorted => Some(v / r),
        _ => None,
    };

    if shorted {
        details.push("Source + reaches return/GND without a resistive load.".to_string());
    } else {
        details.push(format!("{} energized load(s).", energized_loads.len()));
    }

    let (dc, dc_error) = match mna::solve_dc_detailed(components, wires) {
        Ok(dc) => (Some(dc), None),
        Err(error) => {
            details.push(format!(
                "DC solver: {error}. {}",
                error.beginner_explanation()
            ));
            (None, Some(error))
        }
    };
    if let Some(dc) = &dc {
        if dc.max_kcl_residual > 1e-8 {
            details.push(format!(
                "KCL diagnostic: maximum residual {}.",
                mna::format_current(dc.max_kcl_residual)
            ));
        }
        if dc.nonlinear_iterations > 0 {
            if dc.nonlinear_converged {
                details.push(format!(
                    "Nonlinear devices: piecewise model converged in {} iteration(s).",
                    dc.nonlinear_iterations
                ));
            } else {
                details.push(
                    "Nonlinear devices: piecewise model reached the iteration limit; treat currents as approximate.".to_string(),
                );
            }
        }
    }
    apply_engineering_checks(
        components,
        dc.as_ref(),
        shorted,
        &mut component_warnings,
        &mut details,
    );
    prune_unphysical_energized_components(
        components,
        dc.as_ref(),
        shorted,
        &mut energized_components,
    );
    prune_unphysical_energized_wires(dc.as_ref(), &mut energized_wires);

    // Append per-component warnings to details after engineering checks.
    for component in components {
        if let Some(warning) = component_warnings.get(&component.id) {
            details.push(format!("{}: {}", component.label, warning));
        }
    }

    Simulation {
        status: simulation_status_from_solver(shorted, dc_error.as_ref()),
        closed: true,
        shorted,
        energized_components,
        energized_wires,
        summary: if shorted {
            "Short circuit".to_string()
        } else {
            "Current flowing".to_string()
        },
        explanation: if shorted {
            "Source positive reaches return/GND without enough load resistance, so the circuit is unsafe and current arrows are suppressed.".to_string()
        } else if dc_error.is_some() {
            "Connectivity shows a closed path, but the DC solver could not fully trust the numeric operating point. Check floating nodes or ideal-source conflicts.".to_string()
        } else {
            "A closed path exists from the source through at least one load and back to return/GND, so DC current can flow.".to_string()
        },
        details,
        voltage,
        resistance,
        current,
        component_warnings,
        dc,
        dc_error,
        ac: None,        // populated in current_simulation()
        transient: None, // populated in current_simulation()
        erc: Vec::new(), // populated after construction via run_erc()
    }
}

#[derive(Default)]
struct CircuitNodes {
    positions: Vec<Pos2>,
}

impl CircuitNodes {
    fn node_for(&mut self, pos: Pos2) -> usize {
        if let Some(index) = self.find_existing(pos) {
            return index;
        }
        self.positions.push(pos);
        self.positions.len() - 1
    }

    fn find_existing(&self, pos: Pos2) -> Option<usize> {
        self.positions
            .iter()
            .position(|existing| existing.distance(pos) <= 1.0)
    }
}

fn connect(graph: &mut Vec<HashSet<usize>>, a: usize, b: usize) {
    let needed = a.max(b) + 1;
    if graph.len() < needed {
        graph.resize_with(needed, HashSet::new);
    }
    graph[a].insert(b);
    graph[b].insert(a);
}

fn connect_wire_contacts(
    nodes: &mut CircuitNodes,
    graph: &mut Vec<HashSet<usize>>,
    wires: &[Wire],
    components: &[Component],
) {
    for contact in wire_contact_points(components, wires) {
        let contact_node = nodes.node_for(contact);
        for wire in wires {
            for segment in wire.points.windows(2) {
                if point_touches_wire_segment(contact, segment[0], segment[1]) {
                    let a = nodes.node_for(segment[0]);
                    let b = nodes.node_for(segment[1]);
                    connect(graph, contact_node, a);
                    connect(graph, contact_node, b);
                }
            }
        }
    }
}

fn wire_contact_points(components: &[Component], wires: &[Wire]) -> Vec<Pos2> {
    let mut points = Vec::new();
    for wire in wires {
        points.extend(wire.points.iter().copied());
    }
    for component in components {
        points.extend(component_pin_defs(component).into_iter().map(|pin| pin.pos));
    }
    points
}

fn reachable_nodes(graph: &[HashSet<usize>], starts: &[usize]) -> HashSet<usize> {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::new();
    for &start in starts {
        if seen.insert(start) {
            queue.push_back(start);
        }
    }

    while let Some(node) = queue.pop_front() {
        if let Some(neighbors) = graph.get(node) {
            for &neighbor in neighbors {
                if seen.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
    }
    seen
}

fn nodes_connected(graph: &[HashSet<usize>], a: usize, b: usize) -> bool {
    reachable_nodes(graph, &[a]).contains(&b)
}

fn prune_unphysical_energized_wires(
    dc: Option<&mna::DcResult>,
    energized_wires: &mut HashSet<u64>,
) {
    let Some(dc) = dc else {
        return;
    };

    energized_wires.retain(|wire_id| {
        let Some(&current) = dc.wire_current.get(wire_id) else {
            return true;
        };
        dc.wire_current_known.contains(wire_id) && current.abs() > 1.0e-12
    });
}

fn module_pin_can_drive_digital_load(pin: &CircuitPin) -> bool {
    matches!(pin.role, PinRole::Digital | PinRole::Output)
        || (pin.role == PinRole::I2c && pin.label.to_ascii_uppercase().contains("GPIO"))
}

fn prune_uncontrolled_digital_output_paths(
    components: &[Component],
    wires: &[Wire],
    nodes: &CircuitNodes,
    external_graph: &[HashSet<usize>],
    return_nodes: &[usize],
    energized_components: &mut HashSet<u64>,
    energized_wires: &mut HashSet<u64>,
) {
    if return_nodes.is_empty() {
        return;
    }

    for module in components
        .iter()
        .filter(|component| component_is_powered_module(component))
    {
        for pin in component_pin_defs(module)
            .iter()
            .filter(|pin| module_pin_can_drive_digital_load(pin))
        {
            let Some(digital_node) = nodes.find_existing(pin.pos) else {
                continue;
            };
            let path_nodes = reachable_nodes(external_graph, &[digital_node]);
            if !return_nodes
                .iter()
                .any(|return_node| path_nodes.contains(return_node))
            {
                continue;
            }

            for component in components {
                if component.id == module.id
                    || matches!(
                        component.kind,
                        ComponentKind::VSource
                            | ComponentKind::Battery
                            | ComponentKind::ISource
                            | ComponentKind::Ground
                    )
                    || component_is_powered_module(component)
                {
                    continue;
                }
                let comp_nodes = component_pin_defs(component)
                    .iter()
                    .filter_map(|pin| nodes.find_existing(pin.pos))
                    .collect::<Vec<_>>();
                if comp_nodes.len() >= 2 && comp_nodes.iter().all(|node| path_nodes.contains(node))
                {
                    energized_components.remove(&component.id);
                }
            }

            for wire in wires {
                if wire.points.windows(2).any(|segment| {
                    let a = nodes.find_existing(segment[0]);
                    let b = nodes.find_existing(segment[1]);
                    a.is_some_and(|node| path_nodes.contains(&node))
                        && b.is_some_and(|node| path_nodes.contains(&node))
                }) {
                    energized_wires.remove(&wire.id);
                }
            }
        }
    }
}

fn mark_powered_digital_output_paths(
    components: &[Component],
    wires: &[Wire],
    nodes: &CircuitNodes,
    external_graph: &[HashSet<usize>],
    wire_graph: &[HashSet<usize>],
    return_nodes: &[usize],
    energized_components: &mut HashSet<u64>,
    energized_wires: &mut HashSet<u64>,
) {
    if return_nodes.is_empty() {
        return;
    }

    let powered_module_ids = components
        .iter()
        .filter(|component| {
            component_is_powered_module(component) && energized_components.contains(&component.id)
        })
        .map(|component| component.id)
        .collect::<Vec<_>>();

    for module_id in powered_module_ids {
        let Some(module) = components
            .iter()
            .find(|component| component.id == module_id)
        else {
            continue;
        };
        let pins = component_pin_defs(module);
        let digital_pins = pins
            .iter()
            .filter(|pin| module_pin_can_drive_digital_load(pin))
            .filter_map(|pin| nodes.find_existing(pin.pos))
            .collect::<Vec<_>>();

        if digital_pins.is_empty()
            || !module_has_closed_digital_input(
                components,
                nodes,
                wire_graph,
                return_nodes,
                &digital_pins,
            )
        {
            continue;
        }

        for output_node in &digital_pins {
            let path_nodes = reachable_nodes(external_graph, &[*output_node]);
            if !return_nodes
                .iter()
                .any(|return_node| path_nodes.contains(return_node))
            {
                continue;
            }

            for component in components {
                if component.id == module.id
                    || matches!(
                        component.kind,
                        ComponentKind::VSource
                            | ComponentKind::Battery
                            | ComponentKind::ISource
                            | ComponentKind::Ground
                    )
                    || component_is_powered_module(component)
                {
                    continue;
                }
                let conductance = component_conductance(component);
                if conductance == Conductance::Open {
                    continue;
                }
                let comp_pins = component_pin_defs(component);
                let comp_nodes = comp_pins
                    .iter()
                    .filter_map(|pin| nodes.find_existing(pin.pos))
                    .collect::<Vec<_>>();
                if comp_nodes.len() >= 2 && comp_nodes.iter().all(|node| path_nodes.contains(node))
                {
                    energized_components.insert(component.id);
                }
            }

            for wire in wires {
                if wire.points.windows(2).any(|segment| {
                    let a = nodes.find_existing(segment[0]);
                    let b = nodes.find_existing(segment[1]);
                    a.is_some_and(|node| path_nodes.contains(&node))
                        && b.is_some_and(|node| path_nodes.contains(&node))
                }) {
                    energized_wires.insert(wire.id);
                }
            }
        }
    }
}

fn module_has_closed_digital_input(
    components: &[Component],
    nodes: &CircuitNodes,
    wire_graph: &[HashSet<usize>],
    return_nodes: &[usize],
    digital_pins: &[usize],
) -> bool {
    for switch in components.iter().filter(|component| {
        component_is_switch(component.kind) && component_conductance(component) != Conductance::Open
    }) {
        let switch_pins = component_pin_defs(switch);
        if switch_pins.len() < 2 {
            continue;
        }
        let Some(a) = nodes.find_existing(switch_pins[0].pos) else {
            continue;
        };
        let Some(b) = nodes.find_existing(switch_pins[1].pos) else {
            continue;
        };

        let a_at_return = return_nodes
            .iter()
            .any(|return_node| nodes_connected(wire_graph, a, *return_node));
        let b_at_return = return_nodes
            .iter()
            .any(|return_node| nodes_connected(wire_graph, b, *return_node));

        for digital_node in digital_pins {
            let digital_at_a = nodes_connected(wire_graph, *digital_node, a);
            let digital_at_b = nodes_connected(wire_graph, *digital_node, b);
            if (digital_at_a && b_at_return) || (digital_at_b && a_at_return) {
                return true;
            }
        }
    }

    false
}

fn controlled_reachability_graph(
    components: &[Component],
    nodes: &mut CircuitNodes,
    base_graph: &[HashSet<usize>],
    wire_from_positive: &HashSet<usize>,
    wire_from_return: &HashSet<usize>,
    positive_nodes: &[usize],
    return_nodes: &[usize],
) -> (HashSet<usize>, HashSet<usize>) {
    let mut graph = base_graph.to_vec();
    for component in components {
        if matches!(
            component.kind,
            ComponentKind::VSource
                | ComponentKind::Battery
                | ComponentKind::ISource
                | ComponentKind::Ground
        ) || component_is_powered_module(component)
            || component_is_controlled_switch(component.kind)
        {
            continue;
        }

        let conductance = component_conductance(component);
        if conductance == Conductance::Open {
            continue;
        }

        let pins = component_pin_defs(component);
        let pin_nodes = pins
            .iter()
            .map(|pin| nodes.node_for(pin.pos))
            .collect::<Vec<_>>();
        if component_is_polarized_diode(component.kind)
            && diode_appears_reversed(&pins, &pin_nodes, wire_from_positive, wire_from_return)
        {
            continue;
        }
        if let Some((a, b)) = conductive_terminal_nodes(component.kind, &pins, &pin_nodes) {
            connect(&mut graph, a, b);
        }
    }

    (
        reachable_nodes(&graph, positive_nodes),
        reachable_nodes(&graph, return_nodes),
    )
}

fn component_is_controlled_switch(kind: ComponentKind) -> bool {
    matches!(
        kind,
        ComponentKind::NpnTransistor
            | ComponentKind::PnpTransistor
            | ComponentKind::Nmosfet
            | ComponentKind::Pmosfet
    )
}

fn controlled_switch_is_enabled(
    kind: ComponentKind,
    pins: &[CircuitPin],
    pin_nodes: &[usize],
    control_from_positive: &HashSet<usize>,
    control_from_return: &HashSet<usize>,
) -> bool {
    let Some(control_node) = pins
        .iter()
        .zip(pin_nodes)
        .find(|(pin, _)| pin.role == PinRole::Control)
        .map(|(_, &node)| node)
    else {
        return false;
    };

    match kind {
        ComponentKind::NpnTransistor | ComponentKind::Nmosfet => {
            control_from_positive.contains(&control_node)
        }
        ComponentKind::PnpTransistor | ComponentKind::Pmosfet => {
            control_from_return.contains(&control_node)
        }
        _ => false,
    }
}

fn relay_coil_is_enabled(
    pins: &[CircuitPin],
    pin_nodes: &[usize],
    control_from_positive: &HashSet<usize>,
    control_from_return: &HashSet<usize>,
) -> bool {
    let by_label = |label: &str| {
        pins.iter()
            .zip(pin_nodes)
            .find(|(pin, _)| pin.label == label)
            .map(|(_, &node)| node)
    };
    let Some(coil_pos) = by_label("COIL+") else {
        return false;
    };
    let Some(coil_neg) = by_label("COIL-") else {
        return false;
    };
    control_from_positive.contains(&coil_pos) && control_from_return.contains(&coil_neg)
}

fn relay_contact_nodes(pins: &[CircuitPin], pin_nodes: &[usize]) -> Option<(usize, usize)> {
    let by_label = |label: &str| {
        pins.iter()
            .zip(pin_nodes)
            .find(|(pin, _)| pin.label == label)
            .map(|(_, &node)| node)
    };
    Some((by_label("COM")?, by_label("NO")?))
}

fn conductive_terminal_nodes(
    kind: ComponentKind,
    pins: &[CircuitPin],
    pin_nodes: &[usize],
) -> Option<(usize, usize)> {
    let by_label = |label: &str| {
        pins.iter()
            .zip(pin_nodes)
            .find(|(pin, _)| pin.label == label)
            .map(|(_, &node)| node)
    };

    match kind {
        ComponentKind::NpnTransistor | ComponentKind::PnpTransistor => {
            Some((by_label("C")?, by_label("E")?))
        }
        ComponentKind::Nmosfet | ComponentKind::Pmosfet => Some((by_label("D")?, by_label("S")?)),
        ComponentKind::Relay => Some((by_label("COIL+")?, by_label("COIL-")?)),
        _ => Some((*pin_nodes.first()?, *pin_nodes.get(1)?)),
    }
}

fn collect_controller_i2c_nodes(
    components: &[Component],
    nodes: &CircuitNodes,
) -> (Vec<usize>, Vec<usize>) {
    let mut sda = Vec::new();
    let mut scl = Vec::new();
    for component in components {
        if !matches!(
            component.kind,
            ComponentKind::Esp32
                | ComponentKind::Esp32S3
                | ComponentKind::Esp32C3
                | ComponentKind::ArduinoUno
                | ComponentKind::RaspberryPiPico
        ) {
            continue;
        }
        for pin in component_pin_defs(component) {
            if pin.role != PinRole::I2c {
                continue;
            }
            let Some(node) = nodes.find_existing(pin.pos) else {
                continue;
            };
            let label = pin.label.to_lowercase();
            if label.contains("sda") {
                sda.push(node);
            } else if label.contains("scl") {
                scl.push(node);
            }
        }
    }
    (sda, scl)
}

fn validate_i2c_links(
    components: &[Component],
    nodes: &CircuitNodes,
    wire_graph: &[HashSet<usize>],
) -> Vec<String> {
    let (ctrl_sda, ctrl_scl) = collect_controller_i2c_nodes(components, nodes);
    if ctrl_sda.is_empty() && ctrl_scl.is_empty() {
        return Vec::new();
    }

    let mut details = Vec::new();
    for component in components {
        if !matches!(component.kind, ComponentKind::Oled | ComponentKind::Sensor) {
            continue;
        }
        let mut sda_ok = false;
        let mut scl_ok = false;
        for pin in component_pin_defs(component) {
            let Some(node) = nodes.find_existing(pin.pos) else {
                continue;
            };
            let label = pin.label.to_lowercase();
            if label.contains("sda") {
                sda_ok = ctrl_sda
                    .iter()
                    .any(|&ctrl| nodes_connected(wire_graph, node, ctrl));
            }
            if label.contains("scl") {
                scl_ok = ctrl_scl
                    .iter()
                    .any(|&ctrl| nodes_connected(wire_graph, node, ctrl));
            }
        }
        if sda_ok && scl_ok {
            details.push(format!("{} I2C OK.", component.label));
        } else {
            details.push(format!(
                "{} I2C incomplete: SDA {}, SCL {}.",
                component.label,
                if sda_ok { "ok" } else { "missing" },
                if scl_ok { "ok" } else { "missing" }
            ));
        }
    }
    details
}

fn estimate_loop_voltage(
    components: &[Component],
    nodes: &CircuitNodes,
    loop_nodes: &HashSet<usize>,
) -> Option<f32> {
    components
        .iter()
        .filter(|component| {
            matches!(
                component.kind,
                ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource
            )
        })
        .filter(|component| {
            component_pin_defs(component)
                .iter()
                .filter_map(|pin| nodes.find_existing(pin.pos))
                .any(|node| loop_nodes.contains(&node))
        })
        .filter_map(|component| parse_metric_value(&component.value, "v"))
        .next()
}

fn estimate_loop_resistance(
    components: &[Component],
    energized_loads: &HashSet<u64>,
) -> Option<f32> {
    let resistance = components
        .iter()
        .filter(|component| energized_loads.contains(&component.id))
        .filter_map(|component| match component.kind {
            ComponentKind::Resistor => parse_metric_value(&component.value, "ohm"),
            ComponentKind::Potentiometer => {
                parse_metric_value(&component.value, "ohm").map(|r| r * 0.5)
            }
            ComponentKind::Led | ComponentKind::Diode | ComponentKind::ZenerDiode => Some(220.0),
            ComponentKind::Lamp => Some(60.0),
            ComponentKind::NpnTransistor | ComponentKind::PnpTransistor => Some(100.0),
            ComponentKind::Nmosfet | ComponentKind::Pmosfet => Some(50.0),
            ComponentKind::VoltageReg => Some(10.0),
            ComponentKind::Fuse => Some(1.0),
            ComponentKind::Relay => Some(100.0),
            _ => None,
        })
        .sum::<f32>();
    (resistance > 0.0).then_some(resistance)
}

fn explicit_source_to_ground_wire_short(components: &[Component], wires: &[Wire]) -> bool {
    let mut source_positive_pins = Vec::new();
    let mut return_pins = Vec::new();

    for component in components {
        for pin in component_pin_defs(component) {
            if matches!(
                component.kind,
                ComponentKind::Battery | ComponentKind::VSource | ComponentKind::ISource
            ) && pin.role == PinRole::Positive
            {
                source_positive_pins.push(pin.pos);
            }
            if component.kind == ComponentKind::Ground
                || (matches!(
                    component.kind,
                    ComponentKind::Battery | ComponentKind::VSource | ComponentKind::ISource
                ) && pin.role == PinRole::Ground)
            {
                return_pins.push(pin.pos);
            }
        }
    }

    wires.iter().any(|wire| {
        let touches_source = wire_touches_any_pin(wire, &source_positive_pins);
        let touches_return = wire_touches_any_pin(wire, &return_pins);
        touches_source && touches_return
    })
}

fn wire_touches_any_pin(wire: &Wire, pins: &[Pos2]) -> bool {
    pins.iter().any(|pin| {
        wire.points
            .first()
            .is_some_and(|point| point.distance(*pin) <= 5.0)
            || wire
                .points
                .last()
                .is_some_and(|point| point.distance(*pin) <= 5.0)
    })
}

fn has_energized_load_component(
    components: &[Component],
    energized_components: &HashSet<u64>,
) -> bool {
    components.iter().any(|component| {
        energized_components.contains(&component.id)
            && !matches!(
                component.kind,
                ComponentKind::Battery
                    | ComponentKind::VSource
                    | ComponentKind::ISource
                    | ComponentKind::Ground
                    | ComponentKind::TextNote
            )
            && (component_conductance(component) == Conductance::Load
                || component_is_powered_module(component))
    })
}

fn apply_engineering_checks(
    components: &[Component],
    dc: Option<&mna::DcResult>,
    shorted: bool,
    component_warnings: &mut HashMap<u64, String>,
    details: &mut Vec<String>,
) {
    if shorted {
        details.push("Engineering check: fault current path detected; loads are not treated as normally powered.".to_string());
    }

    let Some(dc) = dc else {
        if shorted {
            details
                .push("DC operating point is singular because the source is shorted.".to_string());
        }
        return;
    };

    let mut max_source_current = 0.0_f64;
    for component in components {
        let current = dc
            .branch_current
            .get(&component.id)
            .copied()
            .unwrap_or(0.0)
            .abs();
        let power = dc
            .component_power
            .get(&component.id)
            .copied()
            .unwrap_or(0.0)
            .abs();
        let voltage = dc
            .component_voltage
            .get(&component.id)
            .copied()
            .unwrap_or(0.0);

        if matches!(
            component.kind,
            ComponentKind::Battery | ComponentKind::VSource | ComponentKind::ISource
        ) {
            max_source_current = max_source_current.max(current);
        }

        if let Some(limit) = component_current_limit(component) {
            if current > limit {
                component_warnings.entry(component.id).or_insert_with(|| {
                    format!(
                        "Overcurrent risk: {} through {}, limit about {}.",
                        mna::format_current(current),
                        component.label,
                        mna::format_current(limit)
                    )
                });
            }
        }

        if let Some(limit) = component_power_limit(component) {
            if power > limit {
                component_warnings.entry(component.id).or_insert_with(|| {
                    format!(
                        "Overpower risk: {} in {}, limit about {}.",
                        mna::format_power(power),
                        component.label,
                        mna::format_power(limit)
                    )
                });
            }
        }

        if component.kind == ComponentKind::Led {
            let current_ma = current * 1000.0;
            if current > 0.0 {
                details.push(format!(
                    "{} LED current: {:.2} mA, Vf {:.2} V.",
                    component.label, current_ma, voltage
                ));
            }
        }
    }

    if max_source_current > 2.0 {
        details.push(format!(
            "Engineering check: source current {} is high for a beginner circuit.",
            mna::format_current(max_source_current)
        ));
    }
}

fn prune_unphysical_energized_components(
    components: &[Component],
    dc: Option<&mna::DcResult>,
    shorted: bool,
    energized_components: &mut HashSet<u64>,
) {
    if shorted {
        energized_components.retain(|id| {
            components
                .iter()
                .find(|component| component.id == *id)
                .is_some_and(|component| {
                    matches!(
                        component.kind,
                        ComponentKind::Battery
                            | ComponentKind::VSource
                            | ComponentKind::ISource
                            | ComponentKind::Ground
                    )
                })
        });
        return;
    }

    let Some(dc) = dc else {
        return;
    };

    energized_components.retain(|id| {
        let Some(component) = components.iter().find(|component| component.id == *id) else {
            return false;
        };
        if component_is_powered_module(component)
            || matches!(
                component.kind,
                ComponentKind::Battery
                    | ComponentKind::VSource
                    | ComponentKind::ISource
                    | ComponentKind::Ground
            )
        {
            return true;
        }
        if !component_has_dc_current_model(component.kind) {
            return true;
        }
        dc.branch_current
            .get(id)
            .is_some_and(|current| current.abs() > 1e-9)
    });
}

fn component_has_dc_current_model(kind: ComponentKind) -> bool {
    matches!(
        kind,
        ComponentKind::Resistor
            | ComponentKind::Potentiometer
            | ComponentKind::Thermistor
            | ComponentKind::Varistor
            | ComponentKind::Fuse
            | ComponentKind::Lamp
            | ComponentKind::Relay
            | ComponentKind::VoltageReg
            | ComponentKind::NpnTransistor
            | ComponentKind::PnpTransistor
            | ComponentKind::Nmosfet
            | ComponentKind::Pmosfet
            | ComponentKind::Diode
            | ComponentKind::Led
            | ComponentKind::ZenerDiode
            | ComponentKind::SchottkyDiode
            | ComponentKind::TvsDiode
            | ComponentKind::Phototransistor
            | ComponentKind::Timer555
            | ComponentKind::Voltmeter
            | ComponentKind::Ammeter
    )
}

fn component_current_limit(component: &Component) -> Option<f64> {
    match component.kind {
        ComponentKind::Led => Some(0.025),
        ComponentKind::Diode | ComponentKind::SchottkyDiode | ComponentKind::ZenerDiode => {
            Some(1.0)
        }
        ComponentKind::Fuse => parse_metric_value(&component.value, "a")
            .map(|value| value as f64)
            .filter(|value| *value > 0.0),
        ComponentKind::DcMotor => Some(2.0),
        ComponentKind::Relay => Some(0.2),
        ComponentKind::Ammeter => Some(10.0),
        _ => None,
    }
}

fn component_power_limit(component: &Component) -> Option<f64> {
    match component.kind {
        ComponentKind::Resistor => Some(0.25),
        ComponentKind::Potentiometer | ComponentKind::Thermistor => Some(0.125),
        ComponentKind::Led => Some(0.08),
        ComponentKind::Diode | ComponentKind::SchottkyDiode | ComponentKind::ZenerDiode => {
            Some(0.5)
        }
        ComponentKind::Fuse => Some(0.5),
        ComponentKind::Lamp => Some(40.0),
        ComponentKind::DcMotor => Some(12.0),
        ComponentKind::Relay => Some(1.0),
        ComponentKind::VoltageReg => Some(1.5),
        _ => None,
    }
}

fn format_resistance(ohms: f32) -> String {
    if ohms >= 1_000_000.0 {
        format!("{:.2} Mohm", ohms / 1_000_000.0)
    } else if ohms >= 1_000.0 {
        format!("{:.2} kohm", ohms / 1_000.0)
    } else {
        format!("{:.1} ohm", ohms)
    }
}

fn canvas_value_label(component: &Component) -> Option<String> {
    let raw = component.value.trim();
    if raw.is_empty() {
        return None;
    }
    let label = match component.kind {
        ComponentKind::Resistor => {
            if let Some(ohms) = parse_metric_value(raw, "ohm") {
                if ohms >= 1_000_000.0 {
                    let m = ohms / 1_000_000.0;
                    if m == m.floor() {
                        format!("{}MΩ", m as u32)
                    } else {
                        format!("{:.2}MΩ", m)
                    }
                } else if ohms >= 1_000.0 {
                    let k = ohms / 1_000.0;
                    if k == k.floor() {
                        format!("{}kΩ", k as u32)
                    } else {
                        format!("{:.1}kΩ", k)
                    }
                } else {
                    if ohms == ohms.floor() {
                        format!("{}Ω", ohms as u32)
                    } else {
                        format!("{:.1}Ω", ohms)
                    }
                }
            } else {
                raw.to_string()
            }
        }
        ComponentKind::Capacitor => {
            if let Some(f) = parse_metric_value(raw, "f") {
                if f >= 1e-3 {
                    format!("{:.1}mF", f * 1e3)
                } else if f >= 1e-6 {
                    format!("{:.0}μF", f * 1e6)
                } else if f >= 1e-9 {
                    format!("{:.0}nF", f * 1e9)
                } else {
                    format!("{:.0}pF", f * 1e12)
                }
            } else {
                raw.to_string()
            }
        }
        ComponentKind::Inductor => {
            if let Some(h) = parse_metric_value(raw, "h") {
                if h >= 1.0 {
                    format!("{:.1}H", h)
                } else if h >= 1e-3 {
                    format!("{:.1}mH", h * 1e3)
                } else {
                    format!("{:.0}μH", h * 1e6)
                }
            } else {
                raw.to_string()
            }
        }
        ComponentKind::VSource | ComponentKind::Battery => {
            if let Some(v) = parse_metric_value(raw, "v") {
                if v >= 1.0 {
                    format!("{:.1}V", v)
                } else {
                    format!("{:.0}mV", v * 1e3)
                }
            } else {
                raw.to_string()
            }
        }
        _ => return Some(raw.to_string()),
    };
    Some(label)
}

fn format_current(amps: f32) -> String {
    if amps >= 1.0 {
        format!("{amps:.2} A")
    } else if amps >= 0.001 {
        format!("{:.2} mA", amps * 1_000.0)
    } else {
        format!("{:.2} uA", amps * 1_000_000.0)
    }
}

fn component_conductance(component: &Component) -> Conductance {
    match component.kind {
        ComponentKind::Resistor
        | ComponentKind::Diode
        | ComponentKind::ZenerDiode
        | ComponentKind::Led
        | ComponentKind::Lamp
        | ComponentKind::Fuse => Conductance::Load,
        ComponentKind::Potentiometer => Conductance::Load,
        ComponentKind::NpnTransistor
        | ComponentKind::PnpTransistor
        | ComponentKind::Nmosfet
        | ComponentKind::Pmosfet => Conductance::Load,
        ComponentKind::VoltageReg => Conductance::Load,
        ComponentKind::LogicNot
        | ComponentKind::LogicAnd
        | ComponentKind::LogicOr
        | ComponentKind::LogicNand
        | ComponentKind::LogicNor
        | ComponentKind::LogicXor => Conductance::Open,
        ComponentKind::Inductor => Conductance::Conductor,
        ComponentKind::Switch | ComponentKind::PushButton | ComponentKind::SlideSwitch => {
            let value = component.value.to_lowercase();
            if value.contains("open") || value.contains("off") {
                Conductance::Open
            } else {
                Conductance::Conductor
            }
        }
        ComponentKind::DcMotor | ComponentKind::Relay => Conductance::Load,
        ComponentKind::Capacitor | ComponentKind::OpAmp | ComponentKind::Breadboard => {
            Conductance::Open
        }
        ComponentKind::Esp32
        | ComponentKind::Esp32S3
        | ComponentKind::Esp32C3
        | ComponentKind::ArduinoUno
        | ComponentKind::RaspberryPiPico
        | ComponentKind::Stm32BluePill
        | ComponentKind::Stm32Nucleo64
        | ComponentKind::Servo
        | ComponentKind::Oled
        | ComponentKind::Sensor => Conductance::Open,
        ComponentKind::Ground
        | ComponentKind::VSource
        | ComponentKind::ISource
        | ComponentKind::Battery => Conductance::Open,
        ComponentKind::Thermistor
        | ComponentKind::Varistor
        | ComponentKind::SchottkyDiode
        | ComponentKind::TvsDiode
        | ComponentKind::Phototransistor => Conductance::Load,
        ComponentKind::NetLabel
        | ComponentKind::Crystal
        | ComponentKind::Display7Seg
        | ComponentKind::VoltageRef
        | ComponentKind::MotorDriver
        | ComponentKind::Optocoupler
        | ComponentKind::GenericIc => Conductance::Open,
        ComponentKind::Timer555 => Conductance::Load,
        ComponentKind::Transformer => Conductance::Conductor,
        ComponentKind::Voltmeter => Conductance::Open,
        ComponentKind::Ammeter => Conductance::Conductor,
        ComponentKind::TextNote => Conductance::Open,
        ComponentKind::Buzzer => Conductance::Load,
        ComponentKind::Dht11
        | ComponentKind::Dht22
        | ComponentKind::HcSr04
        | ComponentKind::NeoPixel
        | ComponentKind::PirSensor => Conductance::Open,
    }
}

fn component_is_powered_module(component: &Component) -> bool {
    matches!(
        component.kind,
        ComponentKind::Esp32
            | ComponentKind::Esp32S3
            | ComponentKind::Esp32C3
            | ComponentKind::ArduinoUno
            | ComponentKind::RaspberryPiPico
            | ComponentKind::Servo
            | ComponentKind::Oled
            | ComponentKind::Sensor
    )
}

fn component_is_polarized_diode(kind: ComponentKind) -> bool {
    matches!(
        kind,
        ComponentKind::Diode | ComponentKind::Led | ComponentKind::ZenerDiode
    )
}

fn diode_appears_reversed(
    pins: &[CircuitPin],
    pin_nodes: &[usize],
    wire_from_positive: &HashSet<usize>,
    wire_from_return: &HashSet<usize>,
) -> bool {
    let Some((anode, cathode)) = diode_terminal_nodes(pins, pin_nodes) else {
        return false;
    };
    wire_from_return.contains(&anode) && wire_from_positive.contains(&cathode)
}

fn diode_terminal_nodes(pins: &[CircuitPin], pin_nodes: &[usize]) -> Option<(usize, usize)> {
    if pins.len() < 2 || pin_nodes.len() < 2 {
        return None;
    }

    let anode = pins
        .iter()
        .zip(pin_nodes)
        .find(|(pin, _)| pin.label == "A")
        .map(|(_, &node)| node)
        .unwrap_or(pin_nodes[0]);
    let cathode = pins
        .iter()
        .zip(pin_nodes)
        .find(|(pin, _)| pin.label == "K" || pin.label == "B")
        .map(|(_, &node)| node)
        .unwrap_or(pin_nodes[1]);
    Some((anode, cathode))
}

fn component_is_switch(kind: ComponentKind) -> bool {
    matches!(
        kind,
        ComponentKind::Switch | ComponentKind::PushButton | ComponentKind::SlideSwitch
    )
}

fn component_bounds(component: &Component) -> Rect {
    let size = component_size(component);
    let rot = ((component.rotation % 360) + 360) % 360;
    let eff = if rot == 90 || rot == 270 {
        Vec2::new(size.y, size.x)
    } else {
        size
    };
    Rect::from_center_size(component.pos, eff)
}

fn draw_minimap(
    painter: &egui::Painter,
    canvas: Rect,
    components: &[crate::model::Component],
    wires: &[Wire],
    view: CanvasView,
) {
    let mm_w = 140.0_f32;
    let mm_h = 90.0_f32;
    let margin = 10.0_f32;
    let mm_rect = Rect::from_min_size(
        Pos2::new(
            canvas.right() - mm_w - margin,
            canvas.bottom() - mm_h - margin,
        ),
        Vec2::new(mm_w, mm_h),
    );

    // Find world bounds
    let mut wmin = Pos2::new(f32::INFINITY, f32::INFINITY);
    let mut wmax = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for c in components {
        wmin.x = wmin.x.min(c.pos.x - 50.0);
        wmin.y = wmin.y.min(c.pos.y - 50.0);
        wmax.x = wmax.x.max(c.pos.x + 50.0);
        wmax.y = wmax.y.max(c.pos.y + 50.0);
    }
    for w in wires {
        for &p in &w.points {
            wmin.x = wmin.x.min(p.x);
            wmin.y = wmin.y.min(p.y);
            wmax.x = wmax.x.max(p.x);
            wmax.y = wmax.y.max(p.y);
        }
    }
    if !wmin.x.is_finite() {
        return;
    }

    let world_w = (wmax.x - wmin.x).max(100.0);
    let world_h = (wmax.y - wmin.y).max(100.0);
    let scale_x = mm_w / world_w;
    let scale_y = mm_h / world_h;
    let scale = scale_x.min(scale_y) * 0.9;

    let to_mm = |p: Pos2| -> Pos2 {
        let nx = (p.x - wmin.x) * scale;
        let ny = (p.y - wmin.y) * scale;
        Pos2::new(
            mm_rect.left() + nx + (mm_w - world_w * scale) * 0.5,
            mm_rect.top() + ny + (mm_h - world_h * scale) * 0.5,
        )
    };

    // Background
    painter.rect_filled(
        mm_rect,
        4.0,
        Color32::from_rgba_unmultiplied(10, 14, 20, 210),
    );
    painter.rect_stroke(
        mm_rect,
        4.0,
        Stroke::new(1.0, Color32::from_rgb(50, 65, 85)),
        egui::StrokeKind::Middle,
    );

    // Draw wires
    for wire in wires {
        for seg in wire.points.windows(2) {
            painter.line_segment(
                [to_mm(seg[0]), to_mm(seg[1])],
                Stroke::new(1.0, Color32::from_rgb(70, 130, 200)),
            );
        }
    }

    // Draw components as dots
    for comp in components {
        let p = to_mm(comp.pos);
        painter.circle_filled(p, 2.5, Color32::from_rgb(120, 200, 160));
    }

    // Viewport indicator
    let vp_tl = view.to_world(canvas.min);
    let vp_br = view.to_world(canvas.max);
    let vp_mm_tl = to_mm(vp_tl);
    let vp_mm_br = to_mm(vp_br);
    let vp_rect = Rect::from_two_pos(vp_mm_tl, vp_mm_br).intersect(mm_rect);
    if vp_rect.is_positive() {
        painter.rect_filled(
            vp_rect,
            2.0,
            Color32::from_rgba_unmultiplied(80, 160, 255, 30),
        );
        painter.rect_stroke(
            vp_rect,
            2.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(80, 180, 255, 140)),
            egui::StrokeKind::Middle,
        );
    }
}

fn osc_bar_rows(ui: &mut egui::Ui, rows: &[(String, f64)], max_val: f64, is_voltage: bool) {
    let bar_width = (ui.available_width() - 200.0).max(30.0);
    for (label, value) in rows {
        ui.horizontal(|ui| {
            ui.add_sized(
                Vec2::new(110.0, 16.0),
                egui::Label::new(
                    egui::RichText::new(label)
                        .monospace()
                        .size(10.5)
                        .color(Color32::from_rgb(200, 210, 220)),
                ),
            );
            let norm = ((value / max_val).abs() as f32).min(1.0);
            let fill = if is_voltage {
                if *value >= 0.0 {
                    Color32::from_rgb(60, 180, 120)
                } else {
                    Color32::from_rgb(220, 80, 80)
                }
            } else {
                Color32::from_rgb(80, 160, 255)
            };
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(bar_width, 13.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 2.0, Color32::from_rgba_unmultiplied(40, 50, 60, 200));
            let bar_rect = egui::Rect::from_min_size(rect.min, Vec2::new(bar_width * norm, 13.0));
            ui.painter().rect_filled(bar_rect, 2.0, fill);
            let val_str = if is_voltage {
                mna::format_voltage(*value)
            } else {
                mna::format_current(*value)
            };
            ui.add_sized(
                Vec2::new(70.0, 16.0),
                egui::Label::new(
                    egui::RichText::new(val_str)
                        .monospace()
                        .size(10.5)
                        .color(Color32::from_rgb(240, 230, 120)),
                ),
            );
        });
    }
}

fn draw_grid(painter: &egui::Painter, rect: Rect, grid: f32, view: CanvasView) {
    painter.rect_filled(rect, 0.0, Color32::from_rgb(11, 14, 19));

    let screen_grid = (grid * view.zoom).max(3.0);

    let origin = view.to_screen(Pos2::ZERO);
    let start_x = rect.left() + (origin.x - rect.left()).rem_euclid(screen_grid);
    let start_y = rect.top() + (origin.y - rect.top()).rem_euclid(screen_grid);

    // Minor dots
    if screen_grid >= 5.0 {
        let minor_col = Color32::from_rgb(36, 46, 58);
        let dot_r = if screen_grid > 14.0 { 1.3_f32 } else { 0.9 };
        let mut x = start_x;
        while x <= rect.right() + 1.0 {
            let mut y = start_y;
            while y <= rect.bottom() + 1.0 {
                painter.circle_filled(Pos2::new(x, y), dot_r, minor_col);
                y += screen_grid;
            }
            x += screen_grid;
        }
    }

    // Major dots (every 5 minor)
    let major_grid = screen_grid * 5.0;
    let start_mx = rect.left() + (origin.x - rect.left()).rem_euclid(major_grid);
    let start_my = rect.top() + (origin.y - rect.top()).rem_euclid(major_grid);
    let major_col = Color32::from_rgb(56, 72, 92);
    let mut x = start_mx;
    while x <= rect.right() + 1.0 {
        let mut y = start_my;
        while y <= rect.bottom() + 1.0 {
            painter.circle_filled(Pos2::new(x, y), 2.0, major_col);
            y += major_grid;
        }
        x += major_grid;
    }

    // Axis cross-hair at world origin (faint)
    let org = view.to_screen(Pos2::ZERO);
    if rect.contains(org) {
        let axis_col = Color32::from_rgba_unmultiplied(80, 120, 160, 40);
        painter.line_segment(
            [
                Pos2::new(org.x, rect.top()),
                Pos2::new(org.x, rect.bottom()),
            ],
            Stroke::new(1.0, axis_col),
        );
        painter.line_segment(
            [
                Pos2::new(rect.left(), org.y),
                Pos2::new(rect.right(), org.y),
            ],
            Stroke::new(1.0, axis_col),
        );
    }
}

/// Draw small voltage circles at wire junction/endpoint positions when DC is available.
fn draw_node_voltage_indicators(
    painter: &egui::Painter,
    wires: &[Wire],
    dc: &mna::DcResult,
    view: CanvasView,
    vmax: f64,
) {
    // Collect unique wire endpoints (junction points get drawn once)
    let mut seen: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    for wire in wires {
        for &pt in &wire.points {
            let key = (pt.x.round() as i32, pt.y.round() as i32);
            if !seen.insert(key) {
                continue; // already drawn
            }
            // Find the voltage at this wire point
            if let Some((&_wid, &_net)) = dc.wire_voltage.iter().next() {
                // Use wire_voltage for any wire that contains this point
                let v_opt = wires.iter().find_map(|w| {
                    if w.points
                        .iter()
                        .any(|p| (p.x.round() as i32, p.y.round() as i32) == key)
                    {
                        dc.wire_voltage.get(&w.id).copied()
                    } else {
                        None
                    }
                });
                if let Some(v) = v_opt {
                    let sp = view.to_screen(pt);
                    let col = mna::voltage_color(v, vmax);
                    // Only draw if it's actually a junction (multiple wires meet)
                    let junction_count = wires
                        .iter()
                        .filter(|w| {
                            w.points
                                .first()
                                .map(|p| (p.x.round() as i32, p.y.round() as i32) == key)
                                .unwrap_or(false)
                                || w.points
                                    .last()
                                    .map(|p| (p.x.round() as i32, p.y.round() as i32) == key)
                                    .unwrap_or(false)
                        })
                        .count();
                    if junction_count >= 2 {
                        painter.circle_filled(sp, 5.5, col);
                        painter.circle_stroke(
                            sp,
                            5.5,
                            Stroke::new(1.0, Color32::from_rgb(20, 24, 30)),
                        );
                        // Show voltage label at junctions (only when zoom is high enough)
                        if view.zoom >= 0.8 {
                            painter.text(
                                sp + Vec2::new(7.0, -7.0),
                                Align2::LEFT_BOTTOM,
                                mna::format_voltage(v),
                                egui::FontId::proportional(9.0),
                                Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 210),
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Draw a compact simulation summary box in the top-right corner of the canvas.
fn draw_sim_summary(
    painter: &egui::Painter,
    canvas: Rect,
    simulation: &crate::engine::simulation::Simulation,
) {
    let lines: Vec<String> = {
        let mut v = Vec::new();
        if simulation.shorted {
            v.push("⚡ SHORT CIRCUIT".to_string());
        } else if simulation.closed {
            v.push(format!(
                "Status: {}",
                simulation_status_label(simulation.status)
            ));
            if let Some(dc) = &simulation.dc {
                let total_p: f64 = dc.component_power.values().sum();
                if total_p > 1e-12 {
                    v.push(format!("P total: {}", mna::format_power(total_p)));
                }
                if let Some(&vmax) = dc
                    .net_voltages
                    .values()
                    .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                {
                    v.push(format!("V max: {}", mna::format_voltage(vmax)));
                }
                v.push(format!("Nodes: {}", dc.net_voltages.len()));
            }
            if v.is_empty() {
                v.push("Sim: closed".to_string());
            }
        } else {
            v.push("Sim: open circuit".to_string());
            if !simulation.explanation.is_empty() {
                v.push(simulation.explanation.clone());
            }
        }
        v
    };

    if lines.is_empty() {
        return;
    }

    let font = egui::FontId::proportional(10.5);
    let line_h = 14.0;
    let padding = Vec2::new(8.0, 5.0);
    let w = lines
        .iter()
        .map(|l| l.len() as f32 * 5.8)
        .fold(0.0_f32, f32::max)
        + padding.x * 2.0;
    let h = lines.len() as f32 * line_h + padding.y * 2.0;
    let top_right = canvas.right_top() + Vec2::new(-w - 8.0, 8.0);
    let bg = Rect::from_min_size(top_right, Vec2::new(w, h));

    painter.rect_filled(bg, 4.0, Color32::from_rgba_unmultiplied(15, 20, 28, 215));
    painter.rect_stroke(
        bg,
        4.0,
        Stroke::new(
            1.0,
            if simulation.shorted {
                Color32::from_rgb(220, 60, 60)
            } else if simulation.status == SimulationStatus::Ok {
                Color32::from_rgb(60, 180, 100)
            } else if simulation.status == SimulationStatus::Warning {
                Color32::from_rgb(220, 170, 70)
            } else {
                Color32::from_rgb(80, 90, 100)
            },
        ),
        StrokeKind::Outside,
    );

    for (i, line) in lines.iter().enumerate() {
        let pos = bg.min + Vec2::new(padding.x, padding.y + i as f32 * line_h);
        painter.text(
            pos,
            Align2::LEFT_TOP,
            line,
            font.clone(),
            if simulation.shorted {
                Color32::from_rgb(255, 100, 100)
            } else if simulation.status == SimulationStatus::Ok {
                Color32::from_rgb(130, 230, 160)
            } else if simulation.status == SimulationStatus::Warning {
                Color32::from_rgb(255, 210, 100)
            } else {
                Color32::from_rgb(140, 150, 165)
            },
        );
    }
}

fn draw_title_block(
    painter: &egui::Painter,
    canvas: Rect,
    components: &[Component],
    wires: &[Wire],
    simulation: &Simulation,
) {
    let erc_errors = simulation
        .erc
        .iter()
        .filter(|e| e.severity == ErcSeverity::Error)
        .count();
    let erc_warns = simulation
        .erc
        .iter()
        .filter(|e| e.severity == ErcSeverity::Warning)
        .count();

    let size = Vec2::new(272.0, 148.0);
    let rect = Rect::from_min_size(canvas.right_bottom() - size - Vec2::new(18.0, 18.0), size);
    painter.rect_filled(rect, 4.0, Color32::from_rgba_unmultiplied(14, 17, 22, 238));
    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, Color32::from_rgb(60, 70, 82)),
        StrokeKind::Outside,
    );

    let divider = |y: f32| {
        painter.line_segment(
            [
                Pos2::new(rect.left() + 10.0, rect.top() + y),
                Pos2::new(rect.right() - 10.0, rect.top() + y),
            ],
            Stroke::new(1.0, Color32::from_rgb(50, 58, 68)),
        )
    };

    let mono = |y: f32, txt: String, col: Color32| {
        painter.text(
            rect.left_top() + Vec2::new(12.0, y),
            Align2::LEFT_TOP,
            txt,
            egui::FontId::monospace(10.5),
            col,
        )
    };

    let dim = Color32::from_rgb(110, 120, 132);
    let bright = Color32::from_rgb(200, 210, 222);

    // Header
    painter.text(
        rect.left_top() + Vec2::new(12.0, 9.0),
        Align2::LEFT_TOP,
        "CLUSTER CIRCUIT",
        egui::FontId::proportional(12.0),
        Color32::from_rgb(220, 230, 240),
    );
    painter.text(
        rect.right_top() + Vec2::new(-12.0, 9.0),
        Align2::RIGHT_TOP,
        "v0.3",
        egui::FontId::monospace(10.0),
        dim,
    );
    divider(28.0);

    // Stats row
    mono(
        36.0,
        format!("Parts {:>3}  Wires {:>3}", components.len(), wires.len()),
        bright,
    );

    // Simulation status
    let status_color = if simulation.shorted {
        Color32::from_rgb(255, 95, 80)
    } else if simulation.closed {
        Color32::from_rgb(100, 220, 140)
    } else {
        Color32::from_rgb(130, 140, 155)
    };
    mono(
        53.0,
        format!(
            "Status  {} / {}",
            simulation.summary,
            simulation_status_label(simulation.status)
        ),
        status_color,
    );

    // DC values
    let dc_col = Color32::from_rgb(100, 200, 160);
    if let Some(dc) = &simulation.dc {
        let mut nets: Vec<f64> = dc.net_voltages.values().copied().collect();
        nets.sort_by(|a, b| b.partial_cmp(a).unwrap());
        nets.dedup();
        if let Some(&vmax) = nets.first() {
            mono(70.0, format!("Vmax  {}", mna::format_voltage(vmax)), dc_col);
        }
        if let Some(i) = simulation.current {
            mono(86.0, format!("Iloop {}", format_current(i)), dc_col);
        }
    } else {
        if let Some(v) = simulation.voltage {
            mono(70.0, format!("Vsrc  {:.2} V", v), dc_col);
        }
        if let Some(i) = simulation.current {
            mono(86.0, format!("Iloop {}", format_current(i)), dc_col);
        }
    }
    divider(102.0);

    // ERC summary
    let (erc_str, erc_col) = if erc_errors > 0 {
        (
            format!("ERC  ✗{erc_errors} error(s)  ⚠{erc_warns} warn(s)"),
            Color32::from_rgb(255, 100, 85),
        )
    } else if erc_warns > 0 {
        (
            format!("ERC  ⚠{erc_warns} warning(s)"),
            Color32::from_rgb(255, 200, 80),
        )
    } else if components.is_empty() {
        ("ERC  (no schematic)".to_string(), dim)
    } else {
        (
            "ERC  ✓ No violations".to_string(),
            Color32::from_rgb(100, 200, 140),
        )
    };
    mono(109.0, erc_str, erc_col);
    divider(127.0);
    mono(133.0, "Cluster Workbench  —  cluster.io".to_string(), dim);
}

fn draw_empty_canvas_hint(painter: &egui::Painter, canvas: Rect) {
    let rect = Rect::from_center_size(canvas.center(), Vec2::new(360.0, 120.0));
    painter.rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(20, 24, 30, 225));
    painter.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(58, 66, 76)),
        StrokeKind::Outside,
    );
    painter.text(
        rect.center_top() + Vec2::new(0.0, 24.0),
        Align2::CENTER_TOP,
        "Start a schematic",
        egui::FontId::proportional(18.0),
        Color32::from_rgb(228, 234, 240),
    );
    painter.text(
        rect.center() + Vec2::new(0.0, 6.0),
        Align2::CENTER_CENTER,
        "Pick a part on the left, then click the grid.",
        egui::FontId::proportional(12.0),
        Color32::from_rgb(156, 166, 176),
    );
    painter.text(
        rect.center_bottom() - Vec2::new(0.0, 22.0),
        Align2::CENTER_BOTTOM,
        "Use Wire to connect pins. Enter finishes a wire.",
        egui::FontId::proportional(12.0),
        Color32::from_rgb(156, 166, 176),
    );
}

/// Returns grid-rounded positions of all component pins that have a snapped
/// wire endpoint/control point on the pin. A wire merely passing nearby is not
/// a connection.
fn connected_pin_positions(components: &[Component], wires: &[Wire]) -> Vec<(i32, i32)> {
    let mut connected = Vec::new();
    for component in components {
        for pin in component_pin_defs(component) {
            let key = (pin.pos.x.round() as i32, pin.pos.y.round() as i32);
            let is_conn = wires.iter().any(|w| {
                w.points
                    .windows(2)
                    .any(|segment| point_touches_wire_segment(pin.pos, segment[0], segment[1]))
            });
            if is_conn {
                connected.push(key);
            }
        }
    }
    connected
}

// ─────────────────────────────────────────────────────────────────────────────
//  ERC — Electrical Rules Check
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
fn run_erc(components: &[Component], wires: &[Wire], simulation: &Simulation) -> Vec<ErcViolation> {
    let netlist = build_circuit_netlist(components, wires);
    run_erc_with_netlist(components, wires, simulation, &netlist)
}

fn run_erc_with_netlist(
    components: &[Component],
    wires: &[Wire],
    simulation: &Simulation,
    netlist: &CircuitNetlist,
) -> Vec<ErcViolation> {
    let mut v: Vec<ErcViolation> = Vec::new();

    // 1. Unconnected pins: use netlist, not raw coordinates.
    for comp in components {
        // Skip purely decorative / reference components
        if matches!(
            comp.kind,
            ComponentKind::NetLabel | ComponentKind::Breadboard
        ) {
            continue;
        }
        for pin in netlist
            .pins
            .iter()
            .filter(|pin| pin.component_id == comp.id)
        {
            if !pin.connected_by_wire {
                let sev = if matches!(
                    pin.electrical_type,
                    ElectricalType::PowerIn | ElectricalType::Ground
                ) {
                    ErcSeverity::Error
                } else {
                    ErcSeverity::Warning
                };
                v.push(ErcViolation {
                    rule: ErcRule::FloatingConnectivity,
                    severity: sev,
                    component_id: Some(comp.id),
                    wire_id: None,
                    message: format!("{}: pin \"{}\" unconnected", comp.label, pin.pin_name),
                });
            }
        }
    }

    // 2. No ground reference
    let has_ground = components.iter().any(|c| c.kind == ComponentKind::Ground)
        || components.iter().any(|c| {
            matches!(
                c.kind,
                ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource
            ) && component_pin_defs(c)
                .iter()
                .any(|p| p.role == PinRole::Ground)
        });
    if !has_ground && !components.is_empty() {
        v.push(ErcViolation {
            rule: ErcRule::MissingGround,
            severity: ErcSeverity::Error,
            component_id: None,
            wire_id: None,
            message: "No ground (GND) reference in schematic.".to_string(),
        });
    }

    // 3. No voltage/current source
    let has_source = components.iter().any(|c| {
        matches!(
            c.kind,
            ComponentKind::VSource | ComponentKind::Battery | ComponentKind::ISource
        )
    });
    if !has_source && !components.is_empty() {
        v.push(ErcViolation {
            rule: ErcRule::General,
            severity: ErcSeverity::Warning,
            component_id: None,
            wire_id: None,
            message: "No voltage or current source in schematic.".to_string(),
        });
    }

    // 4. Net-level power conflicts and short circuit
    let net_report = analyze_wire_nets_for_erc(components, wires);
    for conflict in &net_report.power_conflicts {
        v.push(ErcViolation {
            rule: ErcRule::PowerRailConflict,
            severity: ErcSeverity::Error,
            component_id: None,
            wire_id: conflict.wire_id,
            message: conflict.message.clone(),
        });
    }
    if simulation.shorted {
        let already_reported = !net_report.power_conflicts.is_empty();
        if !already_reported {
            v.push(ErcViolation {
                rule: ErcRule::PowerShort,
                severity: ErcSeverity::Error,
                component_id: None,
                wire_id: net_report.first_short_wire,
                message: "Short circuit detected: source + reaches GND without a load.".to_string(),
            });
        }
    }

    for warning in validate_beginner_rules(&netlist) {
        v.push(warning);
    }

    // 5. Component polarity warnings from simulation
    for (id, warn) in &simulation.component_warnings {
        if let Some(comp) = components.iter().find(|c| c.id == *id) {
            v.push(ErcViolation {
                rule: ErcRule::General,
                severity: ErcSeverity::Error,
                component_id: Some(*id),
                wire_id: None,
                message: format!("{}: {}", comp.label, warn),
            });
        }
    }

    // 6. Zero-value resistors
    for comp in components {
        if comp.kind == ComponentKind::Resistor {
            if let Some(r) = parse_metric_value(&comp.value, "ohm") {
                if r <= 0.0 {
                    v.push(ErcViolation {
                        rule: ErcRule::MissingValue,
                        severity: ErcSeverity::Warning,
                        component_id: Some(comp.id),
                        wire_id: None,
                        message: format!(
                            "{}: zero or negative resistance value \"{}\"",
                            comp.label, comp.value
                        ),
                    });
                }
            } else {
                v.push(ErcViolation {
                    rule: ErcRule::MissingValue,
                    severity: ErcSeverity::Warning,
                    component_id: Some(comp.id),
                    wire_id: None,
                    message: format!(
                        "{}: cannot parse resistance value \"{}\"",
                        comp.label, comp.value
                    ),
                });
            }
        }
    }

    // 7. Duplicate labels
    let mut labels: HashMap<&str, Vec<u64>> = HashMap::new();
    for comp in components {
        labels.entry(comp.label.as_str()).or_default().push(comp.id);
    }
    for (label, ids) in &labels {
        if ids.len() > 1 {
            v.push(ErcViolation {
                rule: ErcRule::DuplicateReference,
                severity: ErcSeverity::Warning,
                component_id: Some(ids[0]),
                wire_id: None,
                message: format!("Duplicate label \"{label}\" on {} components.", ids.len()),
            });
        }
    }

    // 8. Broken wires
    for wire in wires {
        if wire.points.len() < 2 || wire_length(wire) <= 0.5 {
            v.push(ErcViolation {
                rule: ErcRule::FloatingConnectivity,
                severity: ErcSeverity::Warning,
                component_id: None,
                wire_id: Some(wire.id),
                message: format!("Wire {} has no usable length.", wire.id),
            });
        }
    }

    v
}

#[derive(Default)]
struct ErcNetReport {
    first_short_wire: Option<u64>,
    power_conflicts: Vec<ErcNetConflict>,
}

struct ErcNetConflict {
    wire_id: Option<u64>,
    message: String,
}

fn analyze_wire_nets_for_erc(components: &[Component], wires: &[Wire]) -> ErcNetReport {
    let mut nodes = CircuitNodes::default();
    let mut graph: Vec<HashSet<usize>> = Vec::new();
    let mut wire_nodes: HashMap<u64, Vec<usize>> = HashMap::new();

    for wire in wires {
        let mut used_nodes = Vec::new();
        for point in &wire.points {
            used_nodes.push(nodes.node_for(*point));
        }
        for segment in wire.points.windows(2) {
            let a = nodes.node_for(segment[0]);
            let b = nodes.node_for(segment[1]);
            connect(&mut graph, a, b);
            used_nodes.push(a);
            used_nodes.push(b);
        }
        used_nodes.sort_unstable();
        used_nodes.dedup();
        wire_nodes.insert(wire.id, used_nodes);
    }

    let mut positive_nodes = Vec::new();
    let mut ground_nodes = Vec::new();
    for component in components {
        for pin in component_pin_defs(component) {
            let node = nodes.node_for(pin.pos);
            match pin.role {
                PinRole::Positive => {
                    positive_nodes.push((node, component.label.clone(), pin.label))
                }
                PinRole::Ground => ground_nodes.push((node, component.label.clone(), pin.label)),
                _ => {}
            }
        }
        if component.kind == ComponentKind::Ground {
            for pin in component_pin_defs(component) {
                let node = nodes.node_for(pin.pos);
                ground_nodes.push((node, component.label.clone(), pin.label));
            }
        }
    }
    connect_wire_contacts(&mut nodes, &mut graph, wires, components);

    let positive_reach: Vec<HashSet<usize>> = positive_nodes
        .iter()
        .map(|(node, _, _)| reachable_nodes(&graph, &[*node]))
        .collect();
    let ground_reach: Vec<HashSet<usize>> = ground_nodes
        .iter()
        .map(|(node, _, _)| reachable_nodes(&graph, &[*node]))
        .collect();

    let mut report = ErcNetReport::default();
    let mut seen_conflicts = HashSet::new();
    for (pos_idx, pos_seen) in positive_reach.iter().enumerate() {
        for (gnd_idx, gnd_seen) in ground_reach.iter().enumerate() {
            if !pos_seen.contains(&ground_nodes[gnd_idx].0)
                && !gnd_seen.contains(&positive_nodes[pos_idx].0)
            {
                continue;
            }
            let wire_id = first_wire_touching_either_set(&wire_nodes, pos_seen, gnd_seen);
            report.first_short_wire = report.first_short_wire.or(wire_id);
            let key = (
                positive_nodes[pos_idx].1.clone(),
                positive_nodes[pos_idx].2,
                ground_nodes[gnd_idx].1.clone(),
                ground_nodes[gnd_idx].2,
                wire_id,
            );
            if seen_conflicts.insert(key) {
                report.power_conflicts.push(ErcNetConflict {
                    wire_id,
                    message: format!(
                        "Power net conflict: {} {} is tied to {} {}.",
                        positive_nodes[pos_idx].1,
                        positive_nodes[pos_idx].2,
                        ground_nodes[gnd_idx].1,
                        ground_nodes[gnd_idx].2
                    ),
                });
            }
        }
    }

    report
}

fn first_wire_touching_either_set(
    wire_nodes: &HashMap<u64, Vec<usize>>,
    a: &HashSet<usize>,
    b: &HashSet<usize>,
) -> Option<u64> {
    let mut ordered_wires = wire_nodes.iter().collect::<Vec<_>>();
    ordered_wires.sort_by_key(|&(&id, _)| id);
    ordered_wires
        .iter()
        .find(|(_, nodes)| {
            nodes.iter().any(|node| a.contains(node)) && nodes.iter().any(|node| b.contains(node))
        })
        .map(|&(&id, _)| id)
        .or_else(|| {
            ordered_wires
                .iter()
                .find(|(_, nodes)| {
                    nodes
                        .iter()
                        .any(|node| a.contains(node) || b.contains(node))
                })
                .map(|&(&id, _)| id)
        })
}

fn draw_component(
    painter: &egui::Painter,
    component: &Component,
    selected: bool,
    show_pins: bool,
    energized: bool,
    connected_pins: &[(i32, i32)],
    view: CanvasView,
    dc_voltage: Option<f64>,
    dc_current: Option<f64>,
    show_dc_overlay: bool,
) {
    let stroke = if selected {
        Stroke::new(2.2, Color32::from_rgb(90, 235, 170))
    } else if energized {
        Stroke::new(2.8, Color32::from_rgb(255, 185, 80))
    } else {
        Stroke::new(2.0, Color32::from_rgb(222, 226, 232))
    };
    let screen_center = view.to_screen(component.pos);
    let size = component_size(component) * view.zoom;
    let rect = Rect::from_center_size(screen_center, size);
    // Effective bounds (swapped for 90/270 rotation)
    let rot = ((component.rotation % 360) + 360) % 360;
    let bounds_size = if rot == 90 || rot == 270 {
        Vec2::new(size.y, size.x)
    } else {
        size
    };
    let bounds = Rect::from_center_size(screen_center, bounds_size);

    if selected {
        let sel_rect = bounds.expand(8.0);
        // Faint fill
        painter.rect_filled(
            sel_rect,
            4.0,
            Color32::from_rgba_unmultiplied(60, 230, 160, 10),
        );
        // Subtle outer rect
        painter.rect_stroke(
            sel_rect,
            4.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(80, 200, 140, 80)),
            StrokeKind::Outside,
        );
        // Corner L-brackets for crisp selection feel
        let col = Color32::from_rgb(70, 220, 150);
        let cs = Stroke::new(2.0, col);
        let cr = sel_rect.width().min(sel_rect.height()) * 0.25;
        let corners = [
            sel_rect.left_top(),
            sel_rect.right_top(),
            sel_rect.left_bottom(),
            sel_rect.right_bottom(),
        ];
        let dx = [Vec2::X, -Vec2::X, Vec2::X, -Vec2::X];
        let dy = [Vec2::Y, Vec2::Y, -Vec2::Y, -Vec2::Y];
        for (i, &c) in corners.iter().enumerate() {
            painter.line_segment([c, c + dx[i] * cr], cs);
            painter.line_segment([c, c + dy[i] * cr], cs);
        }
    }

    match component.kind {
        ComponentKind::Resistor => {
            let ohms = parse_metric_value(&component.value, "ohm").unwrap_or(1000.0) as f64;
            draw_resistor_with_bands(painter, rect, component.rotation, stroke, ohms);
        }
        ComponentKind::Capacitor => draw_capacitor(painter, rect, component.rotation, stroke),
        ComponentKind::Inductor => draw_inductor(painter, rect, component.rotation, stroke),
        ComponentKind::Diode => draw_diode(painter, rect, component.rotation, stroke, false),
        ComponentKind::ZenerDiode => draw_zener(painter, rect, component.rotation, stroke),
        ComponentKind::Led => {
            if energized {
                let (outer, inner) = led_glow_colors(&component.value);
                painter.circle_filled(screen_center, rect.width().max(rect.height()) * 0.7, outer);
                painter.circle_filled(screen_center, rect.width().max(rect.height()) * 0.4, inner);
            }
            draw_led(painter, rect, component.rotation, stroke);
        }
        ComponentKind::NpnTransistor => {
            draw_npn(painter, rect, component.rotation, stroke, energized)
        }
        ComponentKind::PnpTransistor => {
            draw_pnp(painter, rect, component.rotation, stroke, energized)
        }
        ComponentKind::Nmosfet => {
            draw_nmosfet(painter, rect, component.rotation, stroke, energized)
        }
        ComponentKind::Pmosfet => {
            draw_pmosfet(painter, rect, component.rotation, stroke, energized)
        }
        ComponentKind::Potentiometer => {
            draw_potentiometer(painter, rect, component.rotation, stroke)
        }
        ComponentKind::VoltageReg => {
            draw_voltage_reg(painter, rect, component.rotation, stroke, energized)
        }
        ComponentKind::Fuse => draw_fuse(painter, rect, component.rotation, stroke),
        ComponentKind::LogicNot => draw_logic_not(painter, rect, component.rotation, stroke),
        ComponentKind::LogicAnd => draw_logic_and(painter, rect, component.rotation, stroke, false),
        ComponentKind::LogicOr => draw_logic_or(painter, rect, component.rotation, stroke, false),
        ComponentKind::LogicNand => draw_logic_and(painter, rect, component.rotation, stroke, true),
        ComponentKind::LogicNor => draw_logic_or(painter, rect, component.rotation, stroke, true),
        ComponentKind::LogicXor => draw_logic_xor(painter, rect, component.rotation, stroke, false),
        ComponentKind::Switch | ComponentKind::SlideSwitch => {
            let closed = !component.value.to_lowercase().contains("open");
            draw_switch(painter, rect, component.rotation, stroke, closed)
        }
        ComponentKind::PushButton => {
            let closed = !component.value.to_lowercase().contains("open");
            draw_push_button(painter, rect, component.rotation, stroke, closed);
        }
        ComponentKind::Ground => draw_ground(painter, rect, component.rotation, stroke),
        ComponentKind::VSource => draw_vsource(painter, rect, component.rotation, stroke),
        ComponentKind::ISource => draw_isource(painter, rect, component.rotation, stroke),
        ComponentKind::Battery => draw_battery(painter, rect, component.rotation, stroke),
        ComponentKind::OpAmp => draw_opamp(painter, rect, component.rotation, stroke),
        ComponentKind::Lamp => draw_lamp(painter, rect, component.rotation, stroke),
        ComponentKind::Esp32 => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "ESP32",
            &[
                "3V3",
                "GND",
                "GPIO2",
                "GPIO4",
                "GPIO15",
                "GPIO23 MOSI",
                "GPIO22 SCL",
                "GPIO21 SDA",
                "GPIO19 MISO",
                "GPIO18 SCK",
                "GPIO5 SS",
                "TX0",
                "RX0",
                "GND",
            ],
            &[
                "VIN",
                "GND",
                "GPIO34 ADC",
                "GPIO35 ADC",
                "GPIO32",
                "GPIO33",
                "GPIO25 DAC",
                "GPIO26 DAC",
                "GPIO27",
                "GPIO14",
                "GPIO13",
                "GPIO12",
                "GPIO0",
                "EN",
            ],
        ),
        ComponentKind::Esp32S3 => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "ESP32-S3",
            &[
                "3V3",
                "GND",
                "GPIO1",
                "GPIO2 SDA",
                "GPIO3 SCL",
                "GPIO4",
                "TX0",
                "RX0",
            ],
            &[
                "VIN", "GND", "GPIO8", "GPIO9", "GPIO10", "GPIO11", "EN", "RST",
            ],
        ),
        ComponentKind::Esp32C3 => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "ESP32-C3",
            &["3V3", "GND", "GPIO0", "GPIO1 SDA", "GPIO2 SCL", "TX", "RX"],
            &["5V", "GND", "GPIO3", "GPIO4", "GPIO5", "EN", "BOOT"],
        ),
        ComponentKind::ArduinoUno => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "ARDUINO UNO",
            &[
                "VIN", "5V", "3V3", "GND", "A0", "A1", "A2", "A3", "A4 SDA", "A5 SCL",
            ],
            &[
                "D2", "D3 PWM", "D4", "D5 PWM", "D6 PWM", "D7", "D8", "D9 PWM", "D10", "D11 MOSI",
                "D12 MISO", "D13 SCK", "TX", "RX",
            ],
        ),
        ComponentKind::RaspberryPiPico => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "PI PICO",
            &[
                "VSYS", "3V3", "GND", "GP0 TX", "GP1 RX", "GP4 SDA", "GP5 SCL",
            ],
            &["VBUS", "GND", "GP14", "GP15", "GP16", "GP17", "RUN"],
        ),
        ComponentKind::Stm32BluePill => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "STM32 BLUE PILL",
            &[
                "3V3", "GND", "PA0 ADC", "PA1 ADC", "PA2 TX2", "PA3 RX2", "PA5 SCK", "PA6 MISO",
                "PA7 MOSI", "PB6 SCL", "PB7 SDA",
            ],
            &[
                "5V",
                "GND",
                "VBAT",
                "PA9 TX1",
                "PA10 RX1",
                "PA13 SWDIO",
                "PA14 SWCLK",
                "PB0 ADC",
                "PB1 ADC",
                "BOOT0",
                "NRST",
            ],
        ),
        ComponentKind::Stm32Nucleo64 => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "STM32 NUCLEO",
            &[
                "3V3",
                "5V",
                "GND",
                "A0 PA0 ADC",
                "A1 PA1 ADC",
                "D14 PB9 SDA",
                "D15 PB8 SCL",
                "D13 PA5 SCK",
                "D12 PA6 MISO",
                "D11 PA7 MOSI",
            ],
            &[
                "VIN",
                "GND",
                "D0 PA3 RX",
                "D1 PA2 TX",
                "D2 PA10",
                "D3 PB3",
                "D4 PB5",
                "D5 PB4",
                "D6 PB10",
                "NRST",
            ],
        ),
        ComponentKind::Breadboard => draw_breadboard(painter, rect, stroke),
        ComponentKind::Relay => draw_relay(painter, rect, component.rotation, stroke),
        ComponentKind::DcMotor => draw_dc_motor(painter, rect, component.rotation, stroke),
        ComponentKind::Servo => draw_servo(painter, rect, stroke, energized),
        ComponentKind::Oled => draw_oled(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
        ),
        ComponentKind::Sensor => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "SENSOR",
            &["GND", "VCC", "SCL"],
            &["SDA"],
        ),
        ComponentKind::NetLabel => draw_net_label(painter, component, rect, stroke, energized),
        ComponentKind::Timer555 => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "555",
            &["GND", "TR", "Q", "R"],
            &["VCC", "DIS", "THR", "CV"],
        ),
        ComponentKind::Crystal => draw_crystal(painter, rect, component.rotation, stroke),
        ComponentKind::Transformer => draw_transformer(painter, rect, component.rotation, stroke),
        ComponentKind::Display7Seg => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "7-SEG",
            &["COM", "A", "B", "C"],
            &["D", "E", "F", "G"],
        ),
        ComponentKind::Thermistor => draw_thermistor(painter, rect, component.rotation, stroke),
        ComponentKind::Varistor => draw_varistor(painter, rect, component.rotation, stroke),
        ComponentKind::SchottkyDiode => {
            draw_diode(painter, rect, component.rotation, stroke, false);
            // Schottky mark: small horizontal stroke at cathode
            let center = rect.center();
            let r_h = rect.height() * 0.22;
            let s = view.scale_f(1.0);
            let _ = (center, r_h, s);
        }
        ComponentKind::TvsDiode => draw_diode(painter, rect, component.rotation, stroke, true),
        ComponentKind::VoltageRef => {
            draw_ic_box(painter, rect, component.rotation, stroke, energized, "VREF")
        }
        ComponentKind::MotorDriver => draw_module(
            painter,
            component,
            rect,
            stroke,
            energized,
            component.rotation,
            "H-BRIDGE",
            &["VCC", "GND", "IN1", "IN2"],
            &["OUT1", "OUT2", "EN"],
        ),
        ComponentKind::Phototransistor => {
            draw_npn(painter, rect, component.rotation, stroke, energized)
        }
        ComponentKind::Optocoupler => {
            draw_ic_box(painter, rect, component.rotation, stroke, energized, "OPTO")
        }
        ComponentKind::GenericIc => {
            draw_ic_box(painter, rect, component.rotation, stroke, energized, "IC")
        }
        ComponentKind::Voltmeter => {
            draw_meter(painter, rect, component.rotation, stroke, "V", energized);
            if show_dc_overlay {
                if let Some(v) = dc_voltage {
                    let center = view.to_screen(component.pos);
                    let r = (rect.width().min(rect.height()) * 0.44 + 12.0) * view.zoom;
                    painter.text(
                        center + Vec2::new(0.0, r),
                        Align2::CENTER_TOP,
                        mna::format_voltage(v),
                        egui::FontId::proportional(10.5),
                        Color32::from_rgb(100, 240, 170),
                    );
                }
            }
        }
        ComponentKind::TextNote => {
            // Bordered text box with the note text (stored in `value`)
            let text_fill = Color32::from_rgba_unmultiplied(30, 36, 46, 220);
            let border = if selected {
                Stroke::new(1.5, Color32::from_rgb(90, 200, 140))
            } else {
                Stroke::new(1.0, Color32::from_rgb(90, 110, 140))
            };
            painter.rect_filled(rect, 4.0, text_fill);
            painter.rect_stroke(rect, 4.0, border, egui::StrokeKind::Outside);
            let font = egui::FontId::proportional(12.0 * view.zoom.sqrt());
            let line_h = 16.0 * view.zoom.sqrt();
            let lines = component.value.lines().collect::<Vec<_>>();
            let total_h = line_h * lines.len().max(1) as f32;
            let mut y = rect.center().y - total_h * 0.5 + line_h * 0.5;
            for line in lines {
                painter.text(
                    Pos2::new(rect.center().x, y),
                    Align2::CENTER_CENTER,
                    line,
                    font.clone(),
                    Color32::from_rgb(210, 220, 230),
                );
                y += line_h;
            }
        }
        ComponentKind::Ammeter => {
            draw_meter(painter, rect, component.rotation, stroke, "A", energized);
            if show_dc_overlay {
                if let Some(i) = dc_current {
                    let center = view.to_screen(component.pos);
                    let r = (rect.width().min(rect.height()) * 0.44 + 12.0) * view.zoom;
                    painter.text(
                        center + Vec2::new(0.0, r),
                        Align2::CENTER_TOP,
                        mna::format_current(i),
                        egui::FontId::proportional(10.5),
                        Color32::from_rgb(100, 210, 255),
                    );
                }
            }
        }
        ComponentKind::Dht11 => draw_sensor_module(
            painter,
            rect,
            stroke,
            energized,
            "DHT11",
            Color32::from_rgb(30, 100, 180),
        ),
        ComponentKind::Dht22 => draw_sensor_module(
            painter,
            rect,
            stroke,
            energized,
            "DHT22",
            Color32::from_rgb(20, 140, 80),
        ),
        ComponentKind::HcSr04 => draw_hcsr04(painter, rect, stroke, energized),
        ComponentKind::Buzzer => draw_buzzer(painter, rect, component.rotation, stroke, energized),
        ComponentKind::NeoPixel => draw_neopixel(painter, rect, stroke, energized),
        ComponentKind::PirSensor => draw_sensor_module(
            painter,
            rect,
            stroke,
            energized,
            "PIR",
            Color32::from_rgb(160, 80, 30),
        ),
    }

    if show_pins {
        for pin in component_pin_defs(component) {
            let spos = view.to_screen(pin.pos);
            let key = (pin.pos.x.round() as i32, pin.pos.y.round() as i32);
            let is_connected = connected_pins.contains(&key);
            if is_connected {
                painter.circle_filled(spos, 3.0, Color32::from_rgb(250, 205, 95));
                painter.circle_stroke(spos, 4.0, Stroke::new(1.0, Color32::from_rgb(40, 35, 20)));
            } else {
                // Unconnected pin: hollow circle with small cross
                painter.circle_stroke(spos, 4.5, Stroke::new(1.5, Color32::from_rgb(220, 80, 60)));
                let d = 3.0;
                painter.line_segment(
                    [spos - Vec2::new(d, 0.0), spos + Vec2::new(d, 0.0)],
                    Stroke::new(1.0, Color32::from_rgb(220, 80, 60)),
                );
                painter.line_segment(
                    [spos - Vec2::new(0.0, d), spos + Vec2::new(0.0, d)],
                    Stroke::new(1.0, Color32::from_rgb(220, 80, 60)),
                );
            }
            if should_draw_pin_label(component.kind, &pin) {
                let screen_pin = CircuitPin {
                    pos: spos,
                    label: pin.label,
                    role: pin.role,
                };
                draw_pin_label(painter, screen_center, &screen_pin);
            }
        }
    }

    painter.text(
        bounds.center_bottom() + Vec2::new(0.0, 6.0),
        Align2::CENTER_TOP,
        &component.label,
        egui::FontId::proportional(12.0),
        if energized {
            Color32::from_rgb(255, 210, 130)
        } else {
            Color32::from_rgb(225, 228, 232)
        },
    );
    if let Some(val_label) = canvas_value_label(component) {
        let vpos = bounds.center_top() - Vec2::new(0.0, 7.0);
        let font = egui::FontId::proportional(11.0);
        let text_w = val_label.len() as f32 * 5.8 + 6.0;
        let pill = Rect::from_center_size(vpos, Vec2::new(text_w, 14.0));
        painter.rect_filled(pill, 3.5, Color32::from_rgba_unmultiplied(12, 16, 24, 185));
        painter.text(
            vpos,
            Align2::CENTER_CENTER,
            &val_label,
            font,
            Color32::from_rgb(160, 200, 240),
        );
    }

    // ── DC Simulation overlay badge ──────────────────────────────────────
    if show_dc_overlay {
        let mut lines: Vec<String> = Vec::new();
        if let Some(v) = dc_voltage {
            if v.abs() > 1e-9 {
                lines.push(mna::format_voltage(v));
            }
        }
        if let Some(i) = dc_current {
            if i.abs() > 1e-12 {
                lines.push(mna::format_current(i));
            }
        }
        if !lines.is_empty() {
            let text = lines.join(" / ");
            let badge_pos = bounds.left_top() + Vec2::new(-4.0, -4.0);
            let font = egui::FontId::proportional(9.5);
            // Dark background pill
            let text_w = text.len() as f32 * 5.4 + 6.0;
            let bg =
                Rect::from_min_size(badge_pos - Vec2::new(text_w, 14.0), Vec2::new(text_w, 13.0));
            painter.rect_filled(bg, 3.0, Color32::from_rgba_unmultiplied(15, 18, 24, 210));
            painter.rect_stroke(
                bg,
                3.0,
                Stroke::new(
                    0.8,
                    if energized {
                        Color32::from_rgb(200, 140, 30)
                    } else {
                        Color32::from_rgb(60, 70, 85)
                    },
                ),
                StrokeKind::Outside,
            );
            painter.text(
                bg.center(),
                Align2::CENTER_CENTER,
                &text,
                font,
                if energized {
                    Color32::from_rgb(255, 220, 100)
                } else {
                    Color32::from_rgb(140, 200, 255)
                },
            );
        }
    }
}

/// Draw a single wire polyline.
///
/// `dc_current` **must** be `None` when the wire is at a T-junction or
/// multi-branch net — passing a value there would display a physically
/// incorrect net-wide current.  Callers are responsible for supplying
/// `Some(I)` only when `DcResult::wire_current_known` contains the wire ID.
///
/// Voltage colour is always shown when `dc_voltage` is `Some`.  Current
/// arrows and thickness scaling are suppressed when `dc_current` is `None`.
fn draw_wire(
    painter: &egui::Painter,
    wire: &Wire,
    selected: bool,
    energized: bool,
    fault_highlight: bool,
    show_flow: bool,
    flow_phase: f32,
    net_highlighted: bool,
    dc_voltage: Option<f64>,
    dc_current: Option<f64>,
    dc_vmax: f64,
    dc_current_max: f64,
    show_voltage_labels: bool,
    open_wire: bool,
    view: CanvasView,
) {
    // Wire thickness scales with branch current only when it is well-defined.
    // Never scale by a net-wide average — that would be physically misleading.
    let wire_current = dc_current
        .map(|current| {
            if dc_current_max > 1e-12 {
                (current.abs() / dc_current_max).clamp(0.0, 1.0)
            } else {
                0.0
            }
        })
        .unwrap_or(0.0);
    let base_w = if wire_current > 0.0 {
        2.8 + wire_current as f32 * 2.0
    } else if energized {
        2.8
    } else {
        2.0
    };

    let stroke = if selected {
        Stroke::new(3.5, Color32::from_rgb(90, 235, 170))
    } else if fault_highlight {
        Stroke::new(4.2, Color32::from_rgb(255, 72, 58))
    } else if open_wire {
        Stroke::new(2.2, Color32::from_rgb(225, 155, 65))
    } else if let Some(v) = dc_voltage {
        let col = mna::voltage_color(v, dc_vmax);
        Stroke::new(base_w, col)
    } else if energized {
        Stroke::new(base_w, Color32::from_rgb(255, 170, 55))
    } else if net_highlighted {
        Stroke::new(2.8, Color32::from_rgb(140, 210, 255))
    } else {
        Stroke::new(2.0, Color32::from_rgb(105, 178, 255))
    };

    let mut screen_points: Vec<Pos2> = wire.points.iter().map(|&p| view.to_screen(p)).collect();
    if dc_current.is_some_and(|current| current < 0.0) {
        screen_points.reverse();
    }
    for segment in screen_points.windows(2) {
        if open_wire && !selected {
            draw_dashed_segment(painter, segment[0], segment[1], stroke, 8.0, 5.0);
        } else {
            painter.line_segment([segment[0], segment[1]], stroke);
        }
    }

    if fault_highlight {
        draw_short_fault_markers(painter, &screen_points);
    }

    // Voltage/current label overlay
    if show_voltage_labels {
        if let Some(mid) = midpoint_of_polyline(&screen_points) {
            let mut labels = Vec::new();
            if let Some(v) = dc_voltage {
                labels.push(mna::format_voltage(v));
            }
            if let Some(current) = dc_current.filter(|current| current.abs() > 1e-12) {
                labels.push(mna::format_current(current.abs()));
            }
            if !labels.is_empty() {
                let label = labels.join(" / ");
                let col = dc_voltage
                    .map(|voltage| mna::voltage_color(voltage, dc_vmax))
                    .unwrap_or(Color32::from_rgb(100, 210, 255));
                // background pill
                let font = egui::FontId::proportional(10.0);
                let galley = painter.layout_no_wrap(label, font.clone(), col);
                let text_rect = Rect::from_center_size(
                    mid + Vec2::new(0.0, -12.0),
                    galley.size() + Vec2::new(6.0, 3.0),
                );
                painter.rect_filled(
                    text_rect,
                    3.0,
                    Color32::from_rgba_unmultiplied(20, 22, 28, 200),
                );
                painter.text(
                    mid + Vec2::new(0.0, -12.0),
                    Align2::CENTER_CENTER,
                    galley.text(),
                    font,
                    col,
                );
            }
        }
    }

    if show_flow {
        draw_flow_pulses(painter, &screen_points, flow_phase, stroke.width);
        draw_flow_markers(painter, &screen_points, flow_phase);
    }
}

fn draw_dashed_segment(
    painter: &egui::Painter,
    start: Pos2,
    end: Pos2,
    stroke: Stroke,
    dash: f32,
    gap: f32,
) {
    let length = start.distance(end);
    if length <= 0.1 {
        return;
    }
    let direction = (end - start) / length;
    let mut offset = 0.0;
    while offset < length {
        let dash_end = (offset + dash).min(length);
        painter.line_segment(
            [start + direction * offset, start + direction * dash_end],
            stroke,
        );
        offset += dash + gap;
    }
}

fn midpoint_of_polyline(pts: &[Pos2]) -> Option<Pos2> {
    if pts.is_empty() {
        return None;
    }
    if pts.len() == 1 {
        return Some(pts[0]);
    }
    let total = polyline_length(pts);
    point_on_polyline(pts, total * 0.5)
}

fn draw_flow_markers(painter: &egui::Painter, points: &[Pos2], flow_phase: f32) {
    let total = polyline_length(points);
    if total <= 1.0 {
        return;
    }

    let spacing = 42.0;
    let arrow_size = 8.5;
    let mut distance = flow_phase.rem_euclid(spacing);
    if total < spacing {
        distance = flow_phase.rem_euclid(total.max(1.0));
    }

    while distance < total {
        let Some(pos) = point_on_polyline(points, distance) else {
            distance += spacing;
            continue;
        };
        // Direction of the wire at this point (tangent)
        let look_ahead = point_on_polyline(points, (distance + 3.0).min(total - 0.1));
        let dir = match look_ahead {
            Some(next) if pos.distance(next) > 0.01 => (next - pos).normalized(),
            _ => Vec2::new(1.0, 0.0),
        };
        let perp = Vec2::new(-dir.y, dir.x);

        // Arrow head (filled triangle pointing in flow direction)
        let tip = pos + dir * arrow_size;
        let left = pos - dir * arrow_size * 0.3 + perp * arrow_size * 0.45;
        let right = pos - dir * arrow_size * 0.3 - perp * arrow_size * 0.45;

        painter.circle_filled(
            pos - dir * arrow_size * 0.2,
            arrow_size * 0.72,
            Color32::from_rgba_unmultiplied(255, 205, 50, 48),
        );
        painter.add(egui::Shape::convex_polygon(
            vec![tip, left, right],
            Color32::from_rgb(255, 245, 120),
            Stroke::new(1.4, Color32::from_rgb(90, 55, 0)),
        ));

        // Bright dot at tail for glow effect
        painter.circle_filled(
            pos - dir * arrow_size * 0.35,
            2.8,
            Color32::from_rgb(255, 190, 45),
        );

        distance += spacing;
    }
}

fn draw_short_fault_markers(painter: &egui::Painter, points: &[Pos2]) {
    let total = polyline_length(points);
    if total <= 1.0 {
        return;
    }

    let spacing = 70.0;
    let marker_count = (total / spacing).ceil().max(1.0) as usize;
    let stroke = Stroke::new(2.0, Color32::from_rgb(255, 220, 210));
    for idx in 0..marker_count {
        let distance = if marker_count == 1 {
            total * 0.5
        } else {
            (idx as f32 + 0.5) * total / marker_count as f32
        };
        let Some(pos) = point_on_polyline(points, distance) else {
            continue;
        };
        let r = 5.0;
        painter.circle_filled(pos, 7.0, Color32::from_rgba_unmultiplied(120, 0, 0, 120));
        painter.line_segment([pos + Vec2::new(-r, -r), pos + Vec2::new(r, r)], stroke);
        painter.line_segment([pos + Vec2::new(-r, r), pos + Vec2::new(r, -r)], stroke);
    }
}

fn draw_flow_pulses(painter: &egui::Painter, points: &[Pos2], flow_phase: f32, wire_width: f32) {
    let total = polyline_length(points);
    if total <= 1.0 {
        return;
    }

    let spacing = 42.0;
    let pulse_len = 18.0_f32.min(total.max(1.0));
    let mut distance = flow_phase.rem_euclid(spacing) - pulse_len;
    if total < spacing {
        distance = flow_phase.rem_euclid(total.max(1.0)) - pulse_len;
    }

    while distance < total {
        let start = distance.max(0.0);
        let end = (distance + pulse_len).min(total);
        if end > start {
            draw_polyline_range(
                painter,
                points,
                start,
                end,
                Stroke::new(
                    (wire_width + 3.2).max(5.2),
                    Color32::from_rgba_unmultiplied(255, 220, 70, 135),
                ),
            );
            draw_polyline_range(
                painter,
                points,
                start,
                end,
                Stroke::new(
                    (wire_width + 0.8).max(2.8),
                    Color32::from_rgb(255, 252, 170),
                ),
            );
        }
        distance += spacing;
    }
}

fn draw_polyline_range(
    painter: &egui::Painter,
    points: &[Pos2],
    start_distance: f32,
    end_distance: f32,
    stroke: Stroke,
) {
    if end_distance <= start_distance {
        return;
    }

    let mut traveled = 0.0;
    for segment in points.windows(2) {
        let a = segment[0];
        let b = segment[1];
        let length = a.distance(b);
        if length <= 0.1 {
            continue;
        }

        let segment_start = traveled;
        let segment_end = traveled + length;
        if segment_end >= start_distance && segment_start <= end_distance {
            let local_start = ((start_distance - segment_start) / length).clamp(0.0, 1.0);
            let local_end = ((end_distance - segment_start) / length).clamp(0.0, 1.0);
            if local_end > local_start {
                let p0 = a + (b - a) * local_start;
                let p1 = a + (b - a) * local_end;
                painter.line_segment([p0, p1], stroke);
            }
        }
        traveled = segment_end;
        if traveled > end_distance {
            break;
        }
    }
}

fn polyline_length(points: &[Pos2]) -> f32 {
    points
        .windows(2)
        .map(|segment| segment[0].distance(segment[1]))
        .sum()
}

fn point_on_polyline(points: &[Pos2], mut distance: f32) -> Option<Pos2> {
    for segment in points.windows(2) {
        let a = segment[0];
        let b = segment[1];
        let length = a.distance(b);
        if length <= 0.1 {
            continue;
        }
        if distance <= length {
            let t = distance / length;
            return Some(a + (b - a) * t);
        }
        distance -= length;
    }
    points.last().copied()
}

fn should_draw_pin_label(kind: ComponentKind, pin: &CircuitPin) -> bool {
    if matches!(kind, ComponentKind::Battery | ComponentKind::Ground) {
        return false;
    }
    matches!(pin.role, PinRole::Positive | PinRole::Ground)
}

fn draw_pin_label(painter: &egui::Painter, component_center: Pos2, pin: &CircuitPin) {
    let outward = pin.pos - component_center;
    let horizontal = outward.x.abs() >= outward.y.abs();
    let offset = if horizontal {
        Vec2::new(if outward.x >= 0.0 { 10.0 } else { -10.0 }, -10.0)
    } else {
        Vec2::new(10.0, if outward.y >= 0.0 { 10.0 } else { -10.0 })
    };
    let align = if horizontal && outward.x < 0.0 {
        Align2::RIGHT_CENTER
    } else if horizontal {
        Align2::LEFT_CENTER
    } else if outward.y < 0.0 {
        Align2::LEFT_BOTTOM
    } else {
        Align2::LEFT_TOP
    };
    let color = match pin.role {
        PinRole::Positive => Color32::from_rgb(255, 210, 120),
        PinRole::Ground => Color32::from_rgb(170, 210, 255),
        _ => Color32::from_rgb(220, 225, 230),
    };
    painter.text(
        pin.pos + offset,
        align,
        pin.label,
        egui::FontId::proportional(12.0),
        color,
    );
}

fn draw_wire_preview(painter: &egui::Painter, points: &[Pos2]) {
    let stroke = Stroke::new(1.8, Color32::from_rgb(130, 200, 255));
    for segment in points.windows(2) {
        painter.line_segment([segment[0], segment[1]], stroke);
    }
    for point in points {
        painter.circle_filled(*point, 3.0, Color32::from_rgb(130, 200, 255));
    }
}

// component_pin_defs, rotate_point, component_pins, component_size and helpers
// are in model/pin_defs.rs; re-exported at crate root above.

fn draw_zener(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let anode = Pos2::new(center.x - rect.width() * 0.18, center.y);
    let cathode = Pos2::new(center.x + rect.width() * 0.2, center.y);
    let tri_top = Pos2::new(
        center.x - rect.width() * 0.18,
        center.y - rect.height() * 0.42,
    );
    let tri_bottom = Pos2::new(
        center.x - rect.width() * 0.18,
        center.y + rect.height() * 0.42,
    );
    let cathode_top = Pos2::new(cathode.x, center.y - rect.height() * 0.42);
    let cathode_bottom = Pos2::new(cathode.x, center.y + rect.height() * 0.42);
    let zbar_top = Pos2::new(
        cathode.x + rect.width() * 0.06,
        center.y - rect.height() * 0.42,
    );
    let zbar_bottom = Pos2::new(
        cathode.x - rect.width() * 0.06,
        center.y + rect.height() * 0.42,
    );

    let pts = [
        left,
        right,
        anode,
        cathode,
        tri_top,
        tri_bottom,
        cathode_top,
        cathode_bottom,
        zbar_top,
        zbar_bottom,
    ];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([r[0], r[2]], stroke);
    painter.line_segment([r[3], r[1]], stroke);
    painter.line_segment([r[4], r[5]], stroke);
    painter.line_segment([r[5], r[3]], stroke);
    painter.line_segment([r[3], r[4]], stroke);
    painter.line_segment([r[8], r[6]], stroke);
    painter.line_segment([r[6], r[7]], stroke);
    painter.line_segment([r[7], r[9]], stroke);
}

fn draw_npn(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke, energized: bool) {
    let center = rect.center();
    let circle_r = rect.width().min(rect.height()) * 0.46;
    let base_x = rect.left() + rect.width() * 0.18;
    let body_fill = if energized {
        Color32::from_rgba_unmultiplied(255, 185, 80, 18)
    } else {
        Color32::TRANSPARENT
    };
    painter.circle_filled(center, circle_r, body_fill);
    painter.circle_stroke(center, circle_r, stroke);

    let base_in = Pos2::new(base_x, center.y);
    let base_out = Pos2::new(rect.left(), center.y);
    let ce_x = center.x + circle_r * 0.22;
    let c_top = Pos2::new(ce_x, center.y - rect.height() * 0.28);
    let c_pin = Pos2::new(rect.right(), rect.top() + rect.height() * 0.22);
    let e_bottom = Pos2::new(ce_x, center.y + rect.height() * 0.28);
    let e_pin = Pos2::new(rect.right(), rect.bottom() - rect.height() * 0.22);
    let bar_top = Pos2::new(base_x, center.y - rect.height() * 0.32);
    let bar_bot = Pos2::new(base_x, center.y + rect.height() * 0.32);

    let pts = [
        base_in, base_out, c_top, c_pin, e_bottom, e_pin, bar_top, bar_bot,
    ];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([r[1], r[0]], stroke);
    painter.line_segment([r[6], r[7]], stroke);
    painter.line_segment([r[0], r[2]], stroke);
    painter.line_segment([r[2], r[3]], stroke);
    painter.line_segment([r[0], r[4]], stroke);
    painter.line_segment([r[4], r[5]], stroke);
    // Emitter arrow
    let dir = (r[5] - r[4]).normalized();
    let perp = Vec2::new(-dir.y, dir.x);
    let arr = r[4] + dir * 8.0;
    painter.line_segment([arr, arr - dir * 7.0 + perp * 4.0], stroke);
    painter.line_segment([arr, arr - dir * 7.0 - perp * 4.0], stroke);
}

fn draw_pnp(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke, energized: bool) {
    let center = rect.center();
    let circle_r = rect.width().min(rect.height()) * 0.46;
    let base_x = rect.left() + rect.width() * 0.18;
    let body_fill = if energized {
        Color32::from_rgba_unmultiplied(255, 185, 80, 18)
    } else {
        Color32::TRANSPARENT
    };
    painter.circle_filled(center, circle_r, body_fill);
    painter.circle_stroke(center, circle_r, stroke);

    let base_out = Pos2::new(rect.left(), center.y);
    let ce_x = center.x + circle_r * 0.22;
    let e_top = Pos2::new(ce_x, center.y - rect.height() * 0.28);
    let e_pin = Pos2::new(rect.right(), rect.top() + rect.height() * 0.22);
    let c_bottom = Pos2::new(ce_x, center.y + rect.height() * 0.28);
    let c_pin = Pos2::new(rect.right(), rect.bottom() - rect.height() * 0.22);
    let bar_top = Pos2::new(base_x, center.y - rect.height() * 0.32);
    let bar_bot = Pos2::new(base_x, center.y + rect.height() * 0.32);
    let base_in = Pos2::new(base_x, center.y);

    let pts = [
        base_in, base_out, e_top, e_pin, c_bottom, c_pin, bar_top, bar_bot,
    ];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([r[1], r[0]], stroke);
    painter.line_segment([r[6], r[7]], stroke);
    painter.line_segment([r[0], r[2]], stroke);
    painter.line_segment([r[2], r[3]], stroke);
    painter.line_segment([r[0], r[4]], stroke);
    painter.line_segment([r[4], r[5]], stroke);
    // Emitter arrow (inward for PNP)
    let dir = (r[2] - r[3]).normalized();
    let perp = Vec2::new(-dir.y, dir.x);
    let arr = r[2] - dir * 2.0;
    painter.line_segment([arr, arr - dir * 7.0 + perp * 4.0], stroke);
    painter.line_segment([arr, arr - dir * 7.0 - perp * 4.0], stroke);
}

fn draw_nmosfet(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    energized: bool,
) {
    let center = rect.center();
    let circle_r = rect.width().min(rect.height()) * 0.44;
    let body_fill = if energized {
        Color32::from_rgba_unmultiplied(255, 185, 80, 18)
    } else {
        Color32::TRANSPARENT
    };
    painter.circle_filled(center, circle_r, body_fill);
    painter.circle_stroke(center, circle_r, stroke);

    let gate_out = Pos2::new(rect.left(), center.y);
    let gate_in = Pos2::new(center.x - circle_r * 0.55, center.y);
    let gate_bar_top = Pos2::new(center.x - circle_r * 0.55, center.y - rect.height() * 0.30);
    let gate_bar_bot = Pos2::new(center.x - circle_r * 0.55, center.y + rect.height() * 0.30);
    let chan_top = Pos2::new(center.x - circle_r * 0.20, center.y - rect.height() * 0.30);
    let chan_bot = Pos2::new(center.x - circle_r * 0.20, center.y + rect.height() * 0.30);
    let d_inner = Pos2::new(center.x + circle_r * 0.18, center.y - rect.height() * 0.28);
    let d_pin = Pos2::new(rect.right(), rect.top() + rect.height() * 0.22);
    let s_inner = Pos2::new(center.x + circle_r * 0.18, center.y + rect.height() * 0.28);
    let s_pin = Pos2::new(rect.right(), rect.bottom() - rect.height() * 0.22);

    let pts = [
        gate_out,
        gate_in,
        gate_bar_top,
        gate_bar_bot,
        chan_top,
        chan_bot,
        d_inner,
        d_pin,
        s_inner,
        s_pin,
    ];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([r[0], r[1]], stroke);
    painter.line_segment([r[2], r[3]], Stroke::new(stroke.width * 2.0, stroke.color));
    painter.line_segment([r[4], r[5]], stroke);
    painter.line_segment([r[6], r[7]], stroke);
    painter.line_segment([r[8], r[9]], stroke);
    // Arrow (N-type points inward)
    let mid = r[4].lerp(r[5], 0.5);
    let dir = (r[8] - r[6]).normalized();
    painter.line_segment([r[1], mid], stroke);
    let arr = mid + dir * 2.0;
    let perp = Vec2::new(-dir.y, dir.x);
    painter.line_segment([arr, arr - dir * 6.0 + perp * 3.5], stroke);
    painter.line_segment([arr, arr - dir * 6.0 - perp * 3.5], stroke);
}

fn draw_pmosfet(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    energized: bool,
) {
    // Same as NMOSFET but arrow points out, bubble on gate
    draw_nmosfet(painter, rect, rotation, stroke, energized);
    // Draw bubble on gate
    let center = rect.center();
    let bubble_center_nat = Pos2::new(rect.left() + rect.width() * 0.14, center.y);
    let bubble = rotate_point(bubble_center_nat, center, rotation);
    painter.circle_stroke(bubble, 4.5, stroke);
}

fn draw_potentiometer(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let mut points = Vec::new();
    let zig_count = 6;
    let step = rect.width() / (zig_count as f32 + 1.0);
    points.push(left);
    for i in 1..=zig_count {
        let x = rect.left() + step * i as f32;
        let y = if i % 2 == 0 {
            center.y - rect.height() * 0.28
        } else {
            center.y + rect.height() * 0.28
        };
        points.push(Pos2::new(x, y));
    }
    points.push(right);

    let rotated: Vec<Pos2> = points
        .into_iter()
        .map(|p| rotate_point(p, center, rotation))
        .collect();
    for seg in rotated.windows(2) {
        painter.line_segment([seg[0], seg[1]], stroke);
    }

    // Wiper arrow
    let wiper_start_nat = Pos2::new(center.x, center.y - rect.height() * 0.55);
    let wiper_tip_nat = Pos2::new(center.x, center.y);
    let wiper_pin_nat = Pos2::new(center.x, rect.bottom());
    let ws = rotate_point(wiper_start_nat, center, rotation);
    let wt = rotate_point(wiper_tip_nat, center, rotation);
    let wp = rotate_point(wiper_pin_nat, center, rotation);
    painter.line_segment([ws, wt], stroke);
    painter.line_segment([wt, wp], stroke);
    let dir = (wt - ws).normalized();
    let perp = Vec2::new(-dir.y, dir.x);
    painter.line_segment([wt, wt - dir * 6.0 + perp * 3.5], stroke);
    painter.line_segment([wt, wt - dir * 6.0 - perp * 3.5], stroke);
}

fn draw_voltage_reg(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    energized: bool,
) {
    let center = rect.center();
    let body_fill = if energized {
        Color32::from_rgb(38, 52, 28)
    } else {
        Color32::from_rgb(22, 28, 36)
    };
    let box_rect =
        Rect::from_center_size(center, Vec2::new(rect.width() * 0.72, rect.height() * 0.72));
    painter.rect_filled(box_rect, 4.0, body_fill);
    painter.rect_stroke(box_rect, 4.0, stroke, StrokeKind::Outside);
    painter.text(
        center,
        Align2::CENTER_CENTER,
        "REG",
        egui::FontId::proportional(11.0),
        stroke.color,
    );

    let pts = [
        Pos2::new(rect.left(), center.y),
        Pos2::new(box_rect.left(), center.y),
        Pos2::new(center.x, rect.bottom()),
        Pos2::new(center.x, box_rect.bottom()),
        Pos2::new(rect.right(), center.y),
        Pos2::new(box_rect.right(), center.y),
    ];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();
    painter.line_segment([r[0], r[1]], stroke);
    painter.line_segment([r[2], r[3]], stroke);
    painter.line_segment([r[4], r[5]], stroke);
}

fn draw_fuse(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let box_w = rect.width() * 0.44;
    let box_h = rect.height() * 0.56;
    let bl = Pos2::new(center.x - box_w * 0.5, center.y - box_h * 0.5);
    let br = Pos2::new(center.x + box_w * 0.5, center.y - box_h * 0.5);
    let tl = Pos2::new(center.x - box_w * 0.5, center.y + box_h * 0.5);
    let tr = Pos2::new(center.x + box_w * 0.5, center.y + box_h * 0.5);

    let pts = [
        left,
        right,
        bl,
        br,
        tl,
        tr,
        Pos2::new(center.x - box_w * 0.5, center.y),
        Pos2::new(center.x + box_w * 0.5, center.y),
    ];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([r[0], r[6]], stroke);
    painter.line_segment([r[7], r[1]], stroke);
    painter.rect_stroke(
        Rect::from_points(&[r[2], r[3], r[4], r[5]]),
        2.0,
        stroke,
        StrokeKind::Outside,
    );
    // Fuse element line
    painter.line_segment([r[6], r[7]], Stroke::new(stroke.width * 0.8, stroke.color));
}

fn draw_logic_not(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let apex = Pos2::new(rect.right() - 7.0, center.y);
    let tl = Pos2::new(rect.left() + 4.0, rect.top() + 4.0);
    let bl = Pos2::new(rect.left() + 4.0, rect.bottom() - 4.0);
    let in_pin = Pos2::new(rect.left(), center.y);
    let out_pin = Pos2::new(rect.right(), center.y);
    let bubble_center = Pos2::new(rect.right() - 4.5, center.y);

    let pts = [apex, tl, bl, in_pin, out_pin, bubble_center];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([r[1], r[0]], stroke);
    painter.line_segment([r[0], r[2]], stroke);
    painter.line_segment([r[2], r[1]], stroke);
    painter.line_segment([r[3], r[1].lerp(r[2], 0.5)], stroke);
    painter.circle_stroke(r[5], 4.5, stroke);
}

fn draw_logic_and(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    bubble: bool,
) {
    let center = rect.center();
    let x0 = rect.left() + 4.0;
    let x1 = rect.center().x + 2.0;
    let right_x = if bubble {
        rect.right() - 8.0
    } else {
        rect.right()
    };
    let top_y = rect.top() + 4.0;
    let bot_y = rect.bottom() - 4.0;

    let in_a = Pos2::new(rect.left(), center.y - rect.height() * 0.25);
    let in_b = Pos2::new(rect.left(), center.y + rect.height() * 0.25);
    let out = Pos2::new(rect.right(), center.y);
    let tl = Pos2::new(x0, top_y);
    let bl = Pos2::new(x0, bot_y);
    let tr = Pos2::new(x1, top_y);
    let br = Pos2::new(x1, bot_y);
    let arc_right = Pos2::new(right_x, center.y);

    let pts = [in_a, in_b, out, tl, bl, tr, br, arc_right];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([r[3], r[4]], stroke);
    painter.line_segment([r[3], r[5]], stroke);
    painter.line_segment([r[4], r[6]], stroke);
    // Approximated semicircle on right side
    let steps = 12;
    let mut prev = r[5];
    for i in 1..=steps {
        let a = std::f32::consts::PI * 0.5 - std::f32::consts::PI * i as f32 / steps as f32;
        let nat_p = Pos2::new(
            x1 + (bot_y - top_y) * 0.5 * a.cos(),
            center.y - (bot_y - top_y) * 0.5 * a.sin(),
        );
        let p = rotate_point(nat_p, center, rotation);
        painter.line_segment([prev, p], stroke);
        prev = p;
    }
    painter.line_segment(
        [
            r[0],
            rotate_point(
                Pos2::new(x0, center.y - rect.height() * 0.25),
                center,
                rotation,
            ),
        ],
        stroke,
    );
    painter.line_segment(
        [
            r[1],
            rotate_point(
                Pos2::new(x0, center.y + rect.height() * 0.25),
                center,
                rotation,
            ),
        ],
        stroke,
    );
    painter.line_segment([r[7], r[2]], stroke);
    if bubble {
        let bc = rotate_point(Pos2::new(rect.right() - 4.5, center.y), center, rotation);
        painter.circle_stroke(bc, 4.5, stroke);
    }
}

fn draw_logic_or(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke, bubble: bool) {
    let center = rect.center();
    let x0 = rect.left() + 4.0;
    let right_x = if bubble {
        rect.right() - 8.0
    } else {
        rect.right()
    };
    let top_y = rect.top() + 4.0;
    let bot_y = rect.bottom() - 4.0;

    let in_a = Pos2::new(rect.left(), center.y - rect.height() * 0.25);
    let in_b = Pos2::new(rect.left(), center.y + rect.height() * 0.25);
    let out = Pos2::new(rect.right(), center.y);
    let tl = Pos2::new(x0, top_y);
    let bl = Pos2::new(x0, bot_y);

    let pts = [in_a, in_b, out, tl, bl];
    let r: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();

    // Back curve
    let hw = (bot_y - top_y) * 0.5;
    let steps = 16;
    let mut prev_bot = r[4];
    let mut prev_top = r[3];
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let fa = std::f32::consts::PI * 0.5 - std::f32::consts::PI * t;
        let nat_f = Pos2::new(x0 + hw * fa.cos() + hw, center.y - hw * fa.sin());
        let f = rotate_point(nat_f, center, rotation);
        if i == steps {
            painter.line_segment([prev_top, f], stroke);
            painter.line_segment([prev_bot, f], stroke);
        } else {
            painter.line_segment([prev_top, f], stroke);
            prev_top = f;
            let ba = std::f32::consts::PI * t - std::f32::consts::PI * 0.5;
            let nat_b = Pos2::new(x0 + hw * 0.22 * ba.cos(), center.y - hw * ba.sin());
            let b = rotate_point(nat_b, center, rotation);
            painter.line_segment([prev_bot, b], stroke);
            prev_bot = b;
        }
    }
    painter.line_segment(
        [
            r[0],
            rotate_point(
                Pos2::new(x0, center.y - rect.height() * 0.25),
                center,
                rotation,
            ),
        ],
        stroke,
    );
    painter.line_segment(
        [
            r[1],
            rotate_point(
                Pos2::new(x0, center.y + rect.height() * 0.25),
                center,
                rotation,
            ),
        ],
        stroke,
    );

    let end_pt = rotate_point(Pos2::new(right_x, center.y), center, rotation);
    painter.line_segment([end_pt, r[2]], stroke);
    if bubble {
        let bc = rotate_point(Pos2::new(rect.right() - 4.5, center.y), center, rotation);
        painter.circle_stroke(bc, 4.5, stroke);
    }
}

fn draw_logic_xor(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    bubble: bool,
) {
    draw_logic_or(painter, rect, rotation, stroke, bubble);
    let center = rect.center();
    let hw = (rect.bottom() - 4.0 - (rect.top() + 4.0)) * 0.5;
    let x0 = rect.left();
    let steps = 10;
    let bot_y = rect.bottom() - 4.0;
    let mut prev = rotate_point(Pos2::new(x0, bot_y), center, rotation);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let ba = std::f32::consts::PI * t - std::f32::consts::PI * 0.5;
        let nat_b = Pos2::new(x0 - 4.0 + hw * 0.22 * ba.cos(), center.y - hw * ba.sin());
        let b = rotate_point(nat_b, center, rotation);
        painter.line_segment([prev, b], stroke);
        prev = b;
    }
}

// ─── New commercial component drawing functions ──────────────────────────────

fn draw_net_label(
    painter: &egui::Painter,
    component: &Component,
    rect: Rect,
    stroke: Stroke,
    energized: bool,
) {
    let center = rect.center();
    let w = rect.width() * 0.5;
    let h = rect.height() * 0.4;
    // Arrow-flag shape pointing right
    let pts = vec![
        Pos2::new(rect.left(), center.y - h),
        Pos2::new(rect.right() - rect.width() * 0.15, center.y - h),
        Pos2::new(rect.right(), center.y),
        Pos2::new(rect.right() - rect.width() * 0.15, center.y + h),
        Pos2::new(rect.left(), center.y + h),
    ];
    let fill = if energized {
        Color32::from_rgba_unmultiplied(255, 200, 80, 50)
    } else {
        Color32::from_rgba_unmultiplied(80, 160, 255, 35)
    };
    painter.add(egui::Shape::convex_polygon(pts.clone(), fill, stroke));
    // Net name label
    let text_col = if energized {
        Color32::from_rgb(255, 210, 100)
    } else {
        Color32::from_rgb(160, 200, 255)
    };
    painter.text(
        Pos2::new(rect.left() + w * 0.55, center.y),
        Align2::CENTER_CENTER,
        &component.value,
        egui::FontId::monospace(11.0),
        text_col,
    );
}

fn draw_crystal(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let hw = rect.width() * 0.5;
    let hh = rect.height() * 0.45;
    let plate_gap = hw * 0.28;
    let plate_h = hh * 0.55;

    let (p1, p2, p3, p4, plates) = if rotation == 90 || rotation == 270 {
        let a = Pos2::new(center.x, rect.top());
        let b = Pos2::new(center.x, center.y - plate_gap);
        let c = Pos2::new(center.x, center.y + plate_gap);
        let d = Pos2::new(center.x, rect.bottom());
        let ps = vec![
            [
                Pos2::new(center.x - plate_h, center.y - plate_gap),
                Pos2::new(center.x + plate_h, center.y - plate_gap),
            ],
            [
                Pos2::new(center.x - plate_h, center.y + plate_gap),
                Pos2::new(center.x + plate_h, center.y + plate_gap),
            ],
        ];
        (a, b, c, d, ps)
    } else {
        let a = Pos2::new(rect.left(), center.y);
        let b = Pos2::new(center.x - plate_gap, center.y);
        let c = Pos2::new(center.x + plate_gap, center.y);
        let d = Pos2::new(rect.right(), center.y);
        let ps = vec![
            [
                Pos2::new(center.x - plate_gap, center.y - plate_h),
                Pos2::new(center.x - plate_gap, center.y + plate_h),
            ],
            [
                Pos2::new(center.x + plate_gap, center.y - plate_h),
                Pos2::new(center.x + plate_gap, center.y + plate_h),
            ],
        ];
        (a, b, c, d, ps)
    };

    painter.line_segment([p1, p2], stroke);
    painter.line_segment([p3, p4], stroke);
    for plate in plates {
        painter.line_segment(plate, Stroke::new(stroke.width + 1.0, stroke.color));
    }
    // Body box between plates
    let body = if rotation == 90 || rotation == 270 {
        Rect::from_center_size(center, Vec2::new(plate_h * 2.0, plate_gap * 2.0))
    } else {
        Rect::from_center_size(center, Vec2::new(plate_gap * 2.0, plate_h * 2.0))
    };
    painter.rect_stroke(body, 2.0, stroke, StrokeKind::Middle);
}

fn draw_transformer(painter: &egui::Painter, rect: Rect, _rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let hw = rect.width() * 0.46;
    let hh = rect.height() * 0.38;
    // Primary coil (left side)
    let num_loops = 4;
    for i in 0..num_loops {
        painter.circle_stroke(
            Pos2::new(rect.left() + hw * 0.22 + i as f32 * (hw * 0.22), center.y),
            hh * 0.28,
            stroke,
        );
    }
    // Secondary coil (right side)
    for i in 0..num_loops {
        painter.circle_stroke(
            Pos2::new(rect.right() - hw * 0.22 - i as f32 * (hw * 0.22), center.y),
            hh * 0.28,
            stroke,
        );
    }
    // Core line
    let core_x = center.x;
    painter.line_segment(
        [
            Pos2::new(core_x - 2.0, center.y - hh),
            Pos2::new(core_x - 2.0, center.y + hh),
        ],
        Stroke::new(2.0, stroke.color),
    );
    painter.line_segment(
        [
            Pos2::new(core_x + 2.0, center.y - hh),
            Pos2::new(core_x + 2.0, center.y + hh),
        ],
        Stroke::new(2.0, stroke.color),
    );
    // Lead wires
    painter.line_segment(
        [
            Pos2::new(rect.left(), center.y - hh * 0.6),
            Pos2::new(rect.left() + hw * 0.22 - hh * 0.28, center.y - hh * 0.6),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(rect.left(), center.y + hh * 0.6),
            Pos2::new(rect.left() + hw * 0.22 - hh * 0.28, center.y + hh * 0.6),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(rect.right(), center.y - hh * 0.6),
            Pos2::new(rect.right() - hw * 0.22 + hh * 0.28, center.y - hh * 0.6),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(rect.right(), center.y + hh * 0.6),
            Pos2::new(rect.right() - hw * 0.22 + hh * 0.28, center.y + hh * 0.6),
        ],
        stroke,
    );
}

fn draw_thermistor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    // Draw as a resistor with a diagonal arrow through it (NTC symbol)
    draw_resistor(painter, rect, rotation, stroke);
    let center = rect.center();
    let hw = rect.width() * 0.32;
    let hh = rect.height() * 0.55;
    // Diagonal temperature arrow
    let arr_start = Pos2::new(center.x - hw * 0.6, center.y + hh * 0.8);
    let arr_end = Pos2::new(center.x + hw * 0.6, center.y - hh * 0.8);
    painter.line_segment(
        [arr_start, arr_end],
        Stroke::new(1.5, Color32::from_rgb(255, 160, 80)),
    );
    // Arrowhead
    painter.line_segment(
        [arr_end, Pos2::new(arr_end.x - 5.0, arr_end.y + 2.0)],
        Stroke::new(1.5, Color32::from_rgb(255, 160, 80)),
    );
    painter.line_segment(
        [arr_end, Pos2::new(arr_end.x - 2.0, arr_end.y + 5.0)],
        Stroke::new(1.5, Color32::from_rgb(255, 160, 80)),
    );
    painter.text(
        Pos2::new(center.x + hw * 0.7, center.y - hh * 0.9),
        Align2::LEFT_BOTTOM,
        "T",
        egui::FontId::proportional(9.0),
        Color32::from_rgb(255, 160, 80),
    );
}

fn draw_varistor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    draw_resistor(painter, rect, rotation, stroke);
    let center = rect.center();
    // "V" label inside
    painter.text(
        center,
        Align2::CENTER_CENTER,
        "V",
        egui::FontId::proportional(10.0),
        stroke.color,
    );
}

fn draw_ic_box(
    painter: &egui::Painter,
    rect: Rect,
    _rotation: i32,
    stroke: Stroke,
    energized: bool,
    label: &str,
) {
    let fill = if energized {
        Color32::from_rgba_unmultiplied(60, 120, 80, 80)
    } else {
        Color32::from_rgba_unmultiplied(38, 44, 54, 200)
    };
    painter.rect_filled(rect, 4.0, fill);
    painter.rect_stroke(rect, 4.0, stroke, StrokeKind::Middle);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        egui::FontId::monospace(10.0),
        stroke.color,
    );
}

// ─────────────────────────────────────────────────────────────────────────────

fn draw_junctions(painter: &egui::Painter, wires: &[Wire], view: CanvasView) {
    let mut junction_keys: HashSet<(i32, i32)> = HashSet::new();
    let mut junctions: Vec<Pos2> = Vec::new();

    // Pass 1: shared vertices + collect unique endpoint keys in one scan
    let mut counts: HashMap<(i32, i32), (Pos2, u32)> = HashMap::with_capacity(wires.len() * 3);
    let mut endpoint_keys: HashSet<(i32, i32)> = HashSet::with_capacity(wires.len() * 2);
    for wire in wires {
        let n = wire.points.len();
        for (idx, &point) in wire.points.iter().enumerate() {
            let key = (point.x.round() as i32, point.y.round() as i32);
            let entry = counts.entry(key).or_insert((point, 0));
            entry.1 += 1;
            if idx == 0 || idx + 1 == n {
                endpoint_keys.insert(key);
            }
        }
    }
    for (&key, &(pos, count)) in &counts {
        if count > 1 && junction_keys.insert(key) {
            junctions.push(pos);
        }
    }

    // Pass 2: T-intersections — flatten all segments for cache-friendly scan
    let segments: Vec<(Pos2, Pos2)> = wires
        .iter()
        .flat_map(|w| w.points.windows(2).map(|s| (s[0], s[1])))
        .collect();

    for &ep_key in &endpoint_keys {
        if junction_keys.contains(&ep_key) {
            continue;
        }
        let ep = counts[&ep_key].0;
        'seg: for &(sa, sb) in &segments {
            if ep.distance(sa) > 1.5
                && ep.distance(sb) > 1.5
                && distance_to_segment(ep, sa, sb) < 1.5
            {
                if junction_keys.insert(ep_key) {
                    junctions.push(ep);
                }
                break 'seg;
            }
        }
    }

    for pos in &junctions {
        let sp = view.to_screen(*pos);
        let r = view.scale_f(5.0).clamp(3.5, 7.0);
        painter.circle_filled(sp, r, Color32::from_rgb(105, 178, 255));
        painter.circle_stroke(
            sp,
            r + 1.5,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(105, 178, 255, 80)),
        );
    }

    // Draw hop arcs at non-connecting crossings (two wires cross but are NOT in junction list)
    // Flat segment list with AABB min/max pre-computed to skip non-overlapping pairs quickly
    let seg_data: Vec<(Pos2, Pos2, f32, f32, f32, f32)> = segments
        .iter()
        .map(|&(a, b)| (a, b, a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y)))
        .collect();

    let n_seg = seg_data.len();
    for i in 0..n_seg {
        let (a0, a1, ax0, ax1, ay0, ay1) = seg_data[i];
        for j in (i + 1)..n_seg {
            let (b0, b1, bx0, bx1, by0, by1) = seg_data[j];
            // AABB overlap test — eliminates ~90% of pairs before the heavier intersection math
            if ax1 < bx0 || bx1 < ax0 || ay1 < by0 || by1 < ay0 {
                continue;
            }
            if let Some(cross) = segment_intersection(a0, a1, b0, b1) {
                let key = (cross.x.round() as i32, cross.y.round() as i32);
                if junction_keys.contains(&key) {
                    continue;
                }
                let sp = view.to_screen(cross);
                let hop_r = view.scale_f(5.5).clamp(3.0, 8.0);
                let dir = (b1 - b0).normalized();
                let perp = Vec2::new(-dir.y, dir.x);
                let ha0 = sp - dir * hop_r;
                let ha1 = sp + dir * hop_r;
                let ctrl = sp + perp * hop_r * 1.2;
                let p0 = ha0.lerp(ctrl, 0.5);
                let p2 = ha1.lerp(ctrl, 0.5);
                let bg = Color32::from_rgb(18, 22, 28);
                painter.line_segment([ha0, ha1], Stroke::new(5.0, bg));
                let hop_stroke = Stroke::new(2.0, Color32::from_rgb(105, 178, 255));
                painter.line_segment([ha0, p0], hop_stroke);
                painter.line_segment([p0, ctrl], hop_stroke);
                painter.line_segment([ctrl, p2], hop_stroke);
                painter.line_segment([p2, ha1], hop_stroke);
            }
        }
    }
}

fn segment_intersection(a0: Pos2, a1: Pos2, b0: Pos2, b1: Pos2) -> Option<Pos2> {
    let da = a1 - a0;
    let db = b1 - b0;
    let denom = da.x * db.y - da.y * db.x;
    if denom.abs() < 1e-6 {
        return None;
    } // parallel
    let t = ((b0.x - a0.x) * db.y - (b0.y - a0.y) * db.x) / denom;
    let u = ((b0.x - a0.x) * da.y - (b0.y - a0.y) * da.x) / denom;
    if t > 0.01 && t < 0.99 && u > 0.01 && u < 0.99 {
        Some(a0 + da * t)
    } else {
        None
    }
}

fn draw_meter(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    symbol: &str,
    energized: bool,
) {
    let center = rect.center();
    let r = rect.width().min(rect.height()) * 0.44;
    let body_fill = if energized {
        Color32::from_rgba_unmultiplied(60, 120, 80, 30)
    } else {
        Color32::from_rgba_unmultiplied(28, 36, 46, 180)
    };
    painter.circle_filled(center, r, body_fill);
    painter.circle_stroke(center, r, stroke);

    let text_col = if energized {
        Color32::from_rgb(100, 255, 170)
    } else {
        stroke.color
    };
    painter.text(
        center,
        Align2::CENTER_CENTER,
        symbol,
        egui::FontId::proportional(r * 0.85),
        text_col,
    );

    // Terminal leads
    let left_nat = Pos2::new(rect.left(), center.y);
    let left_inner_nat = Pos2::new(center.x - r, center.y);
    let right_inner_nat = Pos2::new(center.x + r, center.y);
    let right_nat = Pos2::new(rect.right(), center.y);
    let points = [left_nat, left_inner_nat, right_inner_nat, right_nat];
    let rp: Vec<Pos2> = points
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();
    painter.line_segment([rp[0], rp[1]], stroke);
    painter.line_segment([rp[2], rp[3]], stroke);

    // Polarity marks: + on left lead, − on right lead
    let plus_pos = rotate_point(
        Pos2::new(rect.left() + (rect.width() * 0.5 - r) * 0.5, center.y - 8.0),
        center,
        rotation,
    );
    let minus_pos = rotate_point(
        Pos2::new(
            rect.right() - (rect.width() * 0.5 - r) * 0.5,
            center.y - 8.0,
        ),
        center,
        rotation,
    );
    let pol_col = Color32::from_rgb(180, 190, 200);
    painter.text(
        plus_pos,
        Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(9.0),
        pol_col,
    );
    painter.text(
        minus_pos,
        Align2::CENTER_CENTER,
        "−",
        egui::FontId::proportional(9.0),
        pol_col,
    );
}

fn resistor_band_color(digit: u8) -> Color32 {
    match digit {
        0 => Color32::from_rgb(20, 20, 20),
        1 => Color32::from_rgb(139, 69, 19),
        2 => Color32::from_rgb(220, 40, 40),
        3 => Color32::from_rgb(255, 140, 0),
        4 => Color32::from_rgb(255, 220, 0),
        5 => Color32::from_rgb(60, 180, 60),
        6 => Color32::from_rgb(50, 80, 220),
        7 => Color32::from_rgb(160, 32, 240),
        8 => Color32::from_rgb(170, 170, 170),
        _ => Color32::WHITE,
    }
}

fn resistor_value_to_bands(ohms: f64) -> [u8; 4] {
    // Returns [band1, band2, multiplier_exp, tolerance=5%=7]
    if ohms <= 0.0 {
        return [0, 0, 0, 7];
    }
    let exp = ohms.log10().floor() as i32 - 1;
    let mantissa = ohms / 10f64.powi(exp);
    let d1 = (mantissa / 10.0).floor().clamp(0.0, 9.0) as u8;
    let d2 = (mantissa % 10.0).floor().clamp(0.0, 9.0) as u8;
    let mult = exp.clamp(0, 9) as u8;
    [d1, d2, mult, 7] // gold = tolerance 5%
}

fn draw_resistor_with_bands(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    ohms: f64,
) {
    let bands = resistor_value_to_bands(ohms);
    draw_resistor_body(painter, rect, rotation, stroke, bands);
}

fn draw_resistor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    draw_resistor_body(painter, rect, rotation, stroke, [1, 0, 3, 7]);
}

fn draw_resistor_body(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    bands: [u8; 4],
) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), rect.center().y);
    let right = Pos2::new(rect.right(), rect.center().y);

    // Body rectangle
    let body_w = rect.width() * 0.55;
    let body_h = rect.height() * 0.48;
    let body = Rect::from_center_size(center, Vec2::new(body_w, body_h));

    // Leads
    let left_inner = Pos2::new(center.x - body_w * 0.5, center.y);
    let right_inner = Pos2::new(center.x + body_w * 0.5, center.y);
    let pts = [left, left_inner, right_inner, right];
    let rpts: Vec<Pos2> = pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();
    painter.line_segment([rpts[0], rpts[1]], stroke);
    painter.line_segment([rpts[2], rpts[3]], stroke);

    // Body fill
    let body_corners: Vec<Pos2> = [
        body.left_top(),
        body.right_top(),
        body.right_bottom(),
        body.left_bottom(),
    ]
    .iter()
    .map(|&p| rotate_point(p, center, rotation))
    .collect();
    painter.add(egui::Shape::convex_polygon(
        body_corners,
        Color32::from_rgb(210, 175, 120),
        stroke,
    ));

    // Color bands (4-band) from value
    let band_positions = [0.18_f32, 0.32, 0.46, 0.74];
    let band_w = body_w * 0.10;
    let band_h = body_h * 0.95;
    // bands passed as parameter
    for (i, &frac) in band_positions.iter().enumerate() {
        let bx = body.left() + body_w * frac;
        let by = center.y;
        let color = resistor_band_color(bands[i]);
        let band_rect =
            Rect::from_center_size(Pos2::new(bx + band_w * 0.5, by), Vec2::new(band_w, band_h));
        let bcs: Vec<Pos2> = [
            band_rect.left_top(),
            band_rect.right_top(),
            band_rect.right_bottom(),
            band_rect.left_bottom(),
        ]
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();
        painter.add(egui::Shape::convex_polygon(bcs, color, egui::Stroke::NONE));
    }
}

fn draw_capacitor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), rect.center().y);
    let right = Pos2::new(rect.right(), rect.center().y);
    let plate_offset = rect.width() * 0.2;
    let plate_height = rect.height() * 0.5;
    let p1 = Pos2::new(center.x - plate_offset, rect.center().y - plate_height);
    let p2 = Pos2::new(center.x - plate_offset, rect.center().y + plate_height);
    let p3 = Pos2::new(center.x + plate_offset, rect.center().y - plate_height);
    let p4 = Pos2::new(center.x + plate_offset, rect.center().y + plate_height);

    let points = [left, right, p1, p2, p3, p4];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    let left = rotated[0];
    let right = rotated[1];
    let p1 = rotated[2];
    let p2 = rotated[3];
    let p3 = rotated[4];
    let p4 = rotated[5];

    painter.line_segment([left, p1.lerp(p2, 0.5)], stroke);
    painter.line_segment([p3.lerp(p4, 0.5), right], stroke);
    painter.line_segment([p1, p2], stroke);
    // Curved plate for positive terminal
    let steps = 12;
    let curve_h = rect.height() * 0.5;
    let curve_x = center.x + plate_offset;
    let curve_depth = rect.width() * 0.06;
    let mut prev = rotate_point(Pos2::new(curve_x, center.y - curve_h), center, rotation);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let angle = std::f32::consts::PI * t;
        let cx = curve_x + curve_depth * angle.sin();
        let cy = center.y - curve_h + curve_h * 2.0 * t;
        let next = rotate_point(Pos2::new(cx, cy), center, rotation);
        painter.line_segment([prev, next], stroke);
        prev = next;
    }
    // + polarity mark near positive plate
    let plus_pos = rotate_point(
        Pos2::new(
            center.x - plate_offset - rect.width() * 0.12,
            center.y - rect.height() * 0.25,
        ),
        center,
        rotation,
    );
    painter.text(
        plus_pos,
        Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(8.0),
        Color32::from_rgb(120, 220, 140),
    );
}

fn draw_inductor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let turns = 4;
    let body_w = rect.width() * 0.72;
    let body_start = center.x - body_w * 0.5;
    let step = body_w / turns as f32;
    let radius = rect.height() * 0.22;
    let seg_steps = 16;

    // Lead lines
    let lead_l = Pos2::new(body_start, center.y);
    let lead_r = Pos2::new(body_start + body_w, center.y);
    painter.line_segment(
        [
            rotate_point(left, center, rotation),
            rotate_point(lead_l, center, rotation),
        ],
        stroke,
    );
    painter.line_segment(
        [
            rotate_point(lead_r, center, rotation),
            rotate_point(right, center, rotation),
        ],
        stroke,
    );

    // Draw smooth arcs (upper semicircles)
    for i in 0..turns {
        let cx = body_start + step * (i as f32 + 0.5);
        let arc_center = Pos2::new(cx, center.y);
        let mut prev = rotate_point(Pos2::new(cx - radius, center.y), center, rotation);
        for s in 1..=seg_steps {
            let theta = std::f32::consts::PI * s as f32 / seg_steps as f32;
            let px = cx - radius * theta.cos();
            let py = center.y - radius * theta.sin();
            let next = rotate_point(Pos2::new(px, py), center, rotation);
            painter.line_segment([prev, next], stroke);
            prev = next;
        }
        let _ = arc_center;
    }
}

fn draw_diode(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke, filled: bool) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let anode = Pos2::new(center.x - rect.width() * 0.18, center.y);
    let cathode = Pos2::new(center.x + rect.width() * 0.2, center.y);
    let tri_top = Pos2::new(
        center.x - rect.width() * 0.18,
        center.y - rect.height() * 0.42,
    );
    let tri_bottom = Pos2::new(
        center.x - rect.width() * 0.18,
        center.y + rect.height() * 0.42,
    );
    let cathode_top = Pos2::new(cathode.x, center.y - rect.height() * 0.42);
    let cathode_bottom = Pos2::new(cathode.x, center.y + rect.height() * 0.42);

    let points = [
        left,
        right,
        anode,
        cathode,
        tri_top,
        tri_bottom,
        cathode_top,
        cathode_bottom,
    ];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[2]], stroke);
    painter.line_segment([rotated[3], rotated[1]], stroke);
    let triangle = vec![rotated[4], rotated[5], rotated[3]];
    if filled {
        painter.add(egui::Shape::convex_polygon(
            triangle.clone(),
            Color32::from_rgba_unmultiplied(
                stroke.color.r(),
                stroke.color.g(),
                stroke.color.b(),
                40,
            ),
            stroke,
        ));
    } else {
        painter.line_segment([triangle[0], triangle[1]], stroke);
        painter.line_segment([triangle[1], triangle[2]], stroke);
        painter.line_segment([triangle[2], triangle[0]], stroke);
    }
    painter.line_segment([rotated[6], rotated[7]], stroke);
}

fn led_glow_colors(value: &str) -> (Color32, Color32) {
    // Scan each token in the value string for a recognized color keyword.
    // This handles "3.2V red", "red 2V", "green", "IR", etc.
    let v = value.trim().to_ascii_lowercase();
    for token in v.split_whitespace() {
        let colors = match token {
            "red" | "r" => Some((
                Color32::from_rgba_unmultiplied(255, 40, 40, 30),
                Color32::from_rgba_unmultiplied(255, 80, 80, 70),
            )),
            "green" | "g" => Some((
                Color32::from_rgba_unmultiplied(40, 255, 80, 30),
                Color32::from_rgba_unmultiplied(80, 255, 120, 70),
            )),
            "blue" | "b" => Some((
                Color32::from_rgba_unmultiplied(40, 100, 255, 30),
                Color32::from_rgba_unmultiplied(80, 140, 255, 70),
            )),
            "yellow" | "y" => Some((
                Color32::from_rgba_unmultiplied(255, 240, 40, 30),
                Color32::from_rgba_unmultiplied(255, 250, 80, 70),
            )),
            "white" | "w" => Some((
                Color32::from_rgba_unmultiplied(220, 220, 255, 30),
                Color32::from_rgba_unmultiplied(240, 240, 255, 70),
            )),
            "orange" | "o" => Some((
                Color32::from_rgba_unmultiplied(255, 140, 20, 30),
                Color32::from_rgba_unmultiplied(255, 165, 60, 70),
            )),
            "uv" | "purple" | "violet" => Some((
                Color32::from_rgba_unmultiplied(160, 40, 255, 30),
                Color32::from_rgba_unmultiplied(190, 80, 255, 70),
            )),
            "ir" | "infrared" => Some((
                Color32::from_rgba_unmultiplied(180, 20, 20, 20),
                Color32::from_rgba_unmultiplied(200, 40, 40, 50),
            )),
            _ => None,
        };
        if let Some(pair) = colors {
            return pair;
        }
    }
    // Default: warm yellow-white
    (
        Color32::from_rgba_unmultiplied(255, 220, 60, 30),
        Color32::from_rgba_unmultiplied(255, 235, 100, 70),
    )
}

fn draw_led(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    draw_diode(painter, rect, rotation, stroke, true);
    let center = rect.center();
    // Two emission arrows with proper arrowheads (45° angle, upper-right direction)
    let arrows = [
        (
            Pos2::new(
                center.x + rect.width() * 0.10,
                center.y - rect.height() * 0.48,
            ),
            Pos2::new(
                center.x + rect.width() * 0.30,
                center.y - rect.height() * 0.70,
            ),
        ),
        (
            Pos2::new(
                center.x + rect.width() * 0.22,
                center.y - rect.height() * 0.30,
            ),
            Pos2::new(
                center.x + rect.width() * 0.42,
                center.y - rect.height() * 0.52,
            ),
        ),
    ];
    for (raw_start, raw_end) in arrows {
        let s = rotate_point(raw_start, center, rotation);
        let e = rotate_point(raw_end, center, rotation);
        painter.line_segment([s, e], stroke);
        // Small arrowhead: two lines fanning back from tip
        let dir = (e - s).normalized();
        let perp = Vec2::new(-dir.y, dir.x);
        let head_len = rect.width() * 0.08;
        let back = e - dir * head_len;
        let head1 = back + perp * head_len * 0.45;
        let head2 = back - perp * head_len * 0.45;
        painter.line_segment([e, head1], Stroke::new(stroke.width * 0.85, stroke.color));
        painter.line_segment([e, head2], Stroke::new(stroke.width * 0.85, stroke.color));
    }
}

fn draw_switch(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke, closed: bool) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let left_contact = Pos2::new(center.x - rect.width() * 0.25, center.y);
    let right_contact = Pos2::new(center.x + rect.width() * 0.25, center.y);
    let blade_end = if closed {
        // Blade horizontal → connects the two contacts
        Pos2::new(center.x + rect.width() * 0.25, center.y)
    } else {
        // Blade angled up → open
        Pos2::new(
            center.x + rect.width() * 0.18,
            center.y - rect.height() * 0.32,
        )
    };
    let points = [left, right, left_contact, right_contact, blade_end];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[2]], stroke);
    painter.line_segment([rotated[3], rotated[1]], stroke);
    painter.circle_filled(rotated[2], 3.2, stroke.color);
    painter.circle_filled(rotated[3], 3.2, stroke.color);
    painter.line_segment([rotated[2], rotated[4]], stroke);
}

fn draw_push_button(
    painter: &egui::Painter,
    rect: Rect,
    rotation: i32,
    stroke: Stroke,
    closed: bool,
) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let left_contact = Pos2::new(center.x - rect.width() * 0.24, center.y);
    let right_contact = Pos2::new(center.x + rect.width() * 0.24, center.y);
    // Bar position: lower when closed (touching contacts), raised when open
    let bar_y_offset = if closed { 0.0 } else { -rect.height() * 0.18 };
    let bar_left = Pos2::new(center.x - rect.width() * 0.18, center.y + bar_y_offset);
    let bar_right = Pos2::new(center.x + rect.width() * 0.18, center.y + bar_y_offset);
    let stem_top = Pos2::new(center.x, rect.top() + rect.height() * 0.08);
    let stem_bottom = Pos2::new(center.x, center.y + bar_y_offset);
    let points = [
        left,
        right,
        left_contact,
        right_contact,
        bar_left,
        bar_right,
        stem_top,
        stem_bottom,
    ];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[2]], stroke);
    painter.line_segment([rotated[3], rotated[1]], stroke);
    painter.circle_filled(rotated[2], 3.2, stroke.color);
    painter.circle_filled(rotated[3], 3.2, stroke.color);
    // Draw bar (filled rect when closed to show solid connection)
    if closed {
        let bar_rect = Rect::from_center_size(
            Pos2::new((rotated[4].x + rotated[5].x) / 2.0, rotated[4].y),
            Vec2::new((rotated[5].x - rotated[4].x).abs() + 3.0, 5.0),
        );
        painter.rect_filled(bar_rect, 0.0, stroke.color);
    } else {
        painter.line_segment([rotated[4], rotated[5]], stroke);
    }
    painter.line_segment([rotated[6], rotated[7]], stroke);
}

fn draw_oled(
    painter: &egui::Painter,
    _component: &Component,
    rect: Rect,
    stroke: Stroke,
    energized: bool,
    rotation: i32,
) {
    let rot = rotation.rem_euclid(360);
    let center = rect.center();

    let body_rect = if rot == 90 || rot == 270 {
        Rect::from_center_size(center, Vec2::new(rect.height(), rect.width()))
    } else {
        rect
    };
    let body_fill = if energized {
        Color32::from_rgb(22, 30, 42)
    } else {
        Color32::from_rgb(20, 26, 34)
    };
    painter.rect_filled(body_rect, 5.0, body_fill);
    painter.rect_stroke(body_rect, 5.0, stroke, StrokeKind::Outside);

    let nat_screen = Rect::from_min_max(
        rect.min + Vec2::new(10.0, 20.0),
        rect.max - Vec2::new(10.0, 10.0),
    );
    let sc_corners = [
        nat_screen.left_top(),
        nat_screen.right_top(),
        nat_screen.right_bottom(),
        nat_screen.left_bottom(),
    ]
    .map(|p| rotate_point(p, center, rotation));
    let screen_rect = Rect::from_points(&sc_corners);

    if energized {
        // True OLED black background (unlit pixels are pure black)
        painter.rect_filled(screen_rect, 3.0, Color32::BLACK);

        // Outer glow around screen
        painter.rect_stroke(
            screen_rect.expand(2.5),
            4.0,
            Stroke::new(2.0, Color32::from_rgba_unmultiplied(80, 200, 255, 60)),
            StrokeKind::Outside,
        );
        painter.rect_stroke(
            screen_rect,
            3.0,
            Stroke::new(1.0, Color32::from_rgb(40, 160, 220)),
            StrokeKind::Outside,
        );

        let sw = screen_rect.width();
        let sh = screen_rect.height();
        let sx = screen_rect.left();
        let sy = screen_rect.top();

        // Top title bar in bright cyan/white — characteristic OLED look
        let title_y = sy + sh * 0.18;
        painter.text(
            Pos2::new(sx + sw * 0.5, sy + sh * 0.08),
            Align2::CENTER_TOP,
            "CLUSTER",
            egui::FontId::monospace(7.0),
            Color32::from_rgb(255, 255, 255),
        );

        // Separator line
        painter.line_segment(
            [
                Pos2::new(sx + 4.0, title_y),
                Pos2::new(sx + sw - 4.0, title_y),
            ],
            Stroke::new(1.0, Color32::from_rgb(60, 160, 220)),
        );

        // Signal-bar icon (3 bars, left side)
        let bar_x = sx + 5.0;
        let bar_bot = sy + sh * 0.52;
        for (i, h_frac) in [0.25_f32, 0.45, 0.65].iter().enumerate() {
            let bh = sh * h_frac;
            let bx = bar_x + i as f32 * 6.0;
            painter.rect_filled(
                Rect::from_min_size(Pos2::new(bx, bar_bot - bh), Vec2::new(4.0, bh)),
                1.0,
                if i < 2 {
                    Color32::from_rgb(80, 220, 180)
                } else {
                    Color32::from_rgb(40, 100, 80)
                },
            );
        }

        // "ON" status text
        painter.text(
            Pos2::new(sx + sw - 6.0, sy + sh * 0.38),
            Align2::RIGHT_CENTER,
            "ON",
            egui::FontId::monospace(7.0),
            Color32::from_rgb(80, 240, 130),
        );

        // Bottom address line (I2C address hint)
        painter.text(
            Pos2::new(sx + sw * 0.5, sy + sh * 0.72),
            Align2::CENTER_TOP,
            "0x3C",
            egui::FontId::monospace(7.0),
            Color32::from_rgb(130, 180, 255),
        );

        // Pixel-dot decoration row
        let dot_y = sy + sh * 0.88;
        let dot_count = ((sw - 10.0) / 5.0) as usize;
        for i in 0..dot_count {
            let brightness = if i % 3 == 0 { 180u8 } else { 60 };
            painter.circle_filled(
                Pos2::new(sx + 5.0 + i as f32 * 5.0, dot_y),
                1.2,
                Color32::from_rgb(brightness, brightness, 255),
            );
        }
    } else {
        // Screen off — OLED goes fully dark
        painter.rect_filled(screen_rect, 3.0, Color32::from_rgb(8, 9, 11));
        painter.rect_stroke(
            screen_rect,
            3.0,
            Stroke::new(1.0, Color32::from_rgb(32, 36, 42)),
            StrokeKind::Outside,
        );
        painter.text(
            screen_rect.center(),
            Align2::CENTER_CENTER,
            "OFF",
            egui::FontId::proportional(9.0),
            Color32::from_rgb(45, 50, 56),
        );
    }

    // Pin header: natural positions at top edge, then rotated
    let step = (rect.width() - 16.0) / 3.0;
    for (i, label) in ["GND", "VCC", "SCL", "SDA"].iter().enumerate() {
        let nat_pin = Pos2::new(rect.left() + 8.0 + i as f32 * step, rect.top());
        let nat_stub = Pos2::new(nat_pin.x, rect.top() + 6.0);
        let nat_label = Pos2::new(nat_pin.x, rect.top() + 11.0);
        let pin_r = rotate_point(nat_pin, center, rotation);
        let stub_r = rotate_point(nat_stub, center, rotation);
        let label_r = rotate_point(nat_label, center, rotation);
        painter.line_segment([pin_r, stub_r], stroke);
        painter.circle_filled(pin_r, 2.5, stroke.color);
        painter.text(
            label_r,
            Align2::CENTER_CENTER,
            *label,
            egui::FontId::proportional(7.5),
            Color32::from_rgb(160, 170, 180),
        );
    }
}

fn draw_breadboard(painter: &egui::Painter, rect: Rect, stroke: Stroke) {
    painter.rect_filled(rect, 4.0, Color32::from_rgb(28, 31, 35));
    painter.rect_stroke(rect, 4.0, stroke, StrokeKind::Outside);
    let plus_y = rect.top() + 24.0;
    let minus_y = rect.top() + 44.0;
    painter.line_segment(
        [
            Pos2::new(rect.left() + 14.0, plus_y),
            Pos2::new(rect.right() - 14.0, plus_y),
        ],
        Stroke::new(2.0, Color32::from_rgb(255, 185, 80)),
    );
    painter.line_segment(
        [
            Pos2::new(rect.left() + 14.0, minus_y),
            Pos2::new(rect.right() - 14.0, minus_y),
        ],
        Stroke::new(2.0, Color32::from_rgb(120, 190, 255)),
    );
    painter.line_segment(
        [
            Pos2::new(rect.center().x, rect.top() + 66.0),
            Pos2::new(rect.center().x, rect.bottom() - 14.0),
        ],
        Stroke::new(1.4, Color32::from_rgb(80, 85, 92)),
    );

    let hole = Color32::from_rgb(70, 76, 84);
    let mut x = rect.left() + 28.0;
    while x <= rect.right() - 28.0 {
        for row in 0..5 {
            let y = rect.top() + 78.0 + row as f32 * 14.0;
            painter.circle_filled(Pos2::new(x, y), 2.2, hole);
            painter.circle_filled(Pos2::new(x, y + 82.0), 2.2, hole);
        }
        x += 18.0;
    }
    painter.text(
        rect.left_top() + Vec2::new(12.0, 8.0),
        Align2::LEFT_TOP,
        "+  -",
        egui::FontId::proportional(12.0),
        Color32::from_rgb(220, 225, 230),
    );
}

fn draw_relay(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let box_rect =
        Rect::from_center_size(center, Vec2::new(rect.width() * 0.72, rect.height() * 0.72));
    painter.rect_stroke(box_rect, 4.0, stroke, StrokeKind::Outside);
    painter.text(
        center,
        Align2::CENTER_CENTER,
        "RELAY",
        egui::FontId::proportional(12.0),
        stroke.color,
    );
    let pins = [
        Pos2::new(rect.left(), center.y - rect.height() * 0.25),
        Pos2::new(box_rect.left(), center.y - rect.height() * 0.25),
        Pos2::new(rect.left(), center.y + rect.height() * 0.25),
        Pos2::new(box_rect.left(), center.y + rect.height() * 0.25),
        Pos2::new(box_rect.right(), center.y - rect.height() * 0.28),
        Pos2::new(rect.right(), center.y - rect.height() * 0.28),
        Pos2::new(box_rect.right(), center.y),
        Pos2::new(rect.right(), center.y),
        Pos2::new(box_rect.right(), center.y + rect.height() * 0.28),
        Pos2::new(rect.right(), center.y + rect.height() * 0.28),
    ];
    let rotated: Vec<Pos2> = pins
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();
    for segment in rotated.chunks(2) {
        painter.line_segment([segment[0], segment[1]], stroke);
    }
}

fn draw_dc_motor(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let radius = rect.height() * 0.34;
    let rotated_left = rotate_point(left, center, rotation);
    let rotated_right = rotate_point(right, center, rotation);
    painter.line_segment(
        [
            rotated_left,
            rotate_point(Pos2::new(center.x - radius, center.y), center, rotation),
        ],
        stroke,
    );
    painter.line_segment(
        [
            rotate_point(Pos2::new(center.x + radius, center.y), center, rotation),
            rotated_right,
        ],
        stroke,
    );
    painter.circle_stroke(center, radius, stroke);
    painter.text(
        center,
        Align2::CENTER_CENTER,
        "M",
        egui::FontId::proportional(18.0),
        stroke.color,
    );
}

fn draw_servo(painter: &egui::Painter, rect: Rect, stroke: Stroke, energized: bool) {
    let fill = if energized {
        Color32::from_rgb(48, 38, 20)
    } else {
        Color32::from_rgb(26, 31, 36)
    };
    painter.rect_filled(rect.shrink(8.0), 4.0, fill);
    painter.rect_stroke(rect.shrink(8.0), 4.0, stroke, StrokeKind::Outside);
    let horn_center = Pos2::new(rect.right() - 24.0, rect.center().y);
    painter.circle_stroke(horn_center, 10.0, stroke);
    painter.line_segment([horn_center, horn_center + Vec2::new(24.0, -12.0)], stroke);
    painter.text(
        rect.center() - Vec2::new(12.0, 0.0),
        Align2::CENTER_CENTER,
        "SERVO",
        egui::FontId::proportional(11.0),
        stroke.color,
    );
}

fn draw_ground(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let stem_top = Pos2::new(rect.center().x, rect.top());
    let stem_bottom = Pos2::new(rect.center().x, rect.center().y);
    let line1_left = Pos2::new(rect.center().x - rect.width() * 0.3, rect.center().y);
    let line1_right = Pos2::new(rect.center().x + rect.width() * 0.3, rect.center().y);
    let line2_left = Pos2::new(
        rect.center().x - rect.width() * 0.2,
        rect.center().y + rect.height() * 0.2,
    );
    let line2_right = Pos2::new(
        rect.center().x + rect.width() * 0.2,
        rect.center().y + rect.height() * 0.2,
    );
    let line3_left = Pos2::new(
        rect.center().x - rect.width() * 0.1,
        rect.center().y + rect.height() * 0.35,
    );
    let line3_right = Pos2::new(
        rect.center().x + rect.width() * 0.1,
        rect.center().y + rect.height() * 0.35,
    );

    let points = [
        stem_top,
        stem_bottom,
        line1_left,
        line1_right,
        line2_left,
        line2_right,
        line3_left,
        line3_right,
    ];

    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[1]], stroke);
    painter.line_segment([rotated[2], rotated[3]], stroke);
    painter.line_segment([rotated[4], rotated[5]], stroke);
    painter.line_segment([rotated[6], rotated[7]], stroke);
}

fn draw_vsource(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.4;
    let left = Pos2::new(rect.left(), rect.center().y);
    let right = Pos2::new(rect.right(), rect.center().y);
    let circle_left = Pos2::new(center.x - radius, center.y);
    let circle_right = Pos2::new(center.x + radius, center.y);
    // + symbol near positive (right) side, - near negative (left) side
    let plus_top = Pos2::new(center.x + radius * 0.28, center.y - radius * 0.32);
    let plus_bottom = Pos2::new(center.x + radius * 0.28, center.y + radius * 0.32);
    let plus_left = Pos2::new(center.x + radius * 0.06, center.y);
    let plus_right = Pos2::new(center.x + radius * 0.5, center.y);
    let minus_left = Pos2::new(center.x - radius * 0.5, center.y);
    let minus_right = Pos2::new(center.x - radius * 0.06, center.y);

    let points = [
        left,
        right,
        circle_left,
        circle_right,
        plus_top,
        plus_bottom,
        plus_left,
        plus_right,
        minus_left,
        minus_right,
    ];

    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[2]], stroke);
    painter.line_segment([rotated[3], rotated[1]], stroke);
    painter.circle_stroke(center, radius, stroke);
    painter.line_segment([rotated[4], rotated[5]], stroke);
    painter.line_segment([rotated[6], rotated[7]], stroke);
    painter.line_segment([rotated[8], rotated[9]], stroke);
}

fn draw_isource(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.4;
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let circle_left = Pos2::new(center.x - radius, center.y);
    let circle_right = Pos2::new(center.x + radius, center.y);
    let arrow_start = Pos2::new(center.x - radius * 0.35, center.y);
    let arrow_end = Pos2::new(center.x + radius * 0.35, center.y);
    let head_a = Pos2::new(center.x + radius * 0.1, center.y - radius * 0.22);
    let head_b = Pos2::new(center.x + radius * 0.1, center.y + radius * 0.22);
    let points = [
        left,
        right,
        circle_left,
        circle_right,
        arrow_start,
        arrow_end,
        head_a,
        head_b,
    ];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[2]], stroke);
    painter.line_segment([rotated[3], rotated[1]], stroke);
    painter.circle_stroke(center, radius, stroke);
    painter.line_segment([rotated[4], rotated[5]], stroke);
    painter.line_segment([rotated[5], rotated[6]], stroke);
    painter.line_segment([rotated[5], rotated[7]], stroke);
}

fn draw_battery(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);

    // Two cells: each cell = short line (−) + long line (+)
    // Pairs at 1/3 and 2/3 horizontally
    let cell_centers = [
        center.x - rect.width() * 0.12,
        center.x + rect.width() * 0.12,
    ];
    let mut all_pts: Vec<Pos2> = vec![left, right];
    for &cx in &cell_centers {
        let sh = rect.height() * 0.25; // short half-height
        let lh = rect.height() * 0.44; // long half-height
        all_pts.push(Pos2::new(cx - rect.width() * 0.04, center.y - sh));
        all_pts.push(Pos2::new(cx - rect.width() * 0.04, center.y + sh));
        all_pts.push(Pos2::new(cx + rect.width() * 0.04, center.y - lh));
        all_pts.push(Pos2::new(cx + rect.width() * 0.04, center.y + lh));
    }
    let rotated: Vec<Pos2> = all_pts
        .iter()
        .map(|&p| rotate_point(p, center, rotation))
        .collect();
    let left_r = rotated[0];
    let right_r = rotated[1];

    // Indices: [0]=left, [1]=right
    // Cell 1: [2,3]=short(−), [4,5]=long(+)
    // Cell 2: [6,7]=short(−), [8,9]=long(+)
    let thick = Stroke::new(stroke.width * 1.5, stroke.color);
    let dim = Stroke::new(
        stroke.width * 0.9,
        Color32::from_rgba_unmultiplied(stroke.color.r(), stroke.color.g(), stroke.color.b(), 180),
    );

    // Lead wires
    painter.line_segment([left_r, midpoint(rotated[2], rotated[3])], stroke); // left → cell1 −
    painter.line_segment([midpoint(rotated[8], rotated[9]), right_r], stroke); // cell2 + → right

    // Cell 1
    painter.line_segment([rotated[2], rotated[3]], stroke); // short (−)
    painter.line_segment([rotated[4], rotated[5]], thick); // long  (+)

    // Inter-cell connection
    painter.line_segment(
        [
            midpoint(rotated[4], rotated[5]),
            midpoint(rotated[6], rotated[7]),
        ],
        dim,
    );

    // Cell 2
    painter.line_segment([rotated[6], rotated[7]], stroke); // short (−)
    painter.line_segment([rotated[8], rotated[9]], thick); // long  (+)

    // Polarity labels
    let minus_pos = rotate_point(
        Pos2::new(
            center.x - rect.width() * 0.35,
            center.y - rect.height() * 0.44,
        ),
        center,
        rotation,
    );
    let plus_pos = rotate_point(
        Pos2::new(
            center.x + rect.width() * 0.35,
            center.y - rect.height() * 0.44,
        ),
        center,
        rotation,
    );
    painter.text(
        minus_pos,
        Align2::CENTER_CENTER,
        "−",
        egui::FontId::proportional(13.0),
        Color32::from_rgb(140, 190, 255),
    );
    painter.text(
        plus_pos,
        Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(13.0),
        Color32::from_rgb(255, 210, 100),
    );
}

fn draw_opamp(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let left_top = Pos2::new(rect.left(), rect.top());
    let left_bottom = Pos2::new(rect.left(), rect.bottom());
    let right = Pos2::new(rect.right(), center.y);
    let in_minus = Pos2::new(rect.left(), center.y - rect.height() * 0.22);
    let in_plus = Pos2::new(rect.left(), center.y + rect.height() * 0.22);
    let minus_text = Pos2::new(
        rect.left() + rect.width() * 0.25,
        center.y - rect.height() * 0.22,
    );
    let plus_text = Pos2::new(
        rect.left() + rect.width() * 0.25,
        center.y + rect.height() * 0.22,
    );
    let points = [
        left_top,
        left_bottom,
        right,
        in_minus,
        in_plus,
        minus_text,
        plus_text,
    ];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], rotated[1]], stroke);
    painter.line_segment([rotated[1], rotated[2]], stroke);
    painter.line_segment([rotated[2], rotated[0]], stroke);
    let minus_lead = rotate_point(Pos2::new(rect.left() - 8.0, in_minus.y), center, rotation);
    let plus_lead = rotate_point(Pos2::new(rect.left() - 8.0, in_plus.y), center, rotation);
    let out_lead = rotate_point(Pos2::new(rect.right() + 8.0, center.y), center, rotation);
    painter.line_segment([minus_lead, rotated[3]], stroke);
    painter.line_segment([plus_lead, rotated[4]], stroke);
    painter.line_segment([rotated[2], out_lead], stroke);
    painter.text(
        rotated[5],
        Align2::CENTER_CENTER,
        "-",
        egui::FontId::proportional(14.0),
        stroke.color,
    );
    painter.text(
        rotated[6],
        Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(14.0),
        stroke.color,
    );
}

fn draw_lamp(painter: &egui::Painter, rect: Rect, rotation: i32, stroke: Stroke) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.34;
    let left = Pos2::new(rect.left(), center.y);
    let right = Pos2::new(rect.right(), center.y);
    let a = Pos2::new(center.x - radius * 0.7, center.y - radius * 0.7);
    let b = Pos2::new(center.x + radius * 0.7, center.y + radius * 0.7);
    let c = Pos2::new(center.x + radius * 0.7, center.y - radius * 0.7);
    let d = Pos2::new(center.x - radius * 0.7, center.y + radius * 0.7);
    let points = [left, right, a, b, c, d];
    let rotated: Vec<Pos2> = points
        .iter()
        .copied()
        .map(|p| rotate_point(p, center, rotation))
        .collect();

    painter.line_segment([rotated[0], center], stroke);
    painter.line_segment([center, rotated[1]], stroke);
    painter.circle_stroke(center, radius, stroke);
    painter.line_segment([rotated[2], rotated[3]], stroke);
    painter.line_segment([rotated[4], rotated[5]], stroke);
}

fn draw_module(
    painter: &egui::Painter,
    component: &Component,
    rect: Rect,
    stroke: Stroke,
    energized: bool,
    rotation: i32,
    title: &str,
    left_labels: &[&str],
    right_labels: &[&str],
) {
    let center = rect.center();
    let rot = rotation.rem_euclid(360);

    // Body: swap dims for 90/270
    let body_rect = if rot == 90 || rot == 270 {
        Rect::from_center_size(center, Vec2::new(rect.height(), rect.width()))
    } else {
        rect
    };
    let body_fill = if energized {
        Color32::from_rgb(62, 46, 22)
    } else {
        Color32::from_rgb(24, 30, 38)
    };
    painter.rect_filled(body_rect, 4.0, body_fill);
    painter.rect_stroke(
        body_rect,
        4.0,
        Stroke::new(stroke.width, stroke.color),
        StrokeKind::Outside,
    );

    painter.text(
        center + Vec2::new(0.0, -7.0),
        Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(14.0),
        stroke.color,
    );
    painter.text(
        center + Vec2::new(0.0, 10.0),
        Align2::CENTER_CENTER,
        &component.value,
        egui::FontId::proportional(10.0),
        Color32::from_rgb(150, 160, 170),
    );

    // Left pins: natural position on left edge, then rotated
    for (i, label) in left_labels.iter().enumerate() {
        let y = module_pin_y(rect, left_labels.len(), i);
        let nat_pin = Pos2::new(rect.left(), y);
        let nat_stub = nat_pin + Vec2::new(10.0, 0.0);
        let nat_label = nat_pin + Vec2::new(13.0, 0.0);
        let pin_r = rotate_point(nat_pin, center, rotation);
        let stub_r = rotate_point(nat_stub, center, rotation);
        let label_r = rotate_point(nat_label, center, rotation);
        painter.line_segment([pin_r, stub_r], stroke);
        painter.text(
            label_r,
            Align2::CENTER_CENTER,
            *label,
            egui::FontId::proportional(9.0),
            Color32::from_rgb(185, 195, 205),
        );
    }

    // Right pins: natural position on right edge, then rotated
    for (i, label) in right_labels.iter().enumerate() {
        let y = module_pin_y(rect, right_labels.len(), i);
        let nat_pin = Pos2::new(rect.right(), y);
        let nat_stub = nat_pin - Vec2::new(10.0, 0.0);
        let nat_label = nat_pin - Vec2::new(13.0, 0.0);
        let pin_r = rotate_point(nat_pin, center, rotation);
        let stub_r = rotate_point(nat_stub, center, rotation);
        let label_r = rotate_point(nat_label, center, rotation);
        painter.line_segment([pin_r, stub_r], stroke);
        painter.text(
            label_r,
            Align2::CENTER_CENTER,
            *label,
            egui::FontId::proportional(9.0),
            Color32::from_rgb(185, 195, 205),
        );
    }
}

fn midpoint(a: Pos2, b: Pos2) -> Pos2 {
    Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}

#[derive(Default)]
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn ensure(&mut self, index: usize) {
        while self.parent.len() <= index {
            self.parent.push(self.parent.len());
        }
    }

    fn find(&mut self, index: usize) -> usize {
        self.ensure(index);
        if self.parent[index] != index {
            self.parent[index] = self.find(self.parent[index]);
        }
        self.parent[index]
    }

    fn union(&mut self, a: usize, b: usize) {
        let a = self.find(a);
        let b = self.find(b);
        if a != b {
            self.parent[b] = a;
        }
    }
}

fn circuit_to_spice_netlist(components: &[Component], wires: &[Wire]) -> String {
    let mut nodes = CircuitNodes::default();
    let mut nets = UnionFind::default();

    for wire in wires {
        for point in &wire.points {
            let node = nodes.node_for(*point);
            nets.ensure(node);
        }
        for segment in wire.points.windows(2) {
            let a = nodes.node_for(segment[0]);
            let b = nodes.node_for(segment[1]);
            nets.union(a, b);
        }
    }

    for component in components {
        for pin in component_pin_defs(component) {
            let node = nodes.node_for(pin.pos);
            nets.ensure(node);
        }
    }
    for contact in wire_contact_points(components, wires) {
        let contact_node = nodes.node_for(contact);
        nets.ensure(contact_node);
        for wire in wires {
            for segment in wire.points.windows(2) {
                if point_touches_wire_segment(contact, segment[0], segment[1]) {
                    let a = nodes.node_for(segment[0]);
                    let b = nodes.node_for(segment[1]);
                    nets.ensure(a);
                    nets.ensure(b);
                    nets.union(contact_node, a);
                    nets.union(contact_node, b);
                }
            }
        }
    }

    let mut label_nodes: HashMap<String, Vec<usize>> = HashMap::new();
    for component in components {
        if component.kind != ComponentKind::NetLabel {
            continue;
        }
        let label = component.value.trim().to_ascii_lowercase();
        if label.is_empty() {
            continue;
        }
        for pin in component_pin_defs(component) {
            label_nodes
                .entry(label.clone())
                .or_default()
                .push(nodes.node_for(pin.pos));
        }
    }
    for nodes_with_label in label_nodes.values() {
        for pair in nodes_with_label.windows(2) {
            nets.union(pair[0], pair[1]);
        }
    }

    let mut ground_roots = HashSet::new();
    for component in components {
        if component.kind != ComponentKind::Ground {
            continue;
        }
        for pin in component_pin_defs(component) {
            let node = nodes.node_for(pin.pos);
            ground_roots.insert(nets.find(node));
        }
    }

    let mut named_roots = HashMap::new();
    for index in 0..nodes.positions.len() {
        let root = nets.find(index);
        if ground_roots.contains(&root) {
            named_roots.insert(root, "0".to_string());
        }
    }
    for component in components {
        if component.kind != ComponentKind::NetLabel {
            continue;
        }
        let Some(name) = spice_node_name(&component.value) else {
            continue;
        };
        for pin in component_pin_defs(component) {
            let root = nets.find(nodes.node_for(pin.pos));
            if !ground_roots.contains(&root) {
                named_roots.entry(root).or_insert_with(|| name.clone());
            }
        }
    }

    let mut roots = (0..nodes.positions.len())
        .map(|index| nets.find(index))
        .collect::<Vec<_>>();
    roots.sort_unstable();
    roots.dedup();

    let mut next_net = 1;
    for root in roots {
        named_roots.entry(root).or_insert_with(|| {
            let name = format!("N{next_net:03}");
            next_net += 1;
            name
        });
    }

    let mut net_name = |pos: Pos2| {
        let node = nodes.node_for(pos);
        let root = nets.find(node);
        named_roots
            .entry(root)
            .or_insert_with(|| {
                let name = format!("N{next_net:03}");
                next_net += 1;
                name
            })
            .clone()
    };

    let mut used_names = HashSet::new();
    let mut lines = Vec::new();
    let mut skipped = Vec::new();
    let mut uses_diode_model = false;
    let mut uses_led_model = false;
    let mut uses_zener_model = false;
    let mut uses_npn_model = false;
    let mut uses_pnp_model = false;
    let mut uses_nmos_model = false;
    let mut uses_pmos_model = false;

    for component in components {
        let pins = component_pin_defs(component);
        let line = match component.kind {
            ComponentKind::Resistor
            | ComponentKind::Capacitor
            | ComponentKind::Inductor
            | ComponentKind::Diode
            | ComponentKind::ZenerDiode
            | ComponentKind::Led
            | ComponentKind::VSource
            | ComponentKind::ISource
            | ComponentKind::Battery
            | ComponentKind::Fuse
            | ComponentKind::Potentiometer => {
                let Some((a, b)) = spice_two_pin_nets(component, &pins, &mut net_name) else {
                    skipped.push(format!("* skipped {}: missing pins", component.label));
                    continue;
                };
                match component.kind {
                    ComponentKind::Resistor | ComponentKind::Potentiometer => Some(format!(
                        "{} {a} {b} {}",
                        unique_spice_name("R", &component.label, component.id, &mut used_names),
                        spice_value(component, "1k")
                    )),
                    ComponentKind::Fuse => Some(format!(
                        "{} {a} {b} 0.1",
                        unique_spice_name("R", &component.label, component.id, &mut used_names),
                    )),
                    ComponentKind::Capacitor => Some(format!(
                        "{} {a} {b} {}",
                        unique_spice_name("C", &component.label, component.id, &mut used_names),
                        spice_value(component, "100n")
                    )),
                    ComponentKind::Inductor => Some(format!(
                        "{} {a} {b} {}",
                        unique_spice_name("L", &component.label, component.id, &mut used_names),
                        spice_value(component, "10u")
                    )),
                    ComponentKind::VSource | ComponentKind::Battery => Some(format!(
                        "{} {a} {b} DC {}",
                        unique_spice_name("V", &component.label, component.id, &mut used_names),
                        spice_value(component, "5")
                    )),
                    ComponentKind::ISource => Some(format!(
                        "{} {a} {b} DC {}",
                        unique_spice_name("I", &component.label, component.id, &mut used_names),
                        spice_value(component, "1m")
                    )),
                    ComponentKind::Diode => {
                        uses_diode_model = true;
                        Some(format!(
                            "{} {a} {b} DGEN",
                            unique_spice_name("D", &component.label, component.id, &mut used_names)
                        ))
                    }
                    ComponentKind::ZenerDiode => {
                        uses_zener_model = true;
                        Some(format!(
                            "{} {a} {b} DZEN",
                            unique_spice_name("D", &component.label, component.id, &mut used_names)
                        ))
                    }
                    ComponentKind::Led => {
                        uses_led_model = true;
                        Some(format!(
                            "{} {a} {b} LEDGEN",
                            unique_spice_name("D", &component.label, component.id, &mut used_names)
                        ))
                    }
                    _ => None,
                }
            }
            ComponentKind::NpnTransistor => {
                let result = (|| -> Option<String> {
                    let c_pin = pins.iter().find(|p| p.label == "C")?;
                    let b_pin = pins.iter().find(|p| p.label == "B")?;
                    let e_pin = pins.iter().find(|p| p.label == "E")?;
                    let c_net = net_name(c_pin.pos);
                    let b_net = net_name(b_pin.pos);
                    let e_net = net_name(e_pin.pos);
                    Some(format!(
                        "{} {c_net} {b_net} {e_net} NBJT",
                        unique_spice_name("Q", &component.label, component.id, &mut used_names)
                    ))
                })();
                uses_npn_model = result.is_some();
                result
            }
            ComponentKind::PnpTransistor => {
                let result = (|| -> Option<String> {
                    let c_pin = pins.iter().find(|p| p.label == "C")?;
                    let b_pin = pins.iter().find(|p| p.label == "B")?;
                    let e_pin = pins.iter().find(|p| p.label == "E")?;
                    let c_net = net_name(c_pin.pos);
                    let b_net = net_name(b_pin.pos);
                    let e_net = net_name(e_pin.pos);
                    Some(format!(
                        "{} {c_net} {b_net} {e_net} PBJT",
                        unique_spice_name("Q", &component.label, component.id, &mut used_names)
                    ))
                })();
                uses_pnp_model = result.is_some();
                result
            }
            ComponentKind::Nmosfet => {
                let result = (|| -> Option<String> {
                    let d_pin = pins.iter().find(|p| p.label == "D")?;
                    let g_pin = pins.iter().find(|p| p.label == "G")?;
                    let s_pin = pins.iter().find(|p| p.label == "S")?;
                    let d_net = net_name(d_pin.pos);
                    let g_net = net_name(g_pin.pos);
                    let s_net = net_name(s_pin.pos);
                    Some(format!(
                        "{} {d_net} {g_net} {s_net} {s_net} NMOS",
                        unique_spice_name("M", &component.label, component.id, &mut used_names)
                    ))
                })();
                uses_nmos_model = result.is_some();
                result
            }
            ComponentKind::Pmosfet => {
                let result = (|| -> Option<String> {
                    let d_pin = pins.iter().find(|p| p.label == "D")?;
                    let g_pin = pins.iter().find(|p| p.label == "G")?;
                    let s_pin = pins.iter().find(|p| p.label == "S")?;
                    let d_net = net_name(d_pin.pos);
                    let g_net = net_name(g_pin.pos);
                    let s_net = net_name(s_pin.pos);
                    Some(format!(
                        "{} {d_net} {g_net} {s_net} {s_net} PMOS",
                        unique_spice_name("M", &component.label, component.id, &mut used_names)
                    ))
                })();
                uses_pmos_model = result.is_some();
                result
            }
            ComponentKind::VoltageReg => (|| -> Option<String> {
                let in_pin = pins.iter().find(|p| p.label == "IN")?;
                let out_pin = pins.iter().find(|p| p.label == "OUT")?;
                let gnd_pin = pins.iter().find(|p| p.label == "GND")?;
                let in_net = net_name(in_pin.pos);
                let out_net = net_name(out_pin.pos);
                let gnd_net = net_name(gnd_pin.pos);
                Some(format!(
                    "* {} LM7805: IN={in_net} OUT={out_net} GND={gnd_net} (5V fixed)",
                    component.label
                ))
            })(),
            ComponentKind::LogicNot
            | ComponentKind::LogicAnd
            | ComponentKind::LogicOr
            | ComponentKind::LogicNand
            | ComponentKind::LogicNor
            | ComponentKind::LogicXor => {
                let kind_str = component_kind_label(component.kind);
                let nets: Vec<String> = pins.iter().map(|p| net_name(p.pos)).collect();
                let net_str = nets.join(" ");
                Some(format!("* {} {} [{}]", component.label, kind_str, net_str))
            }
            ComponentKind::Voltmeter => {
                let Some((a, b)) = spice_two_pin_nets(component, &pins, &mut net_name) else {
                    skipped.push(format!("* skipped {}: missing pins", component.label));
                    continue;
                };
                // Voltmeter = 1 GΩ resistor (ideal high impedance)
                Some(format!(
                    "{} {a} {b} 1G",
                    unique_spice_name("R", &component.label, component.id, &mut used_names)
                ))
            }
            ComponentKind::Ammeter => {
                let Some((a, b)) = spice_two_pin_nets(component, &pins, &mut net_name) else {
                    skipped.push(format!("* skipped {}: missing pins", component.label));
                    continue;
                };
                // Ammeter = 0 V source (ideal current sense)
                Some(format!(
                    "{} {a} {b} DC 0",
                    unique_spice_name("V", &component.label, component.id, &mut used_names)
                ))
            }
            ComponentKind::Ground => None,
            _ => {
                skipped.push(format!(
                    "* skipped {}: {} has no SPICE primitive yet",
                    component.label,
                    component_kind_label(component.kind)
                ));
                None
            }
        };
        if let Some(line) = line {
            lines.push(line);
        }
    }

    let mut output = String::new();
    output.push_str("* Cluster SPICE netlist\n");
    output.push_str("* Generated from the schematic connectivity graph.\n");
    if lines.is_empty() {
        output.push_str("* No supported SPICE primitives in this schematic.\n");
    } else {
        for line in lines {
            output.push_str(&line);
            output.push('\n');
        }
    }
    if uses_diode_model {
        output.push_str(".model DGEN D(Is=2n Rs=0.6 N=1.8)\n");
    }
    if uses_zener_model {
        output.push_str(".model DZEN D(Is=1e-14 Rs=0.5 N=1.0 BV=5.1 IBV=10m)\n");
    }
    if uses_led_model {
        output.push_str(".model LEDGEN D(Is=10n Rs=4 N=2.0 Eg=2.0)\n");
    }
    if uses_npn_model {
        output.push_str(".model NBJT NPN(Is=1e-14 Bf=200 Br=2 Cje=10p Cjc=5p)\n");
    }
    if uses_pnp_model {
        output.push_str(".model PBJT PNP(Is=1e-14 Bf=200 Br=2 Cje=10p Cjc=5p)\n");
    }
    if uses_nmos_model {
        output.push_str(".model NMOS NMOS(Level=1 Vto=2 Kp=200u W=10u L=1u)\n");
    }
    if uses_pmos_model {
        output.push_str(".model PMOS PMOS(Level=1 Vto=-2 Kp=80u W=20u L=1u)\n");
    }
    for line in skipped {
        output.push_str(&line);
        output.push('\n');
    }
    output.push_str(".op\n.end\n");
    output
}

fn circuit_to_netlist_text(netlist: &CircuitNetlist) -> String {
    let mut out = String::new();
    out.push_str("# Cluster netlist\n");
    out.push_str("# Format: Component.Pin -> NET_NAME\n\n");
    for net in &netlist.nets {
        out.push_str(&format!("{}:\n", net.name));
        let mut pins = netlist
            .pins
            .iter()
            .filter(|pin| pin.net_id == net.id)
            .collect::<Vec<_>>();
        pins.sort_by(|a, b| {
            a.component_label
                .cmp(&b.component_label)
                .then_with(|| a.pin_name.cmp(&b.pin_name))
        });
        if pins.is_empty() {
            out.push_str("  (no connected pins)\n");
        } else {
            for pin in pins {
                out.push_str(&format!(
                    "  {}.{} [{:?}] @ ({:.0}, {:.0})\n",
                    pin.component_label,
                    pin.pin_name,
                    pin.electrical_type,
                    pin.position.x,
                    pin.position.y
                ));
            }
        }
        out.push('\n');
    }
    if !netlist.floating_wires.is_empty() {
        out.push_str("Floating wires:\n");
        for wire_id in &netlist.floating_wires {
            let net_name = netlist
                .wire_nets
                .get(wire_id)
                .and_then(|id| netlist.nets.iter().find(|net| net.id == *id))
                .map(|net| net.name.as_str())
                .unwrap_or("UNKNOWN");
            out.push_str(&format!("  Wire {wire_id} -> {net_name}\n"));
        }
    }
    out
}

fn generate_arduino_code(netlist: &CircuitNetlist) -> String {
    let has_oled = netlist
        .pins
        .iter()
        .any(|p| p.component_kind == ComponentKind::Oled);
    let has_button = netlist
        .pins
        .iter()
        .any(|p| p.component_kind == ComponentKind::PushButton);
    let has_led = netlist
        .pins
        .iter()
        .any(|p| p.component_kind == ComponentKind::Led);
    let controller_kind = netlist.pins.iter().find_map(|pin| {
        matches!(
            pin.component_kind,
            ComponentKind::Esp32
                | ComponentKind::Esp32S3
                | ComponentKind::Esp32C3
                | ComponentKind::ArduinoUno
                | ComponentKind::RaspberryPiPico
        )
        .then_some(pin.component_kind)
    });

    let mut i2c_sda = "21".to_string();
    let mut i2c_scl = "22".to_string();

    // GPIO nets: map pin_name → (connected_to_button, connected_to_led)
    let mut button_gpio: Option<String> = None;
    let mut led_gpio: Option<String> = None;

    for net in &netlist.nets {
        let pins: Vec<&NetlistPin> = netlist.pins.iter().filter(|p| p.net_id == net.id).collect();

        if pins
            .iter()
            .any(|p| p.component_kind == ComponentKind::Oled && p.pin_name == "SDA")
            && let Some(ctrl) = pins.iter().find(|p| pin_is_controller_sda(p))
        {
            i2c_sda = digits_from_pin_name(&ctrl.pin_name).unwrap_or_else(|| i2c_sda.clone());
        }
        if pins
            .iter()
            .any(|p| p.component_kind == ComponentKind::Oled && p.pin_name == "SCL")
            && let Some(ctrl) = pins.iter().find(|p| pin_is_controller_scl(p))
        {
            i2c_scl = digits_from_pin_name(&ctrl.pin_name).unwrap_or_else(|| i2c_scl.clone());
        }

        // Detect which GPIO is connected to the button and which to the LED
        let has_button_on_net = pins
            .iter()
            .any(|p| p.component_kind == ComponentKind::PushButton);
        let has_led_on_net = pins.iter().any(|p| p.component_kind == ComponentKind::Led);
        if let Some(gpio_pin) = pins.iter().find(|p| pin_is_microcontroller_gpio(p)) {
            if let Some(gpio_num) = digits_from_pin_name(&gpio_pin.pin_name) {
                if has_button_on_net && button_gpio.is_none() {
                    button_gpio = Some(gpio_num.clone());
                }
                if (has_led_on_net || net_drives_led_through_series_part(netlist, net.id))
                    && led_gpio.is_none()
                {
                    led_gpio = Some(gpio_num);
                }
            }
        }
    }

    let mut gpio_pins: Vec<(String, String)> = netlist
        .pins
        .iter()
        .filter(|p| {
            p.connected_by_wire && pin_is_microcontroller_gpio(p) && !pin_is_i2c_named(&p.pin_name)
        })
        .filter_map(|p| digits_from_pin_name(&p.pin_name).map(|g| (p.pin_name.clone(), g)))
        .collect();
    gpio_pins.sort();
    gpio_pins.dedup();

    let mut code = String::new();
    code.push_str("// Generated by Cluster\n");
    code.push_str("#include <Arduino.h>\n");
    if has_oled {
        code.push_str("#include <Wire.h>\n");
        code.push_str("#include <Adafruit_GFX.h>\n");
        code.push_str("#include <Adafruit_SSD1306.h>\n\n");
        code.push_str("#define SCREEN_WIDTH 128\n#define SCREEN_HEIGHT 64\n");
        code.push_str("Adafruit_SSD1306 display(SCREEN_WIDTH, SCREEN_HEIGHT, &Wire, -1);\n");
    }
    code.push('\n');

    // Pin constants
    for (name, gpio) in &gpio_pins {
        code.push_str(&format!(
            "const int PIN_{} = {};\n",
            sanitize_code_ident(name),
            gpio
        ));
    }

    // Button-toggle pattern: extra state variable
    if has_button && has_led {
        if let (Some(btn), Some(led)) = (&button_gpio, &led_gpio) {
            code.push_str(&format!("\nconst int BUTTON_PIN = {btn};\n"));
            code.push_str(&format!("const int LED_PIN    = {led};\n"));
            code.push_str("const unsigned long DEBOUNCE_MS = 50;\n");
            code.push_str("\nbool ledState = false;\n");
            code.push_str("int lastReading = HIGH;\n");
            code.push_str("int stableState = HIGH;\n");
            code.push_str("unsigned long lastDebounceTime = 0;\n");

            code.push_str("\nvoid setup() {\n  Serial.begin(115200);\n");
            code.push_str("  pinMode(BUTTON_PIN, INPUT_PULLUP);  // active-low button\n");
            code.push_str("  pinMode(LED_PIN, OUTPUT);\n");
            code.push_str("  digitalWrite(LED_PIN, LOW);\n");
            code.push_str("}\n\nvoid loop() {\n");
            code.push_str("  int reading = digitalRead(BUTTON_PIN);\n");
            code.push_str("  if (reading != lastReading) {\n");
            code.push_str("    lastDebounceTime = millis();\n");
            code.push_str("    lastReading = reading;\n");
            code.push_str("  }\n\n");
            code.push_str(
                "  if ((millis() - lastDebounceTime) > DEBOUNCE_MS && reading != stableState) {\n",
            );
            code.push_str("    stableState = reading;\n");
            code.push_str("    if (stableState == LOW) {  // pressed with INPUT_PULLUP\n");
            code.push_str("      ledState = !ledState;\n");
            code.push_str("      digitalWrite(LED_PIN, ledState ? HIGH : LOW);\n");
            code.push_str("      Serial.println(ledState ? \"LED ON\" : \"LED OFF\");\n");
            code.push_str("    }\n");
            code.push_str("  }\n");
            code.push_str("  delay(1);\n");
            code.push_str("}\n");
            return code;
        }
    }

    // OLED setup
    code.push_str("\nvoid setup() {\n  Serial.begin(115200);\n");
    if has_oled {
        if controller_kind == Some(ComponentKind::ArduinoUno) {
            code.push_str("  Wire.begin();  // UNO uses A4 SDA and A5 SCL\n");
        } else {
            code.push_str(&format!("  Wire.begin({i2c_sda}, {i2c_scl});\n"));
        }
        code.push_str("  if (!display.begin(SSD1306_SWITCHCAPVCC, 0x3C)) {\n");
        code.push_str(
            "    Serial.println(\"OLED init failed\");\n    while (true) delay(100);\n  }\n",
        );
        code.push_str("  display.clearDisplay();\n  display.setTextSize(1);\n  display.setTextColor(SSD1306_WHITE);\n  display.setCursor(0, 0);\n  display.println(\"Cluster ready\");\n  display.display();\n");
    }
    for (name, _) in &gpio_pins {
        code.push_str(&format!(
            "  pinMode(PIN_{}, OUTPUT);\n",
            sanitize_code_ident(name)
        ));
    }
    code.push_str("}\n\nvoid loop() {\n");
    if gpio_pins.is_empty() {
        code.push_str("  delay(1000);\n");
    } else {
        for (name, _) in &gpio_pins {
            let id = sanitize_code_ident(name);
            code.push_str(&format!("  digitalWrite(PIN_{id}, HIGH);\n"));
        }
        code.push_str("  delay(500);\n");
        for (name, _) in &gpio_pins {
            let id = sanitize_code_ident(name);
            code.push_str(&format!("  digitalWrite(PIN_{id}, LOW);\n"));
        }
        code.push_str("  delay(500);\n");
    }
    code.push_str("}\n");
    code
}

fn digits_from_pin_name(name: &str) -> Option<String> {
    let digits = name
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    (!digits.is_empty()).then_some(digits)
}

fn net_drives_led_through_series_part(netlist: &CircuitNetlist, net_id: usize) -> bool {
    let series_parts = netlist
        .pins
        .iter()
        .filter(|pin| pin.net_id == net_id)
        .filter(|pin| {
            matches!(
                pin.component_kind,
                ComponentKind::Resistor | ComponentKind::Ammeter | ComponentKind::Fuse
            )
        });

    for part_pin in series_parts {
        let output_net_ids = netlist
            .pins
            .iter()
            .filter(|pin| {
                pin.component_id == part_pin.component_id
                    && pin.pin_name != part_pin.pin_name
                    && pin.net_id != net_id
            })
            .map(|pin| pin.net_id);

        for output_net_id in output_net_ids {
            if netlist
                .pins
                .iter()
                .any(|pin| pin.net_id == output_net_id && pin.component_kind == ComponentKind::Led)
            {
                return true;
            }
        }
    }

    false
}

fn sanitize_code_ident(name: &str) -> String {
    let ident = name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_ascii_uppercase();
    if ident.is_empty() {
        "GPIO".to_string()
    } else {
        ident
    }
}

fn spice_two_pin_nets(
    component: &Component,
    pins: &[CircuitPin],
    net_name: &mut impl FnMut(Pos2) -> String,
) -> Option<(String, String)> {
    match component.kind {
        ComponentKind::VSource
        | ComponentKind::Battery
        | ComponentKind::ISource
        | ComponentKind::DcMotor => {
            let positive = pins.iter().find(|pin| pin.role == PinRole::Positive)?;
            let negative = pins.iter().find(|pin| pin.role == PinRole::Ground)?;
            Some((net_name(positive.pos), net_name(negative.pos)))
        }
        _ => {
            let a = pins.first()?;
            let b = pins.get(1)?;
            Some((net_name(a.pos), net_name(b.pos)))
        }
    }
}

fn unique_spice_name(prefix: &str, label: &str, id: u64, used: &mut HashSet<String>) -> String {
    let mut name = label
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    if name.is_empty() {
        name = format!("{prefix}{id}");
    }
    if !name
        .chars()
        .next()
        .is_some_and(|ch| ch.eq_ignore_ascii_case(&prefix.chars().next().unwrap_or('X')))
    {
        name = format!("{prefix}{name}");
    }
    if used.insert(name.clone()) {
        return name;
    }
    let with_id = format!("{name}_{id}");
    used.insert(with_id.clone());
    with_id
}

fn spice_value(component: &Component, fallback: &str) -> String {
    let normalized = component.value.trim().replace(' ', "");
    if normalized.is_empty() {
        return fallback.to_string();
    }
    let lower = normalized.to_lowercase();
    let stripped = match component.kind {
        ComponentKind::Resistor => lower.strip_suffix("ohm").unwrap_or(&normalized),
        ComponentKind::Capacitor => lower.strip_suffix('f').unwrap_or(&normalized),
        ComponentKind::Inductor => lower.strip_suffix('h').unwrap_or(&normalized),
        ComponentKind::VSource | ComponentKind::Battery => lower
            .strip_suffix("volts")
            .or_else(|| lower.strip_suffix("volt"))
            .or_else(|| lower.strip_suffix('v'))
            .unwrap_or(&normalized),
        ComponentKind::ISource => lower
            .strip_suffix("amps")
            .or_else(|| lower.strip_suffix("amp"))
            .or_else(|| lower.strip_suffix('a'))
            .unwrap_or(&normalized),
        _ => &normalized,
    };
    if stripped.trim().is_empty() {
        fallback.to_string()
    } else {
        stripped.to_string()
    }
}

fn spice_node_name(value: &str) -> Option<String> {
    let mut name = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    while name.contains("__") {
        name = name.replace("__", "_");
    }
    name = name.trim_matches('_').to_string();
    if name.is_empty() {
        return None;
    }
    if name.starts_with(|ch: char| ch.is_ascii_digit()) {
        name.insert_str(0, "N_");
    }
    Some(name)
}

fn circuit_to_svg(components: &[Component], wires: &[Wire]) -> String {
    let bounds = circuit_bounds(components, wires)
        .unwrap_or_else(|| Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(960.0, 640.0)));
    let margin = 40.0;
    let min_x = bounds.left() - margin;
    let min_y = bounds.top() - margin;
    let width = (bounds.width() + margin * 2.0).max(480.0);
    let height = (bounds.height() + margin * 2.0).max(320.0);
    let simulation = analyze_circuit(components, wires);

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{:.1} {:.1} {:.1} {:.1}" width="{:.1}" height="{:.1}">
<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="#101216"/>
<g fill="none" stroke-linecap="round" stroke-linejoin="round">
"##,
        min_x, min_y, width, height, width, height, min_x, min_y, width, height
    ));

    for wire in wires {
        if wire.points.len() < 2 {
            continue;
        }
        let color = if simulation.energized_wires.contains(&wire.id) {
            "#ffaa37"
        } else {
            "#69b2ff"
        };
        let points = wire
            .points
            .iter()
            .map(|p| format!("{:.1},{:.1}", p.x, p.y))
            .collect::<Vec<_>>()
            .join(" ");
        svg.push_str(&format!(
            r##"<polyline points="{}" stroke="{}" stroke-width="2.4"/>"##,
            points, color
        ));
        svg.push('\n');
    }

    for component in components {
        let rect = component_bounds(component);
        let energized = simulation.energized_components.contains(&component.id);
        let stroke = if energized { "#ffb950" } else { "#dee2e8" };
        let fill = if component_is_module(component) {
            if energized { "#3e2e16" } else { "#181e26" }
        } else {
            "none"
        };
        svg.push_str(&format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="4" fill="{}" stroke="{}" stroke-width="2"/>"##,
            rect.left(),
            rect.top(),
            rect.width(),
            rect.height(),
            fill,
            stroke
        ));
        svg.push('\n');
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" fill="{}" font-family="Arial, sans-serif" font-size="12" text-anchor="middle">{}</text>"##,
            rect.center().x,
            rect.center().y - 2.0,
            stroke,
            escape_xml(component_kind_label(component.kind))
        ));
        svg.push('\n');
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" fill="#e1e4e8" font-family="Arial, sans-serif" font-size="11" text-anchor="middle">{}</text>"##,
            rect.center().x,
            rect.bottom() + 15.0,
            escape_xml(&component.label)
        ));
        svg.push('\n');
        if !component.value.trim().is_empty() {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" fill="#9aa4ae" font-family="Arial, sans-serif" font-size="10" text-anchor="middle">{}</text>"##,
                rect.center().x,
                rect.top() - 7.0,
                escape_xml(&component.value)
            ));
            svg.push('\n');
        }
        for pin in component_pins(component) {
            svg.push_str(&format!(
                r##"<circle cx="{:.1}" cy="{:.1}" r="3.2" fill="#facd5f" stroke="#281f14" stroke-width="1"/>"##,
                pin.x, pin.y
            ));
            svg.push('\n');
        }
    }

    svg.push_str("</g>\n</svg>\n");
    svg
}

fn circuit_to_bom_csv(pages: &[(String, Vec<Component>, Vec<Wire>, u64, Counters)]) -> String {
    let mut rows = pages
        .iter()
        .flat_map(|(page_name, components, _, _, _)| {
            components
                .iter()
                .filter(|component| component.kind != ComponentKind::Ground)
                .map(move |component| {
                    (
                        page_name.as_str(),
                        component.label.as_str(),
                        component_kind_label(component.kind),
                        component.value.as_str(),
                    )
                })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        a.0.cmp(b.0)
            .then_with(|| naturalish_label_key(a.1).cmp(&naturalish_label_key(b.1)))
            .then_with(|| a.1.cmp(b.1))
    });

    let mut lines = vec!["Page,Label,Kind,Value".to_string()];
    for (page, label, kind, value) in rows {
        lines.push(format!(
            "{},{},{},{}",
            csv_cell(page),
            csv_cell(label),
            csv_cell(kind),
            csv_cell(value)
        ));
    }
    lines.join("\n") + "\n"
}

fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn naturalish_label_key(label: &str) -> (String, u32) {
    let split_at = label
        .char_indices()
        .find(|(_, ch)| ch.is_ascii_digit())
        .map(|(idx, _)| idx)
        .unwrap_or(label.len());
    let prefix = label[..split_at].to_ascii_uppercase();
    let number = label[split_at..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse::<u32>()
        .unwrap_or(u32::MAX);
    (prefix, number)
}

fn circuit_bounds(components: &[Component], wires: &[Wire]) -> Option<Rect> {
    let mut min = Pos2::new(f32::INFINITY, f32::INFINITY);
    let mut max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut has_content = false;

    for component in components {
        let rect = component_bounds(component);
        min.x = min.x.min(rect.left());
        min.y = min.y.min(rect.top());
        max.x = max.x.max(rect.right());
        max.y = max.y.max(rect.bottom());
        has_content = true;
    }

    for wire in wires {
        for point in &wire.points {
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
            has_content = true;
        }
    }

    has_content.then(|| Rect::from_min_max(min, max))
}

fn component_kind_label(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Resistor => "Resistor",
        ComponentKind::Capacitor => "Capacitor",
        ComponentKind::Inductor => "Inductor",
        ComponentKind::Diode => "Diode",
        ComponentKind::Led => "LED",
        ComponentKind::ZenerDiode => "Zener",
        ComponentKind::Switch => "Switch",
        ComponentKind::PushButton => "Push Button",
        ComponentKind::SlideSwitch => "Slide Switch",
        ComponentKind::Ground => "Ground",
        ComponentKind::VSource => "V Source",
        ComponentKind::ISource => "I Source",
        ComponentKind::Battery => "Battery",
        ComponentKind::OpAmp => "Op Amp",
        ComponentKind::Lamp => "Lamp",
        ComponentKind::Potentiometer => "Potentiometer",
        ComponentKind::NpnTransistor => "NPN BJT",
        ComponentKind::PnpTransistor => "PNP BJT",
        ComponentKind::Nmosfet => "N-MOSFET",
        ComponentKind::Pmosfet => "P-MOSFET",
        ComponentKind::VoltageReg => "Voltage Reg",
        ComponentKind::Fuse => "Fuse",
        ComponentKind::LogicNot => "NOT Gate",
        ComponentKind::LogicAnd => "AND Gate",
        ComponentKind::LogicOr => "OR Gate",
        ComponentKind::LogicNand => "NAND Gate",
        ComponentKind::LogicNor => "NOR Gate",
        ComponentKind::LogicXor => "XOR Gate",
        ComponentKind::Esp32 => "ESP32 WROOM",
        ComponentKind::Esp32S3 => "ESP32-S3",
        ComponentKind::Esp32C3 => "ESP32-C3",
        ComponentKind::ArduinoUno => "Arduino UNO",
        ComponentKind::RaspberryPiPico => "Pi Pico",
        ComponentKind::Stm32BluePill => "STM32 Blue Pill",
        ComponentKind::Stm32Nucleo64 => "STM32 Nucleo-64",
        ComponentKind::Breadboard => "Breadboard",
        ComponentKind::Relay => "Relay",
        ComponentKind::DcMotor => "DC Motor",
        ComponentKind::Servo => "Servo",
        ComponentKind::Oled => "OLED I2C",
        ComponentKind::Sensor => "Sensor",
        ComponentKind::NetLabel => "Net Label",
        ComponentKind::Timer555 => "555 Timer",
        ComponentKind::Crystal => "Crystal",
        ComponentKind::Transformer => "Transformer",
        ComponentKind::Display7Seg => "7-Seg Display",
        ComponentKind::Thermistor => "Thermistor",
        ComponentKind::Varistor => "Varistor",
        ComponentKind::VoltageRef => "Voltage Ref",
        ComponentKind::MotorDriver => "Motor Driver",
        ComponentKind::SchottkyDiode => "Schottky",
        ComponentKind::TvsDiode => "TVS Diode",
        ComponentKind::Phototransistor => "Phototransistor",
        ComponentKind::Optocoupler => "Optocoupler",
        ComponentKind::GenericIc => "Generic IC",
        ComponentKind::Voltmeter => "Voltmeter",
        ComponentKind::Ammeter => "Ammeter",
        ComponentKind::TextNote => "Text Note",
        ComponentKind::Dht11 => "DHT11",
        ComponentKind::Dht22 => "DHT22",
        ComponentKind::HcSr04 => "HC-SR04",
        ComponentKind::Buzzer => "Buzzer",
        ComponentKind::NeoPixel => "NeoPixel",
        ComponentKind::PirSensor => "PIR Sensor",
    }
}

fn component_is_module(component: &Component) -> bool {
    matches!(
        component.kind,
        ComponentKind::Esp32
            | ComponentKind::Esp32S3
            | ComponentKind::Esp32C3
            | ComponentKind::ArduinoUno
            | ComponentKind::RaspberryPiPico
            | ComponentKind::Oled
            | ComponentKind::Sensor
            | ComponentKind::Timer555
            | ComponentKind::Display7Seg
            | ComponentKind::MotorDriver
            | ComponentKind::Optocoupler
            | ComponentKind::GenericIc
            | ComponentKind::Dht11
            | ComponentKind::Dht22
            | ComponentKind::HcSr04
            | ComponentKind::NeoPixel
            | ComponentKind::PirSensor
    )
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ─── New sensor drawing functions ────────────────────────────────────────────

fn draw_sensor_module(
    painter: &egui::Painter,
    rect: Rect,
    stroke: Stroke,
    energized: bool,
    label: &str,
    accent: Color32,
) {
    let center = rect.center();
    let body_fill = if energized {
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 80)
    } else {
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 35)
    };
    painter.rect_filled(rect, 6.0, body_fill);
    painter.rect_stroke(rect, 6.0, stroke, StrokeKind::Middle);
    painter.text(
        center,
        Align2::CENTER_CENTER,
        label,
        egui::FontId::monospace(10.0),
        if energized {
            Color32::from_rgb(255, 230, 150)
        } else {
            Color32::from_rgb(200, 215, 230)
        },
    );
}

fn draw_hcsr04(painter: &egui::Painter, rect: Rect, stroke: Stroke, energized: bool) {
    let center = rect.center();
    let fill = if energized {
        Color32::from_rgba_unmultiplied(80, 180, 255, 70)
    } else {
        Color32::from_rgba_unmultiplied(60, 120, 200, 35)
    };
    painter.rect_filled(rect, 4.0, fill);
    painter.rect_stroke(rect, 4.0, stroke, StrokeKind::Middle);
    // Two transducer circles
    let r = rect.height() * 0.28;
    let left_cx = rect.center().x - rect.width() * 0.22;
    let right_cx = rect.center().x + rect.width() * 0.22;
    painter.circle_stroke(Pos2::new(left_cx, center.y), r, stroke);
    painter.circle_stroke(Pos2::new(right_cx, center.y), r, stroke);
    painter.text(
        Pos2::new(center.x, rect.bottom() - 8.0),
        Align2::CENTER_CENTER,
        "HC-SR04",
        egui::FontId::monospace(8.0),
        if energized {
            Color32::from_rgb(160, 220, 255)
        } else {
            Color32::from_rgb(150, 170, 200)
        },
    );
}

fn draw_buzzer(
    painter: &egui::Painter,
    rect: Rect,
    _rotation: i32,
    stroke: Stroke,
    energized: bool,
) {
    let center = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    let fill = if energized {
        Color32::from_rgba_unmultiplied(255, 200, 50, 80)
    } else {
        Color32::from_rgba_unmultiplied(180, 160, 50, 35)
    };
    painter.circle_filled(center, r, fill);
    painter.circle_stroke(center, r, stroke);
    // Sound wave arcs
    let wave_col = if energized {
        stroke.color
    } else {
        Color32::from_rgb(130, 140, 150)
    };
    for i in 1..=2u32 {
        let arc_r = r + i as f32 * 6.0;
        painter.circle_stroke(center, arc_r, Stroke::new(stroke.width * 0.6, wave_col));
    }
    // Plus/minus
    painter.text(
        Pos2::new(center.x, center.y),
        Align2::CENTER_CENTER,
        "BZ",
        egui::FontId::monospace(9.0),
        if energized {
            Color32::from_rgb(255, 230, 100)
        } else {
            Color32::from_rgb(180, 190, 200)
        },
    );
    // Terminal lines left/right
    painter.line_segment(
        [
            Pos2::new(rect.left(), center.y),
            Pos2::new(center.x - r, center.y),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(rect.right(), center.y),
            Pos2::new(center.x + r, center.y),
        ],
        stroke,
    );
}

fn draw_neopixel(painter: &egui::Painter, rect: Rect, stroke: Stroke, energized: bool) {
    let center = rect.center();
    let inner_fill = if energized {
        Color32::from_rgb(255, 80, 200)
    } else {
        Color32::from_rgba_unmultiplied(80, 50, 100, 60)
    };
    painter.rect_filled(rect, 8.0, Color32::from_rgba_unmultiplied(30, 20, 50, 120));
    painter.rect_stroke(rect, 8.0, stroke, StrokeKind::Middle);
    // Inner LED square
    let inner = Rect::from_center_size(center, Vec2::splat(rect.width().min(rect.height()) * 0.45));
    painter.rect_filled(inner, 4.0, inner_fill);
    painter.text(
        Pos2::new(center.x, rect.bottom() - 7.0),
        Align2::CENTER_CENTER,
        "NP",
        egui::FontId::monospace(8.0),
        if energized {
            Color32::from_rgb(255, 180, 255)
        } else {
            Color32::from_rgb(160, 140, 180)
        },
    );
}

/// Minimal dependency-free PNG encoder (RGBA8).
/// Uses zlib's `deflate` via the `miniz_oxide` crate which is a transitive
/// dependency of eframe, so no new dependency is required.
fn write_png(path: &str, width: usize, height: usize, rgba: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    fn adler32(data: &[u8]) -> u32 {
        let (mut s1, mut s2) = (1u32, 0u32);
        for &b in data {
            s1 = (s1 + b as u32) % 65521;
            s2 = (s2 + s1) % 65521;
        }
        (s2 << 16) | s1
    }
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }
    fn write_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
        let len = data.len() as u32;
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(tag);
        out.extend_from_slice(data);
        let mut crc_data = Vec::with_capacity(4 + data.len());
        crc_data.extend_from_slice(tag);
        crc_data.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_data).to_be_bytes());
    }

    // Build raw scanlines with filter byte 0 (None)
    let mut raw: Vec<u8> = Vec::with_capacity(height * (1 + width * 4));
    for y in 0..height {
        raw.push(0); // filter type None
        raw.extend_from_slice(&rgba[y * width * 4..(y + 1) * width * 4]);
    }

    // Store uncompressed via zlib non-compressed block (no extra deps)
    let mut zlib: Vec<u8> = Vec::new();
    zlib.push(0x78); // CMF: deflate, window=32KB
    zlib.push(0x01); // FLG: no dict, check bits
    // Non-compressed deflate blocks (BFINAL=1, BTYPE=00)
    let mut pos = 0usize;
    while pos < raw.len() {
        let block_len = (raw.len() - pos).min(65535) as u16;
        let last = (pos + block_len as usize) >= raw.len();
        zlib.push(last as u8); // BFINAL + BTYPE=00
        zlib.extend_from_slice(&block_len.to_le_bytes());
        zlib.extend_from_slice(&(!block_len).to_le_bytes());
        zlib.extend_from_slice(&raw[pos..pos + block_len as usize]);
        pos += block_len as usize;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out: Vec<u8> = Vec::new();
    // PNG signature
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    // IHDR
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // colour type RGBA
    ihdr.extend_from_slice(&[0, 0, 0]); // compression, filter, interlace
    write_chunk(&mut out, b"IHDR", &ihdr);
    // IDAT
    write_chunk(&mut out, b"IDAT", &zlib);
    // IEND
    write_chunk(&mut out, b"IEND", &[]);

    let mut f = std::fs::File::create(path)?;
    f.write_all(&out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spice_export_names_connected_nets_and_ground() {
        let mut app = CircuitApp::new();
        app.load_led_demo();

        let netlist = circuit_to_spice_netlist(&app.components, &app.wires);

        assert!(netlist.contains("VBAT1"));
        assert!(netlist.contains("R1"));
        assert!(netlist.contains("DLED1"));
        assert!(netlist.contains(" 0 "));
        assert!(netlist.contains(".model LEDGEN"));
        assert!(netlist.ends_with(".op\n.end\n"));
    }

    #[test]
    fn spice_export_reports_empty_schematic_without_panicking() {
        let netlist = circuit_to_spice_netlist(&[], &[]);

        assert!(netlist.contains("No supported SPICE primitives"));
        assert!(netlist.contains(".end"));
    }

    #[test]
    fn spice_export_uses_sanitized_net_label_value() {
        let mut app = CircuitApp::new();
        let resistor = app.place_component(ComponentKind::Resistor, Pos2::new(300.0, 200.0));
        let label = app.place_component(ComponentKind::NetLabel, Pos2::new(120.0, 200.0));
        app.components
            .iter_mut()
            .find(|component| component.id == label)
            .unwrap()
            .value = "SENSE 3V3".to_string();
        app.add_wire_between(label, "B", resistor, "A");

        let netlist = circuit_to_spice_netlist(&app.components, &app.wires);
        assert!(netlist.contains("SENSE_3V3"), "{netlist}");
    }

    #[test]
    fn circuit_netlist_maps_connected_component_pins() {
        let mut app = CircuitApp::new();
        app.load_led_demo();

        let netlist = build_circuit_netlist(&app.components, &app.wires);
        let r1_b = netlist
            .pins
            .iter()
            .find(|pin| pin.component_label == "R1" && pin.pin_name == "B")
            .unwrap();
        let led_a = netlist
            .pins
            .iter()
            .find(|pin| pin.component_label == "LED1" && pin.pin_name == "A")
            .unwrap();

        assert_eq!(r1_b.net_id, led_a.net_id);
        assert!(netlist.nets.iter().any(|net| net.name == "GND"));
        assert!(circuit_to_netlist_text(&netlist).contains("R1.B"));
    }

    #[test]
    fn beginner_validation_catches_led_without_resistor() {
        let mut app = CircuitApp::new();
        let battery = app.place_component(ComponentKind::Battery, Pos2::new(180.0, 300.0));
        let led = app.place_component(ComponentKind::Led, Pos2::new(380.0, 300.0));
        let ground = app.place_component(ComponentKind::Ground, Pos2::new(580.0, 360.0));
        app.add_wire_between(battery, "+", led, "A");
        app.add_wire_between(led, "B", ground, "GND");
        app.add_wire_between(battery, "-", ground, "GND");

        let sim = analyze_circuit(&app.components, &app.wires);
        let erc = run_erc(&app.components, &app.wires, &sim);

        assert!(erc.iter().any(|violation| {
            violation.component_id == Some(led)
                && violation.message.contains("current limiting resistor")
        }));
    }

    #[test]
    fn beginner_validation_catches_5v_to_esp32_3v3() {
        let mut app = CircuitApp::new();
        let arduino = app.place_component(ComponentKind::ArduinoUno, Pos2::new(180.0, 300.0));
        let esp32 = app.place_component(ComponentKind::Esp32, Pos2::new(480.0, 300.0));
        app.add_wire_between(arduino, "5V", esp32, "3V3");

        let sim = analyze_circuit(&app.components, &app.wires);
        let erc = run_erc(&app.components, &app.wires, &sim);

        assert!(erc.iter().any(|violation| {
            violation.severity == ErcSeverity::Error && violation.message.contains("5V")
        }));
    }

    #[test]
    fn beginner_validation_catches_gpio_driving_motor_directly() {
        let mut app = CircuitApp::new();
        let esp32 = app.place_component(ComponentKind::Esp32, Pos2::new(180.0, 300.0));
        let motor = app.place_component(ComponentKind::DcMotor, Pos2::new(480.0, 300.0));
        app.add_wire_between(esp32, "GPIO18", motor, "+");

        let sim = analyze_circuit(&app.components, &app.wires);
        let erc = run_erc(&app.components, &app.wires, &sim);

        assert!(erc.iter().any(|violation| {
            violation.severity == ErcSeverity::Error && violation.message.contains("motor")
        }));
    }

    #[test]
    fn beginner_validation_warns_relay_without_flyback_diode() {
        let mut app = CircuitApp::new();
        app.load_motor_relay_demo();
        let simulation = app.current_simulation();
        assert!(
            simulation
                .erc
                .iter()
                .any(|violation| { violation.message.contains("flyback diode") })
        );
    }

    #[test]
    fn beginner_validation_warns_i2c_without_pullups() {
        let mut app = CircuitApp::new();
        let esp32 = app.place_component(ComponentKind::Esp32, Pos2::new(300.0, 300.0));
        let oled = app.place_component(ComponentKind::Oled, Pos2::new(600.0, 300.0));
        app.add_wire_between(esp32, "GPIO21", oled, "SDA");
        app.add_wire_between(esp32, "GPIO22", oled, "SCL");
        let simulation = app.current_simulation();
        assert!(simulation.erc.iter().any(|violation| {
            violation.message.contains("I2C SDA") && violation.message.contains("pull-up")
        }));
        assert!(simulation.erc.iter().any(|violation| {
            violation.message.contains("I2C SCL") && violation.message.contains("pull-up")
        }));
    }

    #[test]
    fn arduino_codegen_detects_esp32_oled_i2c_pins() {
        let mut app = CircuitApp::new();
        app.load_esp32_oled_demo();

        let code = generate_arduino_code(&build_circuit_netlist(&app.components, &app.wires));

        assert!(code.contains("#include <Wire.h>"));
        assert!(code.contains("Wire.begin(21, 22);"));
        assert!(code.contains("display.begin"));
    }

    #[test]
    fn esp32_oled_example_has_i2c_pullups() {
        let mut app = CircuitApp::new();
        app.load_esp32_oled_demo();

        let simulation = app.current_simulation();
        assert!(!simulation.erc.iter().any(|violation| {
            violation.message.contains("I2C") && violation.message.contains("pull-up")
        }));
    }

    #[test]
    fn arduino_oled_codegen_uses_uno_i2c_defaults() {
        let mut app = CircuitApp::new();
        app.load_arduino_oled_demo();

        let netlist = build_circuit_netlist(&app.components, &app.wires);
        let code = generate_arduino_code(&netlist);

        assert!(code.contains("Wire.begin();  // UNO uses A4 SDA and A5 SCL"));
        assert!(!code.contains("Wire.begin(21, 22)"));
        assert!(!app.current_simulation().erc.iter().any(|violation| {
            violation.message.contains("I2C") && violation.message.contains("pull-up")
        }));
    }

    #[test]
    fn arduino_codegen_ignores_unconnected_gpio_pins() {
        let mut app = CircuitApp::new();
        app.load_arduino_led_demo();

        let code = generate_arduino_code(&build_circuit_netlist(&app.components, &app.wires));

        assert!(code.contains("PIN_D13"));
        assert!(!code.contains("PIN_D2"));
        assert!(!code.contains("PIN_D3_PWM"));
    }

    #[test]
    fn saved_circuit_round_trips_components_and_wires() {
        let mut app = CircuitApp::new();
        app.load_led_demo();

        let json = serde_json::to_string(&SavedCircuit::from_app(&app)).unwrap();
        let saved = serde_json::from_str::<SavedCircuit>(&json).unwrap();
        let (snapshot, load_notes) = saved.into_snapshot().unwrap();

        assert_eq!(snapshot.components.len(), app.components.len());
        assert_eq!(snapshot.wires.len(), app.wires.len());
        assert!(snapshot.next_id > app.components.len() as u64);
        assert!(load_notes.is_empty());
    }

    #[test]
    fn saved_circuit_round_trips_multiple_pages() {
        let mut app = CircuitApp::new();
        app.load_led_demo();
        app.add_page();
        app.place_component(ComponentKind::Esp32, Pos2::new(300.0, 300.0));
        app.save_current_page();

        let json = serde_json::to_string(&SavedCircuit::from_app(&app)).unwrap();
        let saved = serde_json::from_str::<SavedCircuit>(&json).unwrap();
        let (snapshot, load_notes) = saved.into_snapshot().unwrap();

        assert!(load_notes.is_empty());
        assert_eq!(snapshot.pages.len(), 2);
        assert_eq!(snapshot.current_page, 1);
        assert_eq!(snapshot.components.len(), 1);
        assert_eq!(snapshot.components[0].kind, ComponentKind::Esp32);
        assert!(
            snapshot.pages[0]
                .1
                .iter()
                .any(|component| component.kind == ComponentKind::Led)
        );
    }

    #[test]
    fn page_switch_does_not_dirty_or_reuse_stale_simulation() {
        let mut app = CircuitApp::new();
        app.load_led_demo();
        app.add_page();
        app.history_state.dirty = false;

        let blank = app.current_simulation();
        assert_eq!(blank.summary, "No source or return");

        app.switch_page(0);
        assert!(
            !app.history_state.dirty,
            "Viewing another page should not mark data unsaved."
        );

        let led_page = app.current_simulation();
        assert_eq!(led_page.summary, "Current flowing");
    }

    #[test]
    fn removing_page_is_undoable() {
        let mut app = CircuitApp::new();
        app.load_led_demo();
        app.add_page();
        app.place_component(ComponentKind::Esp32, Pos2::new(300.0, 300.0));
        app.save_current_page();

        app.remove_current_page();
        assert_eq!(app.pages.len(), 1);

        app.undo();
        assert_eq!(app.pages.len(), 2);
        assert_eq!(app.current_page, 1);
        assert!(
            app.components
                .iter()
                .any(|component| component.kind == ComponentKind::Esp32)
        );
    }

    #[test]
    fn bom_csv_includes_all_pages_and_escapes_cells() {
        let mut app = CircuitApp::new();
        app.load_led_demo();
        if let Some(resistor) = app
            .components
            .iter_mut()
            .find(|c| c.kind == ComponentKind::Resistor)
        {
            resistor.value = "10k, 1% \"metal\"".to_string();
        }
        app.add_page();
        app.place_component(ComponentKind::Esp32, Pos2::new(300.0, 300.0));
        app.save_current_page();

        let csv = circuit_to_bom_csv(&app.effective_pages());

        assert!(csv.starts_with("Page,Label,Kind,Value\n"));
        assert!(csv.contains("Page 1,R1,Resistor,\"10k, 1% \"\"metal\"\"\""));
        assert!(csv.contains("Page 2,ESP1,ESP32 WROOM,ESP32-WROOM"));
        assert!(!csv.contains("Ground,0V"));
    }

    #[test]
    fn saved_circuit_repairs_duplicate_ids_and_skips_invalid_geometry() {
        let saved = SavedCircuit {
            schema_version: 1,
            next_id: 2,
            counters: Counters::default(),
            components: vec![
                SavedComponent {
                    id: 1,
                    kind: ComponentKind::Resistor,
                    x: 100.0,
                    y: 100.0,
                    rotation: 450,
                    label: "R1".to_string(),
                    value: "10k".to_string(),
                },
                SavedComponent {
                    id: 1,
                    kind: ComponentKind::Battery,
                    x: 200.0,
                    y: 100.0,
                    rotation: 0,
                    label: "BAT1".to_string(),
                    value: "9V".to_string(),
                },
                SavedComponent {
                    id: 3,
                    kind: ComponentKind::Led,
                    x: f32::NAN,
                    y: 100.0,
                    rotation: 0,
                    label: "LED1".to_string(),
                    value: "red".to_string(),
                },
            ],
            wires: vec![
                SavedWire {
                    id: 1,
                    points: vec![
                        SavedPoint { x: 100.0, y: 100.0 },
                        SavedPoint { x: 160.0, y: 100.0 },
                    ],
                },
                SavedWire {
                    id: 4,
                    points: vec![SavedPoint { x: 0.0, y: 0.0 }],
                },
            ],
            junction_dots: Vec::new(),
            no_connect_markers: Vec::new(),
            pages: Vec::new(),
            current_page: 0,
        };

        let (snapshot, load_notes) = saved.into_snapshot().unwrap();
        let unique_ids = snapshot
            .components
            .iter()
            .map(|component| component.id)
            .chain(snapshot.wires.iter().map(|wire| wire.id))
            .collect::<HashSet<_>>();

        assert_eq!(snapshot.components.len(), 2);
        assert_eq!(snapshot.wires.len(), 1);
        assert_eq!(unique_ids.len(), 3);
        assert_eq!(snapshot.components[0].rotation, 90);
        assert!(snapshot.next_id > unique_ids.iter().copied().max().unwrap());
        assert!(load_notes.len() >= 3);
    }

    #[test]
    fn oled_without_i2c_is_not_energized() {
        // Battery → OLED VCC/GND directly, but NO I2C wires → OLED must stay OFF
        let mut app = CircuitApp::new();
        app.reset_canvas();
        let battery = app.place_component(ComponentKind::Battery, Pos2::new(180.0, 300.0));
        let oled = app.place_component(ComponentKind::Oled, Pos2::new(420.0, 300.0));
        app.add_wire_between(battery, "+", oled, "VCC");
        app.add_wire_between(battery, "-", oled, "GND");

        let sim = analyze_circuit(&app.components, &app.wires);
        let oled_id = app
            .components
            .iter()
            .find(|c| c.kind == ComponentKind::Oled)
            .unwrap()
            .id;

        assert!(
            !sim.energized_components.contains(&oled_id),
            "OLED must NOT be energized without I2C connections"
        );
        assert!(
            sim.component_warnings.contains_key(&oled_id),
            "OLED must have a warning about missing I2C"
        );
    }

    #[test]
    fn reversed_led_opens_loop_and_reports_polarity_warning() {
        let mut app = CircuitApp::new();
        let battery = app.place_component(ComponentKind::Battery, Pos2::new(180.0, 300.0));
        let led = app.place_component(ComponentKind::Led, Pos2::new(420.0, 300.0));
        let ground = app.place_component(ComponentKind::Ground, Pos2::new(620.0, 360.0));

        let bat_pos = app.pin_pos(battery, "+").unwrap();
        let bat_neg = app.pin_pos(battery, "-").unwrap();
        let led_a = app.pin_pos(led, "A").unwrap();
        let led_b = app.pin_pos(led, "B").unwrap();
        let gnd = app.pin_pos(ground, "GND").unwrap();

        app.add_wire(vec![
            bat_pos,
            Pos2::new(bat_pos.x, 220.0),
            Pos2::new(led_b.x, 220.0),
            led_b,
        ]);
        app.add_wire(vec![led_a, Pos2::new(led_a.x, 360.0), gnd]);
        app.add_wire(vec![
            bat_neg,
            Pos2::new(bat_neg.x, 460.0),
            Pos2::new(gnd.x, 460.0),
            gnd,
        ]);

        let sim = analyze_circuit(&app.components, &app.wires);

        assert!(
            !sim.closed,
            "Reversed LED should not close the live path: {:?}",
            sim.details
        );
        assert!(!sim.energized_components.contains(&led));
        assert!(
            sim.component_warnings
                .get(&led)
                .is_some_and(|warning| warning.contains("Polarity warning")),
            "Reversed LED should report a polarity warning: {:?}",
            sim.component_warnings.get(&led)
        );
    }

    #[test]
    fn erc_short_circuit_points_to_problem_wire() {
        let mut app = CircuitApp::new();
        let battery = app.place_component(ComponentKind::Battery, Pos2::new(180.0, 300.0));
        let ground = app.place_component(ComponentKind::Ground, Pos2::new(420.0, 300.0));

        app.add_wire_between(battery, "+", ground, "GND");
        app.add_wire_between(battery, "-", ground, "GND");
        let wire_ids = app.wires.iter().map(|wire| wire.id).collect::<HashSet<_>>();

        let mut sim = analyze_circuit(&app.components, &app.wires);
        sim.erc = run_erc(&app.components, &app.wires, &sim);

        assert!(sim.shorted);
        assert!(
            sim.erc.iter().any(|violation| {
                violation.severity == ErcSeverity::Error
                    && violation
                        .wire_id
                        .is_some_and(|wire_id| wire_ids.contains(&wire_id))
                    && violation.message.contains("Power net conflict")
            }),
            "ERC should point to the wire tying source + to GND: {:?}",
            sim.erc
        );
    }

    #[test]
    fn simulation_connects_pin_when_wire_endpoint_is_snapped_to_pin() {
        let mut app = CircuitApp::new();
        let battery = app.place_component(ComponentKind::Battery, Pos2::new(160.0, 300.0));
        let resistor = app.place_component(ComponentKind::Resistor, Pos2::new(360.0, 300.0));
        let ground = app.place_component(ComponentKind::Ground, Pos2::new(560.0, 360.0));

        let bat_pos = app.pin_pos(battery, "+").unwrap();
        let bat_neg = app.pin_pos(battery, "-").unwrap();
        let r_a = app.pin_pos(resistor, "A").unwrap();
        let r_b = app.pin_pos(resistor, "B").unwrap();
        let gnd = app.pin_pos(ground, "GND").unwrap();

        app.add_wire(vec![Pos2::new(bat_pos.x, r_a.y), r_a]);
        app.add_wire(vec![bat_pos, Pos2::new(bat_pos.x, r_a.y)]);
        app.add_wire(vec![r_b, Pos2::new(r_b.x, gnd.y), gnd]);
        app.add_wire(vec![bat_neg, Pos2::new(bat_neg.x, gnd.y), gnd]);

        let sim = analyze_circuit(&app.components, &app.wires);

        assert_eq!(sim.summary, "Current flowing", "{:?}", sim.details);
        assert!(sim.energized_components.contains(&resistor));
    }

    #[test]
    fn rotating_connected_component_keeps_wire_endpoints_on_pins() {
        let mut app = CircuitApp::new();
        let battery = app.place_component(ComponentKind::Battery, Pos2::new(160.0, 300.0));
        let resistor = app.place_component(ComponentKind::Resistor, Pos2::new(360.0, 300.0));
        let ground = app.place_component(ComponentKind::Ground, Pos2::new(560.0, 360.0));
        app.add_wire_between(battery, "+", resistor, "A");
        app.add_wire_between(resistor, "B", ground, "GND");
        app.add_wire_between(battery, "-", ground, "GND");

        let old_a = app.pin_pos(resistor, "A").unwrap();
        let old_b = app.pin_pos(resistor, "B").unwrap();
        app.selected = Some(Selection::Component(resistor));
        app.rotate_selected();
        let new_a = app.pin_pos(resistor, "A").unwrap();
        let new_b = app.pin_pos(resistor, "B").unwrap();

        assert_ne!(old_a, new_a);
        assert_ne!(old_b, new_b);
        assert!(
            app.wires
                .iter()
                .any(|wire| wire.points.iter().any(|point| point.distance(new_a) <= 0.5)),
            "R.A should remain attached after rotate: {:?}",
            app.wires
        );
        assert!(
            app.wires
                .iter()
                .any(|wire| wire.points.iter().any(|point| point.distance(new_b) <= 0.5)),
            "R.B should remain attached after rotate: {:?}",
            app.wires
        );

        let sim = analyze_circuit(&app.components, &app.wires);
        assert_eq!(sim.summary, "Current flowing", "{:?}", sim.details);
        assert!(sim.energized_components.contains(&resistor));
    }

    #[test]
    fn near_pin_wire_segment_does_not_connect_without_snap_point() {
        let mut app = CircuitApp::new();
        let battery = app.place_component(ComponentKind::Battery, Pos2::new(160.0, 300.0));
        let resistor = app.place_component(ComponentKind::Resistor, Pos2::new(360.0, 300.0));
        let ground = app.place_component(ComponentKind::Ground, Pos2::new(560.0, 360.0));

        let bat_pos = app.pin_pos(battery, "+").unwrap();
        let bat_neg = app.pin_pos(battery, "-").unwrap();
        let r_a = app.pin_pos(resistor, "A").unwrap();
        let r_b = app.pin_pos(resistor, "B").unwrap();
        let gnd = app.pin_pos(ground, "GND").unwrap();

        // This wire passes close to R1.A, but it does not include R1.A as an
        // endpoint/control point. It must remain visually and electrically open.
        app.add_wire(vec![bat_pos, Pos2::new(r_a.x + 20.0, r_a.y + 4.0)]);
        app.add_wire(vec![r_b, Pos2::new(r_b.x, gnd.y), gnd]);
        app.add_wire(vec![bat_neg, Pos2::new(bat_neg.x, gnd.y), gnd]);

        let sim = analyze_circuit(&app.components, &app.wires);

        assert_eq!(sim.summary, "Open circuit", "{:?}", sim.details);
        assert!(!sim.energized_components.contains(&resistor));
    }

    #[test]
    fn transistor_with_open_base_does_not_conduct() {
        let mut app = CircuitApp::new();
        let battery = app.place_component(ComponentKind::Battery, Pos2::new(180.0, 300.0));
        let resistor = app.place_component(ComponentKind::Resistor, Pos2::new(340.0, 220.0));
        let led = app.place_component(ComponentKind::Led, Pos2::new(500.0, 220.0));
        let npn = app.place_component(ComponentKind::NpnTransistor, Pos2::new(600.0, 360.0));
        let ground = app.place_component(ComponentKind::Ground, Pos2::new(700.0, 500.0));

        app.add_wire_between(battery, "+", resistor, "A");
        app.add_wire_between(resistor, "B", led, "A");
        app.add_wire_between(led, "B", npn, "C");
        app.add_wire_between(npn, "E", ground, "GND");
        app.add_wire_between(battery, "-", ground, "GND");

        let sim = analyze_circuit(&app.components, &app.wires);

        assert_eq!(sim.summary, "Open circuit", "{:?}", sim.details);
        assert!(!sim.energized_components.contains(&npn));
        assert!(
            sim.component_warnings
                .get(&npn)
                .is_some_and(|warning| warning.contains("gate/base is open"))
        );
    }

    #[test]
    fn relay_contact_follows_coil_state() {
        let mut app = CircuitApp::new();
        app.load_motor_relay_demo();
        let motor_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::DcMotor)
            .map(|component| component.id)
            .unwrap();
        let button_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::PushButton)
            .map(|component| component.id)
            .unwrap();

        let open_sim = analyze_circuit(&app.components, &app.wires);
        assert!(!open_sim.energized_components.contains(&motor_id));

        app.components
            .iter_mut()
            .find(|component| component.id == button_id)
            .unwrap()
            .value = "closed".to_string();
        let closed_sim = analyze_circuit(&app.components, &app.wires);

        assert_eq!(
            closed_sim.summary, "Current flowing",
            "{:?}",
            closed_sim.details
        );
        assert!(closed_sim.energized_components.contains(&motor_id));
    }

    #[test]
    fn button_toggle_demo_marks_led_output_path_when_button_is_closed() {
        let mut app = CircuitApp::new();
        app.load_button_toggle_led_demo();

        let button_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::PushButton)
            .map(|component| component.id)
            .unwrap();
        let led_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::Led)
            .map(|component| component.id)
            .unwrap();
        let resistor_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::Resistor)
            .map(|component| component.id)
            .unwrap();
        let gpio18_wire = app
            .wires
            .iter()
            .find(|wire| {
                let gpio18 = app.pin_pos(
                    app.components
                        .iter()
                        .find(|component| component.kind == ComponentKind::Esp32)
                        .unwrap()
                        .id,
                    "GPIO18",
                );
                gpio18.is_some_and(|pin| wire.points.iter().any(|point| point.distance(pin) <= 0.5))
            })
            .map(|wire| wire.id)
            .unwrap();

        let open_sim = analyze_circuit(&app.components, &app.wires);
        assert!(!open_sim.energized_components.contains(&led_id));
        assert!(!open_sim.energized_wires.contains(&gpio18_wire));

        app.components
            .iter_mut()
            .find(|component| component.id == button_id)
            .unwrap()
            .value = "closed".to_string();

        let closed_sim = analyze_circuit(&app.components, &app.wires);
        assert!(
            closed_sim.energized_components.contains(&led_id),
            "{:?}",
            closed_sim.details
        );
        assert!(closed_sim.energized_components.contains(&resistor_id));
        assert!(closed_sim.energized_wires.contains(&gpio18_wire));
    }

    #[test]
    fn esp32_button_debounce_demo_current_follows_button_state() {
        let mut app = CircuitApp::new();
        app.load_esp32_button_debounce_demo();

        let button_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::PushButton)
            .map(|component| component.id)
            .unwrap();
        let led_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::Led)
            .map(|component| component.id)
            .unwrap();
        let resistor_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::Resistor)
            .map(|component| component.id)
            .unwrap();
        let gpio18_wire = app
            .wires
            .iter()
            .find(|wire| {
                let gpio18 = app.pin_pos(
                    app.components
                        .iter()
                        .find(|component| component.kind == ComponentKind::Esp32)
                        .unwrap()
                        .id,
                    "GPIO18",
                );
                gpio18.is_some_and(|pin| wire.points.iter().any(|point| point.distance(pin) <= 0.5))
            })
            .map(|wire| wire.id)
            .unwrap();

        let open_sim = analyze_circuit(&app.components, &app.wires);
        assert!(!open_sim.energized_components.contains(&led_id));
        assert!(!open_sim.energized_components.contains(&resistor_id));
        assert!(!open_sim.energized_wires.contains(&gpio18_wire));

        app.components
            .iter_mut()
            .find(|component| component.id == button_id)
            .unwrap()
            .value = "closed".to_string();

        let closed_sim = analyze_circuit(&app.components, &app.wires);
        assert!(
            closed_sim.energized_components.contains(&led_id),
            "{:?}",
            closed_sim.details
        );
        assert!(closed_sim.energized_components.contains(&resistor_id));
        assert!(closed_sim.energized_wires.contains(&gpio18_wire));
        assert_ne!(closed_sim.summary, "Short circuit");
    }

    #[test]
    fn arduino_codegen_uses_millis_debounce_for_button_led() {
        let mut app = CircuitApp::new();
        app.load_esp32_button_debounce_demo();

        let netlist = build_circuit_netlist(&app.components, &app.wires);
        let code = generate_arduino_code(&netlist);

        assert!(
            code.contains("const int BUTTON_PIN = 21;"),
            "{code}\n\npins: {:?}\nnets: {:?}",
            netlist.pins,
            netlist.nets
        );
        assert!(code.contains("const unsigned long DEBOUNCE_MS = 50;"));
        assert!(code.contains("pinMode(BUTTON_PIN, INPUT_PULLUP);"));
        assert!(code.contains("lastDebounceTime = millis();"));
        assert!(code.contains("(millis() - lastDebounceTime) > DEBOUNCE_MS"));
        assert!(code.contains("stableState == LOW"));
    }

    #[test]
    fn manually_wired_led_loop_marks_current_path() {
        let mut app = CircuitApp::new();
        let battery = app.place_component(ComponentKind::Battery, Pos2::new(160.0, 300.0));
        let resistor = app.place_component(ComponentKind::Resistor, Pos2::new(340.0, 300.0));
        let led = app.place_component(ComponentKind::Led, Pos2::new(500.0, 300.0));
        let ground = app.place_component(ComponentKind::Ground, Pos2::new(660.0, 360.0));

        app.add_wire_between(battery, "+", resistor, "A");
        app.add_wire_between(resistor, "B", led, "A");
        app.add_wire_between(led, "B", ground, "GND");
        app.add_wire_between(battery, "-", ground, "GND");

        let sim = analyze_circuit(&app.components, &app.wires);

        assert_eq!(sim.summary, "Current flowing", "{:?}", sim.details);
        assert_eq!(sim.status, SimulationStatus::Ok);
        assert!(sim.explanation.contains("closed path"));
        assert!(sim.energized_components.contains(&resistor));
        assert!(sim.energized_components.contains(&led));
        assert_eq!(sim.energized_wires.len(), app.wires.len());
    }

    #[test]
    fn manually_wired_controller_switch_led_path_follows_switch_state() {
        let mut app = CircuitApp::new();
        let esp32 = app.place_component(ComponentKind::Esp32, Pos2::new(420.0, 320.0));
        let switch = app.place_component(ComponentKind::Switch, Pos2::new(180.0, 220.0));
        let resistor = app.place_component(ComponentKind::Resistor, Pos2::new(660.0, 200.0));
        let led = app.place_component(ComponentKind::Led, Pos2::new(780.0, 200.0));
        let battery = app.place_component(ComponentKind::Battery, Pos2::new(180.0, 440.0));
        let ground = app.place_component(ComponentKind::Ground, Pos2::new(880.0, 340.0));

        app.components
            .iter_mut()
            .find(|component| component.id == switch)
            .unwrap()
            .value = "open".to_string();
        app.add_wire_between(battery, "+", esp32, "VIN");
        app.add_wire_between(battery, "-", ground, "GND");
        app.add_wire_between(esp32, "GND", ground, "GND");
        app.add_wire_between(esp32, "GPIO23", switch, "A");
        app.add_wire_between(switch, "B", ground, "GND");
        app.add_wire_between(esp32, "GPIO5", resistor, "A");
        app.add_wire_between(resistor, "B", led, "A");
        app.add_wire_between(led, "B", ground, "GND");

        let open_sim = analyze_circuit(&app.components, &app.wires);
        assert!(!open_sim.energized_components.contains(&led));

        app.components
            .iter_mut()
            .find(|component| component.id == switch)
            .unwrap()
            .value = "closed".to_string();

        let closed_sim = analyze_circuit(&app.components, &app.wires);
        assert!(
            closed_sim.energized_components.contains(&led),
            "{:?}",
            closed_sim.details
        );
        assert!(closed_sim.energized_components.contains(&resistor));
    }

    #[test]
    fn esp32_oled_demo_energizes_oled_via_3v3() {
        let mut app = CircuitApp::new();
        app.load_esp32_oled_demo();

        let sim = analyze_circuit(&app.components, &app.wires);

        let oled_id = app
            .components
            .iter()
            .find(|c| c.kind == ComponentKind::Oled)
            .map(|c| c.id)
            .expect("OLED not placed");

        assert!(sim.closed, "Circuit should be closed");
        assert!(!sim.shorted, "Circuit should not be shorted");
        assert!(
            sim.energized_components.contains(&oled_id),
            "OLED should be energized when powered via ESP32 3V3 with I2C wired"
        );
        assert!(
            sim.component_warnings.get(&oled_id).is_none(),
            "OLED should have no warnings: {:?}",
            sim.component_warnings.get(&oled_id)
        );
    }

    #[test]
    fn beginner_example_switch_led_flows_when_closed() {
        let mut app = CircuitApp::new();
        app.load_switch_led_demo();

        let sim = analyze_circuit(&app.components, &app.wires);
        let led_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::Led)
            .map(|component| component.id)
            .unwrap();

        assert_eq!(sim.summary, "Current flowing", "{:?}", sim.details);
        assert!(sim.energized_components.contains(&led_id));
    }

    #[test]
    fn lesson_open_switch_led_does_not_conduct() {
        let mut app = CircuitApp::new();
        app.load_open_switch_led_demo();

        let sim = analyze_circuit(&app.components, &app.wires);
        let led_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::Led)
            .map(|component| component.id)
            .unwrap();

        assert_eq!(sim.summary, "Open circuit", "{:?}", sim.details);
        assert_eq!(sim.status, SimulationStatus::Warning);
        assert!(sim.explanation.contains("0 A"));
        assert!(!sim.energized_components.contains(&led_id));
    }

    #[test]
    fn lesson_capacitor_blocks_dc_current() {
        let mut app = CircuitApp::new();
        app.load_capacitor_dc_block_demo();

        let sim = analyze_circuit(&app.components, &app.wires);
        let capacitor_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::Capacitor)
            .map(|component| component.id)
            .unwrap();

        assert_eq!(sim.summary, "Open circuit", "{:?}", sim.details);
        assert!(!sim.energized_components.contains(&capacitor_id));
    }

    #[test]
    fn lesson_missing_return_wire_keeps_led_off() {
        let mut app = CircuitApp::new();
        app.load_missing_return_demo();

        let sim = analyze_circuit(&app.components, &app.wires);
        let led_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::Led)
            .map(|component| component.id)
            .unwrap();

        assert_eq!(sim.summary, "Open circuit", "{:?}", sim.details);
        assert!(!sim.energized_components.contains(&led_id));
    }

    #[test]
    fn lesson_short_circuit_reports_error() {
        let mut app = CircuitApp::new();
        app.load_short_circuit_lesson_demo();

        let mut sim = analyze_circuit(&app.components, &app.wires);
        sim.erc = run_erc(&app.components, &app.wires, &sim);

        assert!(sim.shorted, "{:?}", sim.details);
        assert_eq!(sim.summary, "Short circuit");
        assert_eq!(sim.status, SimulationStatus::Failed);
        assert!(sim.explanation.contains("unsafe"));
        assert!(sim.erc.iter().any(|violation| {
            violation.severity == ErcSeverity::Error
                && (violation.message.contains("Short")
                    || violation.message.contains("Power net conflict"))
        }));
    }

    #[test]
    fn short_circuit_disables_current_flow_arrows() {
        let mut short_app = CircuitApp::new();
        short_app.load_short_circuit_lesson_demo();
        let short_sim = analyze_circuit(&short_app.components, &short_app.wires);

        assert!(short_sim.shorted);
        assert!(!short_sim.energized_wires.is_empty());
        assert!(!flow_overlay_enabled(&short_sim, true));

        let mut led_app = CircuitApp::new();
        led_app.load_led_demo();
        let led_sim = analyze_circuit(&led_app.components, &led_app.wires);

        assert!(!led_sim.shorted);
        assert!(!led_sim.energized_wires.is_empty());
        assert!(flow_overlay_enabled(&led_sim, true));
        assert!(!flow_overlay_enabled(&led_sim, false));
    }

    #[test]
    fn branched_wire_is_not_marked_as_single_energized_current_path() {
        let bat = Component {
            id: 1,
            kind: ComponentKind::Battery,
            pos: Pos2::new(0.0, 0.0),
            rotation: 0,
            label: "BAT1".to_string(),
            value: "5V".to_string(),
        };
        let r1 = Component {
            id: 2,
            kind: ComponentKind::Resistor,
            pos: Pos2::new(300.0, 0.0),
            rotation: 0,
            label: "R1".to_string(),
            value: "1k".to_string(),
        };
        let r2 = Component {
            id: 3,
            kind: ComponentKind::Resistor,
            pos: Pos2::new(164.0, 36.0),
            rotation: 90,
            label: "R2".to_string(),
            value: "1k".to_string(),
        };

        let bat_pins = component_pin_defs(&bat);
        let r1_pins = component_pin_defs(&r1);
        let r2_pins = component_pin_defs(&r2);
        let bat_p = bat_pins.iter().find(|pin| pin.label == "+").unwrap().pos;
        let bat_n = bat_pins.iter().find(|pin| pin.label == "-").unwrap().pos;
        let r1_a = r1_pins.iter().find(|pin| pin.label == "A").unwrap().pos;
        let r1_b = r1_pins.iter().find(|pin| pin.label == "B").unwrap().pos;
        let r2_a = r2_pins.iter().find(|pin| pin.label == "A").unwrap().pos;
        let r2_b = r2_pins.iter().find(|pin| pin.label == "B").unwrap().pos;
        let components = vec![bat, r1, r2];
        let wires = vec![
            Wire {
                id: 10,
                points: vec![bat_p, r2_a, r1_a],
            },
            Wire {
                id: 11,
                points: vec![r1_b, Pos2::new(r1_b.x, 80.0), bat_n],
            },
            Wire {
                id: 12,
                points: vec![r2_b, Pos2::new(r2_b.x, 120.0), bat_n],
            },
        ];

        let sim = analyze_circuit(&components, &wires);

        assert_eq!(sim.summary, "Current flowing", "{:?}", sim.details);
        assert!(
            !sim.energized_wires.contains(&10),
            "Branched polyline current differs by segment, so whole-wire current highlight is unsafe"
        );
        assert!(
            sim.dc
                .as_ref()
                .is_some_and(|dc| !dc.wire_current_known.contains(&10))
        );
    }

    #[test]
    fn short_circuit_does_not_light_load_components() {
        let mut app = CircuitApp::new();
        let battery = app.place_component(ComponentKind::Battery, Pos2::new(160.0, 300.0));
        let resistor = app.place_component(ComponentKind::Resistor, Pos2::new(340.0, 220.0));
        let led = app.place_component(ComponentKind::Led, Pos2::new(500.0, 220.0));
        let ground = app.place_component(ComponentKind::Ground, Pos2::new(660.0, 360.0));

        app.add_wire_between(battery, "+", resistor, "A");
        app.add_wire_between(resistor, "B", led, "A");
        app.add_wire_between(led, "B", ground, "GND");
        app.add_wire_between(battery, "-", ground, "GND");
        app.add_wire_between(battery, "+", ground, "GND");

        let sim = analyze_circuit(&app.components, &app.wires);

        assert!(sim.shorted);
        assert!(!sim.energized_components.contains(&resistor));
        assert!(!sim.energized_components.contains(&led));
        assert!(!flow_overlay_enabled(&sim, true));
    }

    #[test]
    fn engineering_checks_report_led_overcurrent_without_resistor() {
        let mut app = CircuitApp::new();
        let battery = app.place_component(ComponentKind::Battery, Pos2::new(180.0, 300.0));
        let led = app.place_component(ComponentKind::Led, Pos2::new(420.0, 300.0));
        let ground = app.place_component(ComponentKind::Ground, Pos2::new(620.0, 360.0));

        app.add_wire_between(battery, "+", led, "A");
        app.add_wire_between(led, "B", ground, "GND");
        app.add_wire_between(battery, "-", ground, "GND");

        let sim = analyze_circuit(&app.components, &app.wires);
        let warning = sim
            .component_warnings
            .get(&led)
            .cloned()
            .unwrap_or_default();

        assert!(warning.contains("Overcurrent risk"), "{warning}");
        assert!(
            sim.dc
                .as_ref()
                .and_then(|dc| dc.branch_current.get(&led))
                .is_some_and(|current| current.abs() > 0.025)
        );
    }

    #[test]
    fn lesson_direct_gpio_motor_reports_warning_and_motor_off() {
        let mut app = CircuitApp::new();
        app.load_direct_gpio_motor_warning_demo();

        let sim = analyze_circuit(&app.components, &app.wires);
        let erc = run_erc(&app.components, &app.wires, &sim);
        let motor_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::DcMotor)
            .map(|component| component.id)
            .unwrap();

        assert!(
            !sim.energized_components.contains(&motor_id),
            "{:?}",
            sim.details
        );
        assert!(erc.iter().any(|violation| {
            violation.message.contains("GPIO") && violation.message.contains("motor")
        }));
    }

    #[test]
    fn lesson_report_passes_current_flow_example() {
        let mut app = CircuitApp::new();
        app.load_led_demo();

        let sim = app.current_simulation();
        let report = lesson_report(&app.components, &sim).unwrap();

        assert!(
            report.checks.iter().all(|check| check.passed),
            "{:?}",
            report.title
        );
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.label == "Closed path")
        );
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.label == "LED output")
        );
    }

    #[test]
    fn lesson_report_passes_open_circuit_example() {
        let mut app = CircuitApp::new();
        app.load_open_switch_led_demo();

        let sim = app.current_simulation();
        let report = lesson_report(&app.components, &sim).unwrap();

        assert!(
            report.checks.iter().all(|check| check.passed),
            "{:?}",
            report.title
        );
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.label == "No closed current path")
        );
    }

    #[test]
    fn lesson_report_catches_when_expected_on_is_broken() {
        let mut app = CircuitApp::new();
        app.load_led_demo();
        if let Some(wire) = app.wires.pop() {
            app.status = format!("Removed wire {} for test.", wire.id);
        }
        app.mark_dirty();

        let sim = app.current_simulation();
        let report = lesson_report(&app.components, &sim).unwrap();

        assert!(
            report.checks.iter().any(|check| !check.passed),
            "{:?}",
            report.title
        );
    }

    #[test]
    fn lesson_report_passes_short_and_gpio_warning_examples() {
        let mut short_app = CircuitApp::new();
        short_app.load_short_circuit_lesson_demo();
        let short_sim = short_app.current_simulation();
        let short_report = lesson_report(&short_app.components, &short_sim).unwrap();
        assert!(
            short_report.checks.iter().all(|check| check.passed),
            "{:?}",
            short_report.title
        );

        let mut motor_app = CircuitApp::new();
        motor_app.load_direct_gpio_motor_warning_demo();
        let motor_sim = motor_app.current_simulation();
        let motor_report = lesson_report(&motor_app.components, &motor_sim).unwrap();
        assert!(
            motor_report.checks.iter().all(|check| check.passed),
            "{:?}",
            motor_report.title
        );
        assert!(
            motor_report
                .checks
                .iter()
                .any(|check| check.label == "GPIO motor rule")
        );
    }

    #[test]
    fn beginner_example_parallel_leds_has_two_lit_leds() {
        let mut app = CircuitApp::new();
        app.load_parallel_led_demo();

        let sim = analyze_circuit(&app.components, &app.wires);
        let lit_leds = app
            .components
            .iter()
            .filter(|component| component.kind == ComponentKind::Led)
            .filter(|component| sim.energized_components.contains(&component.id))
            .count();
        let erc = run_erc(&app.components, &app.wires, &sim);

        assert_eq!(lit_leds, 2, "{:?}", sim.details);
        assert!(
            !erc.iter()
                .any(|violation| violation.message.contains("current limiting resistor")),
            "{:?}",
            erc
        );
    }

    #[test]
    fn beginner_example_ohms_law_meter_has_series_current_and_parallel_voltage() {
        let mut app = CircuitApp::new();
        app.load_ohms_law_meter_demo();

        let sim = app.current_simulation();
        let led_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::Led)
            .map(|component| component.id)
            .unwrap();
        let report = lesson_report(&app.components, &sim).unwrap();

        assert_eq!(sim.summary, "Current flowing", "{:?}", sim.details);
        assert!(sim.energized_components.contains(&led_id));
        assert!(
            app.components
                .iter()
                .any(|c| c.kind == ComponentKind::Ammeter)
        );
        assert!(
            app.components
                .iter()
                .any(|c| c.kind == ComponentKind::Voltmeter)
        );
        assert!(
            report.checks.iter().all(|check| check.passed),
            "{:?}",
            report.title
        );
    }

    #[test]
    fn beginner_example_reversed_led_reports_polarity() {
        let mut app = CircuitApp::new();
        app.load_reversed_led_warning_demo();

        let sim = analyze_circuit(&app.components, &app.wires);
        let erc = run_erc(&app.components, &app.wires, &sim);

        assert!(erc.iter().any(|violation| {
            violation.severity == ErcSeverity::Error && violation.message.contains("reversed")
        }));
    }

    #[test]
    fn beginner_example_esp32_sensor_energizes_sensor() {
        let mut app = CircuitApp::new();
        app.load_esp32_sensor_demo();

        let sim = analyze_circuit(&app.components, &app.wires);
        let sensor_id = app
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::Sensor)
            .map(|component| component.id)
            .unwrap();

        assert!(
            sim.energized_components.contains(&sensor_id),
            "{:?}",
            sim.details
        );
        assert!(sim.component_warnings.get(&sensor_id).is_none());
    }

    #[test]
    fn beginner_example_motor_driver_avoids_direct_gpio_motor_warning() {
        let mut app = CircuitApp::new();
        app.load_motor_driver_demo();

        let sim = analyze_circuit(&app.components, &app.wires);
        let erc = run_erc(&app.components, &app.wires, &sim);

        assert!(
            app.components
                .iter()
                .any(|c| c.kind == ComponentKind::MotorDriver)
        );
        assert!(
            app.components
                .iter()
                .any(|c| c.kind == ComponentKind::DcMotor)
        );
        assert!(
            !erc.iter().any(|violation| {
                violation.message.contains("GPIO") && violation.message.contains("motor")
            }),
            "{:?}",
            erc
        );
    }

    #[test]
    fn breadboard_guide_tracks_esp32_oled_i2c_jumpers() {
        let mut app = CircuitApp::new();
        app.load_esp32_oled_demo();
        let netlist = build_circuit_netlist(&app.components, &app.wires);
        let guide = build_breadboard_guide(&app.components, &netlist);

        assert_eq!(guide.routes.len(), 4, "{guide:?}");
        assert!(
            guide.routes.iter().all(|route| route.connected),
            "{guide:?}"
        );
        assert!(guide.routes.iter().any(|route| {
            route.from_pin.contains("GPIO21")
                && route.to_pin == "SDA"
                && route.purpose == "I2C data"
        }));
        assert!(guide.routes.iter().any(|route| {
            route.from_pin.contains("GPIO22")
                && route.to_pin == "SCL"
                && route.purpose == "I2C clock"
        }));
    }

    #[test]
    fn breadboard_guide_tracks_arduino_oled_i2c_jumpers() {
        let mut app = CircuitApp::new();
        app.load_arduino_oled_demo();
        let netlist = build_circuit_netlist(&app.components, &app.wires);
        let guide = build_breadboard_guide(&app.components, &netlist);

        assert_eq!(guide.routes.len(), 4, "{guide:?}");
        assert!(
            guide.routes.iter().all(|route| route.connected),
            "{guide:?}"
        );
        assert!(guide.routes.iter().any(|route| {
            route.from_pin == "A4 SDA" && route.to_pin == "SDA" && route.purpose == "I2C data"
        }));
        assert!(guide.routes.iter().any(|route| {
            route.from_pin == "A5 SCL" && route.to_pin == "SCL" && route.purpose == "I2C clock"
        }));
    }

    #[test]
    fn ac_frequency_is_part_of_simulation_cache_key() {
        let mut app = CircuitApp::new();
        app.load_led_demo();

        let _ = app.current_simulation();
        let first_key = app
            .cached_simulation
            .as_ref()
            .map(|(_, key, _)| *key)
            .unwrap();

        app.simulation_ui.ac_freq_hz = 10_000.0;
        let _ = app.current_simulation();
        let second_key = app
            .cached_simulation
            .as_ref()
            .map(|(_, key, _)| *key)
            .unwrap();

        assert_ne!(first_key, second_key);
        assert_eq!(second_key, 10_000.0f32.to_bits());
    }
}

fn main() -> eframe::Result<()> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--export-demo-svg") {
        let Some(path) = args.next() else {
            eprintln!("Usage: Cluster --export-demo-svg <path>");
            std::process::exit(2);
        };
        let mut app = CircuitApp::new();
        app.load_esp32_oled_demo();
        if let Some(parent) = std::path::Path::new(&path).parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            eprintln!("Failed to create {}: {error}", parent.display());
            std::process::exit(1);
        }
        if let Err(error) = fs::write(&path, circuit_to_svg(&app.components, &app.wires)) {
            eprintln!("Failed to export {path}: {error}");
            std::process::exit(1);
        }
        println!("Exported {path}");
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Cluster Circuits")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1180.0, 760.0]),
        run_and_return: false,
        ..Default::default()
    };
    eframe::run_native(
        "Cluster Circuits",
        options,
        Box::new(|_cc| Ok(Box::new(CircuitApp::new()))),
    )
}

/*
This is an egui/eframe Rust circuit editor.
Refactor it toward a professional SPICE frontend.

Do not rewrite the whole app.
First add:
1. SPICE netlist export
2. net naming from connected wire/pin graph
3. Export .cir button
4. Keep existing analyze_circuit() as quick live check
5. Support Resistor, Capacitor, Inductor, Battery/VSource, ISource, Diode, LED, Ground

Use serde only if needed.
Keep the UI working.
Explain changes after patch.
*/
