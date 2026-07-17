use super::*;
use std::fs;

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
fn relay_flyback_auto_fix_places_and_wires_diode() {
    let mut app = CircuitApp::new();
    app.load_motor_relay_demo();
    let simulation = app.current_simulation();
    let fix = simulation
        .erc
        .iter()
        .find(|violation| violation.message.contains("flyback diode"))
        .and_then(|violation| violation.auto_fix())
        .expect("relay flyback warning should offer auto fix");

    app.apply_erc_auto_fix(fix);

    let simulation = app.current_simulation();
    assert!(
        !simulation
            .erc
            .iter()
            .any(|violation| violation.message.contains("flyback diode"))
    );
    assert!(
        app.components
            .iter()
            .any(|component| component.kind == ComponentKind::Diode)
    );
    assert!(app.status.contains("flyback diode"));
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
fn i2c_pullup_auto_fix_places_and_wires_resistors() {
    let mut app = CircuitApp::new();
    let esp32 = app.place_component(ComponentKind::Esp32, Pos2::new(300.0, 300.0));
    let oled = app.place_component(ComponentKind::Oled, Pos2::new(600.0, 300.0));
    app.add_wire_between(esp32, "GPIO21", oled, "SDA");
    app.add_wire_between(esp32, "GPIO22", oled, "SCL");

    let simulation = app.current_simulation();
    let fix = simulation
        .erc
        .iter()
        .find(|violation| {
            violation.message.contains("I2C SDA") && violation.message.contains("pull-up")
        })
        .and_then(|violation| violation.auto_fix())
        .expect("missing SDA pull-up should offer auto fix");

    app.apply_erc_auto_fix(fix);

    let simulation = app.current_simulation();
    let pullup_messages = simulation
        .erc
        .iter()
        .filter(|violation| {
            violation.message.contains("I2C") && violation.message.contains("pull-up")
        })
        .map(|violation| violation.message.clone())
        .collect::<Vec<_>>();
    assert!(
        !simulation.erc.iter().any(|violation| {
            violation.message.contains("I2C") && violation.message.contains("pull-up")
        }),
        "{pullup_messages:?}\n{}",
        circuit_to_netlist_text(&build_circuit_netlist(&app.components, &app.wires))
    );
    let pullups = app
        .components
        .iter()
        .filter(|component| component.kind == ComponentKind::Resistor && component.value == "4.7k")
        .count();
    assert_eq!(pullups, 2);
    assert!(app.status.contains("wired"));
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

    let before = crate::engine::netlist::build_canonical_connectivity(&app.components, &app.wires);
    let after =
        crate::engine::netlist::build_canonical_connectivity(&snapshot.components, &snapshot.wires);
    assert_eq!(before.pin_nets, after.pin_nets);
    assert_eq!(before.junction_nets, after.junction_nets);
    assert_eq!(before.wire_segment_nets, after.wire_segment_nets);
    assert_eq!(before.netlist.wire_nets, after.netlist.wire_nets);
    assert_eq!(before.diagnostics, after.diagnostics);
}

#[test]
fn saved_circuit_round_trips_annotations_and_exact_connectivity() {
    let mut app = CircuitApp::new();
    app.place_component(ComponentKind::Resistor, Pos2::new(240.0, 200.0));
    let no_connect_position = component_pin_defs(&app.components[0])[0].pos;
    app.wires = vec![
        Wire::new(100, vec![Pos2::new(100.0, 100.0), Pos2::new(200.0, 100.0)]),
        Wire::new(101, vec![Pos2::new(150.0, 50.0), Pos2::new(150.0, 150.0)]),
    ];
    app.annotations = SchematicAnnotations {
        junction_dots: vec![JunctionDot {
            id: JunctionId(50),
            position: Pos2::new(150.0, 100.0),
        }],
        no_connect_markers: vec![NoConnectDot {
            id: 51,
            position: no_connect_position,
        }],
    };
    app.next_id = 102;
    app.invalidate_connectivity_cache();

    let before = app.current_connectivity();
    let saved = SavedCircuit::from_app(&app);
    assert_eq!(saved.junction_dots.len(), 1);
    assert_eq!(saved.no_connect_markers.len(), 1);
    assert_eq!(saved.pages[0].junction_dots.len(), 1);
    assert_eq!(saved.pages[0].no_connect_markers.len(), 1);

    let json = serde_json::to_string(&saved).unwrap();
    let (snapshot, load_notes) = serde_json::from_str::<SavedCircuit>(&json)
        .unwrap()
        .into_snapshot()
        .unwrap();
    assert!(load_notes.is_empty(), "{load_notes:?}");
    assert_eq!(snapshot.annotations, app.annotations);

    let restored_annotations = snapshot.annotations.netlist_annotations();
    let after = crate::engine::netlist::build_canonical_connectivity_with_annotations(
        &snapshot.components,
        &snapshot.wires,
        &restored_annotations,
    );
    assert_eq!(before.pin_nets, after.pin_nets);
    assert_eq!(before.junction_id_nets, after.junction_id_nets);
    assert_eq!(before.junction_nets, after.junction_nets);
    assert_eq!(before.wire_segment_nets, after.wire_segment_nets);
    assert_eq!(
        after.net_for_junction_id(JunctionId(50)),
        Some(after.netlist.wire_nets[&100])
    );
    assert_eq!(after.netlist.wire_nets[&100], after.netlist.wire_nets[&101]);
    assert_eq!(after.netlist.no_connects.len(), 1);
}

#[test]
fn legacy_wire_endpoint_migration_preserves_canonical_connectivity() {
    let mut app = CircuitApp::new();
    app.load_led_demo();
    let before = crate::engine::netlist::build_canonical_connectivity(&app.components, &app.wires);

    let mut saved = SavedCircuit::from_app(&app);
    for wire in &mut saved.wires {
        wire.start = None;
        wire.end = None;
    }
    let (snapshot, notes) = saved.into_snapshot().unwrap();
    let after =
        crate::engine::netlist::build_canonical_connectivity(&snapshot.components, &snapshot.wires);

    assert_eq!(
        before.pin_nets, after.pin_nets,
        "migration notes: {notes:?}"
    );
    assert_eq!(before.wire_segment_nets, after.wire_segment_nets);
    assert_eq!(before.netlist.wire_nets, after.netlist.wire_nets);
}

#[test]
fn canonical_projection_is_shared_by_erc_codegen_and_pcb_sync() {
    let mut app = CircuitApp::new();
    app.load_esp32_oled_demo();
    let connectivity = app.current_connectivity();
    let expected_pin_nets = connectivity.pin_nets.clone();

    let _erc = crate::engine::validation::validate_beginner_rules(&connectivity.netlist);
    let code = generate_arduino_code(&connectivity.netlist);
    let cad =
        crate::model::cad::CadProjectData::from_schematic(&app.components, &connectivity.netlist);

    assert!(code.contains("Wire.begin"));
    for cad_net in &cad.nets {
        for pin in &cad_net.connected_pins {
            assert_eq!(expected_pin_nets.get(pin), Some(&cad_net.net_id));
        }
    }
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
            .components
            .iter()
            .any(|component| component.kind == ComponentKind::Led)
    );
}

#[test]
fn page_switch_and_save_preserve_annotations_per_page() {
    let mut app = CircuitApp::new();
    app.annotations.junction_dots.push(JunctionDot {
        id: JunctionId(10),
        position: Pos2::new(100.0, 100.0),
    });
    app.add_page();
    app.annotations.no_connect_markers.push(NoConnectDot {
        id: 20,
        position: Pos2::new(200.0, 200.0),
    });
    app.save_current_page();

    app.switch_page(0);
    assert_eq!(app.annotations.junction_dots[0].id, JunctionId(10));
    assert!(app.annotations.no_connect_markers.is_empty());
    app.switch_page(1);
    assert!(app.annotations.junction_dots.is_empty());
    assert_eq!(app.annotations.no_connect_markers[0].id, 20);

    let json = serde_json::to_string(&SavedCircuit::from_app(&app)).unwrap();
    let (snapshot, notes) = serde_json::from_str::<SavedCircuit>(&json)
        .unwrap()
        .into_snapshot()
        .unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(
        snapshot.pages[0].annotations.junction_dots[0].id,
        JunctionId(10)
    );
    assert_eq!(snapshot.pages[1].annotations.no_connect_markers[0].id, 20);
}

#[test]
fn legacy_single_page_annotations_load_without_schema_change() {
    let saved = SavedCircuit {
        schema_version: 4,
        next_id: 2,
        counters: Counters::default(),
        components: Vec::new(),
        wires: Vec::new(),
        junction_dots: vec![SavedJunctionDot {
            id: 1,
            x: 25.0,
            y: 50.0,
        }],
        no_connect_markers: vec![SavedNoConnectMarker {
            id: 2,
            x: 75.0,
            y: 100.0,
        }],
        pages: Vec::new(),
        current_page: 0,
    };

    let (snapshot, notes) = saved.into_snapshot().unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(snapshot.annotations.junction_dots[0].id, JunctionId(1));
    assert_eq!(snapshot.annotations.no_connect_markers[0].id, 2);
    assert_eq!(snapshot.pages[0].annotations, snapshot.annotations);
    assert_eq!(snapshot.next_id, 3);
}

#[test]
fn annotation_changes_are_undoable_entity_deltas() {
    let mut app = CircuitApp::new();
    app.record_history();
    app.annotations.junction_dots.push(JunctionDot {
        id: JunctionId(10),
        position: Pos2::new(100.0, 100.0),
    });
    app.mark_dirty();

    app.undo();
    assert!(app.annotations.junction_dots.is_empty());
    app.redo();
    assert_eq!(app.annotations.junction_dots[0].id, JunctionId(10));
}

#[test]
fn page_switch_does_not_dirty_or_reuse_stale_simulation() {
    let mut app = CircuitApp::new();
    app.load_led_demo();
    app.add_page();
    app.editor.history.dirty = false;

    let blank = app.current_simulation();
    assert_eq!(blank.summary, "No source or return");

    app.switch_page(0);
    assert!(
        !app.editor.history.dirty,
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
                part_id: None,
            },
            SavedComponent {
                id: 1,
                kind: ComponentKind::Battery,
                x: 200.0,
                y: 100.0,
                rotation: 0,
                label: "BAT1".to_string(),
                value: "9V".to_string(),
                part_id: None,
            },
            SavedComponent {
                id: 3,
                kind: ComponentKind::Led,
                x: f32::NAN,
                y: 100.0,
                rotation: 0,
                label: "LED1".to_string(),
                value: "red".to_string(),
                part_id: None,
            },
        ],
        wires: vec![
            SavedWire {
                id: 1,
                points: vec![
                    SavedPoint { x: 100.0, y: 100.0 },
                    SavedPoint { x: 160.0, y: 100.0 },
                ],
                start: None,
                end: None,
            },
            SavedWire {
                id: 4,
                points: vec![SavedPoint { x: 0.0, y: 0.0 }],
                start: None,
                end: None,
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
    app.editor.selected = Some(Selection::Component(resistor));
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
        !sim.component_warnings.contains_key(&oled_id),
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
        part_id: None,
    };
    let r1 = Component {
        id: 2,
        kind: ComponentKind::Resistor,
        pos: Pos2::new(300.0, 0.0),
        rotation: 0,
        label: "R1".to_string(),
        value: "1k".to_string(),
        part_id: None,
    };
    let r2 = Component {
        id: 3,
        kind: ComponentKind::Resistor,
        pos: Pos2::new(164.0, 36.0),
        rotation: 90,
        label: "R2".to_string(),
        value: "1k".to_string(),
        part_id: None,
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
        Wire::new(10, vec![bat_p, r2_a, r1_a]),
        Wire::new(11, vec![r1_b, Pos2::new(r1_b.x, 80.0), bat_n]),
        Wire::new(12, vec![r2_b, Pos2::new(r2_b.x, 120.0), bat_n]),
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
    assert!(!sim.component_warnings.contains_key(&sensor_id));
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
        route.from_pin.contains("GPIO21") && route.to_pin == "SDA" && route.purpose == "I2C data"
    }));
    assert!(guide.routes.iter().any(|route| {
        route.from_pin.contains("GPIO22") && route.to_pin == "SCL" && route.purpose == "I2C clock"
    }));
}

#[test]
fn breadboard_route_can_add_missing_jumper_to_schematic() {
    let mut app = CircuitApp::new();
    app.load_esp32_oled_demo();
    app.wires.clear();
    app.mark_dirty();

    let netlist = build_circuit_netlist(&app.components, &app.wires);
    let guide = build_breadboard_guide(&app.components, &netlist);
    let route = guide
        .routes
        .iter()
        .find(|route| route.from_pin.contains("GPIO21") && route.to_pin == "SDA")
        .cloned()
        .expect("ESP32 OLED guide should expose SDA route");
    assert!(!route.connected);

    app.connect_breadboard_route(route);

    let netlist = build_circuit_netlist(&app.components, &app.wires);
    let guide = build_breadboard_guide(&app.components, &netlist);
    assert!(guide.routes.iter().any(|route| {
        route.from_pin.contains("GPIO21") && route.to_pin == "SDA" && route.connected
    }));
    assert!(app.status.contains("Added jumper"));
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
fn update_pcb_syncs_footprints_ratsnest_and_dirty_state() {
    let mut app = CircuitApp::new();
    app.load_led_demo();
    app.update_pcb_from_schematic();

    let summary = app.pcb_dock_summary();
    assert!(summary.footprint_count >= 2, "{summary:?}");
    assert!(summary.ratsnest_count > 0, "{summary:?}");
    assert!(!summary.dirty, "{summary:?}");
    assert!(app.status.contains("PCB updated"));

    app.mark_dirty();
    assert!(app.pcb_dock_summary().dirty);
}

#[test]
fn pcb_auto_place_and_route_reduce_unplaced_and_ratsnest_counts() {
    let mut app = CircuitApp::new();
    app.load_led_demo();
    app.update_pcb_from_schematic();
    let before = app.pcb_dock_summary();
    assert!(before.unplaced_count > 0, "{before:?}");
    assert!(before.ratsnest_count > 0, "{before:?}");

    app.auto_place_pcb_footprints();
    let placed = app.pcb_dock_summary();
    assert_eq!(placed.unplaced_count, 0, "{placed:?}");
    assert!(
        app.document
            .board
            .footprints
            .iter()
            .all(|footprint| footprint.placed),
        "{:?}",
        app.document.board.footprints
    );

    app.route_pcb_ratsnest();
    let routed = app.pcb_dock_summary();
    assert_eq!(routed.ratsnest_count, 0, "{routed:?}");
    assert!(!app.document.board.tracks.is_empty());
    assert!(app.status.contains("track"));
}

#[test]
fn pcb_fit_board_contains_placed_footprints_and_tracks() {
    let mut app = CircuitApp::new();
    app.load_led_demo();
    app.update_pcb_from_schematic();
    app.auto_place_pcb_footprints();
    app.route_pcb_ratsnest();

    if let Some(footprint) = app.document.board.footprints.first_mut() {
        footprint.position.x = 130.0;
        footprint.position.y = 90.0;
    }
    app.fit_pcb_board_to_contents();

    let summary = app.pcb_dock_summary();
    assert!(summary.preview.width_mm >= 25.0, "{summary:?}");
    assert!(summary.preview.height_mm >= 20.0, "{summary:?}");
    assert!(summary.preview.footprints.iter().all(|footprint| {
        footprint.x_mm >= 0.0
            && footprint.y_mm >= 0.0
            && footprint.x_mm <= summary.preview.width_mm
            && footprint.y_mm <= summary.preview.height_mm
    }));
    assert!(app.status.contains("Fit PCB board"));
}

#[test]
fn project_folder_save_writes_schematic_board_and_project_json() {
    let root = std::env::temp_dir().join(format!("cluster-project-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);

    let mut app = CircuitApp::new();
    app.load_led_demo();
    app.update_pcb_from_schematic();
    app.auto_place_pcb_footprints();
    app.route_pcb_ratsnest();
    app.save_project_folder_to(&root).unwrap();

    assert!(root.join("schematic.json").exists());
    assert!(root.join("board.json").exists());
    assert!(root.join("project.json").exists());
    let board_json = fs::read_to_string(root.join("board.json")).unwrap();
    assert!(board_json.contains("\"tracks\""));
    let project_json = fs::read_to_string(root.join("project.json")).unwrap();
    assert!(project_json.contains("\"document_revision\""));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn pcb_drc_selection_updates_summary_preview_and_schematic_focus() {
    let mut app = CircuitApp::new();
    app.load_led_demo();
    app.update_pcb_from_schematic();
    let footprint = app.document.board.footprints.first().cloned().unwrap();
    app.document.board.outline = crate::pcb::board::BoardOutline::rectangular(1.0, 1.0);
    let cad = app.analysis.pcb_cad.clone().unwrap();
    app.analysis.pcb_drc = crate::pcb::drc::run_drc_with_nets(&app.document.board, &cad.nets);
    let drc_index = app
        .analysis
        .pcb_drc
        .iter()
        .position(|violation| violation.object_id == Some(footprint.id))
        .expect("footprint outside board should be tied to the footprint object");

    app.select_pcb_drc_violation(drc_index);
    let summary = app.pcb_dock_summary();

    assert!(summary.drc.iter().any(|row| row.selected));
    assert!(
        summary
            .preview
            .diagnostics
            .iter()
            .any(|marker| marker.selected)
    );
    assert_eq!(
        app.editor.selected,
        footprint
            .symbol_instance_id
            .map(crate::app::Selection::Component)
    );
    assert!(app.status.contains("Selected PCB DRC"));
}

#[test]
fn pcb_fabrication_export_is_blocked_by_drc_errors() {
    let mut app = CircuitApp::new();
    app.load_led_demo();
    app.update_pcb_from_schematic();
    app.document
        .board
        .tracks
        .push(crate::pcb::track::TrackSegment {
            id: 999,
            net_id: 1,
            layer: crate::pcb::layer::BoardLayer::FrontCopper,
            start: crate::model::cad::Point2::new(0.01, 0.01),
            end: crate::model::cad::Point2::new(4.0, 0.01),
            width_mm: 0.01,
        });
    let cad = app.analysis.pcb_cad.clone().unwrap();
    app.analysis.pcb_drc = crate::pcb::drc::run_drc_with_nets(&app.document.board, &cad.nets);

    app.export_pcb_fabrication_files();

    assert!(app.status.contains("export blocked"), "{}", app.status);
}

#[test]
fn project_folder_load_restores_schematic_board_and_pcb_analysis() {
    let root =
        std::env::temp_dir().join(format!("cluster-project-load-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);

    let mut saved_app = CircuitApp::new();
    saved_app.load_led_demo();
    saved_app.update_pcb_from_schematic();
    saved_app.auto_place_pcb_footprints();
    saved_app.route_pcb_ratsnest();
    saved_app.save_project_folder_to(&root).unwrap();
    let saved_component_count = saved_app.components.len();
    let saved_track_count = saved_app.document.board.tracks.len();

    let mut loaded_app = CircuitApp::new();
    loaded_app.load_project_folder_from(&root).unwrap();

    assert_eq!(loaded_app.components.len(), saved_component_count);
    assert_eq!(loaded_app.document.board.tracks.len(), saved_track_count);
    assert!(!loaded_app.pcb_dock_summary().dirty);
    assert!(loaded_app.analysis.pcb_cad.is_some());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ac_frequency_is_part_of_simulation_cache_key() {
    let mut app = CircuitApp::new();
    app.load_led_demo();

    let _ = app.current_simulation();
    let first_key = app
        .analysis
        .cached_simulation
        .as_ref()
        .map(|(_, key, _)| *key)
        .unwrap();

    app.simulation_ui.ac_freq_hz = 10_000.0;
    let _ = app.current_simulation();
    let second_key = app
        .analysis
        .cached_simulation
        .as_ref()
        .map(|(_, key, _)| *key)
        .unwrap();

    assert_ne!(first_key, second_key);
    assert_eq!(second_key, 10_000.0f32.to_bits());
}

// ── Custom parts (cluster_parts/*.json) ──────────────────────────────────────

fn register_test_part(id: &str, prefix: &str) {
    let def = crate::model::custom_part::parse_custom_part_json(&format!(
        r#"{{
            "id": "{id}",
            "name": "Test Part {id}",
            "label_prefix": "{prefix}",
            "default_value": "TP-01",
            "pins": [
                {{"name": "VCC", "role": "positive", "side": "left"}},
                {{"name": "GND", "role": "ground", "side": "left"}},
                {{"name": "OUT", "role": "output", "side": "right"}}
            ]
        }}"#
    ))
    .expect("test part json parses");
    crate::model::custom_part::register_custom_part(def);
}

#[test]
fn placing_custom_part_uses_definition_pins_label_and_value() {
    register_test_part("user:test-place", "SNS");
    let mut app = CircuitApp::new();

    let id = app.place_custom_component("user:test-place", Pos2::new(200.0, 200.0));

    let component = app
        .components
        .iter()
        .find(|component| component.id == id)
        .unwrap();
    assert_eq!(component.kind, ComponentKind::Custom);
    assert_eq!(component.part_id.as_deref(), Some("user:test-place"));
    assert_eq!(component.label, "SNS1");
    assert_eq!(component.value, "TP-01");

    let pins = component_pin_defs(component);
    assert_eq!(pins.len(), 3);
    assert!(pins.iter().any(|pin| pin.label == "OUT"));

    let second = app.place_custom_component("user:test-place", Pos2::new(400.0, 200.0));
    let second = app
        .components
        .iter()
        .find(|component| component.id == second)
        .unwrap();
    assert_eq!(second.label, "SNS2");
}

#[test]
fn custom_part_wires_into_netlist_like_builtin_parts() {
    register_test_part("user:test-netlist", "CN");
    let mut app = CircuitApp::new();
    let part = app.place_custom_component("user:test-netlist", Pos2::new(200.0, 200.0));
    let resistor = app.place_component(ComponentKind::Resistor, Pos2::new(420.0, 200.0));
    app.add_wire_between(part, "OUT", resistor, "A");

    let netlist = build_circuit_netlist(&app.components, &app.wires);

    let out_net = netlist
        .pins
        .iter()
        .find(|pin| pin.component_id == part && pin.pin_name == "OUT")
        .map(|pin| pin.net_id)
        .expect("custom OUT pin is in the netlist");
    let resistor_net = netlist
        .pins
        .iter()
        .find(|pin| pin.component_id == resistor && pin.pin_name == "A")
        .map(|pin| pin.net_id)
        .expect("resistor pin is in the netlist");
    assert_eq!(out_net, resistor_net);
}

#[test]
fn custom_part_round_trips_part_id_through_save_and_load() {
    register_test_part("user:test-roundtrip", "RT");
    let mut app = CircuitApp::new();
    app.place_custom_component("user:test-roundtrip", Pos2::new(200.0, 200.0));

    let json = serde_json::to_string(&SavedCircuit::from_app(&app)).unwrap();
    assert!(json.contains("user:test-roundtrip"));
    let saved = serde_json::from_str::<SavedCircuit>(&json).unwrap();
    let (snapshot, load_notes) = saved.into_snapshot().unwrap();

    assert!(load_notes.is_empty(), "notes: {load_notes:?}");
    let restored = snapshot
        .components
        .iter()
        .find(|component| component.kind == ComponentKind::Custom)
        .unwrap();
    assert_eq!(restored.part_id.as_deref(), Some("user:test-roundtrip"));
    assert_eq!(component_pin_defs(restored).len(), 3);
}

#[test]
fn loading_circuit_with_unknown_custom_part_reports_note_and_keeps_component() {
    register_test_part("user:test-known", "KN");
    let mut app = CircuitApp::new();
    app.place_custom_component("user:test-known", Pos2::new(200.0, 200.0));

    let json = serde_json::to_string(&SavedCircuit::from_app(&app))
        .unwrap()
        .replace("user:test-known", "user:never-registered-xyz");
    let saved = serde_json::from_str::<SavedCircuit>(&json).unwrap();
    let (snapshot, load_notes) = saved.into_snapshot().unwrap();

    assert!(
        load_notes
            .iter()
            .any(|note| note.contains("user:never-registered-xyz")),
        "notes: {load_notes:?}"
    );
    let restored = snapshot
        .components
        .iter()
        .find(|component| component.kind == ComponentKind::Custom)
        .unwrap();
    assert_eq!(
        restored.part_id.as_deref(),
        Some("user:never-registered-xyz")
    );
    assert!(component_pin_defs(restored).is_empty());
}

#[test]
fn old_schema_component_without_part_id_field_still_loads() {
    let json = r#"{
        "id": 7, "kind": "Resistor", "x": 100.0, "y": 100.0,
        "rotation": 0, "label": "R1", "value": "10k"
    }"#;
    let saved: SavedComponent = serde_json::from_str(json).unwrap();
    assert_eq!(saved.part_id, None);
}
