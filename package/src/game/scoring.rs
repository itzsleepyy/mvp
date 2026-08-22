/// Formats a score with thousands separators, e.g. `4820` becomes `"4,820"`.
pub fn format_score(value: u64) -> String {
    let raw = value.to_string();
    let mut out = String::with_capacity(raw.len() + raw.len() / 3);
    let mut count = 0;
    for ch in raw.chars().rev() {
        if count == 3 {
            out.push(',');
            count = 0;
        }
        out.push(ch);
        count += 1;
    }
    out.chars().rev().collect()
}

/// Formats a duration in seconds as `M:SS` (e.g. 75.4 -> "1:15").
pub fn format_elapsed(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_scores_with_separators() {
        assert_eq!(format_score(0), "0");
        assert_eq!(format_score(999), "999");
        assert_eq!(format_score(1_000), "1,000");
        assert_eq!(format_score(4_820), "4,820");
        assert_eq!(format_score(1_234_567), "1,234,567");
    }

    #[test]
    fn formats_elapsed_as_minutes_seconds() {
        assert_eq!(format_elapsed(0.0), "0:00");
        assert_eq!(format_elapsed(42.7), "0:42");
        assert_eq!(format_elapsed(75.4), "1:15");
        assert_eq!(format_elapsed(-3.0), "0:00");
    }
}
