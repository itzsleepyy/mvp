use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::game::GameKind;

const FILE_NAME: &str = "highscore.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct HighScoreData {
    /// The best score across every game, kept for older versions of the
    /// menu that show a single number.
    #[serde(default)]
    high_score: u64,
    /// Best score per game kind, keyed by [`GameKind::id`].
    #[serde(default)]
    games: BTreeMap<String, u64>,
}

/// Local high-score storage. All filesystem problems — missing file, empty
/// file, malformed JSON, permission errors — degrade to zero scores instead
/// of preventing the game from launching.
#[derive(Debug)]
pub struct HighScoreStore {
    path: PathBuf,
    data: HighScoreData,
}

impl HighScoreStore {
    /// Locates the platform-appropriate config directory and loads the high
    /// scores from it, if any.
    pub fn discover() -> Self {
        let path = platform_path().unwrap_or_else(|| PathBuf::from(FILE_NAME));
        Self::load(path)
    }

    /// Loads the high scores from `path`. Any failure yields zero scores.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<HighScoreData>(&text).ok())
            .unwrap_or_default();
        Self { path, data }
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
    /// one. Persistence failures are ignored so storage problems never break
    /// the game. Returns `true` when a new record was set.
    pub fn record(&mut self, kind: GameKind, score: u64) -> bool {
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

    fn save(&self) {
        let Ok(data) = serde_json::to_string_pretty(&self.data) else {
            return;
        };
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&self.path, data);
    }
}

fn platform_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "WaitState")
        .map(|dirs| dirs.config_dir().join(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("waitstate_test_{}_{}", std::process::id(), name));
        path
    }

    fn clean(path: &Path) {
        let _ = fs::remove_file(path);
    }

    #[test]
    fn missing_file_loads_zero() {
        let path = temp_path("missing.json");
        clean(&path);
        let store = HighScoreStore::load(&path);
        assert_eq!(store.high_score(), 0);
        assert_eq!(store.best_score(GameKind::StackJump), 0);
        assert_eq!(store.best_score(GameKind::TwentyOne), 0);
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
        assert_eq!(store.best_score(GameKind::StackJump), 0);
        clean(&path);
    }

    #[test]
    fn record_round_trips_through_disk_per_game() {
        let path = temp_path("roundtrip.json");
        clean(&path);

        let mut store = HighScoreStore::load(&path);
        assert!(store.record(GameKind::StackJump, 4_820));
        assert!(store.record(GameKind::TwentyOne, 1_100));
        assert_eq!(store.high_score(), 4_820);
        assert_eq!(store.best_score(GameKind::StackJump), 4_820);
        assert_eq!(store.best_score(GameKind::TwentyOne), 1_100);

        let reloaded = HighScoreStore::load(&path);
        assert_eq!(reloaded.high_score(), 4_820);
        assert_eq!(reloaded.best_score(GameKind::StackJump), 4_820);
        assert_eq!(reloaded.best_score(GameKind::TwentyOne), 1_100);

        assert!(!store.record(GameKind::StackJump, 3_000));
        assert!(!store.record(GameKind::TwentyOne, 900));
        clean(&path);
    }

    #[test]
    fn record_writes_expected_file_shape() {
        let path = temp_path("shape.json");
        clean(&path);

        let mut store = HighScoreStore::load(&path);
        store.record(GameKind::StackJump, 12_480);

        let raw = fs::read_to_string(&path).unwrap();
        let data: HighScoreData = serde_json::from_str(&raw).unwrap();
        assert_eq!(data.high_score, 12_480);
        assert_eq!(data.games["stack-jump"], 12_480);
        clean(&path);
    }

    #[test]
    fn record_zero_is_not_a_new_best() {
        let path = temp_path("zero.json");
        clean(&path);
        let mut store = HighScoreStore::load(&path);
        assert!(!store.record(GameKind::StackJump, 0));
        assert!(!store.record(GameKind::TwentyOne, 0));
        clean(&path);
    }
}
