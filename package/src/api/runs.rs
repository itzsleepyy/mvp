use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{ApiClient, ApiError, Session};
use crate::game::CompletedRun;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GameId {
    StackOverflow,
    DailyPr,
    DailyFix,
}

impl GameId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StackOverflow => "stack_overflow",
            Self::DailyPr => "daily_pr",
            Self::DailyFix => "daily_fix",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Challenge {
    pub date: String,
    pub version: i32,
    pub id: String,
}

pub type RunResult = Map<String, Value>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct RunPayload {
    pub client_run_id: uuid::Uuid,
    pub game_id: GameId,
    pub raw_score: i32,
    pub duration_ms: u32,
    pub client_version: String,
    pub result: RunResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub challenge: Option<Challenge>,
}

impl RunPayload {
    pub fn from_completed(completed: &CompletedRun) -> Result<Self, ApiError> {
        let version = env!("CARGO_PKG_VERSION");
        match completed {
            CompletedRun::StackOverflow {
                score,
                duration_ms,
                height,
            } => {
                let mut result = Map::new();
                result.insert("height".into(), Value::from(*height));
                Self::new(
                    GameId::StackOverflow,
                    checked_i32(*score, "score")?,
                    checked_u32(*duration_ms, "duration")?,
                    version,
                    result,
                    None,
                )
            }
            CompletedRun::DailyPr {
                client_run_id,
                day,
                solved,
                guesses,
                duration_ms,
            } => {
                let mut result = Map::new();
                result.insert("solved".into(), Value::from(*solved));
                let mut run = Self::new(
                    GameId::DailyPr,
                    i32::from(*guesses),
                    checked_u32(*duration_ms, "duration")?,
                    version,
                    result,
                    Some(challenge(GameId::DailyPr, *day)?),
                )?;
                if let Some(client_run_id) = client_run_id {
                    run.client_run_id = *client_run_id;
                }
                Ok(run)
            }
            CompletedRun::DailyFix {
                day,
                charged_duration_ms,
                attempts,
                hint_used,
            } => {
                let mut result = Map::new();
                result.insert("solved".into(), Value::Bool(true));
                result.insert("attempts".into(), Value::from(*attempts));
                result.insert("hint_used".into(), Value::Bool(*hint_used));
                Self::new(
                    GameId::DailyFix,
                    checked_i32(*charged_duration_ms, "charged duration")?,
                    checked_u32(*charged_duration_ms, "charged duration")?,
                    version,
                    result,
                    Some(challenge(GameId::DailyFix, *day)?),
                )
            }
        }
    }

    pub fn new(
        game_id: GameId,
        raw_score: i32,
        duration_ms: u32,
        client_version: impl Into<String>,
        result: RunResult,
        challenge: Option<Challenge>,
    ) -> Result<Self, ApiError> {
        let run = Self {
            client_run_id: uuid::Uuid::new_v4(),
            game_id,
            raw_score,
            duration_ms,
            client_version: client_version.into(),
            result,
            challenge,
        };
        run.validate()?;
        Ok(run)
    }

    pub fn validate(&self) -> Result<(), ApiError> {
        let invalid = |message: &str| ApiError::Validation {
            code: "invalid_run".into(),
            message: message.into(),
        };
        if self.raw_score < 0 {
            return Err(invalid("raw_score must be non-negative"));
        }
        if self.duration_ms > 86_400_000 {
            return Err(invalid("duration_ms exceeds one day"));
        }
        if self.client_version.is_empty() || self.client_version.len() > 64 {
            return Err(invalid("client_version must contain 1..64 bytes"));
        }
        match self.game_id {
            GameId::StackOverflow if self.challenge.is_some() => {
                return Err(invalid("stack_overflow must omit challenge"));
            }
            GameId::DailyPr | GameId::DailyFix if self.challenge.is_none() => {
                return Err(invalid("daily games require challenge"));
            }
            _ => {}
        }
        if matches!(self.game_id, GameId::DailyPr | GameId::DailyFix)
            && !self.result.get("solved").is_some_and(Value::is_boolean)
        {
            return Err(invalid("daily result.solved must be a boolean"));
        }
        match self.game_id {
            GameId::StackOverflow => {
                if self.raw_score > 100_000 || self.raw_score % 50 != 0 {
                    return Err(invalid(
                        "stack_overflow score must be at most 100000 and divisible by 50",
                    ));
                }
                if self.raw_score > 0 && self.duration_ms == 0 {
                    return Err(invalid("a scored stack_overflow run needs a duration"));
                }
            }
            GameId::DailyPr => {
                let solved = self.result["solved"].as_bool().unwrap_or(false);
                if !(1..=6).contains(&self.raw_score) {
                    return Err(invalid("daily_pr guesses must be 1..6"));
                }
                if self.duration_ms > 3_600_000 {
                    return Err(invalid("daily_pr duration exceeds one hour"));
                }
                if !solved && self.raw_score != 6 {
                    return Err(invalid("an unsolved daily_pr must use all 6 guesses"));
                }
            }
            GameId::DailyFix => {
                let solved = self.result["solved"].as_bool().unwrap_or(false);
                let attempts = self.result.get("attempts").and_then(Value::as_u64);
                let hint = self.result.get("hint_used").and_then(Value::as_bool);
                if !solved
                    || attempts.is_none_or(|attempts| attempts > 100)
                    || hint != Some(attempts.unwrap_or(0) >= 2)
                {
                    return Err(invalid("daily_fix completion metrics are invalid"));
                }
                if self.raw_score != self.duration_ms as i32 {
                    return Err(invalid("daily_fix score must equal charged duration"));
                }
                let minimum =
                    attempts.unwrap_or(0) * 5_000 + u64::from(hint == Some(true)) * 15_000;
                if u64::from(self.duration_ms) < minimum || self.duration_ms > 3_600_000 {
                    return Err(invalid("daily_fix charged duration is impossible"));
                }
            }
        }
        Ok(())
    }
}

fn checked_i32(value: u64, name: &str) -> Result<i32, ApiError> {
    i32::try_from(value).map_err(|_| ApiError::Validation {
        code: "invalid_run".into(),
        message: format!("{name} is too large"),
    })
}

fn checked_u32(value: u64, name: &str) -> Result<u32, ApiError> {
    u32::try_from(value).map_err(|_| ApiError::Validation {
        code: "invalid_run".into(),
        message: format!("{name} is too large"),
    })
}

fn challenge(game: GameId, day: u64) -> Result<Challenge, ApiError> {
    let seconds = day
        .checked_mul(86_400)
        .ok_or_else(|| ApiError::Validation {
            code: "invalid_challenge".into(),
            message: "challenge day is out of range".into(),
        })?;
    let seconds = i64::try_from(seconds).map_err(|_| ApiError::Validation {
        code: "invalid_challenge".into(),
        message: "challenge day is out of range".into(),
    })?;
    let date = chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0)
        .ok_or_else(|| ApiError::Validation {
            code: "invalid_challenge".into(),
            message: "challenge day is out of range".into(),
        })?
        .format("%Y-%m-%d")
        .to_string();
    Ok(Challenge {
        date: date.clone(),
        version: 1,
        id: format!("{}:v1:{date}", game.as_str()),
    })
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct SubmittedRun {
    pub id: uuid::Uuid,
    pub client_run_id: uuid::Uuid,
    pub game_id: GameId,
    pub challenge_date: String,
    pub challenge_version: Option<i32>,
    pub challenge_id: Option<String>,
    pub raw_score: i32,
    pub normalized_score: i32,
    pub result: RunResult,
    pub duration_ms: u32,
    pub client_version: String,
    pub normalization_version: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ApiClient {
    pub async fn submit_run(
        &self,
        session: &Session,
        run: &RunPayload,
    ) -> Result<SubmittedRun, ApiError> {
        run.validate()?;
        self.post("/v1/runs", Some(session.token()), Some(run))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_uses_exact_underscore_contract_and_optional_challenge() {
        let run =
            RunPayload::new(GameId::StackOverflow, 50, 20, "0.1.0", Map::new(), None).unwrap();
        let value = serde_json::to_value(run).unwrap();
        assert_eq!(value["game_id"], "stack_overflow");
        assert!(value.get("challenge").is_none());
        assert!(value["client_run_id"].as_str().unwrap().contains('-'));
    }

    #[test]
    fn daily_completion_captures_versioned_utc_challenge_and_raw_metrics() {
        use chrono::TimeZone;

        let day = chrono::Utc
            .with_ymd_and_hms(2026, 8, 20, 0, 0, 0)
            .unwrap()
            .timestamp() as u64
            / 86_400;
        let run = RunPayload::from_completed(&CompletedRun::DailyFix {
            day,
            charged_duration_ms: 60_000,
            attempts: 2,
            hint_used: true,
        })
        .unwrap();
        assert_eq!(run.game_id, GameId::DailyFix);
        assert_eq!(run.raw_score, 60_000);
        assert_eq!(run.duration_ms, 60_000);
        assert_eq!(run.result["attempts"], 2);
        assert_eq!(run.result["hint_used"], true);
        assert_eq!(run.challenge.unwrap().id, "daily_fix:v1:2026-08-20");
    }
}
