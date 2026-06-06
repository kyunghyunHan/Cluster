# Cluster

**Beginner-friendly ESP32/Arduino circuit design with real validation and DC simulation.**

Cluster is a schematic editor and circuit simulator focused on making electronics accessible to makers, students, and hobbyists who use ESP32 and Arduino boards.

---

## Why Cluster?

| Tool | Ease of use | Smart validation | DC simulation | ESP32 focus |
|---|---|---|---|---|
| KiCad | Hard | ✅ | External (SPICE) | Generic |
| Fritzing | Easy | ❌ | ❌ | ✅ |
| Tinkercad | Easy | ✅ | ✅ | Limited |
| **Cluster** | **Easy** | **✅** | **✅** | **✅** |

Cluster is **easier than KiCad**, **smarter than Fritzing**, and **focused on real beginner circuits**.

---

## Features

### Circuit Design
- Drag-and-drop component palette with search
- Orthogonal wire routing with T-junction detection
- Grid snapping and zoom/pan
- Multi-page schematics
- Component labels, values, and rotation
- Undo/redo (80-step history)
- Copy/paste and group selection
- Find, align, and distribute components

### Supported Components
- **Microcontrollers**: ESP32, ESP32-S3, ESP32-C3, Arduino Uno, Raspberry Pi Pico
- **Passives**: Resistor, Capacitor, Inductor, Potentiometer, Thermistor, Varistor
- **Diodes**: Standard, LED, Zener, Schottky, TVS, Phototransistor
- **Transistors**: NPN, PNP, N-MOSFET, P-MOSFET
- **Power**: Battery, Voltage source, Current source, Voltage regulator, Voltage reference
- **Switches**: Toggle switch, Push button, Slide switch, Relay
- **ICs**: Op-amp, 555 Timer, Logic gates (AND/OR/NOT/NAND/NOR/XOR), Generic IC, Optocoupler
- **Peripherals**: OLED display, DC motor, Servo, Sensor, Motor driver, 7-segment display
- **Misc**: Crystal, Transformer, Fuse, Lamp, Breadboard, Net label

### Validation (ERC)

Cluster runs automatic Electrical Rule Checks after every change:

**Errors (must fix):**
- Power short: supply connected directly to GND
- No GND reference in schematic
- ESP32/Arduino 3.3V pin connected to 5V rail
- GPIO driving a motor directly (use a driver IC)
- Reversed LED polarity (anode on GND net)
- Reversed diode polarity
- OLED SDA/SCL lines swapped

**Warnings (should fix):**
- LED without a current-limiting resistor
- Resistor with no resistance value
- Battery or voltage source with no voltage value
- Floating wire not connected to any component
- Unconnected component pin

Clicking a validation message selects the offending component or wire on the canvas.

### DC Simulation

Cluster uses Modified Nodal Analysis (MNA) to compute:
- **Net voltages** — voltage at every node in volts
- **Branch currents** — current through every component in amps
- **Component power** — power dissipated in watts

Simulated components: Resistor, Battery, Voltage/Current source, LED (Vf≈2V), Diode (Vf≈0.65V), Zener, Schottky, Switch (open/closed), Potentiometer, Fuse, Lamp, DC Motor, NPN/PNP transistor (linearised), MOSFET (linearised), 555 Timer (stub), Relay coil.

Energized wires are highlighted in orange. Voltage labels can be toggled on wires.

### Export
- **SVG** — schematic as a scalable vector graphic (`cluster_circuit.svg`)
- **SPICE netlist** — for use in LTspice or similar (`cluster_circuit.sp`)
- **BOM CSV** — bill of materials for part ordering (`cluster_bom.csv`)
- **Arduino code** — starter sketch for I2C peripherals

### Save & Load
- JSON format with schema version tracking
- Automatic `.bak` backup before every manual save
- Autosave to `cluster_autorecover.json` every 30 seconds
- Corrupt or old-schema files are repaired on load — never silently discarded

---

## Installation

### Prerequisites
- [Rust](https://rustup.rs/) 1.80 or later (2024 edition)

### Build from source
```bash
git clone https://github.com/your-username/Cluster
cd Cluster
cargo run --release
```

Release binaries appear in `target/release/Cluster`.

---

## Usage

### Placing Components
1. Click a component category in the left palette (or type in the search box).
2. Click on the canvas to place it.
3. Press **R** to rotate before or after placing.

### Drawing Wires
1. Press **W** or click the wire tool in the toolbar.
2. Click a component pin to start a wire.
3. Click another pin to complete the connection.
4. Wires snap to pins automatically. T-junctions are detected and merged.

### Keyboard Shortcuts

| Key | Action |
|---|---|
| `W` | Wire tool |
| `Esc` | Select tool / cancel current action |
| `R` | Rotate selected component |
| `Delete` | Delete selected |
| `Ctrl+Z` | Undo |
| `Ctrl+Y` | Redo |
| `Ctrl+C` | Copy selection |
| `Ctrl+V` | Paste |
| `Ctrl+S` | Save circuit |
| `Ctrl+O` | Load circuit |
| `F` | Zoom to fit |
| `Space` | Toggle simulation on/off |
| `Ctrl+F` | Find component |

### Running the Simulation
Toggle simulation with **Space** or the toolbar button. Energized paths light up in orange/yellow. Click any wire or component to inspect its voltage, current, and power dissipation in the right panel.

### Reading Validation Results
The ERC (Electrical Rules Check) panel at the bottom shows errors and warnings in real time. Red items are errors that will prevent correct operation. Yellow items are warnings. Click any message to jump to the relevant component or wire.

---

## Project Structure

```
src/
  main.rs               # App entry point, UI event loop, canvas drawing
  model/
    component.rs        # ComponentKind enum, Component struct
    pin.rs              # PinRole, CircuitPin, NetlistPin
    wire.rs             # Wire
    net.rs              # Net, CircuitNetlist
    circuit.rs          # Counters, snapshots, save/load types
  engine/
    netlist.rs          # Netlist builder (union-find, T-junction detection)
    validation.rs       # ERC rules engine (10+ rules)
    simulation.rs       # Simulation result wrapper
    mna.rs              # Modified Nodal Analysis DC solver
  ui/
    validation_panel.rs # ERC panel UI renderer
  storage/
    save.rs             # Serialisation helpers, backup write
    autosave.rs         # Autosave timer utility
  export/
    svg.rs              # SVG schematic export
```

---

## Simulation Limitations

Cluster's MNA solver is educational-grade, not a drop-in SPICE replacement:

- **DC operating point only** — no transient, AC, or frequency sweep
- Capacitors are open circuits in DC analysis
- Inductors are short circuits in DC analysis
- Transistors and MOSFETs use simplified linearised models
- Singular or non-convergent circuits return a safe failure (no panic)

For production simulation, use the SPICE export with LTspice or ngspice.

---

## Roadmap

- [ ] Full Arduino sketch generation from schematic
- [ ] Breadboard view
- [ ] Component library search by part number
- [ ] Interactive value sliders in simulation
- [ ] PCB layout export (KiCad format)
- [ ] WASM web build

---

## Contributing

Issues and pull requests welcome. Please open an issue first for large changes.

---

## License

MIT — see [LICENSE](LICENSE).

---

*Built with [egui](https://github.com/emilk/egui) and [eframe](https://github.com/emilk/egui/tree/master/crates/eframe).*
