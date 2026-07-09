/// Parse a human-readable SI value string into an `f32`.
///
/// Recognises SI multipliers (p, n, u, m, k, Meg) and a `unit_hint` to
/// disambiguate ambiguous suffixes (e.g. `"m"` means milli for volts but
/// mega for ohms in some notations).
///
/// # Examples
/// ```
/// assert_eq!(parse_metric_value("10k", "ohm"), Some(10_000.0));
/// assert_eq!(parse_metric_value("100nF", "f"),  Some(100e-9));
/// assert_eq!(parse_metric_value("3.3V", "v"),   Some(3.3));
/// ```
pub(crate) fn parse_metric_value(value: &str, unit_hint: &str) -> Option<f32> {
    let normalized = value
        .trim()
        .to_lowercase()
        .replace('\u{03a9}', "ohm")
        .replace('\u{00b5}', "u");
    let number_end = metric_number_end(&normalized);
    if number_end == 0 {
        return None;
    }
    let number = normalized.get(..number_end)?.parse::<f32>().ok()?;
    if !number.is_finite() {
        return None;
    }
    let suffix = normalized.get(number_end..)?.trim();
    let multiplier = if suffix.starts_with('m') && unit_hint == "v" {
        0.001
    } else if suffix.starts_with('k') {
        1_000.0
    } else if suffix.starts_with("meg") || (suffix.starts_with('m') && unit_hint == "ohm") {
        1_000_000.0
    } else if suffix.starts_with('m') {
        0.001
    } else if suffix.starts_with('u') {
        0.000_001
    } else if suffix.starts_with('n') {
        0.000_000_001
    } else if suffix.starts_with('p') {
        0.000_000_000_001
    } else {
        1.0
    };
    let value = number * multiplier;
    value.is_finite().then_some(value)
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
}
