use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::game::GameKind;
use crate::game::daily_pr::DailyPrProgress;

const FILE_NAME: &str = "highscore.json";
const MVP_FILE_NAME: &str = "mvp_day.json";
/// Name used before the rebrand to MVP; its config directory is migrated
/// from on first run so existing scores survive.
const LEGACY_APP_NAME: &str = "WaitState";
const APP_NAME: &str = "MVP";
/// Name assigned to a player who skips the name prompt.
pub const ANONYMOUS: &str = "Anonymous";
/// Longest accepted player name.
const NAME_LIMIT: usize = 24;

#[derive(Debug, Default, Serialize, Deserialize)]
struct HighScoreData {
    /// The best score across every game, kept for older versions of the
    /// menu that show a single number.
    #[serde(default)]
    high_score: u64,
    /// Best score per game kind, keyed by [`GameKind::id`].
    #[serde(default)]
    games: BTreeMap<String, u64>,
    /// The player's chosen name; empty until the first-run prompt.
    #[serde(default)]
    player_name: String,
    /// Submitted guesses for today's Daily PR. Keeping this beside scores
    /// prevents restarting the app from restoring the attempt count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    daily_pr: Option<DailyPrProgress>,
}

/// Who owns the best score on a given day, across all games. This is the
/// "MVP of the day": whoever beat everyone else's best that day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyMvp {
    /// The local day (YYYY-MM-DD) this MVP was set for.
    pub date: String,
    /// The player's name.
    pub name: String,
    /// Their best score that day, across every game.
    pub score: u64,
    /// [`GameKind::id`] of the game the score was set in.
    pub game: String,
    /// Human-readable result shown on the board (e.g. "3 guesses" or
    /// "fixed in 0:42"), when the raw score is not meaningful on its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl DailyMvp {
    /// The game the MVP score was set in.
    pub fn game_kind(&self) -> GameKind {
        GameKind::ALL
            .iter()
            .find(|k| k.id() == self.game)
            .copied()
            .unwrap_or(GameKind::StackOverflow)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DailyMvpData {
    #[serde(default)]
    mvp: Option<DailyMvp>,
}

/// Local high-score storage. All filesystem problems — missing file, empty
/// file, malformed JSON, permission errors — degrade to zero scores instead
/// of preventing the game from launching.
#[derive(Debug)]
pub struct HighScoreStore {
    path: PathBuf,
    data: HighScoreData,
    mvp_path: PathBuf,
    mvp_data: DailyMvpData,
}

impl HighScoreStore {
    /// Locates the platform-appropriate config directory and loads the high
    /// scores and the daily MVP from it, migrating any pre-rebrand data.
    pub fn discover() -> Self {
        let Some(dirs) = directories::ProjectDirs::from("", "", APP_NAME) else {
            let fallback = PathBuf::from(FILE_NAME);
            let store = Self::load(fallback);
            return store;
        };
        let path = dirs.config_dir().join(FILE_NAME);
        let mvp_path = dirs.config_dir().join(MVP_FILE_NAME);
        if let Some(legacy) = directories::ProjectDirs::from("", "", LEGACY_APP_NAME) {
            migrate_from_legacy(&path, &legacy.config_dir().join(FILE_NAME));
            migrate_from_legacy(&mvp_path, &legacy.config_dir().join(MVP_FILE_NAME));
        }
        Self::load_at(path, mvp_path)
    }

    /// Loads the high scores and daily MVP from files next to `path`.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let mvp_path = path.with_file_name(MVP_FILE_NAME);
        Self::load_at(path, mvp_path)
    }

    fn load_at(path: PathBuf, mvp_path: PathBuf) -> Self {
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<HighScoreData>(&text).ok())
            .unwrap_or_default();
        let mvp_data = fs::read_to_string(&mvp_path)
            .ok()
            .and_then(|text| serde_json::from_str::<DailyMvpData>(&text).ok())
            .unwrap_or_default();
        Self {
            path,
            data,
            mvp_path,
            mvp_data,
        }
    }

    /// The best score across every game.
    pub fn high_score(&self) -> u64 {
        self.data.high_score
    }

    /// The best score for one game.
    pub fn best_score(&self, kind: GameKind) -> u64 {
        self.data.games.get(kind.id()).copied().unwrap_or(0)
    }

    /// Records `score` as the new best for `kind` if it beats the current
    /// one. Daily challenges normalize their raw metrics (guesses, time)
    /// into higher-is-better scores, so one comparison serves every game.
    /// Scores of zero are never a new best. Persistence failures are
    /// ignored so storage problems never break the game. Returns `true`
    /// when a new record was set.
    pub fn record(&mut self, kind: GameKind, score: u64) -> bool {
        if score == 0 {
            return false;
        }
        let entry = self.data.games.entry(kind.id().to_string()).or_insert(0);
        if score <= *entry {
            return false;
        }
        *entry = score;
        if score > self.data.high_score {
            self.data.high_score = score;
        }
        self.save();
        true
    }

    // ---- player name ------------------------------------------------------

    /// The stored player name. Empty until the first-run prompt is
    /// answered.
    #[cfg(test)]
    pub fn player_name_raw(&self) -> &str {
        &self.data.player_name
    }

    /// True once the player answered the first-run name prompt.
    pub fn has_player_name(&self) -> bool {
        !self.data.player_name.is_empty()
    }

    /// The name shown in the UI; never empty.
    pub fn player_name(&self) -> &str {
        if self.data.player_name.is_empty() {
            ANONYMOUS
        } else {
            &self.data.player_name
        }
    }

    /// Stores the player's chosen name (trimmed, capped). An empty name
    /// clears the stored name; the UI then falls back to [`ANONYMOUS`].
    pub fn set_player_name(&mut self, name: &str) {
        let name = name.trim();
        let name = if name.is_empty() {
            String::new()
        } else {
            name.chars().take(NAME_LIMIT).collect()
        };
        if self.data.player_name == name {
            return;
        }
        self.data.player_name = name;
        self.save();
    }

    // ---- daily challenge progress ----------------------------------------

    pub fn daily_pr_progress(&self, day: u64) -> Option<&DailyPrProgress> {
        self.data.daily_pr.as_ref().filter(|saved| saved.day == day)
    }

    pub fn set_daily_pr_progress(&mut self, progress: DailyPrProgress) {
        if self.data.daily_pr.as_ref() == Some(&progress) {
            return;
        }
        self.data.daily_pr = Some(progress);
        self.save();
    }

    // ---- daily MVP --------------------------------------------------------

    /// Today's MVP of the day, if a score has been set yet.
    pub fn daily_mvp(&self) -> Option<&DailyMvp> {
        self.mvp_data.mvp.as_ref()
    }

    /// Records `score` as today's MVP if it beats the current one for
    /// today. A new day always replaces yesterday's MVP. Daily challenges
    /// normalize their raw metrics into higher-is-better scores, so one
    /// comparison serves every game; a zero score is a did-not-finish and
    /// never crowns anyone. `note` is the human-readable result shown on
    /// the board. Returns `true` when this score became the MVP of the day.
    pub fn record_daily_mvp(
        &mut self,
        name: &str,
        kind: GameKind,
        score: u64,
        note: Option<&str>,
    ) -> bool {
        if score == 0 {
            return false;
        }
        let today = today_key();
        let current = self.mvp_data.mvp.as_ref();
        if current.is_some_and(|m| m.date == today && m.score >= score) {
            return false;
        }
        self.mvp_data.mvp = Some(DailyMvp {
            date: today,
            name: name.to_string(),
            score,
            game: kind.id().to_string(),
            note: note.map(str::to_string),
        });
        self.save_mvp();
        true
    }

    fn save(&self) {
        let Ok(data) = serde_json::to_string_pretty(&self.data) else {
            return;
        };
        write_file(&self.path, &data);
    }

    fn save_mvp(&self) {
        let Ok(data) = serde_json::to_string_pretty(&self.mvp_data) else {
            return;
        };
        write_file(&self.mvp_path, &data);
    }
}

fn write_file(path: &Path, data: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, data);
}

#[cfg(test)]
fn platform_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", APP_NAME).map(|dirs| dirs.config_dir().join(FILE_NAME))
}

/// Copies a pre-rebrand config file into the MVP directory when the new
/// location does not exist yet. Never overwrites.
fn migrate_from_legacy(new: &Path, legacy: &Path) {
    if new.exists() || !legacy.exists() {
        return;
    }
    if let Some(parent) = new.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::copy(legacy, new);
}

/// Days since the Unix epoch — the stable "today" index used to seed the
/// daily challenges. Same value for everyone on a given day, so the whole
/// world plays the same word and the same bug.
pub fn today_ordinal() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

/// The local day as YYYY-MM-DD, from the civil-date algorithm (Howard
/// Hinnant's `civil_from_days`). No external date dependency needed.
fn today_key() -> String {
    let days = today_ordinal() as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn temp_path(name: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mvp_test_{}_{}_{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed),
            name
        ));
        let _ = fs::create_dir_all(&dir);
        dir.join(FILE_NAME)
    }

    fn clean(path: &Path) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(path.with_file_name(MVP_FILE_NAME));
    }

    #[test]
    fn missing_file_loads_zero() {
        let path = temp_path("missing.json");
        clean(&path);
        let store = HighScoreStore::load(&path);
        assert_eq!(store.high_score(), 0);
        assert_eq!(store.best_score(GameKind::StackOverflow), 0);
        assert_eq!(store.best_score(GameKind::DailyPr), 0);
        assert_eq!(store.best_score(GameKind::DailyFix), 0);
        assert_eq!(store.daily_mvp(), None);
    }

    #[test]
    fn empty_file_loads_zero() {
        let path = temp_path("empty.json");
        fs::write(&path, "").unwrap();
        assert_eq!(HighScoreStore::load(&path).high_score(), 0);
        clean(&path);
    }

    #[test]
    fn malformed_file_loads_zero() {
        let path = temp_path("malformed.json");
        fs::write(&path, "{ not json !!!").unwrap();
        assert_eq!(HighScoreStore::load(&path).high_score(), 0);
        clean(&path);
    }

    #[test]
    fn legacy_file_shape_still_loads() {
        let path = temp_path("legacy.json");
        clean(&path);
        fs::write(&path, "{\n  \"high_score\": 4820\n}").unwrap();
        let store = HighScoreStore::load(&path);
        assert_eq!(store.high_score(), 4_820);
        assert_eq!(store.best_score(GameKind::StackOverflow), 0);
        assert!(!store.has_player_name());
        clean(&path);
    }

    #[test]
    fn record_round_trips_through_disk_per_game() {
        let path = temp_path("roundtrip.json");
        clean(&path);

        let mut store = HighScoreStore::load(&path);
        assert!(store.record(GameKind::StackOverflow, 4_820));
        assert!(store.record(GameKind::DailyPr, 6_000_000));
        assert!(store.record(GameKind::DailyFix, 999_000_000));
        assert_eq!(store.high_score(), 999_000_000);
        assert_eq!(store.best_score(GameKind::StackOverflow), 4_820);
        assert_eq!(store.best_score(GameKind::DailyPr), 6_000_000);
        assert_eq!(store.best_score(GameKind::DailyFix), 999_000_000);

        let reloaded = HighScoreStore::load(&path);
        assert_eq!(reloaded.high_score(), 999_000_000);
        assert_eq!(reloaded.best_score(GameKind::StackOverflow), 4_820);
        assert_eq!(reloaded.best_score(GameKind::DailyPr), 6_000_000);
        assert_eq!(reloaded.best_score(GameKind::DailyFix), 999_000_000);

        assert!(!store.record(GameKind::StackOverflow, 3_000));
        assert!(!store.record(GameKind::DailyPr, 5_000_000));
        assert!(!store.record(GameKind::DailyFix, 998_000_000));
        clean(&path);
    }

    #[test]
    fn record_writes_expected_file_shape() {
        let path = temp_path("shape.json");
        clean(&path);

        let mut store = HighScoreStore::load(&path);
        store.record(GameKind::StackOverflow, 12_480);

        let raw = fs::read_to_string(&path).unwrap();
        let data: HighScoreData = serde_json::from_str(&raw).unwrap();
        assert_eq!(data.high_score, 12_480);
        assert_eq!(data.games["stack-overflow"], 12_480);
        clean(&path);
    }

    #[test]
    fn record_zero_is_not_a_new_best() {
        let path = temp_path("zero.json");
        clean(&path);
        let mut store = HighScoreStore::load(&path);
        assert!(!store.record(GameKind::StackOverflow, 0));
        assert!(!store.record(GameKind::DailyPr, 0));
        assert!(!store.record(GameKind::DailyFix, 0));
        clean(&path);
    }

    #[test]
    fn lower_normalized_scores_are_not_better_records() {
        let path = temp_path("lower.json");
        clean(&path);
        let mut store = HighScoreStore::load(&path);
        // Daily PR: 1 guess (6,000,000) beats 5 guesses (2,000,000).
        assert!(store.record(GameKind::DailyPr, 6_000_000));
        assert!(!store.record(GameKind::DailyPr, 2_000_000));
        assert_eq!(store.best_score(GameKind::DailyPr), 6_000_000);
        clean(&path);
    }

    // ---- player name ------------------------------------------------------

    #[test]
    fn player_name_starts_anonymous_and_is_editable() {
        let path = temp_path("name.json");
        clean(&path);
        let mut store = HighScoreStore::load(&path);
        assert!(!store.has_player_name());
        assert_eq!(store.player_name(), ANONYMOUS);

        store.set_player_name("  Alex  ");
        assert!(store.has_player_name());
        assert_eq!(store.player_name(), "Alex");
        assert_eq!(store.player_name_raw(), "Alex");

        let reloaded = HighScoreStore::load(&path);
        assert_eq!(reloaded.player_name(), "Alex");
        clean(&path);
    }

    #[test]
    fn empty_or_whitespace_name_becomes_anonymous() {
        let path = temp_path("anon.json");
        clean(&path);
        let mut store = HighScoreStore::load(&path);
        store.set_player_name("   ");
        assert!(!store.has_player_name());
        assert_eq!(store.player_name(), ANONYMOUS);
        clean(&path);
    }

    #[test]
    fn names_are_capped_at_the_name_limit() {
        let path = temp_path("longname.json");
        clean(&path);
        let mut store = HighScoreStore::load(&path);
        let long = "a".repeat(100);
        store.set_player_name(&long);
        assert_eq!(store.player_name().len(), NAME_LIMIT);
        clean(&path);
    }

    // ---- daily MVP ---------------------------------------------------------

    #[test]
    fn daily_mvp_round_trips_through_disk() {
        let path = temp_path("mvp.json");
        clean(&path);

        let mut store = HighScoreStore::load(&path);
        assert!(store.record_daily_mvp("Alex", GameKind::StackOverflow, 4_820, None));
        let mvp = store.daily_mvp().expect("mvp set");
        assert_eq!(mvp.name, "Alex");
        assert_eq!(mvp.score, 4_820);
        assert_eq!(mvp.game_kind(), GameKind::StackOverflow);
        assert_eq!(mvp.date, today_key());

        let reloaded = HighScoreStore::load(&path);
        assert_eq!(reloaded.daily_mvp(), store.daily_mvp());
        clean(&path);
    }

    #[test]
    fn daily_mvp_only_replaced_by_a_better_score_same_day() {
        let path = temp_path("mvpbeat.json");
        clean(&path);

        let mut store = HighScoreStore::load(&path);
        store.record_daily_mvp("Alex", GameKind::StackOverflow, 1_000, None);
        assert!(
            !store.record_daily_mvp("Sam", GameKind::DailyPr, 900, None),
            "lower score must not dethrone the MVP"
        );
        assert_eq!(store.daily_mvp().unwrap().name, "Alex");

        assert!(
            store.record_daily_mvp("Sam", GameKind::DailyPr, 1_100, Some("note")),
            "higher score must crown a new MVP"
        );
        assert_eq!(store.daily_mvp().unwrap().name, "Sam");
        assert_eq!(store.daily_mvp().unwrap().game_kind(), GameKind::DailyPr);
        assert_eq!(store.daily_mvp().unwrap().note.as_deref(), Some("note"));
        clean(&path);
    }

    #[test]
    fn yesterday_mvp_is_replaced_today() {
        let path = temp_path("mvprollover.json");
        clean(&path);

        let mut store = HighScoreStore::load(&path);
        store.mvp_data.mvp = Some(DailyMvp {
            date: "2000-01-01".to_string(),
            name: "Old Guard".to_string(),
            score: 99_999,
            game: "stack-overflow".to_string(),
            note: None,
        });
        assert!(
            store.record_daily_mvp("Fresh", GameKind::StackOverflow, 10, None),
            "a new day always starts fresh"
        );
        assert_eq!(store.daily_mvp().unwrap().name, "Fresh");
        assert_eq!(store.daily_mvp().unwrap().date, today_key());
        clean(&path);
    }

    #[test]
    fn zero_score_never_crowns_or_dethrones() {
        let path = temp_path("mvpzero.json");
        clean(&path);

        let mut store = HighScoreStore::load(&path);
        assert!(
            !store.record_daily_mvp("Alex", GameKind::StackOverflow, 0, None),
            "a did-not-finish must not crown the MVP"
        );
        assert!(store.record_daily_mvp("Alex", GameKind::StackOverflow, 100, None));
        assert!(!store.record_daily_mvp("Sam", GameKind::StackOverflow, 0, None));
        assert_eq!(store.daily_mvp().unwrap().name, "Alex");
        clean(&path);
    }

    // ---- legacy migration --------------------------------------------------

    #[test]
    fn legacy_directory_is_migrated_on_first_run() {
        let legacy_dir =
            std::env::temp_dir().join(format!("mvp_test_{}_legacy_dir", std::process::id()));
        let new_dir = std::env::temp_dir().join(format!("mvp_test_{}_new_dir", std::process::id()));
        let _ = fs::remove_dir_all(&legacy_dir);
        let _ = fs::remove_dir_all(&new_dir);
        fs::create_dir_all(&legacy_dir).unwrap();
        fs::write(legacy_dir.join(FILE_NAME), r#"{"high_score": 1234}"#).unwrap();
        fs::write(
            legacy_dir.join(MVP_FILE_NAME),
            r#"{"mvp": {"date": "2026-08-19", "name": "Old", "score": 100, "game": "stack-jump"}}"#,
        )
        .unwrap();

        migrate_from_legacy(&new_dir.join(FILE_NAME), &legacy_dir.join(FILE_NAME));
        migrate_from_legacy(
            &new_dir.join(MVP_FILE_NAME),
            &legacy_dir.join(MVP_FILE_NAME),
        );

        let store = HighScoreStore::load_at(new_dir.join(FILE_NAME), new_dir.join(MVP_FILE_NAME));
        assert_eq!(store.high_score(), 1_234, "high scores must migrate");
        assert_eq!(
            store.daily_mvp().unwrap().name,
            "Old",
            "daily MVP must migrate"
        );

        let _ = fs::remove_dir_all(&legacy_dir);
        let _ = fs::remove_dir_all(&new_dir);
    }

    #[test]
    fn migration_never_overwrites_existing_new_files() {
        let legacy = temp_path("miglegacy.json");
        let new = temp_path("mignew.json");
        clean(&new);
        fs::write(&legacy, r#"{"high_score": 1}"#).unwrap();
        fs::write(&new, r#"{"high_score": 999}"#).unwrap();

        migrate_from_legacy(&new, &legacy);
        assert_eq!(HighScoreStore::load(&new).high_score(), 999);
        clean(&new);
        clean(&legacy);
    }

    // ---- date --------------------------------------------------------------

    #[test]
    fn today_key_is_formatted_yyyymmdd() {
        let key = today_key();
        let parts: Vec<&str> = key.split('-').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].len(), 4);
        assert_eq!(parts[1].len(), 2);
        assert_eq!(parts[2].len(), 2);
    }

    #[test]
    fn platform_path_uses_the_mvp_directory() {
        let path = platform_path().expect("platform path");
        let display = path.display().to_string();
        assert!(
            display.contains("MVP") || display.contains("mvp"),
            "expected the MVP config directory, got {display}"
        );
        assert!(!display.contains("WaitState"));
    }
}
