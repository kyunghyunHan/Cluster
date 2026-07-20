mod app;
mod commands;
mod editor;
mod engine;
mod examples;
mod export;
mod lessons;
mod model;
mod pcb;
pub mod performance;
mod storage;
mod ui;

pub(crate) use engine::parse_metric_value;
pub use model::DocumentRevisions;
pub(crate) use model::*;
pub(crate) use model::{component_pin_defs, point_touches_wire_segment};
pub(crate) use ui::app::{
    CircuitApp, CircuitNodes, UnionFind, analyze_circuit, circuit_bounds, circuit_to_bom_csv,
    circuit_to_netlist_text, circuit_to_svg, component_kind_label, generate_arduino_code,
    move_attached_wire_endpoints, push_unique_point, run_erc_with_netlist, simplify_wire,
    tidy_wire_points, wire_path_pin_crossings,
};

pub fn run() -> eframe::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("--export-demo-svg") {
        let Some(path) = args.get(1) else {
            eprintln!("Usage: Cluster --export-demo-svg <path>");
            std::process::exit(2);
        };
        let mut app = CircuitApp::new();
        app.load_esp32_oled_demo();
        if let Some(parent) = std::path::Path::new(&path).parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            eprintln!("Failed to create {}: {error}", parent.display());
            std::process::exit(1);
        }
        if let Err(error) = std::fs::write(path, circuit_to_svg(&app.components, &app.wires)) {
            eprintln!("Failed to export {path}: {error}");
            std::process::exit(1);
        }
        println!("Exported {path}");
        return Ok(());
    }
    let option = |name: &str| {
        args.iter()
            .position(|argument| argument == name)
            .and_then(|index| args.get(index + 1))
            .cloned()
    };
    let startup_demo = option("--demo");
    let startup_workspace = option("--workspace");
    let startup_panel = option("--panel");
    let startup_capture = option("--capture");

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Cluster Circuits")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 620.0]),
        run_and_return: false,
        ..Default::default()
    };
    eframe::run_native(
        "Cluster Circuits",
        options,
        Box::new(move |_cc| {
            let mut app = CircuitApp::new();
            match startup_demo.as_deref() {
                Some("esp32-oled") => app.load_esp32_oled_demo(),
                Some("switch-led") => app.load_switch_led_demo(),
                Some("short") => app.load_short_circuit_lesson_demo(),
                Some("open") => app.load_open_switch_led_demo(),
                Some("motor-driver") => app.load_motor_driver_demo(),
                Some(_) | None => {}
            }
            match startup_panel.as_deref() {
                Some("erc") => {
                    app.workspace_state.bottom_dock_tab =
                        crate::ui::bottom_dock::BottomDockTab::Erc;
                }
                Some("simulation") => {
                    app.workspace_state.bottom_dock_tab =
                        crate::ui::bottom_dock::BottomDockTab::Simulation;
                }
                Some("breadboard") => {
                    app.workspace_state.bottom_dock_tab =
                        crate::ui::bottom_dock::BottomDockTab::Breadboard;
                }
                Some("pcb") => {
                    app.workspace_state.bottom_dock_tab =
                        crate::ui::bottom_dock::BottomDockTab::Pcb;
                }
                Some(_) | None => {}
            }
            if startup_workspace.as_deref() == Some("pcb") {
                app.update_pcb_from_schematic();
                app.auto_place_pcb_footprints();
                app.workspace_state.workspace = crate::ui::app::Workspace::Pcb;
                app.workspace_state.bottom_dock_open = false;
                app.workspace_state.bottom_dock_tab = crate::ui::bottom_dock::BottomDockTab::Pcb;
            }
            app.automated_capture_path = startup_capture;
            Ok(Box::new(app))
        }),
    )
}
