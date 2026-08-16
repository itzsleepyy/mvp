use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "highscore.json";
const SETTINGS_FILE_NAME: &str = "settings.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct HighScoreData {
    high_score: u64,
}

/// Local high-score storage. All filesystem problems — missing file, empty
/// file, malformed JSON, permission errors — degrade to a zero score instead
/// of preventing the game from launching.
#[derive(Debug)]
pub struct HighScoreStore {
    path: PathBuf,
    value: u64,
}

impl HighScoreStore {
    /// Locates the platform-appropriate config directory and loads the high
    /// score from it, if any.
    pub fn discover() -> Self {
        let path = platform_path().unwrap_or_else(|| PathBuf::from(FILE_NAME));
        Self::load(path)
    }

    /// Loads the high score from `path`. Any failure yields a score of zero.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let value = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<HighScoreData>(&text).ok())
            .map(|data| data.high_score)
            .unwrap_or(0);
        Self { path, value }
    }

    pub fn high_score(&self) -> u64 {
        self.value
    }

    /// Records `score` as the new best if it beats the current one.
    /// Persistence failures are ignored so storage problems never break the
    /// game. Returns `true` when a new record was set.
    pub fn record(&mut self, score: u64) -> bool {
        if score <= self.value {
            return false;
        }
        self.value = score;
        self.save();
        true
    }

    fn save(&self) {
        let Ok(data) = serde_json::to_string_pretty(&HighScoreData {
            high_score: self.value,
        }) else {
            return;
        };
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&self.path, data);
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
struct SettingsData {
    codex_auto_play: bool,
}

impl Default for SettingsData {
    fn default() -> Self {
        Self {
            codex_auto_play: true,
        }
    }
}

/// Small local preferences shared by CLI management and managed sessions.
pub struct SettingsStore {
    path: PathBuf,
    data: SettingsData,
}

impl SettingsStore {
    pub fn discover() -> Self {
        let path = directories::ProjectDirs::from("", "", "WaitState")
            .map(|dirs| dirs.config_dir().join(SETTINGS_FILE_NAME))
            .unwrap_or_else(|| PathBuf::from(SETTINGS_FILE_NAME));
        Self::load(path)
    }

    fn load(path: PathBuf) -> Self {
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { path, data }
    }

    pub fn codex_auto_play(&self) -> bool {
        self.data.codex_auto_play
    }

    pub fn set_codex_auto_play(&mut self, enabled: bool) {
        self.data.codex_auto_play = enabled;
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.data) {
            let _ = fs::write(&self.path, json);
        }
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
        assert_eq!(HighScoreStore::load(&path).high_score(), 0);
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
    fn record_round_trips_through_disk() {
        let path = temp_path("roundtrip.json");
        clean(&path);

        let mut store = HighScoreStore::load(&path);
        assert!(store.record(4_820));
        assert_eq!(HighScoreStore::load(&path).high_score(), 4_820);

        assert!(!store.record(3_000));
        assert_eq!(HighScoreStore::load(&path).high_score(), 4_820);
        clean(&path);
    }

    #[test]
    fn record_writes_expected_file_shape() {
        let path = temp_path("shape.json");
        clean(&path);

        let mut store = HighScoreStore::load(&path);
        store.record(12_480);

        let raw = fs::read_to_string(&path).unwrap();
        let data: HighScoreData = serde_json::from_str(&raw).unwrap();
        assert_eq!(data.high_score, 12_480);
        clean(&path);
    }

    #[test]
    fn record_zero_is_not_a_new_best() {
        let path = temp_path("zero.json");
        clean(&path);
        let mut store = HighScoreStore::load(&path);
        assert!(!store.record(0));
        clean(&path);
    }

    #[test]
    fn codex_auto_play_defaults_on_and_round_trips() {
        let path = temp_path("settings.json");
        clean(&path);
        let mut settings = SettingsStore::load(path.clone());
        assert!(settings.codex_auto_play());
        settings.set_codex_auto_play(false);
        assert!(!SettingsStore::load(path.clone()).codex_auto_play());
        clean(&path);
    }
}
