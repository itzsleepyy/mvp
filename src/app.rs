use std::time::Duration;

use crate::config::HighScoreStore;
use crate::event::AppInput;
use crate::game::{GameInput, StackJump};
use crate::ui;

/// Minimum usable terminal size, in columns × rows.
pub const MIN_COLS: u16 = 60;
pub const MIN_ROWS: u16 = 20;

/// Top-level application states. `Playing`/`Paused`/`GameOver` are all
/// sub-states of a live run; `Menu` is the idle screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Menu,
    Playing,
    Paused,
    GameOver,
}

/// The application: owns state transitions and the active game. All
/// transitions are plain methods so they can be driven by keyboard input
/// today and by agent events (pause on `AgentNeedsInput`, etc.) later.
pub struct App {
    pub state: AppState,
    game: Option<StackJump>,
    store: HighScoreStore,
    terminal_size: Option<(u16, u16)>,
    new_record: bool,
    should_quit: bool,
}

impl App {
    pub fn new(store: HighScoreStore) -> Self {
        Self {
            state: AppState::Menu,
            game: None,
            store,
            terminal_size: None,
            new_record: false,
            should_quit: false,
        }
    }

    // ---- input -----------------------------------------------------------

    pub fn handle_input(&mut self, input: AppInput) {
        match input {
            AppInput::Quit => self.quit(),
            AppInput::Confirm => match self.state {
                AppState::Menu | AppState::GameOver => self.start_game(),
                AppState::Playing | AppState::Paused => {}
            },
            AppInput::Jump => {
                if self.state == AppState::Playing
                    && let Some(game) = &mut self.game
                {
                    game.handle_input(GameInput::Jump);
                }
            }
            AppInput::TogglePause => self.toggle_pause(),
            AppInput::Restart => {
                if self.state == AppState::GameOver {
                    self.start_game();
                }
            }
            AppInput::Back => match self.state {
                AppState::Menu => {}
                AppState::Playing | AppState::Paused | AppState::GameOver => self.back_to_menu(),
            },
            AppInput::Resize(cols, rows) => self.set_terminal_size(cols, rows),
        }
    }

    // ---- transitions (programmatic, reusable) ----------------------------

    pub fn start_game(&mut self) {
        let (cols, rows) = self.playfield_dims();
        self.game = Some(StackJump::new(rand::random::<u64>(), cols, rows));
        self.new_record = false;
        self.state = AppState::Playing;
    }

    pub fn pause(&mut self) {
        if self.state == AppState::Playing {
            self.state = AppState::Paused;
        }
    }

    pub fn resume(&mut self) {
        if self.state == AppState::Paused {
            self.state = AppState::Playing;
        }
    }

    pub fn toggle_pause(&mut self) {
        match self.state {
            AppState::Playing => self.pause(),
            AppState::Paused => self.resume(),
            AppState::Menu | AppState::GameOver => {}
        }
    }

    pub fn back_to_menu(&mut self) {
        self.state = AppState::Menu;
        self.game = None;
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    // ---- simulation ------------------------------------------------------

    /// Advances the simulation by `dt`. The game only advances while
    /// actively playing on a sufficiently large terminal, so pause and
    /// terminal-size handling live in exactly one place.
    pub fn tick(&mut self, dt: Duration) {
        if self.state != AppState::Playing || self.too_small() {
            return;
        }
        let Some(game) = &mut self.game else {
            return;
        };
        game.update(dt);
        if game.is_game_over() {
            let score = game.score();
            self.finish_run(score);
        }
    }

    fn finish_run(&mut self, score: u64) {
        self.new_record = self.store.record(score);
        self.state = AppState::GameOver;
    }

    // ---- terminal size ----------------------------------------------------

    pub fn set_terminal_size(&mut self, cols: u16, rows: u16) {
        self.terminal_size = Some((cols, rows));
        let (play_cols, play_rows) = self.playfield_dims();
        if let Some(game) = &mut self.game {
            game.set_viewport(play_cols, play_rows);
        }
    }

    pub fn terminal_size(&self) -> Option<(u16, u16)> {
        self.terminal_size
    }

    pub fn too_small(&self) -> bool {
        match self.terminal_size {
            Some((cols, rows)) => cols < MIN_COLS || rows < MIN_ROWS,
            None => false,
        }
    }

    fn playfield_dims(&self) -> (u16, u16) {
        match self.terminal_size {
            Some((cols, rows)) => ui::playfield_dims(cols, rows),
            None => (58, 12),
        }
    }

    // ---- accessors --------------------------------------------------------

    pub fn game(&self) -> Option<&StackJump> {
        self.game.as_ref()
    }

    pub fn best_score(&self) -> u64 {
        self.store.high_score()
    }

    /// True while the finished run just set a new high score (shown on the
    /// game-over panel until the next run starts).
    pub fn is_new_record(&self) -> bool {
        self.new_record
    }

    #[cfg(test)]
    pub(crate) fn spawn_test_obstacle(&mut self) {
        use crate::game::obstacle::{Obstacle, ObstacleKind};
        if let Some(game) = &mut self.game {
            game.world
                .obstacles
                .push(Obstacle::new(0.5, ObstacleKind::Small));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::obstacle::{Obstacle, ObstacleKind};

    fn temp_store(name: &str) -> HighScoreStore {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "waitstate_app_test_{}_{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_file(&path);
        HighScoreStore::load(path)
    }

    fn app_with_size(store: HighScoreStore, cols: u16, rows: u16) -> App {
        let mut app = App::new(store);
        app.set_terminal_size(cols, rows);
        app
    }

    fn playing_app() -> App {
        let mut app = app_with_size(temp_store("playing.json"), 100, 30);
        app.start_game();
        app
    }

    fn collide(app: &mut App) {
        let obstacle = Obstacle::new(0.5, ObstacleKind::Small);
        app.game.as_mut().unwrap().world.obstacles.push(obstacle);
    }

    #[test]
    fn starts_in_menu() {
        let app = app_with_size(temp_store("menu.json"), 100, 30);
        assert_eq!(app.state, AppState::Menu);
        assert!(app.game().is_none());
        assert!(!app.should_quit());
    }

    #[test]
    fn confirm_starts_game_from_menu() {
        let mut app = app_with_size(temp_store("start.json"), 100, 30);
        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::Playing);
        assert!(app.game().is_some());
    }

    #[test]
    fn quit_flag_is_set_by_input() {
        let mut app = app_with_size(temp_store("quit.json"), 100, 30);
        app.handle_input(AppInput::Quit);
        assert!(app.should_quit());
    }

    #[test]
    fn score_grows_while_playing_and_stops_when_paused() {
        let mut app = playing_app();
        app.tick(Duration::from_secs(1));
        let after_first = app.game().unwrap().score();
        assert!(after_first > 0);

        app.pause();
        assert_eq!(app.state, AppState::Paused);
        app.tick(Duration::from_secs(1));
        assert_eq!(app.game().unwrap().score(), after_first);

        app.resume();
        assert_eq!(app.state, AppState::Playing);
        app.tick(Duration::from_secs(1));
        assert!(app.game().unwrap().score() > after_first);
    }

    #[test]
    fn pause_input_toggles_state() {
        let mut app = playing_app();
        app.handle_input(AppInput::TogglePause);
        assert_eq!(app.state, AppState::Paused);
        app.handle_input(AppInput::TogglePause);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn jump_input_reaches_the_game_only_while_playing() {
        let mut app = playing_app();
        app.handle_input(AppInput::Jump);
        assert!(!app.game().unwrap().render_state().player.grounded);

        app.pause();
        let player = *app.game().unwrap().render_state().player;
        app.handle_input(AppInput::Jump);
        assert_eq!(*app.game().unwrap().render_state().player, player);
    }

    #[test]
    fn collision_causes_game_over_and_records_high_score() {
        let mut app = playing_app();
        app.tick(Duration::from_secs(1));
        collide(&mut app);
        app.tick(Duration::from_millis(16));

        assert_eq!(app.state, AppState::GameOver);
        let score = app.game().unwrap().score();
        assert_eq!(app.best_score(), score);
        assert!(app.is_new_record());
    }

    #[test]
    fn restart_starts_a_fresh_run() {
        let mut app = playing_app();
        app.tick(Duration::from_secs(1));
        collide(&mut app);
        app.tick(Duration::from_millis(16));
        let best = app.best_score();
        assert!(best > 0);

        app.handle_input(AppInput::Restart);
        assert_eq!(app.state, AppState::Playing);
        assert_eq!(app.game().unwrap().score(), 0);
        assert_eq!(app.best_score(), best);
        assert!(!app.is_new_record());
    }

    #[test]
    fn lower_score_does_not_replace_best() {
        let mut app = playing_app();
        app.tick(Duration::from_secs(2));
        collide(&mut app);
        app.tick(Duration::from_millis(16));
        let best = app.best_score();

        app.start_game();
        collide(&mut app); // immediate death: score ~0
        app.tick(Duration::from_millis(16));
        assert_eq!(app.best_score(), best);
        assert!(!app.is_new_record());
    }

    #[test]
    fn escape_returns_to_menu_from_any_run_state() {
        let mut playing = playing_app();
        playing.handle_input(AppInput::Back);
        assert_eq!(playing.state, AppState::Menu);
        assert!(playing.game().is_none());

        let mut paused = playing_app();
        paused.pause();
        paused.handle_input(AppInput::Back);
        assert_eq!(paused.state, AppState::Menu);

        let mut over = playing_app();
        collide(&mut over);
        over.tick(Duration::from_millis(16));
        over.handle_input(AppInput::Back);
        assert_eq!(over.state, AppState::Menu);
    }

    #[test]
    fn too_small_terminal_freezes_the_game() {
        let mut app = app_with_size(temp_store("small.json"), 100, 30);
        app.start_game();
        app.tick(Duration::from_secs(1));
        let before = app.game().unwrap().score();
        assert!(before > 0);

        app.set_terminal_size(42, 14);
        assert!(app.too_small());
        app.tick(Duration::from_secs(1));
        assert_eq!(app.game().unwrap().score(), before);

        app.set_terminal_size(100, 30);
        assert!(!app.too_small());
        app.tick(Duration::from_secs(1));
        assert!(app.game().unwrap().score() > before);
    }

    #[test]
    fn resize_updates_game_viewport() {
        let mut app = playing_app();
        app.set_terminal_size(120, 40);
        let (cols, rows) = ui::playfield_dims(120, 40);
        assert_eq!(
            app.game().unwrap().world.spawn_x,
            f64::from(cols - crate::game::player_col(cols)) + 1.0
        );
        assert_eq!(rows, ui::playfield_dims(120, 40).1);
    }

    #[test]
    fn back_on_menu_does_nothing() {
        let mut app = app_with_size(temp_store("menuback.json"), 100, 30);
        app.handle_input(AppInput::Back);
        assert_eq!(app.state, AppState::Menu);
    }

    #[test]
    fn high_score_survives_across_app_instances() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "waitstate_app_test_{}_{}",
            std::process::id(),
            "persist.json"
        ));
        let _ = std::fs::remove_file(&path);

        let mut first = app_with_size(HighScoreStore::load(&path), 100, 30);
        first.start_game();
        first.tick(Duration::from_secs(3));
        collide(&mut first);
        first.tick(Duration::from_millis(16));
        let best = first.best_score();
        assert!(best > 0);

        let reloaded = app_with_size(HighScoreStore::load(&path), 100, 30);
        assert_eq!(reloaded.best_score(), best);
    }
}
