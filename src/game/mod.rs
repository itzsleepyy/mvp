pub mod collision;
pub mod obstacle;
pub mod player;
pub mod scoring;
pub mod state;
pub mod twenty_one;
pub mod world;

use std::time::Duration;

use obstacle::Obstacle;
use player::Player;
use scoring::Score;
use state::RunState;
use twenty_one::TwentyOne;
use world::World;

/// The longest simulation step accepted in a single update, protecting the
/// physics from huge frame gaps (terminal stalls, debugger pauses, ...).
pub const MAX_FRAME_TIME: Duration = Duration::from_millis(50);

/// Input events as understood by a game. Deliberately tiny: each game reacts
/// only to its own inputs and ignores the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameInput {
    /// SPACE/↑ — Stack Jump.
    Jump,
    /// H — Twenty One.
    Hit,
    /// S — Twenty One.
    Stand,
    /// ENTER — Twenty One (next round).
    Confirm,
}

/// The games WaitState ships, in menu order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameKind {
    StackJump,
    TwentyOne,
}

impl GameKind {
    pub const ALL: [GameKind; 2] = [GameKind::StackJump, GameKind::TwentyOne];

    /// Stable identifier used for high-score storage.
    pub fn id(self) -> &'static str {
        match self {
            GameKind::StackJump => "stack-jump",
            GameKind::TwentyOne => "twenty-one",
        }
    }

    /// Display name.
    pub fn title(self) -> &'static str {
        match self {
            GameKind::StackJump => "Stack Jump",
            GameKind::TwentyOne => "Twenty One",
        }
    }

    /// One-line description for the game menu.
    pub fn blurb(self) -> &'static str {
        match self {
            GameKind::StackJump => "hop the obstacles",
            GameKind::TwentyOne => "beat the dealer to 21",
        }
    }
}

/// A live game of one of the supported kinds. Exposes the small surface the
/// app and renderer need, so neither depends on any one game's rules.
pub enum ActiveGame {
    StackJump(StackJump),
    TwentyOne(TwentyOne),
}

impl ActiveGame {
    pub fn new(kind: GameKind, seed: u64, playfield_cols: u16, playfield_rows: u16) -> Self {
        match kind {
            GameKind::StackJump => {
                ActiveGame::StackJump(StackJump::new(seed, playfield_cols, playfield_rows))
            }
            GameKind::TwentyOne => ActiveGame::TwentyOne(TwentyOne::new(seed)),
        }
    }

    pub fn kind(&self) -> GameKind {
        match self {
            ActiveGame::StackJump(_) => GameKind::StackJump,
            ActiveGame::TwentyOne(_) => GameKind::TwentyOne,
        }
    }

    pub fn handle_input(&mut self, input: GameInput) {
        match self {
            ActiveGame::StackJump(game) => game.handle_input(input),
            ActiveGame::TwentyOne(game) => game.handle_input(input),
        }
    }

    pub fn update(&mut self, dt: Duration) {
        match self {
            ActiveGame::StackJump(game) => game.update(dt),
            ActiveGame::TwentyOne(game) => game.update(dt),
        }
    }

    pub fn is_game_over(&self) -> bool {
        match self {
            ActiveGame::StackJump(game) => game.is_game_over(),
            ActiveGame::TwentyOne(game) => game.is_game_over(),
        }
    }

    pub fn score(&self) -> u64 {
        match self {
            ActiveGame::StackJump(game) => game.score(),
            ActiveGame::TwentyOne(game) => game.score(),
        }
    }

    pub fn set_viewport(&mut self, playfield_cols: u16, playfield_rows: u16) {
        match self {
            ActiveGame::StackJump(game) => game.set_viewport(playfield_cols, playfield_rows),
            ActiveGame::TwentyOne(_) => {}
        }
    }

    #[cfg(test)]
    pub fn as_stack_jump(&self) -> Option<&StackJump> {
        match self {
            ActiveGame::StackJump(game) => Some(game),
            ActiveGame::TwentyOne(_) => None,
        }
    }

    #[cfg(test)]
    pub fn as_stack_jump_mut(&mut self) -> Option<&mut StackJump> {
        match self {
            ActiveGame::StackJump(game) => Some(game),
            ActiveGame::TwentyOne(_) => None,
        }
    }

    #[cfg(test)]
    pub fn as_twenty_one(&self) -> Option<&TwentyOne> {
        match self {
            ActiveGame::TwentyOne(game) => Some(game),
            ActiveGame::StackJump(_) => None,
        }
    }

    #[cfg(test)]
    pub fn as_twenty_one_mut(&mut self) -> Option<&mut TwentyOne> {
        match self {
            ActiveGame::TwentyOne(game) => Some(game),
            ActiveGame::StackJump(_) => None,
        }
    }
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
            GameInput::Hit | GameInput::Stand | GameInput::Confirm => {}
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

    #[test]
    fn game_kinds_are_stable_and_ordered() {
        assert_eq!(GameKind::ALL, [GameKind::StackJump, GameKind::TwentyOne]);
        assert_eq!(GameKind::StackJump.id(), "stack-jump");
        assert_eq!(GameKind::TwentyOne.id(), "twenty-one");
        assert_eq!(GameKind::StackJump.title(), "Stack Jump");
        assert_eq!(GameKind::TwentyOne.title(), "Twenty One");
    }

    #[test]
    fn active_game_constructs_each_kind() {
        let jump = ActiveGame::new(GameKind::StackJump, 1, COLS, ROWS);
        assert_eq!(jump.kind(), GameKind::StackJump);
        assert!(jump.as_stack_jump().is_some());
        assert!(jump.as_twenty_one().is_none());
        assert_eq!(jump.score(), 0);

        let twenty_one = ActiveGame::new(GameKind::TwentyOne, 1, COLS, ROWS);
        assert_eq!(twenty_one.kind(), GameKind::TwentyOne);
        assert!(twenty_one.as_twenty_one().is_some());
        assert!(twenty_one.as_stack_jump().is_none());
        assert_eq!(twenty_one.score(), twenty_one::STARTING_CHIPS);
    }

    #[test]
    fn active_game_forwards_input_and_update() {
        let mut game = ActiveGame::new(GameKind::StackJump, 1, COLS, ROWS);
        game.handle_input(GameInput::Jump);
        assert!(!game.as_stack_jump().unwrap().render_state().player.grounded);
        game.update(Duration::from_secs(1));
        assert!(game.score() > 0);

        let mut twenty_one = ActiveGame::new(GameKind::TwentyOne, 1, COLS, ROWS);
        let cards_before = twenty_one.as_twenty_one().unwrap().player_hand().len();
        twenty_one.handle_input(GameInput::Jump); // ignored
        assert_eq!(
            twenty_one.as_twenty_one().unwrap().player_hand().len(),
            cards_before
        );
        twenty_one.update(Duration::from_secs(5)); // no-op
        assert_eq!(twenty_one.score(), twenty_one::STARTING_CHIPS);
    }

    #[test]
    fn active_game_viewport_only_affects_stack_jump() {
        let mut game = ActiveGame::new(GameKind::StackJump, 1, COLS, ROWS);
        game.set_viewport(100, ROWS);
        let world = game.as_stack_jump().unwrap();
        assert_eq!(world.world.spawn_x, f64::from(100 - player_col(100)) + 1.0);

        let mut twenty_one = ActiveGame::new(GameKind::TwentyOne, 1, COLS, ROWS);
        twenty_one.set_viewport(100, ROWS); // must not panic or change anything
        assert_eq!(twenty_one.score(), twenty_one::STARTING_CHIPS);
    }
}
