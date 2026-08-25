//! SI value parser and display-formatting helpers for the MNA solver.
//!
//! These functions are purely presentational — they format raw f64 values for
//! the canvas overlay and the inspector panel.

/// Parse a value string with optional SI suffix into a plain `f64`.
///
/// Examples: `"10k"` → 10 000.0,  `"100nF"` → 100e-9,  `"3.3V"` → 3.3,
///           `"10mA"` → 0.01,  `"1Meg"` → 1 000 000.0
pub fn parse_si_value(s: &str) -> Option<f64> {
    crate::engine::units::parse_metric_value_f64(s, "")
}

pub fn format_voltage(v: f64) -> String {
    if v.abs() >= 1000.0 {
        format!("{:.1}kV", v / 1000.0)
    } else if v.abs() >= 1.0 {
        format!("{:.3}V", v)
    } else if v.abs() >= 0.001 {
        format!("{:.1}mV", v * 1000.0)
    } else {
        format!("{:.1}µV", v * 1_000_000.0)
    }
}

pub fn format_current(i: f64) -> String {
    let a = i.abs();
    if a >= 1.0 {
        format!("{:.3}A", i)
    } else if a >= 0.001 {
        format!("{:.2}mA", i * 1000.0)
    } else if a >= 1e-6 {
        format!("{:.2}µA", i * 1_000_000.0)
    } else {
        format!("{:.2}nA", i * 1e9)
    }
}

/// Format a value with SI prefix and unit (e.g. 1e-6 F → "1.00µF")
pub fn format_si(val: f64, unit: &str) -> String {
    let a = val.abs();
    if a >= 1.0 {
        format!("{:.3}{}", val, unit)
    } else if a >= 1e-3 {
        format!("{:.3}m{}", val * 1e3, unit)
    } else if a >= 1e-6 {
        format!("{:.3}µ{}", val * 1e6, unit)
    } else if a >= 1e-9 {
        format!("{:.3}n{}", val * 1e9, unit)
    } else {
        format!("{:.3}p{}", val * 1e12, unit)
    }
}

pub fn format_power(w: f64) -> String {
    if w >= 1.0 {
        format!("{:.3}W", w)
    } else if w >= 0.001 {
        format!("{:.2}mW", w * 1000.0)
    } else {
        format!("{:.2}µW", w * 1_000_000.0)
    }
}

/// Map a voltage to a display colour gradient:
/// GND (0 V) → steel-blue, low → cyan, mid → green, high → orange, very-high → red.
pub fn voltage_color(v: f64, vmax: f64) -> egui::Color32 {
    use egui::Color32;
    if vmax < 0.001 {
        return Color32::from_rgb(80, 120, 160);
    }
    let t = (v / vmax).clamp(-1.0, 1.0);
    if t < 0.0 {
        let s = (-t) as f32;
        return Color32::from_rgb(
            (80.0 + s * 120.0) as u8,
            (80.0 - s * 60.0) as u8,
            (200.0 + s * 55.0) as u8,
        );
    }
    let s = t as f32;
    if s < 0.25 {
        let u = s / 0.25;
        Color32::from_rgb(
            (40.0 + u * 20.0) as u8,
            (180.0 + u * 60.0) as u8,
            (220.0 - u * 80.0) as u8,
        )
    } else if s < 0.5 {
        let u = (s - 0.25) / 0.25;
        Color32::from_rgb(
            (60.0 + u * 130.0) as u8,
            (240.0 - u * 30.0) as u8,
            (140.0 - u * 100.0) as u8,
        )
    } else if s < 0.75 {
        let u = (s - 0.5) / 0.25;
        Color32::from_rgb(
            (190.0 + u * 60.0) as u8,
            (210.0 - u * 100.0) as u8,
            (40.0 - u * 30.0) as u8,
        )
    } else {
        let u = (s - 0.75) / 0.25;
        Color32::from_rgb((250.0 - u * 20.0) as u8, (110.0 - u * 100.0) as u8, 10u8)
    }
}
