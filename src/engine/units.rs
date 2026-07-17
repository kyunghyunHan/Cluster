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
pub(crate) fn parse_metric_value(value: &str, _unit_hint: &str) -> Option<f32> {
    let normalized = value
        .trim()
        .replace('\u{03a9}', "ohm")
        .replace('\u{00b5}', "u")
        .replace(char::is_whitespace, "");
    if let Some(parsed) = parse_embedded_multiplier(&normalized) {
        return parsed;
    }
    let number_end = metric_number_end(&normalized);
    if number_end == 0 {
        return None;
    }
    let number = normalized.get(..number_end)?.parse::<f32>().ok()?;
    if !number.is_finite() {
        return None;
    }
    let suffix = normalized.get(number_end..)?.trim();
    let suffix_lower = suffix.to_ascii_lowercase();
    let multiplier = if suffix_lower.starts_with("meg") || suffix.starts_with('M') {
        1_000_000.0
    } else if suffix.starts_with(['k', 'K']) {
        1_000.0
    } else if suffix.starts_with('m') {
        0.001
    } else if suffix.starts_with(['u', 'U']) {
        0.000_001
    } else if suffix.starts_with(['n', 'N']) {
        0.000_000_001
    } else if suffix.starts_with(['p', 'P']) {
        0.000_000_000_001
    } else {
        1.0
    };
    let value = number * multiplier;
    value.is_finite().then_some(value)
}

fn parse_embedded_multiplier(value: &str) -> Option<Option<f32>> {
    for (index, prefix) in value.char_indices() {
        let multiplier = match prefix {
            'p' | 'P' => 1e-12,
            'n' | 'N' => 1e-9,
            'u' | 'U' => 1e-6,
            'm' => 1e-3,
            'k' | 'K' => 1e3,
            'M' => 1e6,
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
        if !left.chars().enumerate().all(|(position, character)| {
            character.is_ascii_digit() || (position == 0 && matches!(character, '+' | '-'))
        }) {
            continue;
        }
        let number = format!("{left}.{right}").parse::<f32>().ok()?;
        let parsed = number * multiplier;
        return Some(parsed.is_finite().then_some(parsed));
    }
    None
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
    use super::parse_metric_value;

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
    }
}
