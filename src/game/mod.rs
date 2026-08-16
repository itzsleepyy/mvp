pub mod collision;
pub mod obstacle;
pub mod player;
pub mod scoring;
pub mod state;
pub mod world;

use std::time::Duration;

use obstacle::Obstacle;
use player::Player;
use scoring::Score;
use state::RunState;
use world::World;

/// The longest simulation step accepted in a single update, protecting the
/// physics from huge frame gaps (terminal stalls, debugger pauses, ...).
pub const MAX_FRAME_TIME: Duration = Duration::from_millis(50);

/// Input events as understood by a game. Deliberately tiny: gameplay is
/// one-button by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameInput {
    Jump,
}

/// The first WaitState game: an endless runner where the player hops over
/// obstacles while the world scrolls past. Pure logic — no terminal types —
/// so it can be unit tested and simulated headlessly.
pub struct StackJump {
    player: Player,
    pub(crate) world: World,
    score: Score,
    state: RunState,
}

impl StackJump {
    pub fn new(seed: u64, playfield_cols: u16, playfield_rows: u16) -> Self {
        let col = player_col(playfield_cols);
        let _ = playfield_rows; // vertical extent not needed yet; kept for future games
        Self {
            player: Player::new(),
            world: World::new(seed, col, playfield_cols),
            score: Score::new(),
            state: RunState::Playing,
        }
    }

    /// Advances the simulation by `dt`. Clamped to [`MAX_FRAME_TIME`] so
    /// behaviour stays stable regardless of render rate.
    pub fn update(&mut self, dt: Duration) {
        if self.state == RunState::GameOver {
            return;
        }
        let dt = dt.min(MAX_FRAME_TIME).as_secs_f64();
        let multiplier = self.world.speed_multiplier();
        self.player.update(dt);
        let passed = self.world.update(dt);
        self.score.update(dt, multiplier);
        self.score.add_bonus(passed as u64 * scoring::PASS_REWARD);
        if self.collided() {
            self.state = RunState::GameOver;
        }
    }

    pub fn handle_input(&mut self, input: GameInput) {
        match input {
            GameInput::Jump => self.player.jump(),
        }
    }

    /// The playfield changed size (terminal resize). Player column and spawn
    /// position adapt so gameplay stays fair.
    pub fn set_viewport(&mut self, playfield_cols: u16, playfield_rows: u16) {
        let _ = playfield_rows;
        self.world
            .set_viewport(player_col(playfield_cols), playfield_cols);
    }

    pub fn is_game_over(&self) -> bool {
        self.state == RunState::GameOver
    }

    pub fn score(&self) -> u64 {
        self.score.value()
    }

    pub fn elapsed(&self) -> f64 {
        self.score.elapsed()
    }

    pub fn speed_multiplier(&self) -> f64 {
        self.world.speed_multiplier()
    }

    /// Snapshot of everything the UI needs to draw this frame.
    pub fn render_state(&self) -> GameRenderState<'_> {
        GameRenderState {
            player: &self.player,
            obstacles: self.world.obstacles(),
        }
    }

    fn collided(&self) -> bool {
        let player_rect = self.player.rect();
        self.world
            .obstacles()
            .iter()
            .any(|obstacle| collision::intersects(&player_rect, &obstacle.rect()))
    }
}

/// Plain-data snapshot of the game for the renderer. Keeps game state
/// independent from Ratatui widgets.
pub struct GameRenderState<'a> {
    pub player: &'a Player,
    pub obstacles: &'a [Obstacle],
}

/// The column (from the left of the playfield) where the player stands,
/// anchored near the left edge so incoming obstacles stay visible.
pub fn player_col(playfield_cols: u16) -> u16 {
    (playfield_cols / 5).clamp(6, 14)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLS: u16 = 58;
    const ROWS: u16 = 12;

    #[test]
    fn new_game_is_playing_with_zero_score() {
        let game = StackJump::new(1, COLS, ROWS);
        assert!(!game.is_game_over());
        assert_eq!(game.score(), 0);
        assert_eq!(game.elapsed(), 0.0);
    }

    #[test]
    fn jump_input_launches_player() {
        let mut game = StackJump::new(1, COLS, ROWS);
        game.handle_input(GameInput::Jump);
        assert!(!game.render_state().player.grounded);
    }

    #[test]
    fn score_increases_during_gameplay() {
        let mut game = StackJump::new(1, COLS, ROWS);
        game.update(Duration::from_secs(1));
        assert!(game.score() > 0);
    }

    #[test]
    fn collision_causes_game_over() {
        let mut game = StackJump::new(1, COLS, ROWS);
        let obstacle = Obstacle::new(0.5, obstacle::ObstacleKind::Small);
        game.world.obstacles.push(obstacle);
        game.update(Duration::from_millis(16));
        assert!(game.is_game_over());
        assert_eq!(game.state, RunState::GameOver);
    }

    #[test]
    fn game_over_freezes_simulation() {
        let mut game = StackJump::new(1, COLS, ROWS);
        let obstacle = Obstacle::new(0.5, obstacle::ObstacleKind::Small);
        game.world.obstacles.push(obstacle);
        game.update(Duration::from_millis(16));
        let final_score = game.score();
        game.update(Duration::from_secs(1));
        assert_eq!(game.score(), final_score);
    }

    #[test]
    fn passing_an_obstacle_awards_a_bonus() {
        let mut game = StackJump::new(1, COLS, ROWS);
        let obstacle = Obstacle::new(-3.0, obstacle::ObstacleKind::Small);
        game.world.obstacles.push(obstacle);
        game.update(Duration::from_millis(16));
        assert!(!game.is_game_over());
        assert!(game.score() >= scoring::PASS_REWARD);
        assert!(game.world.obstacles.is_empty());
    }

    #[test]
    fn viewport_move_is_applied_to_world() {
        let mut game = StackJump::new(1, COLS, ROWS);
        game.set_viewport(100, ROWS);
        assert_eq!(game.world.spawn_x, f64::from(100 - player_col(100)) + 1.0);
    }

    #[test]
    fn player_col_is_clamped() {
        assert_eq!(player_col(0), 6);
        assert_eq!(player_col(10), 6);
        assert_eq!(player_col(58), 11);
        assert_eq!(player_col(1000), 14);
    }
}
