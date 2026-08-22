pub mod daily_fix;
pub mod daily_pr;
pub mod scoring;
pub mod stack_overflow;
pub mod state;
pub mod words;

use std::time::Duration;

use daily_fix::DailyFix;
use daily_pr::DailyPr;
use stack_overflow::StackOverflow;

/// The longest simulation step accepted in a single update, protecting the
/// physics from huge frame gaps (terminal stalls, debugger pauses, ...).
pub const MAX_FRAME_TIME: Duration = Duration::from_millis(50);

/// Input events as understood by a game. Deliberately tiny: each game reacts
/// only to its own inputs and ignores the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameInput {
    /// SPACE/↑ — drop the block (Stack Overflow).
    Jump,
    /// ENTER — submit the current guess or fix.
    Confirm,
    /// A printable character typed into the game.
    Type(char),
    /// Backspace in the game's input buffer.
    Backspace,
}

/// The games MVP ships, in menu order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameKind {
    StackOverflow,
    DailyPr,
    DailyFix,
}

/// Immutable raw metrics captured when a game completes. These values are
/// transport-neutral; the online layer converts them into its versioned API
/// contract while local play can ignore them entirely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletedRun {
    StackOverflow {
        score: u64,
        duration_ms: u64,
        height: u64,
    },
    DailyPr {
        client_run_id: Option<uuid::Uuid>,
        day: u64,
        solved: bool,
        guesses: u8,
        duration_ms: u64,
    },
    DailyFix {
        day: u64,
        charged_duration_ms: u64,
        attempts: u32,
        hint_used: bool,
    },
}

impl GameKind {
    pub const ALL: [GameKind; 3] = [
        GameKind::StackOverflow,
        GameKind::DailyPr,
        GameKind::DailyFix,
    ];

    /// Stable identifier used for high-score storage.
    pub fn id(self) -> &'static str {
        match self {
            GameKind::StackOverflow => "stack-overflow",
            GameKind::DailyPr => "daily-pr",
            GameKind::DailyFix => "daily-fix",
        }
    }

    /// Display name.
    pub fn title(self) -> &'static str {
        match self {
            GameKind::StackOverflow => "Stack Overflow",
            GameKind::DailyPr => "The Daily PR",
            GameKind::DailyFix => "The Daily Fix",
        }
    }

    /// One-line description for the game menu.
    pub fn blurb(self) -> &'static str {
        match self {
            GameKind::StackOverflow => "build the call stack, don't overflow",
            GameKind::DailyPr => "today's word — fewest guesses wins",
            GameKind::DailyFix => "spot today's bug, fix it fastest",
        }
    }

    /// Whether the game is a daily challenge: one shared puzzle per day,
    /// seeded from [`crate::config::today_ordinal`] so everyone plays the
    /// same word and the same bug.
    pub fn is_daily(self) -> bool {
        matches!(self, GameKind::DailyPr | GameKind::DailyFix)
    }
}

/// A live game of one of the supported kinds. Exposes the small surface the
/// app and renderer need, so neither depends on any one game's rules.
pub enum ActiveGame {
    StackOverflow(StackOverflow),
    DailyPr(DailyPr),
    DailyFix(DailyFix),
}

impl ActiveGame {
    pub fn new(kind: GameKind, seed: u64, playfield_cols: u16, _playfield_rows: u16) -> Self {
        match kind {
            GameKind::StackOverflow => {
                ActiveGame::StackOverflow(StackOverflow::new(seed, playfield_cols))
            }
            GameKind::DailyPr => ActiveGame::DailyPr(DailyPr::new(seed)),
            GameKind::DailyFix => ActiveGame::DailyFix(DailyFix::new(seed)),
        }
    }

    pub fn kind(&self) -> GameKind {
        match self {
            ActiveGame::StackOverflow(_) => GameKind::StackOverflow,
            ActiveGame::DailyPr(_) => GameKind::DailyPr,
            ActiveGame::DailyFix(_) => GameKind::DailyFix,
        }
    }

    /// Whether the game consumes typed text while playing. Drives the
    /// input mode of the event loop.
    pub fn text_input(&self) -> bool {
        matches!(self, ActiveGame::DailyPr(_) | ActiveGame::DailyFix(_))
    }

    pub fn handle_input(&mut self, input: GameInput) {
        match self {
            ActiveGame::StackOverflow(game) => game.handle_input(input),
            ActiveGame::DailyPr(game) => game.handle_input(input),
            ActiveGame::DailyFix(game) => game.handle_input(input),
        }
    }

    pub fn update(&mut self, dt: Duration) {
        match self {
            ActiveGame::StackOverflow(game) => game.update(dt),
            ActiveGame::DailyPr(game) => game.update(dt),
            ActiveGame::DailyFix(game) => game.update(dt),
        }
    }

    pub fn is_game_over(&self) -> bool {
        match self {
            ActiveGame::StackOverflow(game) => game.is_game_over(),
            ActiveGame::DailyPr(game) => game.is_game_over(),
            ActiveGame::DailyFix(game) => game.is_game_over(),
        }
    }

    pub fn score(&self) -> u64 {
        match self {
            ActiveGame::StackOverflow(game) => game.score(),
            ActiveGame::DailyPr(game) => game.score(),
            ActiveGame::DailyFix(game) => game.score(),
        }
    }

    pub fn completed_run(&self) -> Option<CompletedRun> {
        if !self.is_game_over() {
            return None;
        }
        Some(match self {
            ActiveGame::StackOverflow(game) => CompletedRun::StackOverflow {
                score: game.score(),
                duration_ms: game.elapsed_millis(),
                height: game.height(),
            },
            ActiveGame::DailyPr(game) => CompletedRun::DailyPr {
                client_run_id: game.completion_run_id(),
                day: game.commit_number(),
                solved: game.state() == daily_pr::PrState::Solved,
                guesses: game.guesses() as u8,
                duration_ms: game.elapsed_millis(),
            },
            ActiveGame::DailyFix(game) => CompletedRun::DailyFix {
                day: game.challenge_day(),
                charged_duration_ms: game.charged_duration_millis(),
                attempts: game.attempts(),
                hint_used: game.hint_shown(),
            },
        })
    }

    /// Seconds elapsed in the run so far (scoring or solving time).
    #[cfg(test)]
    pub fn elapsed(&self) -> f64 {
        match self {
            ActiveGame::StackOverflow(game) => game.elapsed(),
            ActiveGame::DailyPr(game) => game.elapsed(),
            ActiveGame::DailyFix(game) => game.elapsed(),
        }
    }

    /// Human-readable result for the MVP board (e.g. "3 guesses").
    /// `None` for games whose raw score reads well on its own.
    pub fn score_note(&self) -> Option<String> {
        match self {
            ActiveGame::StackOverflow(_) => None,
            ActiveGame::DailyPr(game) => Some(format!("{} guesses", game.guesses())),
            ActiveGame::DailyFix(game) => Some(format!(
                "fixed in {}",
                scoring::format_elapsed(game.total_seconds())
            )),
        }
    }

    pub fn set_viewport(&mut self, playfield_cols: u16, playfield_rows: u16) {
        match self {
            ActiveGame::StackOverflow(game) => game.set_viewport(playfield_cols),
            ActiveGame::DailyPr(_) | ActiveGame::DailyFix(_) => {
                let _ = (playfield_cols, playfield_rows);
            }
        }
    }

    #[cfg(test)]
    pub fn as_stack_overflow(&self) -> Option<&StackOverflow> {
        match self {
            ActiveGame::StackOverflow(game) => Some(game),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn as_stack_overflow_mut(&mut self) -> Option<&mut StackOverflow> {
        match self {
            ActiveGame::StackOverflow(game) => Some(game),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn as_daily_pr(&self) -> Option<&DailyPr> {
        match self {
            ActiveGame::DailyPr(game) => Some(game),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn as_daily_fix(&self) -> Option<&DailyFix> {
        match self {
            ActiveGame::DailyFix(game) => Some(game),
            _ => None,
        }
    }
}
