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
mod editor;
mod engine;
mod examples;
mod export;
mod lessons;
mod model;
mod pcb;
mod storage;
mod ui;

pub(crate) use engine::parse_metric_value;
pub(crate) use model::*;
pub(crate) use model::{
    component_pin_defs, component_pins, component_size, distance_to_segment,
    point_touches_wire_segment, rotate_point,
};
pub(crate) use ui::app::{
    AUTORECOVER_PATH, CircuitApp, CircuitNodes, SAVE_PATH, UnionFind, analyze_circuit,
    circuit_bounds, circuit_to_bom_csv, circuit_to_netlist_text, circuit_to_spice_netlist,
    circuit_to_svg, component_kind_label, connected_pin_positions, generate_arduino_code,
    move_attached_wire_endpoints, push_unique_point, run_erc_with_netlist, simplify_wire,
    tidy_wire_points, wire_contact_points, wire_path_pin_crossings,
};

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
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            eprintln!("Failed to create {}: {error}", parent.display());
            std::process::exit(1);
        }
        if let Err(error) = std::fs::write(&path, circuit_to_svg(&app.components, &app.wires)) {
            eprintln!("Failed to export {path}: {error}");
            std::process::exit(1);
        }
        println!("Exported {path}");
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
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
