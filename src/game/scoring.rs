/// Points earned per second of survival, before the difficulty multiplier.
pub const POINTS_PER_SECOND: f64 = 25.0;
/// Extra points awarded per obstacle passed.
pub const PASS_REWARD: u64 = 100;

/// Accumulated run score. Only advances when `update` is called, so pausing
/// is as simple as not updating.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Score {
    value: u64,
    /// Fractional point accumulator: points accrue continuously and are
    /// surfaced as whole numbers, so short frames still contribute.
    points: f64,
    elapsed_millis: u64,
}

impl Score {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds survival points for `dt` seconds of gameplay at the given
    /// difficulty multiplier.
    pub fn update(&mut self, dt: f64, multiplier: f64) {
        self.elapsed_millis += (dt * 1000.0) as u64;
        self.points += dt * POINTS_PER_SECOND * multiplier;
        self.value = self.points as u64;
    }

    pub fn add_bonus(&mut self, points: u64) {
        self.value += points;
        self.points += points as f64;
    }

    pub fn value(&self) -> u64 {
        self.value
    }

    pub fn elapsed(&self) -> f64 {
        self.elapsed_millis as f64 / 1000.0
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_score_starts_at_zero() {
        let score = Score::new();
        assert_eq!(score.value(), 0);
        assert_eq!(score.elapsed(), 0.0);
    }

    #[test]
    fn update_increases_score_proportionally() {
        let mut score = Score::new();
        score.update(2.0, 1.0);
        assert_eq!(score.value(), (2.0 * POINTS_PER_SECOND) as u64);
        assert!((score.elapsed() - 2.0).abs() < 0.01);
    }

    #[test]
    fn short_frames_accumulate_instead_of_truncating() {
        // One frame adds less than a point; many frames must add up.
        let mut score = Score::new();
        for _ in 0..60 {
            score.update(1.0 / 60.0, 1.0);
        }
        assert!(score.value() > 0);
        assert!((score.value() as f64 - POINTS_PER_SECOND).abs() < 2.0);
    }

    #[test]
    fn bonuses_survive_further_updates() {
        let mut score = Score::new();
        score.add_bonus(PASS_REWARD);
        for _ in 0..60 {
            score.update(1.0 / 60.0, 1.0);
        }
        assert!(score.value() >= PASS_REWARD);
    }

    #[test]
    fn multiplier_scales_survival_points() {
        let mut base = Score::new();
        let mut boosted = Score::new();
        base.update(1.0, 1.0);
        boosted.update(1.0, 2.0);
        assert_eq!(boosted.value(), base.value() * 2);
    }

    #[test]
    fn score_only_changes_when_updated() {
        let mut score = Score::new();
        score.update(1.0, 1.0);
        let frozen = score;
        // No update call — simulating a paused game — leaves the score alone.
        assert_eq!(score, frozen);
    }

    #[test]
    fn bonuses_accumulate() {
        let mut score = Score::new();
        score.add_bonus(PASS_REWARD);
        score.add_bonus(PASS_REWARD);
        assert_eq!(score.value(), PASS_REWARD * 2);
    }

    #[test]
    fn formats_scores_with_separators() {
        assert_eq!(format_score(0), "0");
        assert_eq!(format_score(999), "999");
        assert_eq!(format_score(1_000), "1,000");
        assert_eq!(format_score(4_820), "4,820");
        assert_eq!(format_score(1_234_567), "1,234,567");
    }
}
