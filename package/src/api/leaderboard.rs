use serde::Deserialize;

use super::auth::User;
use super::{ApiClient, ApiError, GameId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LeaderboardRequest {
    Daily { date: Option<String>, limit: u8 },
    Weekly { limit: u8 },
    AllTime { limit: u8 },
    Game { game: GameId, limit: u8 },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct LeaderboardEntry {
    pub rank: u32,
    pub user: User,
    pub points: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Leaderboard {
    pub period: String,
    pub from: Option<String>,
    pub through: String,
    pub game_id: Option<GameId>,
    pub entries: Vec<LeaderboardEntry>,
}

impl ApiClient {
    pub async fn leaderboard(&self, request: LeaderboardRequest) -> Result<Leaderboard, ApiError> {
        let limit = match &request {
            LeaderboardRequest::Daily { limit, .. }
            | LeaderboardRequest::Weekly { limit }
            | LeaderboardRequest::AllTime { limit }
            | LeaderboardRequest::Game { limit, .. } => *limit,
        };
        if !(1..=100).contains(&limit) {
            return Err(ApiError::Validation {
                code: "invalid_limit".into(),
                message: "leaderboard limit must be 1..100".into(),
            });
        }
        let path = match request {
            LeaderboardRequest::Daily { date, limit } => match date {
                Some(date) => format!("/v1/leaderboards/daily?date={date}&limit={limit}"),
                None => format!("/v1/leaderboards/daily?limit={limit}"),
            },
            LeaderboardRequest::Weekly { limit } => {
                format!("/v1/leaderboards/weekly?limit={limit}")
            }
            LeaderboardRequest::AllTime { limit } => {
                format!("/v1/leaderboards/all-time?limit={limit}")
            }
            LeaderboardRequest::Game { game, limit } => {
                format!(
                    "/v1/leaderboards/games/{}?period=all_time&limit={limit}",
                    game.as_str()
                )
            }
        };
        self.get(&path, None).await
    }
}
