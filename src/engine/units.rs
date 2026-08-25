/// Parse a human-readable SI value string into an `f32`.
///
/// Recognises SI multipliers (p, n, u, m, k, M, Meg). Uppercase `M` and
/// `Meg` mean mega; lowercase `m` always means milli, matching SPICE.
///
/// # Examples
/// ```
/// assert_eq!(parse_metric_value("10k", "ohm"), Some(10_000.0));
/// assert_eq!(parse_metric_value("100nF", "f"),  Some(100e-9));
/// assert_eq!(parse_metric_value("3.3V", "v"),   Some(3.3));
/// ```
pub(crate) fn parse_metric_value(value: &str, unit_hint: &str) -> Option<f32> {
    parse_metric_value_f64(value, unit_hint).map(|value| value as f32)
}

pub(crate) fn parse_metric_value_f64(value: &str, unit_hint: &str) -> Option<f64> {
    let normalized = value
        .trim()
        .replace('\u{03a9}', "ohm")
        .replace(['\u{00b5}', '\u{03bc}'], "u")
        .replace(char::is_whitespace, "");
    if let Some(parsed) = parse_embedded_multiplier(&normalized, unit_hint) {
        return parsed;
    }
    let number_end = metric_number_end(&normalized);
    if number_end == 0 {
        return None;
    }
    let number = normalized.get(..number_end)?.parse::<f64>().ok()?;
    if !number.is_finite() {
        return None;
    }
    let suffix = normalized.get(number_end..)?.trim();
    let suffix_lower = suffix.to_ascii_lowercase();
    let (multiplier, unit) = if suffix_lower.starts_with("mega") {
        (1_000_000.0, suffix.get(4..)?)
    } else if suffix_lower.starts_with("meg") {
        (1_000_000.0, suffix.get(3..)?)
    } else if suffix.starts_with('M') {
        (1_000_000.0, suffix.get(1..)?)
    } else if suffix.starts_with(['k', 'K']) {
        (1_000.0, suffix.get(1..)?)
    } else if suffix.starts_with('m') {
        (0.001, suffix.get(1..)?)
    } else if suffix.starts_with(['u', 'U']) {
        (0.000_001, suffix.get(1..)?)
    } else if suffix.starts_with(['n', 'N']) {
        (0.000_000_001, suffix.get(1..)?)
    } else if suffix.starts_with(['p', 'P']) {
        (0.000_000_000_001, suffix.get(1..)?)
    } else if suffix.starts_with('f') && !unit_matches("f", unit_hint) {
        (0.000_000_000_000_001, suffix.get(1..)?)
    } else {
        (1.0, suffix)
    };
    if !unit_matches(unit, unit_hint) {
        return None;
    }
    let value = number * multiplier;
    value.is_finite().then_some(value)
}

/// Parse a numeric value that may be followed by source annotations such as
/// `PWM 100Hz 25%`. This is intentionally separate from the strict editor
/// parser so arbitrary trailing text is not accepted for normal components.
pub(crate) fn parse_leading_metric_value(value: &str, unit_hint: &str) -> Option<f32> {
    parse_metric_value(value, unit_hint).or_else(|| {
        value
            .split(|character: char| {
                character.is_ascii_whitespace() || matches!(character, ',' | ';')
            })
            .find_map(|token| parse_metric_value(token, unit_hint))
    })
}

/// Convert an editor value to an unambiguous SPICE numeric literal.
///
/// SPICE treats a bare `M` as milli while Cluster's editor follows normal SI
/// notation (`M` = mega). Exporting scientific notation prevents the same
/// project from changing meaning between the built-in solver and ngspice.
pub(crate) fn normalize_spice_value(value: &str, unit_hint: &str, fallback: &str) -> String {
    let parsed = parse_metric_value_f64(value, unit_hint)
        .or_else(|| parse_metric_value_f64(fallback, unit_hint))
        .unwrap_or(0.0);
    format!("{parsed:.12e}")
}

/// Validate values edited by a user without mutating persisted legacy data.
/// Empty values remain allowed so ERC can report the existing "missing value"
/// issue and users can clear a field while typing a replacement.
pub(crate) fn component_value_error(
    kind: crate::model::ComponentKind,
    value: &str,
) -> Option<&'static str> {
    use crate::model::ComponentKind;

    if value.trim().is_empty() {
        return None;
    }
    let (unit, positive) = match kind {
        ComponentKind::Resistor
        | ComponentKind::Potentiometer
        | ComponentKind::Thermistor
        | ComponentKind::Varistor => ("ohm", true),
        ComponentKind::Capacitor => ("f", true),
        ComponentKind::Inductor => ("h", true),
        ComponentKind::Fuse => ("a", true),
        ComponentKind::VSource | ComponentKind::Battery | ComponentKind::VoltageRef => {
            let parsed = parse_leading_metric_value(value, "v");
            return match parsed {
                Some(_) => None,
                None => Some("Enter a voltage such as 3.3V or 5V."),
            };
        }
        ComponentKind::ISource => ("a", false),
        _ => return None,
    };
    match parse_metric_value(value, unit) {
        None => Some(match unit {
            "ohm" => "Enter a resistance such as 220R, 4.7k, or 1M.",
            "f" => "Enter a capacitance such as 100nF or 10uF.",
            "h" => "Enter an inductance such as 10uH or 1mH.",
            "a" => "Enter a current such as 20mA or 1A.",
            _ => "Enter a valid engineering value.",
        }),
        Some(parsed) if positive && parsed <= 0.0 => Some("Value must be greater than zero."),
        Some(_) => None,
    }
}

fn parse_embedded_multiplier(value: &str, unit_hint: &str) -> Option<Option<f64>> {
    for (index, prefix) in value.char_indices() {
        let multiplier = match prefix {
            'p' | 'P' => 1e-12,
            'n' | 'N' => 1e-9,
            'u' | 'U' => 1e-6,
            'm' => 1e-3,
            'k' | 'K' => 1e3,
            'M' => 1e6,
            'R' | 'r'
                if matches!(
                    unit_hint.to_ascii_lowercase().as_str(),
                    "ohm" | "ohms" | "r"
                ) =>
            {
                1.0
            }
            _ => continue,
        };
        let left = value.get(..index)?;
        let after_prefix = value.get(index + prefix.len_utf8()..)?;
        let fractional_end = after_prefix
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(after_prefix.len());
        if left.is_empty() || fractional_end == 0 {
            continue;
        }
        let right = after_prefix.get(..fractional_end)?;
        let unit = after_prefix.get(fractional_end..)?;
        if !unit_matches(unit, unit_hint) {
            return Some(None);
        }
        if !left.chars().enumerate().all(|(position, character)| {
            character.is_ascii_digit() || (position == 0 && matches!(character, '+' | '-'))
        }) {
            continue;
        }
        let number = format!("{left}.{right}").parse::<f64>().ok()?;
        let parsed = number * multiplier;
        return Some(parsed.is_finite().then_some(parsed));
    }
    None
}

fn unit_matches(unit: &str, hint: &str) -> bool {
    let unit = unit.to_ascii_lowercase();
    let hint = hint.to_ascii_lowercase();
    if unit.is_empty() {
        return true;
    }
    if hint.is_empty() {
        return matches!(
            unit.as_str(),
            "r" | "ohm" | "ohms" | "f" | "h" | "v" | "vdc" | "vac" | "a" | "w" | "hz" | "s"
        );
    }
    match hint.as_str() {
        "ohm" | "ohms" | "r" => matches!(unit.as_str(), "r" | "ohm" | "ohms"),
        "v" => matches!(unit.as_str(), "v" | "vdc" | "vac"),
        "a" => unit == "a",
        "w" => unit == "w",
        "f" => unit == "f",
        "h" => unit == "h",
        "hz" => unit == "hz",
        "s" => unit == "s",
        _ => unit == hint,
    }
}

fn metric_number_end(value: &str) -> usize {
    let mut end = 0usize;
    let mut chars = value.char_indices().peekable();

    if let Some((idx, ch)) = chars.peek().copied()
        && idx == 0
        && matches!(ch, '+' | '-')
    {
        end = ch.len_utf8();
        chars.next();
    }

    let mut saw_digit = false;
    let mut saw_dot = false;
    while let Some((idx, ch)) = chars.peek().copied() {
        if ch.is_ascii_digit() {
            saw_digit = true;
            end = idx + ch.len_utf8();
            chars.next();
        } else if ch == '.' && !saw_dot {
            saw_dot = true;
            end = idx + ch.len_utf8();
            chars.next();
        } else {
            break;
        }
    }

    if !saw_digit {
        return 0;
    }

    if let Some((exp_idx, exp_ch)) = chars.peek().copied()
        && matches!(exp_ch, 'e' | 'E')
    {
        let mut probe = chars.clone();
        probe.next();
        if let Some((_, sign_ch)) = probe.peek().copied()
            && matches!(sign_ch, '+' | '-')
        {
            probe.next();
        }

        let mut exp_end = exp_idx + exp_ch.len_utf8();
        let mut exp_digits = false;
        for (idx, ch) in probe {
            if ch.is_ascii_digit() {
                exp_digits = true;
                exp_end = idx + ch.len_utf8();
            } else {
                break;
            }
        }
        if exp_digits {
            end = exp_end;
        }
    }

    end
}

#[cfg(test)]
mod tests {
    use super::{
        component_value_error, normalize_spice_value, parse_leading_metric_value,
        parse_metric_value,
    };
    use crate::model::ComponentKind;

    #[test]
    fn parses_signed_and_exponential_metric_values() {
        assert_eq!(parse_metric_value("-5V", "v"), Some(-5.0));
        assert_eq!(parse_metric_value("+3.3V", "v"), Some(3.3));
        assert_eq!(parse_metric_value("1e3", "ohm"), Some(1_000.0));
        assert_eq!(parse_metric_value("1e-6F", "f"), Some(1e-6));
        assert_eq!(parse_metric_value("2.2e3ohm", "ohm"), Some(2_200.0));
    }

    #[test]
    fn rejects_non_finite_metric_values() {
        assert_eq!(parse_metric_value("1e1000V", "v"), None);
    }

    #[test]
    fn parses_common_editor_and_spice_multiplier_forms() {
        assert_eq!(parse_metric_value("1 kΩ", "ohm"), Some(1_000.0));
        assert_eq!(parse_metric_value("4u7", "f"), Some(4.7e-6));
        assert_eq!(parse_metric_value("4.7µF", "f"), Some(4.7e-6));
        assert_eq!(parse_metric_value("1Meg", "ohm"), Some(1_000_000.0));
        assert_eq!(parse_metric_value("1M", "ohm"), Some(1_000_000.0));
        assert_eq!(parse_metric_value("1m", "ohm"), Some(0.001));
        let current = parse_metric_value("2.2mA", "a").expect("valid current");
        assert!((current - 0.0022).abs() < 1e-8);
        assert_eq!(parse_metric_value("100Hz", "hz"), Some(100.0));
        assert_eq!(parse_metric_value("1ms", "s"), Some(0.001));
        assert_eq!(parse_metric_value("10R", "ohm"), Some(10.0));
        assert_eq!(parse_metric_value("4k7", "ohm"), Some(4_700.0));
        assert_eq!(parse_metric_value("4R7", "ohm"), Some(4.7));
        assert_eq!(parse_metric_value("4u7F", "f"), Some(4.7e-6));
        assert_eq!(parse_metric_value("3.3VDC", "v"), Some(3.3));
        assert_eq!(parse_metric_value("10 bananas", "ohm"), None);
        assert_eq!(parse_metric_value("100nV", "f"), None);
        assert_eq!(parse_metric_value("4k7V", "ohm"), None);
    }

    #[test]
    fn spice_normalization_never_emits_ambiguous_uppercase_m() {
        assert_eq!(normalize_spice_value("1M", "ohm", "1k"), "1.000000000000e6");
        assert_eq!(
            normalize_spice_value("10 bananas", "ohm", "1k"),
            "1.000000000000e3"
        );
        assert_eq!(
            normalize_spice_value("4u7F", "f", "100n"),
            "4.700000000000e-6"
        );
    }

    #[test]
    fn annotated_sources_parse_without_weakening_strict_values() {
        assert_eq!(
            parse_leading_metric_value("3.3V PWM 1kHz 25%", "v"),
            Some(3.3)
        );
        assert_eq!(parse_metric_value("3.3V PWM 1kHz 25%", "v"), None);
    }

    #[test]
    fn component_value_validation_is_typed_and_allows_legacy_empty_values() {
        assert_eq!(component_value_error(ComponentKind::Resistor, "4.7k"), None);
        assert!(component_value_error(ComponentKind::Resistor, "10 bananas").is_some());
        assert!(component_value_error(ComponentKind::Capacitor, "100nV").is_some());
        assert!(component_value_error(ComponentKind::Inductor, "0H").is_some());
        assert_eq!(
            component_value_error(ComponentKind::VSource, "3.3V PWM 1kHz"),
            None
        );
        assert_eq!(component_value_error(ComponentKind::Resistor, ""), None);
        assert_eq!(component_value_error(ComponentKind::Led, "red"), None);
    }
}
