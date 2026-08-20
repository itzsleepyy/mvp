//! Stack Overflow — the arcade stacker, themed as a call stack. A block as
//! wide as the current tower slides side to side; SPACE drops it. Overhang
//! is trimmed from both block and tower; a drop that misses entirely is a
//! stack overflow. Pure logic — no terminal types — so it can be unit
//! tested and simulated headlessly.

use std::time::Duration;

use super::{GameInput, MAX_FRAME_TIME};
use crate::game::state::RunState;

/// Starting width of the tower, in cells.
pub const START_WIDTH: i32 = 14;
/// Points per frame placed.
const FRAME_REWARD: u64 = 100;
/// Bonus for a perfectly aligned drop.
const PERFECT_REWARD: u64 = 250;
/// Base slide speed in cells per second.
const BASE_SPEED: f64 = 6.0;
/// Slide speed grows with stack height.
const HEIGHT_SPEED: f64 = 0.04;
/// ...and with the perfect-drop streak.
const COMBO_SPEED: f64 = 1.1;
/// Slide speed is capped so the game stays playable.
const MAX_SPEED: f64 = 42.0;

/// One placed frame of the stack, in absolute playfield cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layer {
    /// Left edge of the layer in playfield columns.
    pub left: i32,
    /// Width of the layer in cells.
    pub width: i32,
}

/// A run of the stacker. The tower lives in `layers` (bottom first); a new
/// block slides across the top layer between `-w` and `+w` cells of
/// horizontal offset, where `w` is the top layer's width.
pub struct StackOverflow {
    state: RunState,
    layers: Vec<Layer>,
    /// The sliding block's offset from the top layer's left edge, in cells,
    /// ranging over `[-w, w]`.
    block_offset: f64,
    /// Slide direction: +1 right, -1 left.
    direction: i32,
    speed: f64,
    score: u64,
    /// Consecutive perfect drops; resets on any trim.
    perfect_streak: u32,
    elapsed_millis: u64,
    viewport_cols: u16,
}

impl StackOverflow {
    pub fn new(seed: u64, playfield_cols: u16) -> Self {
        let width = START_WIDTH;
        let left = center_left(playfield_cols, width);
        // The first block starts somewhere off-center, seeded for variety.
        let span = 2 * width + 1;
        let offset = (seed % span as u64) as i32 - width;
        Self {
            state: RunState::Playing,
            layers: vec![Layer { left, width }],
            block_offset: offset as f64,
            direction: if offset < 0 { 1 } else { -1 },
            speed: BASE_SPEED,
            score: 0,
            perfect_streak: 0,
            elapsed_millis: 0,
            viewport_cols: playfield_cols,
        }
    }

    /// Advances the simulation by `dt`. Physics steps are clamped to
    /// [`MAX_FRAME_TIME`] so behaviour stays stable regardless of render
    /// rate; the run clock counts real time even after a long stall.
    pub fn update(&mut self, dt: Duration) {
        if self.state != RunState::Playing {
            return;
        }
        self.elapsed_millis += dt.as_millis() as u64;
        let dt = dt.min(MAX_FRAME_TIME).as_secs_f64();
        let width = self.top().width as f64;
        self.block_offset += f64::from(self.direction) * self.speed * dt;
        if self.block_offset >= width {
            self.block_offset = width;
            self.direction = -1;
        } else if self.block_offset <= -width {
            self.block_offset = -width;
            self.direction = 1;
        }
    }

    pub fn handle_input(&mut self, input: GameInput) {
        match input {
            GameInput::Jump => self.drop(),
            GameInput::Confirm | GameInput::Type(_) | GameInput::Backspace => {}
        }
    }

    /// Drops the sliding block onto the stack. The new layer is the
    /// intersection of block and tower top; a drop with no overlap is a
    /// stack overflow and ends the run.
    pub fn drop(&mut self) {
        if self.state != RunState::Playing {
            return;
        }
        let top = *self.top();
        let offset = self.block_offset.round();
        if offset.abs() >= f64::from(top.width) {
            self.state = RunState::GameOver;
            return;
        }
        let overlap = f64::from(top.width) - offset.abs();
        let perfect = offset == 0.0;
        let left = top.left + offset.max(0.0) as i32;
        self.layers.push(Layer {
            left,
            width: overlap as i32,
        });
        self.score += FRAME_REWARD;
        if perfect {
            self.perfect_streak += 1;
            self.score += PERFECT_REWARD;
        } else {
            self.perfect_streak = 0;
        }
        let combo = COMBO_SPEED.powi(self.perfect_streak as i32);
        self.speed =
            (BASE_SPEED * (1.0 + HEIGHT_SPEED * self.height() as f64) * combo).min(MAX_SPEED);
        // The next block starts centered on the new, narrower top.
        self.block_offset = 0.0;
        self.direction = 1;
    }

    /// The playfield changed size: the whole tower recenters so gameplay
    /// stays fair across resizes.
    pub fn set_viewport(&mut self, playfield_cols: u16) {
        let delta = (i32::from(playfield_cols) - i32::from(self.viewport_cols)) / 2;
        for layer in &mut self.layers {
            layer.left += delta;
        }
        self.viewport_cols = playfield_cols;
    }

    pub fn is_game_over(&self) -> bool {
        self.state == RunState::GameOver
    }

    pub fn score(&self) -> u64 {
        self.score
    }

    pub fn elapsed(&self) -> f64 {
        self.elapsed_millis as f64 / 1000.0
    }

    pub fn elapsed_millis(&self) -> u64 {
        self.elapsed_millis
    }

    /// Stack height in frames.
    pub fn height(&self) -> u64 {
        self.layers.len() as u64
    }

    pub fn perfect_streak(&self) -> u32 {
        self.perfect_streak
    }

    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// The sliding block's current offset from the top layer's left edge.
    pub fn block_offset(&self) -> f64 {
        self.block_offset
    }

    pub fn direction(&self) -> i32 {
        self.direction
    }

    #[cfg(test)]
    pub fn speed(&self) -> f64 {
        self.speed
    }

    fn top(&self) -> &Layer {
        self.layers.last().expect("the base layer never leaves")
    }

    /// The top layer of the stack (what the block slides across).
    pub fn top_layer(&self) -> &Layer {
        self.top()
    }

    /// Test hook: forces a game over so app-flow tests can drive the
    /// game-over path deterministically.
    #[cfg(test)]
    pub(crate) fn debug_force_overflow(&mut self) {
        self.state = RunState::GameOver;
    }

    /// Test hook: positions the sliding block so app-flow tests can drop
    /// it at a known offset.
    #[cfg(test)]
    pub(crate) fn debug_set_block(&mut self, offset: f64) {
        self.block_offset = offset;
    }
}

/// Centers a tower of `width` cells in a playfield of `cols` columns.
fn center_left(cols: u16, width: i32) -> i32 {
    ((i32::from(cols) - width) / 2).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLS: u16 = 58;

    fn game() -> StackOverflow {
        StackOverflow::new(1, COLS)
    }

    fn top_left(game: &StackOverflow) -> i32 {
        game.top().left
    }

    fn top_width(game: &StackOverflow) -> i32 {
        game.top().width
    }

    #[test]
    fn new_game_has_a_centered_base_and_zero_score() {
        let game = game();
        assert!(!game.is_game_over());
        assert_eq!(game.score(), 0);
        assert_eq!(game.height(), 1);
        assert_eq!(top_width(&game), START_WIDTH);
        assert_eq!(top_left(&game), (COLS as i32 - START_WIDTH) / 2);
        assert_eq!(game.speed(), BASE_SPEED);
    }

    #[test]
    fn the_block_slides_and_bounces_at_the_edges() {
        let mut game = game();
        game.block_offset = 3.0;
        game.direction = 1;
        game.speed = 1.0;
        let width = top_width(&game);
        for _ in 0..2000 {
            game.update(Duration::from_millis(16));
            assert!(
                game.block_offset >= -f64::from(width) - 0.1
                    && game.block_offset <= f64::from(width) + 0.1
            );
        }
        // The block must have crossed the tower in both directions.
        assert_ne!(game.direction, 0);
        assert!(!game.is_game_over(), "sliding must never end the game");
    }

    #[test]
    fn a_perfect_drop_keeps_the_width_and_awards_a_bonus() {
        let mut game = game();
        game.block_offset = 0.0;
        game.drop();
        assert_eq!(top_width(&game), START_WIDTH);
        assert_eq!(game.score(), FRAME_REWARD + PERFECT_REWARD);
        assert_eq!(game.perfect_streak(), 1);
        assert_eq!(game.height(), 2);
        assert!(!game.is_game_over());
    }

    #[test]
    fn a_partial_drop_trims_both_block_and_tower() {
        let mut game = game();
        game.block_offset = 4.0;
        game.drop();
        assert_eq!(top_width(&game), START_WIDTH - 4);
        assert_eq!(game.score(), FRAME_REWARD);
        assert_eq!(game.perfect_streak(), 0);
        // The new layer is shifted right by the overhang on the left.
        assert_eq!(top_left(&game), (COLS as i32 - START_WIDTH) / 2 + 4);
    }

    #[test]
    fn an_off_center_drop_to_the_left_shifts_the_layer_left() {
        let mut game = game();
        game.block_offset = -3.0;
        game.drop();
        assert_eq!(top_width(&game), START_WIDTH - 3);
        assert_eq!(top_left(&game), (COLS as i32 - START_WIDTH) / 2);
    }

    #[test]
    fn a_full_miss_is_a_stack_overflow() {
        let mut g = game();
        g.block_offset = f64::from(START_WIDTH);
        g.drop();
        assert!(g.is_game_over());
        assert_eq!(g.height(), 1, "no layer may be placed");
        assert_eq!(g.score(), 0);

        let mut g2 = game();
        g2.block_offset = -f64::from(START_WIDTH);
        g2.drop();
        assert!(g2.is_game_over());
    }

    #[test]
    fn game_over_freezes_the_simulation() {
        let mut game = game();
        game.debug_force_overflow();
        let score = game.score();
        game.update(Duration::from_secs(1));
        game.drop();
        assert_eq!(game.score(), score);
        assert_eq!(game.height(), 1);
    }

    #[test]
    fn speed_grows_with_height_and_streak_but_is_capped() {
        let mut game = game();
        for _ in 0..40 {
            game.block_offset = 0.0;
            game.drop(); // perfect drops: streak keeps climbing
        }
        assert!(game.speed() > BASE_SPEED);
        assert!(game.speed() <= MAX_SPEED);
        assert_eq!(game.perfect_streak(), 40);
    }

    #[test]
    fn a_missed_perfect_resets_the_streak_and_slows_the_game() {
        let mut game = game();
        for _ in 0..3 {
            game.block_offset = 0.0;
            game.drop();
        }
        let before = game.speed();
        game.block_offset = 2.0;
        game.drop(); // trimmed: streak broken
        assert_eq!(game.perfect_streak(), 0);
        assert!(game.speed() < before, "speed must drop after a trim");
    }

    #[test]
    fn elapsed_accumulates_only_while_playing() {
        let mut game = game();
        game.update(Duration::from_millis(500));
        assert!((game.elapsed() - 0.5).abs() < 0.01);
        game.debug_force_overflow();
        let frozen = game.elapsed();
        game.update(Duration::from_secs(1));
        assert_eq!(game.elapsed(), frozen);
    }

    #[test]
    fn viewport_change_recenters_the_tower() {
        let mut game = game();
        let before = top_left(&game);
        game.set_viewport(100);
        assert_eq!(top_left(&game), (100 - START_WIDTH) / 2);
        assert!(top_left(&game) > before);
        let width = top_width(&game);
        game.set_viewport(58);
        assert_eq!(top_left(&game), (58 - START_WIDTH) / 2);
        assert_eq!(top_width(&game), width);
    }

    #[test]
    fn ignored_inputs_do_nothing() {
        let mut game = game();
        let score = game.score();
        game.handle_input(GameInput::Confirm);
        game.handle_input(GameInput::Type('a'));
        game.handle_input(GameInput::Backspace);
        assert_eq!(game.score(), score);
        assert_eq!(game.height(), 1);
    }

    #[test]
    fn seeded_runs_start_identically() {
        let a = StackOverflow::new(42, COLS);
        let b = StackOverflow::new(42, COLS);
        assert_eq!(a.block_offset(), b.block_offset());
        assert_eq!(a.direction(), b.direction());
        let c = StackOverflow::new(43, COLS);
        assert_ne!(a.block_offset(), c.block_offset());
    }

    #[test]
    fn a_sequence_of_drops_never_panics() {
        let mut game = game();
        for raw in [0.0, 3.0, -5.0, 0.0, 7.0, -9.0, 1.0] {
            let offset: f64 = raw;
            game.block_offset = offset.min(f64::from(top_width(&game)));
            game.drop();
            game.update(Duration::from_millis(16));
        }
    }
}
