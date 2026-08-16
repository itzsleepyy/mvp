use std::collections::HashMap;
use std::time::Duration;

use crate::agent::{AgentDisplay, AgentEvent, AgentKind, AgentState, AgentStatus};
use crate::config::HighScoreStore;
use crate::event::AppInput;
use crate::game::{ActiveGame, GameInput, GameKind};
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
/// `GameOver` are all sub-states of a live run; `Menu` is the idle screen
/// and `GameMenu` the game selection between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Menu,
    GameMenu,
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
    game: Option<ActiveGame>,
    /// Index into [`GameKind::ALL`]; remembered between runs.
    game_selection: usize,
    store: HighScoreStore,
    terminal_size: Option<(u16, u16)>,
    new_record: bool,
    should_quit: bool,
    /// One entry per agent kind; the aggregate view drives the UI and the
    /// pause/resume transitions.
    agents: HashMap<AgentKind, AgentState>,
    /// Monotonic event counter; the agent with the highest `activity` value
    /// most recently emitted an event.
    activity_seq: u64,
    /// The agents whose status caused the current [`AppState::PausedAgent`]
    /// pause. Kept because the pause reason can outlive the agent status
    /// (a completed agent that starts working again does not clear the
    /// pause), and the overlay must still name the right agents.
    pause_involved: Vec<AgentKind>,
    /// Which agent to display (`--agent codex`, `--agent auto`).
    display_preference: Option<AgentDisplay>,
    /// Whether an agent `Working` event may resume a run that the agent
    /// paused. Turned off in the application layer, never in the adapter.
    /// A run paused because an agent *completed* is only auto-resumed when
    /// a different agent (or a fresh session) starts working: the developer
    /// decides when to continue a finished run of the completing agent.
    agent_auto_resume: bool,
}

impl App {
    pub fn new(store: HighScoreStore) -> Self {
        Self {
            state: AppState::Menu,
            game: None,
            game_selection: 0,
            store,
            terminal_size: None,
            new_record: false,
            should_quit: false,
            agents: HashMap::new(),
            activity_seq: 0,
            pause_involved: Vec::new(),
            display_preference: None,
            agent_auto_resume: true,
        }
    }

    // ---- input -----------------------------------------------------------

    pub fn handle_input(&mut self, input: AppInput) {
        match input {
            AppInput::Quit => self.quit(),
            AppInput::Confirm => match self.state {
                AppState::Menu => self.state = AppState::GameMenu,
                AppState::GameMenu => self.start_game(),
                AppState::GameOver => self.restart_same_game(),
                AppState::PausedAgent(_) => self.resume(),
                AppState::Playing => self.game_input(GameInput::Confirm),
                AppState::PausedManual => {}
            },
            AppInput::Jump => self.playing_game_input(GameInput::Jump),
            AppInput::Hit => self.playing_game_input(GameInput::Hit),
            AppInput::Stand => self.playing_game_input(GameInput::Stand),
            AppInput::Up => match self.state {
                AppState::GameMenu => self.move_selection(-1),
                AppState::Playing => self.game_input(GameInput::Jump),
                _ => {}
            },
            AppInput::Down => {
                if self.state == AppState::GameMenu {
                    self.move_selection(1);
                }
            }
            AppInput::TogglePause => self.toggle_pause(),
            AppInput::Restart => {
                if self.state == AppState::GameOver {
                    self.restart_same_game();
                }
            }
            AppInput::Back => match self.state {
                AppState::Menu => {}
                AppState::GameMenu => self.state = AppState::Menu,
                AppState::Playing
                | AppState::PausedManual
                | AppState::PausedAgent(_)
                | AppState::GameOver => self.back_to_menu(),
            },
            AppInput::Resize(cols, rows) => self.set_terminal_size(cols, rows),
        }
    }

    // ---- transitions (programmatic, reusable) ----------------------------

    /// Starts the currently selected game in the game menu.
    pub fn start_game(&mut self) {
        self.start_game_of(self.selected_kind());
    }

    /// Starts a specific game with a fresh seed.
    pub fn start_game_of(&mut self, kind: GameKind) {
        let (cols, rows) = self.playfield_dims();
        self.game = Some(ActiveGame::new(kind, rand::random::<u64>(), cols, rows));
        self.new_record = false;
        self.state = AppState::Playing;
    }

    /// Restarts the game of the finished run (play again keeps the game).
    fn restart_same_game(&mut self) {
        let kind = self
            .game
            .as_ref()
            .map(ActiveGame::kind)
            .unwrap_or_else(|| self.selected_kind());
        self.start_game_of(kind);
    }

    fn move_selection(&mut self, delta: i64) {
        let count = GameKind::ALL.len() as i64;
        let current = self.game_selection as i64;
        self.game_selection = ((current + delta).rem_euclid(count)) as usize;
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
            AppState::Menu | AppState::GameMenu | AppState::Playing | AppState::GameOver => {}
        }
    }

    pub fn toggle_pause(&mut self) {
        match self.state {
            AppState::Playing => self.pause(),
            AppState::PausedManual => self.resume(),
            // Pressing P while the agent paused the game hands control
            // back to the developer as a manual pause.
            AppState::PausedAgent(_) => self.state = AppState::PausedManual,
            AppState::Menu | AppState::GameMenu | AppState::GameOver => {}
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

    fn playing_game_input(&mut self, input: GameInput) {
        if self.state == AppState::Playing {
            self.game_input(input);
        }
    }

    fn game_input(&mut self, input: GameInput) {
        if let Some(game) = &mut self.game {
            game.handle_input(input);
        }
    }

    // ---- agent events -----------------------------------------------------

    /// Applies one lifecycle event from one agent. Runs on the main loop
    /// only, so application state stays single-threaded. Defensive by
    /// design: duplicates are idempotent and unexpected orderings never
    /// panic — they at most update the agent status.
    pub fn handle_agent_event(&mut self, kind: AgentKind, event: AgentEvent) {
        let before = self.state;
        self.activity_seq += 1;
        let prev_status = self.agents.entry(kind).or_default().status;
        let state = self.agents.get_mut(&kind).expect("entry exists");
        state.apply(event);
        state.activity = self.activity_seq;

        match event {
            AgentEvent::Started => {}
            AgentEvent::Working => {
                // A Completed pause is sticky for the agent that completed:
                // only a *different* agent working (or the completing agent
                // starting a fresh cycle) auto-resumes the run.
                let sticky = self.state == AppState::PausedAgent(PauseReason::Completed)
                    && prev_status == AgentStatus::Completed;
                if !sticky {
                    self.recompute_attention();
                }
            }
            AgentEvent::NeedsInput | AgentEvent::Completed | AgentEvent::Stopped => {
                self.recompute_attention();
            }
        }
        if before != self.state {
            crate::debug_log!(
                "agent event {} {event}: {before:?} → {:?}",
                kind.id(),
                self.state
            );
        } else {
            crate::debug_log!(
                "agent event {} {event}: state unchanged ({:?})",
                kind.id(),
                self.state
            );
        }
    }

    /// Derives the pause/play state from the per-agent statuses:
    ///
    /// ```text
    /// if ANY active agent NeedsInput → pause (needs input)
    /// else if ANY active agent Working → play (or resume)
    /// else if ANY agent Completed    → pause (completed)
    /// else if ANY agent Stopped      → pause (session ended)
    /// else                           → unchanged
    /// ```
    ///
    /// Manual pauses, the menu and the game-over screen are never touched.
    fn recompute_attention(&mut self) {
        let needs = self.any_status(AgentStatus::NeedsInput);
        let working = self.any_status(AgentStatus::Working);
        let completed = self.any_status(AgentStatus::Completed);
        let stopped = self.any_status(AgentStatus::Stopped);
        match self.state {
            AppState::Playing | AppState::PausedAgent(_) => {
                if needs {
                    self.state = AppState::PausedAgent(PauseReason::NeedsInput);
                    self.pause_involved = self.agents_with_status(AgentStatus::NeedsInput);
                } else if working {
                    if self.agent_auto_resume {
                        self.state = AppState::Playing;
                    }
                } else if completed {
                    self.state = AppState::PausedAgent(PauseReason::Completed);
                    self.pause_involved = self.agents_with_status(AgentStatus::Completed);
                } else if stopped {
                    self.state = AppState::PausedAgent(PauseReason::Stopped);
                    self.pause_involved = self.agents_with_status(AgentStatus::Stopped);
                }
            }
            AppState::Menu | AppState::GameMenu | AppState::PausedManual | AppState::GameOver => {}
        }
    }

    fn any_status(&self, status: AgentStatus) -> bool {
        self.agents.values().any(|s| s.status == status)
    }

    /// The connected agents' current states, or disconnected.
    pub fn agents(&self) -> &HashMap<AgentKind, AgentState> {
        &self.agents
    }

    /// The aggregate status over all connected agents.
    pub fn agent_aggregate(&self) -> AgentStatus {
        let mut aggregate = AgentStatus::Disconnected;
        for state in self.agents.values() {
            if state.status.attention_weight() > aggregate.attention_weight() {
                aggregate = state.status;
            }
        }
        aggregate
    }

    /// How many agents have reported in (are not disconnected).
    #[cfg(test)]
    pub fn connected_agent_count(&self) -> usize {
        self.agents
            .values()
            .filter(|s| s.status != AgentStatus::Disconnected)
            .count()
    }

    /// The agent whose status matches `status`, ordered by kind. Used by
    /// the pause overlays to say *who* needs attention.
    pub fn agents_with_status(&self, status: AgentStatus) -> Vec<AgentKind> {
        let mut kinds: Vec<AgentKind> = self
            .agents
            .iter()
            .filter(|(_, s)| s.status == status)
            .map(|(k, _)| *k)
            .collect();
        kinds.sort_unstable();
        kinds
    }

    /// The agents that caused the current agent pause. Unlike the live
    /// statuses, this survives a completing agent starting to work again
    /// (the pause itself is sticky, and the overlay keeps naming who
    /// finished).
    pub fn pause_involved(&self) -> &[AgentKind] {
        &self.pause_involved
    }

    /// Which agent the UI should display. `Specific(k)` always yields `k`;
    /// otherwise the most recently active agent (nothing until the first
    /// event).
    #[cfg(test)]
    pub fn display_focus(&self) -> Option<AgentKind> {
        match self.display_preference {
            Some(AgentDisplay::Specific(kind)) => Some(kind),
            None | Some(AgentDisplay::Auto) => self.most_recent_active(),
        }
    }

    /// The display preference set via `--agent`.
    pub fn display_preference(&self) -> Option<AgentDisplay> {
        self.display_preference
    }

    /// The most recently active non-disconnected agent, if any.
    #[cfg(test)]
    pub fn most_recent_active(&self) -> Option<AgentKind> {
        self.agents
            .iter()
            .filter(|(_, s)| s.status != AgentStatus::Disconnected)
            .max_by_key(|(_, s)| s.activity)
            .map(|(k, _)| *k)
    }

    /// Announces which agent to display before any event has arrived
    /// (`--agent claude`). The indicator still shows the live status.
    #[cfg(test)]
    pub fn set_agent_kind(&mut self, kind: AgentKind) {
        self.set_agent_display(Some(AgentDisplay::Specific(kind)));
    }

    /// Controls which agent the UI focuses on (`--agent auto` included).
    pub fn set_agent_display(&mut self, display: Option<AgentDisplay>) {
        self.display_preference = display;
        if let Some(AgentDisplay::Specific(kind)) = display {
            let entry = self.agents.entry(kind).or_default();
            if entry.status == AgentStatus::Disconnected {
                entry.status = AgentStatus::Idle;
            }
        }
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
        let kind = self
            .game
            .as_ref()
            .map(ActiveGame::kind)
            .unwrap_or(GameKind::StackJump);
        self.new_record = self.store.record(kind, score);
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

    pub fn game(&self) -> Option<&ActiveGame> {
        self.game.as_ref()
    }

    /// The best score across every game.
    pub fn best_score(&self) -> u64 {
        self.store.high_score()
    }

    /// The best score for one game.
    pub fn best_score_for(&self, kind: GameKind) -> u64 {
        self.store.best_score(kind)
    }

    /// The game currently highlighted in the game menu.
    pub fn selected_kind(&self) -> GameKind {
        GameKind::ALL[self.game_selection]
    }

    /// True while the finished run just set a new high score (shown on the
    /// game-over panel until the next run starts).
    pub fn is_new_record(&self) -> bool {
        self.new_record
    }

    #[cfg(test)]
    pub(crate) fn spawn_test_obstacle(&mut self) {
        use crate::game::obstacle::{Obstacle, ObstacleKind};
        if let Some(ActiveGame::StackJump(game)) = &mut self.game {
            game.world
                .obstacles
                .push(Obstacle::new(0.5, ObstacleKind::Small));
        }
    }

    #[cfg(test)]
    pub(crate) fn force_test_chips(&mut self, chips: u64) {
        if let Some(ActiveGame::TwentyOne(game)) = &mut self.game {
            game.force_chips(chips);
        }
    }

    /// Test hook: deals a deterministic, natural-free hand to a running
    /// Twenty One game, so render tests never depend on shuffle order.
    #[cfg(test)]
    pub(crate) fn setup_test_twenty_one_hands(&mut self) {
        use crate::game::twenty_one::{Card, Rank, Suit};
        if let Some(ActiveGame::TwentyOne(game)) = &mut self.game {
            game.debug_set_hands(
                vec![
                    Card {
                        rank: Rank::Two,
                        suit: Suit::Clubs,
                    },
                    Card {
                        rank: Rank::Three,
                        suit: Suit::Hearts,
                    },
                ],
                vec![
                    Card {
                        rank: Rank::Ten,
                        suit: Suit::Spades,
                    },
                    Card {
                        rank: Rank::Six,
                        suit: Suit::Diamonds,
                    },
                ],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::obstacle::{Obstacle, ObstacleKind};

    const C: AgentKind = AgentKind::ClaudeCode;
    const X: AgentKind = AgentKind::Codex;
    const G: AgentKind = AgentKind::GeminiCli;
    const O: AgentKind = AgentKind::OpenCode;

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
        app.game
            .as_mut()
            .unwrap()
            .as_stack_jump_mut()
            .unwrap()
            .world
            .obstacles
            .push(obstacle);
    }

    #[test]
    fn starts_in_menu() {
        let app = app_with_size(temp_store("menu.json"), 100, 30);
        assert_eq!(app.state, AppState::Menu);
        assert!(app.game().is_none());
        assert!(!app.should_quit());
        assert_eq!(app.agent_aggregate(), AgentStatus::Disconnected);
        assert_eq!(app.connected_agent_count(), 0);
    }

    #[test]
    fn confirm_opens_the_game_menu_and_starts_the_selected_game() {
        let mut app = app_with_size(temp_store("start.json"), 100, 30);
        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::GameMenu);
        assert!(app.game().is_none());
        assert_eq!(app.selected_kind(), GameKind::StackJump);

        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::Playing);
        assert!(app.game().is_some());
        assert_eq!(app.game().unwrap().kind(), GameKind::StackJump);
    }

    #[test]
    fn game_menu_navigation_wraps_and_selects_a_game() {
        let mut app = app_with_size(temp_store("select.json"), 100, 30);
        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::GameMenu);

        app.handle_input(AppInput::Down);
        assert_eq!(app.selected_kind(), GameKind::TwentyOne);
        app.handle_input(AppInput::Down);
        assert_eq!(app.selected_kind(), GameKind::StackJump, "selection wraps");
        app.handle_input(AppInput::Up);
        assert_eq!(app.selected_kind(), GameKind::TwentyOne, "up also wraps");

        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::Playing);
        assert_eq!(app.game().unwrap().kind(), GameKind::TwentyOne);
    }

    #[test]
    fn back_from_game_menu_returns_to_the_main_menu() {
        let mut app = app_with_size(temp_store("gamemenuback.json"), 100, 30);
        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::GameMenu);
        app.handle_input(AppInput::Back);
        assert_eq!(app.state, AppState::Menu);
    }

    #[test]
    fn game_menu_ignores_agent_events_without_crashing() {
        let mut app = app_with_size(temp_store("gamemenuagent.json"), 100, 30);
        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::GameMenu);
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        app.handle_agent_event(X, AgentEvent::Working);
        assert_eq!(app.state, AppState::GameMenu);
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
        assert!(
            !app.game()
                .unwrap()
                .as_stack_jump()
                .unwrap()
                .render_state()
                .player
                .grounded
        );

        app.pause();
        let player = *app
            .game()
            .unwrap()
            .as_stack_jump()
            .unwrap()
            .render_state()
            .player;
        app.handle_input(AppInput::Jump);
        assert_eq!(
            *app.game()
                .unwrap()
                .as_stack_jump()
                .unwrap()
                .render_state()
                .player,
            player
        );
    }

    #[test]
    fn up_jumps_while_playing_and_navigates_the_game_menu() {
        let mut app = playing_app();
        app.handle_input(AppInput::Up);
        assert!(
            !app.game()
                .unwrap()
                .as_stack_jump()
                .unwrap()
                .render_state()
                .player
                .grounded
        );

        let mut menu = app_with_size(temp_store("up.json"), 100, 30);
        menu.handle_input(AppInput::Confirm);
        assert_eq!(menu.state, AppState::GameMenu);
        menu.handle_input(AppInput::Up);
        assert_eq!(menu.selected_kind(), GameKind::TwentyOne);
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
        agent_paused.handle_agent_event(C, AgentEvent::NeedsInput);
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
            app.game().unwrap().as_stack_jump().unwrap().world.spawn_x,
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
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        assert_eq!(app.agent_aggregate(), AgentStatus::NeedsInput);
    }

    #[test]
    fn working_resumes_an_agent_paused_game() {
        let mut app = playing_app();
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn manual_pause_is_never_overridden_by_agent_events() {
        let mut app = playing_app();
        app.pause();
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.state, AppState::PausedManual);
        app.handle_agent_event(X, AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedManual);
        app.handle_agent_event(G, AgentEvent::Completed);
        assert_eq!(app.state, AppState::PausedManual);
    }

    #[test]
    fn game_over_is_not_disturbed_by_agent_events() {
        let mut app = playing_app();
        collide(&mut app);
        app.tick(Duration::from_millis(16));
        assert_eq!(app.state, AppState::GameOver);
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.state, AppState::GameOver);
        app.handle_agent_event(X, AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::GameOver);
    }

    #[test]
    fn menu_ignores_agent_events_without_crashing() {
        let mut app = app_with_size(temp_store("menuagent.json"), 100, 30);
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::Menu);
        app.handle_agent_event(X, AgentEvent::Working);
        assert_eq!(app.state, AppState::Menu);
        app.handle_agent_event(G, AgentEvent::Completed);
        assert_eq!(app.state, AppState::Menu);
        app.handle_agent_event(O, AgentEvent::Stopped);
        assert_eq!(app.state, AppState::Menu);
    }

    #[test]
    fn duplicate_events_are_idempotent() {
        let mut app = playing_app();
        app.handle_agent_event(C, AgentEvent::Working);
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);

        app.handle_agent_event(C, AgentEvent::NeedsInput);
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));

        app.handle_agent_event(C, AgentEvent::Working);
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn completion_pauses_the_run_and_requires_manual_resume() {
        let mut app = playing_app();
        app.tick(Duration::from_secs(1));
        app.handle_agent_event(C, AgentEvent::Completed);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Completed));

        // Claude working again does not silently continue a finished run.
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Completed));

        // The developer decides when to continue it.
        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn session_end_pauses_and_working_resumes() {
        let mut app = playing_app();
        app.handle_agent_event(C, AgentEvent::Stopped);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Stopped));
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn pause_input_takes_control_from_agent_pause() {
        let mut app = playing_app();
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        app.handle_input(AppInput::TogglePause);
        assert_eq!(app.state, AppState::PausedManual);
        app.handle_input(AppInput::TogglePause);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn later_agent_status_replaces_the_pause_reason() {
        let mut app = playing_app();
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        app.handle_agent_event(C, AgentEvent::Completed);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Completed));
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
    }

    #[test]
    fn agent_pause_preserves_the_whole_run() {
        use crate::game::obstacle::Obstacle;
        use crate::game::player::Player;

        fn snapshot(app: &App) -> (u64, f64, Player, Vec<Obstacle>, f64) {
            let game = app.game().unwrap().as_stack_jump().unwrap();
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

        app.handle_agent_event(C, AgentEvent::NeedsInput);
        app.tick(Duration::from_secs(2));
        assert_eq!(snapshot(&app), before, "agent pause must freeze the run");

        app.handle_agent_event(C, AgentEvent::Working);
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
        app.handle_agent_event(C, AgentEvent::Completed);
        app.handle_agent_event(C, AgentEvent::Working);
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        app.handle_agent_event(C, AgentEvent::Stopped);
        app.handle_agent_event(C, AgentEvent::Started);
        assert_eq!(app.state, AppState::Menu);

        let mut app = playing_app();
        app.handle_agent_event(C, AgentEvent::Stopped);
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        app.handle_agent_event(C, AgentEvent::Started);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn agent_status_tracks_the_lifecycle() {
        let mut app = playing_app();
        app.handle_agent_event(C, AgentEvent::Started);
        assert_eq!(app.agent_aggregate(), AgentStatus::Idle);
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.agent_aggregate(), AgentStatus::Working);
        app.handle_agent_event(C, AgentEvent::Completed);
        assert_eq!(app.agent_aggregate(), AgentStatus::Completed);
        app.handle_agent_event(C, AgentEvent::Stopped);
        assert_eq!(app.agent_aggregate(), AgentStatus::Stopped);
        assert_eq!(app.agents()[&C].last_event, Some(AgentEvent::Stopped));
    }

    #[test]
    fn setting_agent_kind_makes_the_indicator_idle() {
        let mut app = app_with_size(temp_store("kind.json"), 100, 30);
        app.set_agent_kind(AgentKind::ClaudeCode);
        assert_eq!(app.display_focus(), Some(AgentKind::ClaudeCode));
        assert_eq!(app.agent_aggregate(), AgentStatus::Idle);
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
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(
            app.state,
            AppState::PausedAgent(PauseReason::NeedsInput),
            "auto-resume must be off"
        );
        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::Playing);
    }

    // ---- multi-agent attention ---------------------------------------------

    #[test]
    fn two_agents_working_keep_the_game_playable() {
        let mut app = playing_app();
        app.handle_agent_event(C, AgentEvent::Working);
        app.handle_agent_event(X, AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn one_completed_agent_does_not_pause_while_another_works() {
        let mut app = playing_app();
        app.handle_agent_event(X, AgentEvent::Working);
        app.handle_agent_event(C, AgentEvent::Completed);
        assert_eq!(app.state, AppState::Playing);
        assert_eq!(app.agent_aggregate(), AgentStatus::Working);
    }

    #[test]
    fn needs_input_dominates_working_agents() {
        let mut app = playing_app();
        app.handle_agent_event(X, AgentEvent::Working);
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        assert_eq!(app.agents_with_status(AgentStatus::NeedsInput), vec![C]);
    }

    #[test]
    fn working_from_the_needing_agent_resumes_after_input() {
        let mut app = playing_app();
        app.handle_agent_event(X, AgentEvent::Working);
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn two_agents_needing_input_report_both() {
        let mut app = playing_app();
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        app.handle_agent_event(X, AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        let needing = app.agents_with_status(AgentStatus::NeedsInput);
        assert_eq!(needing, vec![C, X]);
    }

    #[test]
    fn all_agents_completed_causes_a_completed_pause() {
        let mut app = playing_app();
        app.handle_agent_event(C, AgentEvent::Completed);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Completed));
        app.handle_agent_event(X, AgentEvent::Completed);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Completed));
        assert_eq!(app.agents_with_status(AgentStatus::Completed), vec![C, X]);
    }

    #[test]
    fn completed_pause_is_sticky_for_the_completing_agent() {
        let mut app = playing_app();
        app.handle_agent_event(C, AgentEvent::Completed);
        app.handle_agent_event(X, AgentEvent::Completed);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Completed));

        // The completing agents working again never silently resumes the
        // run; the developer decides.
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Completed));
        app.handle_agent_event(X, AgentEvent::Working);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Completed));
        // The overlay keeps attributing the pause to the completing agents
        // even though both are working again.
        assert_eq!(app.pause_involved(), &[C, X]);

        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn completed_pause_resumes_when_a_different_agent_works() {
        let mut app = playing_app();
        app.handle_agent_event(C, AgentEvent::Completed);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Completed));

        // Codex (never completed this run) starts working: the run resumes.
        app.handle_agent_event(X, AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn completed_pause_resumes_after_a_fresh_session_start() {
        let mut app = playing_app();
        app.handle_agent_event(C, AgentEvent::Completed);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Completed));

        // A new session cycle (Started → Working) is a fresh start, not the
        // completing agent continuing its finished run.
        app.handle_agent_event(C, AgentEvent::Started);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Completed));
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn one_stopped_agent_does_not_pause_while_another_works() {
        let mut app = playing_app();
        app.handle_agent_event(X, AgentEvent::Working);
        app.handle_agent_event(C, AgentEvent::Stopped);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn all_agents_stopped_pauses_the_run() {
        let mut app = playing_app();
        app.handle_agent_event(X, AgentEvent::Stopped);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Stopped));
    }

    #[test]
    fn phase_3_key_scenario_all_agents_working() {
        // §55: the architectural test of the whole phase.
        let mut app = playing_app();
        app.handle_agent_event(C, AgentEvent::Working);
        app.handle_agent_event(X, AgentEvent::Working);
        app.handle_agent_event(G, AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);

        // Codex needs attention → pause, Codex is flagged.
        app.handle_agent_event(X, AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));
        assert_eq!(app.agents_with_status(AgentStatus::NeedsInput), vec![X]);

        // Codex is unblocked → resume.
        app.handle_agent_event(X, AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);

        // Claude and Codex finish; Gemini is still working → playable.
        app.handle_agent_event(C, AgentEvent::Completed);
        assert_eq!(app.state, AppState::Playing);
        app.handle_agent_event(X, AgentEvent::Completed);
        assert_eq!(app.state, AppState::Playing);

        // Gemini finishes too → completion pause.
        app.handle_agent_event(G, AgentEvent::Completed);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::Completed));
    }

    #[test]
    fn manual_pause_survives_any_number_of_working_events() {
        let mut app = playing_app();
        app.pause();
        app.handle_agent_event(C, AgentEvent::Working);
        app.handle_agent_event(X, AgentEvent::Working);
        app.handle_agent_event(G, AgentEvent::Working);
        app.handle_agent_event(O, AgentEvent::Working);
        assert_eq!(app.state, AppState::PausedManual);
    }

    #[test]
    fn game_over_survives_multi_agent_lifecycle() {
        let mut app = playing_app();
        collide(&mut app);
        app.tick(Duration::from_millis(16));
        assert_eq!(app.state, AppState::GameOver);
        app.handle_agent_event(C, AgentEvent::Working);
        app.handle_agent_event(X, AgentEvent::NeedsInput);
        app.handle_agent_event(G, AgentEvent::Completed);
        app.handle_agent_event(O, AgentEvent::Stopped);
        assert_eq!(app.state, AppState::GameOver);
    }

    #[test]
    fn most_recently_active_agent_is_tracked() {
        let mut app = playing_app();
        assert_eq!(app.most_recent_active(), None);

        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.most_recent_active(), Some(C));
        app.handle_agent_event(X, AgentEvent::Working);
        assert_eq!(app.most_recent_active(), Some(X));
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.most_recent_active(), Some(C));
    }

    #[test]
    fn display_focus_defaults_to_most_recent_activity() {
        let mut app = playing_app();
        assert_eq!(app.display_focus(), None);
        app.handle_agent_event(X, AgentEvent::Working);
        assert_eq!(app.display_focus(), Some(X));
    }

    #[test]
    fn display_focus_auto_follows_the_most_recent_agent() {
        let mut app = playing_app();
        app.set_agent_display(Some(AgentDisplay::Auto));
        assert_eq!(app.display_focus(), None);
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.display_focus(), Some(C));
        app.handle_agent_event(G, AgentEvent::Working);
        assert_eq!(app.display_focus(), Some(G));
    }

    #[test]
    fn display_focus_specific_is_sticky() {
        let mut app = playing_app();
        app.set_agent_kind(X);
        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.display_focus(), Some(X));
    }

    #[test]
    fn per_agent_states_are_independent() {
        let mut app = playing_app();
        app.handle_agent_event(C, AgentEvent::Working);
        app.handle_agent_event(X, AgentEvent::NeedsInput);
        assert_eq!(app.agents()[&C].status, AgentStatus::Working);
        assert_eq!(app.agents()[&X].status, AgentStatus::NeedsInput);
        assert_eq!(app.agents()[&C].last_event, Some(AgentEvent::Working));
        assert_eq!(app.agents()[&X].last_event, Some(AgentEvent::NeedsInput));
        assert!(app.agents().get(&G).is_none());
    }

    // ---- Twenty One ---------------------------------------------------------

    use crate::game::twenty_one::{Card, Phase, Rank, Suit};

    fn card(rank: Rank, suit: Suit) -> Card {
        Card { rank, suit }
    }

    fn twenty_one_app(name: &str) -> App {
        let mut app = app_with_size(temp_store(name), 100, 30);
        app.handle_input(AppInput::Confirm); // Menu → GameMenu
        app.handle_input(AppInput::Down); // select Twenty One
        app.handle_input(AppInput::Confirm); // start
        app
    }

    #[test]
    fn twenty_one_is_reachable_from_the_game_menu() {
        let app = twenty_one_app("reach.json");
        assert_eq!(app.state, AppState::Playing);
        assert_eq!(app.game().unwrap().kind(), GameKind::TwentyOne);
    }

    #[test]
    fn hit_and_stand_reach_the_twenty_one_game() {
        let mut app = twenty_one_app("input.json");
        app.game
            .as_mut()
            .unwrap()
            .as_twenty_one_mut()
            .unwrap()
            .debug_set_hands(
                vec![
                    card(Rank::Two, Suit::Clubs),
                    card(Rank::Three, Suit::Hearts),
                ],
                vec![
                    card(Rank::Ten, Suit::Spades),
                    card(Rank::Ten, Suit::Diamonds),
                ],
            );
        assert_eq!(
            app.game().unwrap().as_twenty_one().unwrap().phase(),
            Phase::PlayerTurn
        );

        app.handle_input(AppInput::Hit);
        let cards = app
            .game()
            .unwrap()
            .as_twenty_one()
            .unwrap()
            .player_hand()
            .len();
        assert_eq!(cards, 3, "H must deal one card");

        app.handle_input(AppInput::Stand);
        let game = app.game().unwrap().as_twenty_one().unwrap();
        assert_eq!(game.phase(), Phase::RoundOver, "S must settle the round");

        // ENTER deals the next round (hands reset to two cards each).
        app.handle_input(AppInput::Confirm);
        let game = app.game().unwrap().as_twenty_one().unwrap();
        assert_eq!(game.player_hand().len(), 2);
        assert_eq!(game.dealer_hand().len(), 2);
    }

    #[test]
    fn twenty_one_ignores_jump_and_stacks_do_not_hit() {
        let mut app = twenty_one_app("mixed.json");
        app.game
            .as_mut()
            .unwrap()
            .as_twenty_one_mut()
            .unwrap()
            .debug_set_hands(
                vec![
                    card(Rank::Four, Suit::Clubs),
                    card(Rank::Five, Suit::Hearts),
                ],
                vec![
                    card(Rank::Ten, Suit::Spades),
                    card(Rank::Six, Suit::Diamonds),
                ],
            );
        app.handle_input(AppInput::Jump);
        app.handle_input(AppInput::Up);
        let game = app.game().unwrap().as_twenty_one().unwrap();
        assert_eq!(game.phase(), Phase::PlayerTurn, "jump must not settle 21");
        assert_eq!(game.player_hand().len(), 2, "jump must not deal cards");

        // A stack jump run ignores Twenty One inputs entirely.
        let mut jump = playing_app();
        jump.handle_input(AppInput::Hit);
        jump.handle_input(AppInput::Stand);
        jump.handle_input(AppInput::Confirm);
        assert_eq!(jump.state, AppState::Playing);
    }

    #[test]
    fn agent_pause_freezes_twenty_one() {
        let mut app = twenty_one_app("freeze.json");
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        assert_eq!(app.state, AppState::PausedAgent(PauseReason::NeedsInput));

        let snapshot = {
            let game = app.game().unwrap().as_twenty_one().unwrap();
            (
                game.chips(),
                game.phase(),
                game.player_hand().to_vec(),
                game.dealer_hand().to_vec(),
            )
        };
        app.tick(Duration::from_secs(2));
        app.handle_input(AppInput::Hit);
        app.handle_input(AppInput::Stand);
        let game = app.game().unwrap().as_twenty_one().unwrap();
        assert_eq!(
            (
                game.chips(),
                game.phase(),
                game.player_hand().to_vec(),
                game.dealer_hand().to_vec()
            ),
            snapshot,
            "agent pause must freeze the table and ignore game input"
        );

        app.handle_agent_event(C, AgentEvent::Working);
        assert_eq!(app.state, AppState::Playing);
        assert_eq!(
            app.game().unwrap().as_twenty_one().unwrap().phase(),
            Phase::PlayerTurn
        );
    }

    #[test]
    fn twenty_one_game_over_records_the_best_and_restarts_the_same_game() {
        let mut app = twenty_one_app("over.json");
        app.game
            .as_mut()
            .unwrap()
            .as_twenty_one_mut()
            .unwrap()
            .force_chips(crate::game::twenty_one::BET - 1);
        app.handle_input(AppInput::Confirm); // cannot afford the next bet
        assert!(app.game().unwrap().as_twenty_one().unwrap().is_game_over());
        app.tick(Duration::from_millis(16));
        assert_eq!(app.state, AppState::GameOver);
        assert_eq!(
            app.best_score_for(GameKind::TwentyOne),
            crate::game::twenty_one::STARTING_CHIPS
        );

        app.handle_input(AppInput::Restart);
        assert_eq!(app.state, AppState::Playing);
        let game = app.game().unwrap().as_twenty_one().unwrap();
        assert_eq!(game.chips(), crate::game::twenty_one::STARTING_CHIPS);
        assert!(!game.is_game_over());
    }
}
