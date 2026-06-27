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
    let number_end = normalized
        .char_indices()
        .find(|(_, ch)| !(ch.is_ascii_digit() || *ch == '.'))
        .map(|(idx, _)| idx)
        .unwrap_or(normalized.len());
    let number = normalized.get(..number_end)?.parse::<f32>().ok()?;
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
    Some(number * multiplier)
}
