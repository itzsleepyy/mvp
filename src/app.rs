use std::time::Duration;

use crate::agent::{AgentEvent, AgentKind, AgentState};
use crate::config::HighScoreStore;
use crate::event::AppInput;
use crate::game::{GameInput, StackJump};
use crate::ui;

/// Minimum usable terminal size, in columns × rows.
pub const MIN_COLS: u16 = 60;
pub const MIN_ROWS: u16 = 20;

/// Why the game was paused by the agent. Manual pauses are tracked
/// separately as [`AppState::PausedManual`] so agent events can never
/// override them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    NeedsInput,
    Completed,
    Stopped,
}

/// Top-level application states. `Playing`/`PausedManual`/`PausedAgent`/
/// `GameOver` are all sub-states of a live run; `Menu` is the idle screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Menu,
    Playing,
    PausedManual,
    PausedAgent(PauseReason),
    GameOver,
}

/// The application: owns state transitions and the active game. All
/// transitions are plain methods so they can be driven by keyboard input
/// and by agent lifecycle events without either knowing about the other.
pub struct App {
    pub state: AppState,
    game: Option<StackJump>,
    store: HighScoreStore,
    terminal_size: Option<(u16, u16)>,
    new_record: bool,
    should_quit: bool,
    agent: AgentState,
    /// Whether an agent `Working` event may resume a run that the agent
    /// paused. Turned off in the application layer, never in the adapter.
    /// A run paused because the agent *completed* is never auto-resumed:
    /// the developer decides when to continue that run.
    agent_auto_resume: bool,
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
            agent: AgentState::default(),
            agent_auto_resume: true,
        }
    }

    // ---- input -----------------------------------------------------------

    pub fn handle_input(&mut self, input: AppInput) {
        match input {
            AppInput::Quit => self.quit(),
            AppInput::Confirm => match self.state {
                AppState::Menu | AppState::GameOver => self.start_game(),
                AppState::PausedAgent(_) => self.resume(),
                AppState::Playing | AppState::PausedManual => {}
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
                AppState::Playing
                | AppState::PausedManual
                | AppState::PausedAgent(_)
                | AppState::GameOver => self.back_to_menu(),
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

    /// Manual pause. Never entered by agent events.
    pub fn pause(&mut self) {
        if self.state == AppState::Playing {
            self.state = AppState::PausedManual;
        }
    }

    /// Resumes a run paused manually or by the agent.
    pub fn resume(&mut self) {
        match self.state {
            AppState::PausedManual | AppState::PausedAgent(_) => {
                self.state = AppState::Playing;
            }
            AppState::Menu | AppState::Playing | AppState::GameOver => {}
        }
    }

    pub fn toggle_pause(&mut self) {
        match self.state {
            AppState::Playing => self.pause(),
            AppState::PausedManual => self.resume(),
            // Pressing P while the agent paused the game hands control
            // back to the developer as a manual pause.
            AppState::PausedAgent(_) => self.state = AppState::PausedManual,
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

    // ---- agent events -----------------------------------------------------

    /// Applies one agent lifecycle event. Runs on the main loop only, so
    /// application state stays single-threaded. Defensive by design:
    /// duplicates are idempotent and unexpected orderings never panic —
    /// they at most update the agent status.
    pub fn handle_agent_event(&mut self, event: AgentEvent) {
        let before = self.state;
        self.agent.apply(event);
        match event {
            AgentEvent::Started => {}
            AgentEvent::Working => {
                if self.agent_auto_resume
                    && let AppState::PausedAgent(reason) = self.state
                    && reason != PauseReason::Completed
                {
                    self.state = AppState::Playing;
                }
            }
            AgentEvent::NeedsInput => self.set_agent_pause(PauseReason::NeedsInput),
            AgentEvent::Completed => self.set_agent_pause(PauseReason::Completed),
            AgentEvent::Stopped => self.set_agent_pause(PauseReason::Stopped),
        }
        if before != self.state {
            crate::debug_log!("agent event {event}: {before:?} → {:?}", self.state);
        } else {
            crate::debug_log!("agent event {event}: state unchanged ({:?})", self.state);
        }
    }

    /// Enters the agent-paused state, or replaces the pause reason when
    /// already paused by the agent. Manual pauses are never touched.
    fn set_agent_pause(&mut self, reason: PauseReason) {
        match self.state {
            AppState::Playing | AppState::PausedAgent(_) => {
                self.state = AppState::PausedAgent(reason);
            }
            AppState::Menu | AppState::PausedManual | AppState::GameOver => {}
        }
    }

    /// The connected agent's current status (or disconnected).
    pub fn agent(&self) -> &AgentState {
        &self.agent
    }

    /// Announces which agent to display before any event has arrived
    /// (`--agent claude`). The indicator still shows the live status.
    pub fn set_agent_kind(&mut self, kind: AgentKind) {
        self.agent.agent = kind;
        self.agent.status = crate::agent::AgentStatus::Idle;
    }

    /// Controls whether agent `Working` events may resume an agent-paused
    /// run (`--no-auto-resume` disables it).
    pub fn set_agent_auto_resume(&mut self, enabled: bool) {
        self.agent_auto_resume = enabled;
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
    use crate::agent::AgentStatus;
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
        assert_eq!(app.agent().status, AgentStatus::Disconnected);
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
        assert_eq!(app.state, AppState::PausedManual);
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
        assert_eq!(app.state, AppState::PausedManual);
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

        let mut agent_paused = playing_app();
        agent_paused.handle_agent_event(AgentEvent::NeedsInput);
        agent_paused.handle_input(AppInput::Back);
        assert_eq!(agent_paused.state, AppState::Menu);

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

    // ---- agent lifecycle transitions --------------------------------------

    #[test]
    fn needs_input_pauses_a_playing_game_for_the_agent() {
        let mut app = playing_app();
        app.tick(Duration::from_secs(1));
        app.handle_agent_event(AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        assert_eq!(app.agent().status, AgentStatus::NeedsInput);
    }

    #[test]
    fn working_resumes_an_agent_paused_game() {
        let mut app = playing_app();
        app.handle_agent_event(AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        app.handle_agent_event(AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn manual_pause_is_never_overridden_by_agent_events() {
        let mut app = playing_app();
        app.pause();
        app.handle_agent_event(AgentEvent::Working);
        assert_eq!(app.state, AppState::PausedManual);
        app.handle_agent_event(AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedManual);
        app.handle_agent_event(AgentEvent::Completed);
        assert_eq!(app.state, AppState::PausedManual);
    }

    #[test]
    fn game_over_is_not_disturbed_by_agent_events() {
        let mut app = playing_app();
        collide(&mut app);
        app.tick(Duration::from_millis(16));
        assert_eq!(app.state, AppState::GameOver);
        app.handle_agent_event(AgentEvent::Working);
        assert_eq!(app.state, AppState::GameOver);
        app.handle_agent_event(AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::GameOver);
    }

    #[test]
    fn menu_ignores_agent_events_without_crashing() {
        let mut app = app_with_size(temp_store("menuagent.json"), 100, 30);
        app.handle_agent_event(AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::Menu);
        app.handle_agent_event(AgentEvent::Working);
        assert_eq!(app.state, AppState::Menu);
        app.handle_agent_event(AgentEvent::Completed);
        assert_eq!(app.state, AppState::Menu);
        app.handle_agent_event(AgentEvent::Stopped);
        assert_eq!(app.state, AppState::Menu);
    }

    #[test]
    fn duplicate_events_are_idempotent() {
        let mut app = playing_app();
        app.handle_agent_event(AgentEvent::Working);
        app.handle_agent_event(AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);

        app.handle_agent_event(AgentEvent::NeedsInput);
        app.handle_agent_event(AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));

        app.handle_agent_event(AgentEvent::Working);
        app.handle_agent_event(AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn completion_pauses_the_run_and_requires_manual_resume() {
        let mut app = playing_app();
        app.tick(Duration::from_secs(1));
        app.handle_agent_event(AgentEvent::Completed);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Completed));

        // Claude working again does not silently continue a finished run.
        app.handle_agent_event(AgentEvent::Working);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Completed));

        // The developer decides when to continue it.
        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn session_end_pauses_and_working_resumes() {
        let mut app = playing_app();
        app.handle_agent_event(AgentEvent::Stopped);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Stopped));
        app.handle_agent_event(AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn pause_input_takes_control_from_agent_pause() {
        let mut app = playing_app();
        app.handle_agent_event(AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        app.handle_input(AppInput::TogglePause);
        assert_eq!(app.state, AppState::PausedManual);
        app.handle_input(AppInput::TogglePause);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn later_agent_status_replaces_the_pause_reason() {
        let mut app = playing_app();
        app.handle_agent_event(AgentEvent::NeedsInput);
        app.handle_agent_event(AgentEvent::Completed);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Completed));
        app.handle_agent_event(AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
    }

    #[test]
    fn agent_pause_preserves_the_whole_run() {
        use crate::game::obstacle::Obstacle;
        use crate::game::player::Player;

        fn snapshot(app: &App) -> (u64, f64, Player, Vec<Obstacle>, f64) {
            let game = app.game().unwrap();
            (
                game.score(),
                game.elapsed(),
                *game.render_state().player,
                game.world.obstacles.clone(),
                game.speed_multiplier(),
            )
        }

        let mut app = playing_app();
        app.tick(Duration::from_secs(2));
        let before = snapshot(&app);

        app.handle_agent_event(AgentEvent::NeedsInput);
        app.tick(Duration::from_secs(2));
        assert_eq!(snapshot(&app), before, "agent pause must freeze the run");

        app.handle_agent_event(AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);
        assert_eq!(
            snapshot(&app),
            before,
            "agent resume must not reset score, player, obstacles or difficulty"
        );
    }

    #[test]
    fn out_of_order_events_never_panic() {
        let mut app = app_with_size(temp_store("outoforder.json"), 100, 30);
        app.handle_agent_event(AgentEvent::Completed);
        app.handle_agent_event(AgentEvent::Working);
        app.handle_agent_event(AgentEvent::NeedsInput);
        app.handle_agent_event(AgentEvent::Stopped);
        app.handle_agent_event(AgentEvent::Started);
        assert_eq!(app.state, AppState::Menu);

        let mut app = playing_app();
        app.handle_agent_event(AgentEvent::Stopped);
        app.handle_agent_event(AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        app.handle_agent_event(AgentEvent::Started);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        app.handle_agent_event(AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn agent_status_tracks_the_lifecycle() {
        let mut app = playing_app();
        app.handle_agent_event(AgentEvent::Started);
        assert_eq!(app.agent().status, AgentStatus::Idle);
        app.handle_agent_event(AgentEvent::Working);
        assert_eq!(app.agent().status, AgentStatus::Working);
        app.handle_agent_event(AgentEvent::Completed);
        assert_eq!(app.agent().status, AgentStatus::Completed);
        app.handle_agent_event(AgentEvent::Stopped);
        assert_eq!(app.agent().status, AgentStatus::Stopped);
        assert_eq!(app.agent().last_event, Some(AgentEvent::Stopped));
    }

    #[test]
    fn setting_agent_kind_makes_the_indicator_idle() {
        let mut app = app_with_size(temp_store("kind.json"), 100, 30);
        app.set_agent_kind(AgentKind::ClaudeCode);
        assert_eq!(app.agent().agent, AgentKind::ClaudeCode);
        assert_eq!(app.agent().status, AgentStatus::Idle);
    }

    #[test]
    fn auto_resume_is_on_by_default() {
        let app = app_with_size(temp_store("autoresume.json"), 100, 30);
        assert!(app.agent_auto_resume);
    }

    #[test]
    fn disabling_auto_resume_keeps_agent_pauses_manual() {
        let mut app = playing_app();
        app.set_agent_auto_resume(false);
        app.handle_agent_event(AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        app.handle_agent_event(AgentEvent::Working);
        assert_eq!(
            app.state,
            AppState::PausedAgent(PauseReason::NeedsInput),
            "auto-resume must be off"
        );
        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::Playing);
    }
}
