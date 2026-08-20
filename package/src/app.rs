use std::collections::HashMap;
use std::time::Duration;

use crate::agent::{AgentDisplay, AgentEvent, AgentKind, AgentState, AgentStatus};
use crate::config::{DailyMvp, HighScoreStore};
use crate::event::AppInput;
use crate::game::{ActiveGame, GameInput, GameKind};
use crate::ui;

/// Minimum usable terminal size, in columns × rows.
pub const MIN_COLS: u16 = 60;
pub const MIN_ROWS: u16 = 20;

/// Longest accepted player name.
const NAME_LIMIT: usize = 24;

/// Characters a player name may contain.
const NAME_LEGAL: &str = " -_.'!@#$&+=()";

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
/// `GameOver` are all sub-states of a live run; `Menu` is the idle screen,
/// `GameMenu` the game selection between them, and `NamePrompt` the
/// first-run (or rename) player-name entry screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Menu,
    GameMenu,
    NamePrompt,
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
    /// The name being typed in the [`AppState::NamePrompt`]; saved to the
    /// store on confirm.
    name_buffer: String,
    /// True while the finished run just became today's MVP.
    mvp_just_set: bool,
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
            name_buffer: String::new(),
            mvp_just_set: false,
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
                AppState::NamePrompt => self.submit_name(),
                AppState::GameOver => self.restart_same_game(),
                AppState::PausedAgent(_) => self.resume(),
                AppState::Playing => self.game_input(GameInput::Confirm),
                AppState::PausedManual => {}
            },
            AppInput::Jump => self.playing_game_input(GameInput::Jump),
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
            AppInput::Rename => {
                if self.state == AppState::Menu {
                    self.open_name_prompt();
                }
            }
            AppInput::Text(c) => match self.state {
                AppState::NamePrompt => self.type_name(c),
                AppState::Playing => self.game_input(GameInput::Type(c)),
                _ => {}
            },
            AppInput::Backspace => match self.state {
                AppState::NamePrompt => {
                    self.name_buffer.pop();
                }
                AppState::Playing => self.game_input(GameInput::Backspace),
                _ => {}
            },
            AppInput::Back => match self.state {
                AppState::Menu => {}
                AppState::NamePrompt => self.dismiss_name_prompt(),
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

    /// Starts a specific game. Daily challenges are seeded from the day so
    /// everyone plays the same word and the same bug; everything else gets
    /// a fresh random seed.
    pub fn start_game_of(&mut self, kind: GameKind) {
        let (cols, rows) = self.playfield_dims();
        let seed = if kind.is_daily() {
            crate::config::today_ordinal()
        } else {
            rand::random::<u64>()
        };
        self.game = Some(if kind == GameKind::DailyPr {
            let progress = self.store.daily_pr_progress(seed).cloned();
            ActiveGame::DailyPr(crate::game::daily_pr::DailyPr::restore(seed, progress))
        } else {
            ActiveGame::new(kind, seed, cols, rows)
        });
        self.new_record = false;
        self.mvp_just_set = false;
        self.state = AppState::Playing;
    }

    /// Restarts the game of the finished run (play again keeps the game).
    fn restart_same_game(&mut self) {
        let kind = self
            .game
            .as_ref()
            .map(ActiveGame::kind)
            .unwrap_or_else(|| self.selected_kind());
        if kind == GameKind::DailyPr {
            self.back_to_menu();
        } else {
            self.start_game_of(kind);
        }
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
            AppState::Menu
            | AppState::GameMenu
            | AppState::NamePrompt
            | AppState::Playing
            | AppState::GameOver => {}
        }
    }

    pub fn toggle_pause(&mut self) {
        match self.state {
            AppState::Playing => self.pause(),
            AppState::PausedManual => self.resume(),
            // Pressing P while the agent paused the game hands control
            // back to the developer as a manual pause.
            AppState::PausedAgent(_) => self.state = AppState::PausedManual,
            AppState::Menu | AppState::GameMenu | AppState::NamePrompt | AppState::GameOver => {}
        }
    }

    // ---- name prompt ------------------------------------------------------

    /// Opens the player-name prompt with a fresh buffer.
    pub fn open_name_prompt(&mut self) {
        self.name_buffer.clear();
        self.state = AppState::NamePrompt;
    }

    /// Confirms the typed name: an empty buffer keeps prompting.
    fn submit_name(&mut self) {
        let name = self.name_buffer.trim();
        if name.is_empty() {
            return;
        }
        self.store.set_player_name(name);
        self.name_buffer.clear();
        self.state = AppState::Menu;
    }

    /// Cancels a rename. The first-run prompt cannot be dismissed until a
    /// non-empty name has been saved.
    fn dismiss_name_prompt(&mut self) {
        if !self.store.has_player_name() {
            return;
        }
        self.name_buffer.clear();
        self.state = AppState::Menu;
    }

    /// Appends one character to the name being typed, when it is
    /// name-legal and the buffer is under the limit.
    fn type_name(&mut self, c: char) {
        if self.name_buffer.chars().count() >= NAME_LIMIT {
            return;
        }
        if !c.is_alphanumeric() && !NAME_LEGAL.contains(c) {
            return;
        }
        self.name_buffer.push(c);
    }

    pub fn back_to_menu(&mut self) {
        self.persist_daily_pr();
        self.state = AppState::Menu;
        self.game = None;
    }

    pub fn quit(&mut self) {
        self.persist_daily_pr();
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
        if input == GameInput::Confirm {
            self.persist_daily_pr();
        }
    }

    fn persist_daily_pr(&mut self) {
        let progress = match self.game.as_ref() {
            Some(ActiveGame::DailyPr(game)) => Some(game.progress()),
            _ => None,
        };
        if let Some(progress) = progress {
            self.store.set_daily_pr_progress(progress);
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
            AppState::Menu
            | AppState::GameMenu
            | AppState::NamePrompt
            | AppState::PausedManual
            | AppState::GameOver => {}
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
            .unwrap_or(GameKind::StackOverflow);
        let note = self.game.as_ref().and_then(ActiveGame::score_note);
        let name = self.player_name().to_string();
        self.new_record = self.store.record(kind, score);
        self.mvp_just_set = self
            .store
            .record_daily_mvp(&name, kind, score, note.as_deref());
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

    /// Whether the event loop should treat keystrokes as text. True for the
    /// name prompt and for games that are driven by typed input (The Daily
    /// PR, The Daily Fix).
    pub fn text_mode(&self) -> bool {
        match self.state {
            AppState::NamePrompt => true,
            AppState::Playing => self.game.as_ref().is_some_and(ActiveGame::text_input),
            _ => false,
        }
    }

    /// Mutable access for tests that need to force a game state.
    #[cfg(test)]
    pub(crate) fn game_mut(&mut self) -> Option<&mut ActiveGame> {
        self.game.as_mut()
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

    /// The name shown in the UI ("Anonymous" until the prompt is answered).
    pub fn player_name(&self) -> &str {
        self.store.player_name()
    }

    /// True once the first-run name prompt has been answered.
    pub fn has_player_name(&self) -> bool {
        self.store.has_player_name()
    }

    /// The name being typed in the name prompt.
    pub fn name_buffer(&self) -> &str {
        &self.name_buffer
    }

    /// Today's MVP of the day, if one exists.
    pub fn daily_mvp(&self) -> Option<&DailyMvp> {
        self.store.daily_mvp()
    }

    /// True while the finished run just became today's MVP (shown on the
    /// game-over panel until the next run starts).
    pub fn mvp_just_set(&self) -> bool {
        self.mvp_just_set
    }

    #[cfg(test)]
    pub(crate) fn debug_force_overflow(&mut self) {
        if let Some(ActiveGame::StackOverflow(game)) = &mut self.game {
            game.debug_force_overflow();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: AgentKind = AgentKind::ClaudeCode;
    const X: AgentKind = AgentKind::Codex;
    const G: AgentKind = AgentKind::GeminiCli;
    const O: AgentKind = AgentKind::OpenCode;

    /// Unique-per-call temp directory: parallel tests never share a store
    /// (and the daily-MVP file inside it).
    fn temp_dir(name: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mvp_app_test_{}_{}_{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn temp_store(name: &str) -> HighScoreStore {
        HighScoreStore::load(temp_dir(name).join("highscore.json"))
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

    /// Drops the stacker block dead-center: a perfect, scoring drop.
    fn perfect_drop(app: &mut App) {
        app.game_mut()
            .as_mut()
            .unwrap()
            .as_stack_overflow_mut()
            .unwrap()
            .debug_set_block(0.0);
        app.handle_input(AppInput::Jump);
    }

    /// Ends the stacker run without scoring (a full miss).
    fn overflow(app: &mut App) {
        app.debug_force_overflow();
        app.tick(Duration::from_millis(16));
    }

    /// A scoring run: one perfect drop, then a full miss.
    fn scored_run(app: &mut App) {
        perfect_drop(app);
        overflow(app);
    }

    /// Selects and starts a game from the game menu.
    fn start_from_menu(app: &mut App, kind: GameKind) {
        app.handle_input(AppInput::Confirm); // Menu → GameMenu
        while app.selected_kind() != kind {
            app.handle_input(AppInput::Down);
        }
        app.handle_input(AppInput::Confirm); // start
    }

    /// Types a word into the active Daily PR game and submits it, then lets
    /// the app tick so a solve reaches the game-over state.
    fn type_word(app: &mut App, word: &str) {
        for c in word.chars() {
            app.handle_input(AppInput::Text(c));
        }
        app.handle_input(AppInput::Confirm);
        app.tick(Duration::from_millis(16));
    }

    /// Types a line into the active Daily Fix game and submits it, then
    /// lets the app tick so a solve reaches the game-over state.
    fn type_fix(app: &mut App, line: &str) {
        for c in line.chars() {
            app.handle_input(AppInput::Text(c));
        }
        app.handle_input(AppInput::Confirm);
        app.tick(Duration::from_millis(16));
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
        assert_eq!(app.selected_kind(), GameKind::StackOverflow);

        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::Playing);
        assert!(app.game().is_some());
        assert_eq!(app.game().unwrap().kind(), GameKind::StackOverflow);
    }

    #[test]
    fn game_menu_navigation_wraps_and_selects_a_game() {
        let mut app = app_with_size(temp_store("select.json"), 100, 30);
        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::GameMenu);

        app.handle_input(AppInput::Down);
        assert_eq!(app.selected_kind(), GameKind::DailyPr);
        app.handle_input(AppInput::Down);
        assert_eq!(app.selected_kind(), GameKind::DailyFix);
        app.handle_input(AppInput::Down);
        assert_eq!(
            app.selected_kind(),
            GameKind::StackOverflow,
            "selection wraps"
        );
        app.handle_input(AppInput::Up);
        assert_eq!(app.selected_kind(), GameKind::DailyFix, "up also wraps");

        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::Playing);
        assert_eq!(app.game().unwrap().kind(), GameKind::DailyFix);
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
    fn the_run_clock_grows_while_playing_and_stops_when_paused() {
        let mut app = playing_app();
        app.tick(Duration::from_secs(1));
        let after_first = app.game().unwrap().elapsed();
        assert!(after_first > 0.0);

        app.pause();
        assert_eq!(app.state, AppState::PausedManual);
        app.tick(Duration::from_secs(1));
        assert_eq!(app.game().unwrap().elapsed(), after_first);

        app.resume();
        assert_eq!(app.state, AppState::Playing);
        app.tick(Duration::from_secs(1));
        assert!(app.game().unwrap().elapsed() > after_first);
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
    fn drop_input_reaches_the_game_only_while_playing() {
        let mut app = playing_app();
        app.game_mut()
            .as_mut()
            .unwrap()
            .as_stack_overflow_mut()
            .unwrap()
            .debug_set_block(3.0);
        app.handle_input(AppInput::Jump);
        assert_eq!(
            app.game().unwrap().as_stack_overflow().unwrap().height(),
            2,
            "SPACE must drop a block while playing"
        );

        app.pause();
        let height = app.game().unwrap().as_stack_overflow().unwrap().height();
        app.handle_input(AppInput::Jump);
        assert_eq!(
            app.game().unwrap().as_stack_overflow().unwrap().height(),
            height,
            "SPACE must not drop while paused"
        );
    }

    #[test]
    fn up_drops_while_playing_and_navigates_the_game_menu() {
        let mut app = playing_app();
        app.game_mut()
            .as_mut()
            .unwrap()
            .as_stack_overflow_mut()
            .unwrap()
            .debug_set_block(3.0);
        app.handle_input(AppInput::Up);
        assert_eq!(
            app.game().unwrap().as_stack_overflow().unwrap().height(),
            2,
            "Up must drop a block while playing"
        );

        let mut menu = app_with_size(temp_store("up.json"), 100, 30);
        menu.handle_input(AppInput::Confirm);
        assert_eq!(menu.state, AppState::GameMenu);
        menu.handle_input(AppInput::Up);
        assert_eq!(menu.selected_kind(), GameKind::DailyFix);
    }

    #[test]
    fn overflow_causes_game_over_and_records_high_score() {
        let mut app = playing_app();
        app.tick(Duration::from_secs(1));
        perfect_drop(&mut app);
        overflow(&mut app);

        assert_eq!(app.state, AppState::GameOver);
        let score = app.game().unwrap().score();
        assert_eq!(app.best_score(), score);
        assert!(app.is_new_record());
    }

    #[test]
    fn restart_starts_a_fresh_run() {
        let mut app = playing_app();
        scored_run(&mut app);
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
        scored_run(&mut app);
        let best = app.best_score();

        app.start_game();
        overflow(&mut app); // immediate miss: score 0
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
        overflow(&mut over);
        over.handle_input(AppInput::Back);
        assert_eq!(over.state, AppState::Menu);
    }

    #[test]
    fn too_small_terminal_freezes_the_game() {
        let mut app = app_with_size(temp_store("small.json"), 100, 30);
        app.start_game();
        app.tick(Duration::from_secs(1));
        let before = app.game().unwrap().elapsed();
        assert!(before > 0.0);

        app.set_terminal_size(42, 14);
        assert!(app.too_small());
        app.tick(Duration::from_secs(1));
        assert_eq!(app.game().unwrap().elapsed(), before);

        app.set_terminal_size(100, 30);
        assert!(!app.too_small());
        app.tick(Duration::from_secs(1));
        assert!(app.game().unwrap().elapsed() > before);
    }

    #[test]
    fn resize_recenters_the_stack() {
        let mut app = playing_app();
        app.set_terminal_size(120, 40);
        let (cols, _) = ui::playfield_dims(120, 40);
        let game = app.game().unwrap().as_stack_overflow().unwrap();
        assert_eq!(
            game.layers()[0].left,
            (i32::from(cols) - crate::game::stack_overflow::START_WIDTH) / 2,
            "the tower must recenter in the wider playfield"
        );
    }

    #[test]
    fn back_on_menu_does_nothing() {
        let mut app = app_with_size(temp_store("menuback.json"), 100, 30);
        app.handle_input(AppInput::Back);
        assert_eq!(app.state, AppState::Menu);
    }

    #[test]
    fn high_score_survives_across_app_instances() {
        let dir = temp_dir("persist");
        let path = dir.join("highscore.json");

        let mut first = app_with_size(HighScoreStore::load(&path), 100, 30);
        first.start_game();
        scored_run(&mut first);
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
        overflow(&mut app);
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
        fn snapshot(app: &App) -> (u64, f64, Vec<crate::game::stack_overflow::Layer>, f64, i32) {
            let game = app.game().unwrap().as_stack_overflow().unwrap();
            (
                game.score(),
                game.elapsed(),
                game.layers().to_vec(),
                game.block_offset(),
                game.direction(),
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
            "agent resume must not reset score, layers or the block"
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
        overflow(&mut app);
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

    // ---- daily challenges ---------------------------------------------------

    fn daily_pr_app(name: &str) -> App {
        let mut app = app_with_size(temp_store(name), 100, 30);
        start_from_menu(&mut app, GameKind::DailyPr);
        app
    }

    fn daily_fix_app(name: &str) -> App {
        let mut app = app_with_size(temp_store(name), 100, 30);
        start_from_menu(&mut app, GameKind::DailyFix);
        app
    }

    #[test]
    fn daily_pr_is_reachable_and_typed_input_reaches_it() {
        let mut app = daily_pr_app("prreach.json");
        assert_eq!(app.state, AppState::Playing);
        assert_eq!(app.game().unwrap().kind(), GameKind::DailyPr);
        assert!(app.text_mode(), "Daily PR is a text-driven game");

        let word = app.game().unwrap().as_daily_pr().unwrap().word();
        type_word(&mut app, &word);
        assert_eq!(app.state, AppState::GameOver, "the word must solve the PR");
        assert!(app.best_score_for(GameKind::DailyPr) > 0);
    }

    #[test]
    fn daily_fix_is_reachable_and_the_fix_line_solves_it() {
        let mut app = daily_fix_app("fixreach.json");
        assert_eq!(app.state, AppState::Playing);
        assert_eq!(app.game().unwrap().kind(), GameKind::DailyFix);
        assert!(app.text_mode(), "Daily Fix is a text-driven game");

        let fix = app.game().unwrap().as_daily_fix().unwrap().bug().fix;
        type_fix(&mut app, fix);
        assert_eq!(app.state, AppState::GameOver, "the fix must solve the run");
        assert!(app.best_score_for(GameKind::DailyFix) > 0);
    }

    #[test]
    fn daily_challenges_seed_from_today_for_everyone() {
        let a = daily_pr_app("seed1.json");
        let b = daily_pr_app("seed2.json");
        assert_eq!(
            a.game().unwrap().as_daily_pr().unwrap().word(),
            b.game().unwrap().as_daily_pr().unwrap().word(),
            "the whole world plays the same word"
        );

        let c = daily_fix_app("seed3.json");
        let d = daily_fix_app("seed4.json");
        assert_eq!(
            c.game().unwrap().as_daily_fix().unwrap().bug(),
            d.game().unwrap().as_daily_fix().unwrap().bug(),
            "the whole world fixes the same bug"
        );
    }

    #[test]
    fn text_and_backspace_are_ignored_by_stack_overflow() {
        let mut app = playing_app();
        assert!(!app.text_mode(), "Stack Overflow is not text-driven");
        let height = app.game().unwrap().as_stack_overflow().unwrap().height();
        app.handle_input(AppInput::Text('x'));
        app.handle_input(AppInput::Backspace);
        assert_eq!(
            app.game().unwrap().as_stack_overflow().unwrap().height(),
            height
        );
    }

    #[test]
    fn daily_pr_best_of_the_day_keeps_the_fewest_guesses() {
        let mut app = daily_pr_app("prbest.json");
        let word = app.game().unwrap().as_daily_pr().unwrap().word();
        type_word(&mut app, &word); // solved in 1
        assert_eq!(app.state, AppState::GameOver);
        let one_guess = app.best_score_for(GameKind::DailyPr);
        let mvp = app.daily_mvp().unwrap();
        assert_eq!(mvp.game_kind(), GameKind::DailyPr);
        assert_eq!(mvp.note.as_deref(), Some("1 guesses"));

        // A completed daily board cannot be restarted for another attempt.
        app.handle_input(AppInput::Restart);
        assert_eq!(app.state, AppState::Menu);
        start_from_menu(&mut app, GameKind::DailyPr);
        assert_eq!(
            app.game().unwrap().as_daily_pr().unwrap().word(),
            word,
            "returning keeps today's word"
        );
        assert_eq!(app.game().unwrap().as_daily_pr().unwrap().guesses(), 1);
        assert!(app.game().unwrap().as_daily_pr().unwrap().locked());
        app.tick(Duration::from_millis(16));
        assert_eq!(app.state, AppState::GameOver);
        assert_eq!(app.best_score_for(GameKind::DailyPr), one_guess);
        assert_eq!(app.daily_mvp().unwrap().name, "Anonymous");
        assert!(!app.mvp_just_set(), "the same board must not crown twice");
    }

    #[test]
    fn daily_pr_attempts_survive_leaving_and_restarting_the_app() {
        let path = temp_dir("prprogress").join("highscore.json");
        let mut app = app_with_size(HighScoreStore::load(&path), 100, 30);
        start_from_menu(&mut app, GameKind::DailyPr);
        let word = app.game().unwrap().as_daily_pr().unwrap().word();
        let filler = crate::game::daily_pr::GUESSES
            .iter()
            .find(|guess| !guess.eq_ignore_ascii_case(&word))
            .expect("a different word exists");
        type_word(&mut app, filler);
        assert_eq!(app.game().unwrap().as_daily_pr().unwrap().guesses(), 1);

        app.handle_input(AppInput::Back);
        start_from_menu(&mut app, GameKind::DailyPr);
        assert_eq!(
            app.game().unwrap().as_daily_pr().unwrap().guesses(),
            1,
            "leaving for the menu must not restore an attempt"
        );

        drop(app);
        let mut reopened = app_with_size(HighScoreStore::load(&path), 100, 30);
        start_from_menu(&mut reopened, GameKind::DailyPr);
        assert_eq!(
            reopened.game().unwrap().as_daily_pr().unwrap().guesses(),
            1,
            "restarting MVP must not restore an attempt"
        );
    }

    #[test]
    fn daily_pr_failure_scores_zero_and_crowns_nobody() {
        let mut app = daily_pr_app("prfail.json");
        let word = app.game().unwrap().as_daily_pr().unwrap().word();
        let filler = crate::game::daily_pr::GUESSES
            .iter()
            .find(|w| **w != word)
            .expect("a different word exists");
        for _ in 0..crate::game::daily_pr::MAX_GUESSES {
            type_word(&mut app, filler);
        }
        assert_eq!(app.state, AppState::GameOver);
        assert_eq!(
            app.best_score_for(GameKind::DailyPr),
            0,
            "a DNF is no record"
        );
        assert_eq!(app.daily_mvp(), None, "a DNF must not crown the MVP");
    }

    #[test]
    fn daily_fix_faster_fixes_win_the_day() {
        let mut app = daily_fix_app("fixbest.json");
        let fix = app.game().unwrap().as_daily_fix().unwrap().bug().fix;
        type_fix(&mut app, fix); // solved fast
        assert_eq!(app.state, AppState::GameOver);
        let fast = app.best_score_for(GameKind::DailyFix);
        assert!(fast > 0);
        let mvp = app.daily_mvp().unwrap();
        assert_eq!(mvp.game_kind(), GameKind::DailyFix);
        assert!(mvp.note.as_deref().unwrap().starts_with("fixed in"));

        // A slower solve must not dethrone the fast one.
        app.handle_input(AppInput::Restart);
        assert_eq!(app.state, AppState::Playing);
        type_fix(&mut app, "definitely wrong");
        type_fix(&mut app, fix); // same bug, penalized and slower
        assert_eq!(app.state, AppState::GameOver);
        assert_eq!(app.best_score_for(GameKind::DailyFix), fast);
        assert!(!app.mvp_just_set());
    }

    #[test]
    fn restarting_a_daily_fix_keeps_todays_bug() {
        let mut app = daily_fix_app("fixrestart.json");
        let bug = app.game().unwrap().as_daily_fix().unwrap().bug();
        app.handle_input(AppInput::Back); // back to the menu
        start_from_menu(&mut app, GameKind::DailyFix);
        assert_eq!(app.game().unwrap().as_daily_fix().unwrap().bug(), bug);
    }

    // ---- name prompt --------------------------------------------------------

    #[test]
    fn first_run_opens_the_name_prompt_and_confirms_a_name() {
        let mut app = app_with_size(temp_store("nameprompt.json"), 100, 30);
        app.open_name_prompt();
        assert_eq!(app.state, AppState::NamePrompt);
        assert!(!app.has_player_name());

        app.handle_input(AppInput::Text('A'));
        app.handle_input(AppInput::Text('l'));
        app.handle_input(AppInput::Text('e'));
        app.handle_input(AppInput::Text('x'));
        assert_eq!(app.name_buffer(), "Alex");

        app.handle_input(AppInput::Confirm);
        assert_eq!(app.state, AppState::Menu);
        assert!(app.has_player_name());
        assert_eq!(app.player_name(), "Alex");
    }

    #[test]
    fn player_name_is_remembered_across_app_sessions() {
        let path = temp_dir("namesession").join("highscore.json");
        let mut first = app_with_size(HighScoreStore::load(&path), 100, 30);
        first.open_name_prompt();
        for c in "Alex".chars() {
            first.handle_input(AppInput::Text(c));
        }
        first.handle_input(AppInput::Confirm);
        drop(first);

        let second = app_with_size(HighScoreStore::load(&path), 100, 30);
        assert!(second.has_player_name());
        assert_eq!(second.player_name(), "Alex");
    }

    #[test]
    fn name_prompt_requires_a_non_empty_name() {
        let mut app = app_with_size(temp_store("emptyname.json"), 100, 30);
        app.open_name_prompt();
        app.handle_input(AppInput::Confirm);
        assert_eq!(
            app.state,
            AppState::NamePrompt,
            "an empty name must keep prompting"
        );
        assert!(!app.has_player_name());
    }

    #[test]
    fn first_run_name_prompt_cannot_be_skipped_and_rename_can_be_cancelled() {
        let mut app = app_with_size(temp_store("escnone.json"), 100, 30);
        app.open_name_prompt();
        app.handle_input(AppInput::Back);
        assert_eq!(app.state, AppState::NamePrompt);
        assert!(!app.has_player_name());

        for c in "Alex".chars() {
            app.handle_input(AppInput::Text(c));
        }
        app.handle_input(AppInput::Confirm);
        assert_eq!(app.player_name(), "Alex");

        app.open_name_prompt();
        for c in "Sam".chars() {
            app.handle_input(AppInput::Text(c));
        }
        app.handle_input(AppInput::Back);
        assert_eq!(app.state, AppState::Menu);
        assert_eq!(app.player_name(), "Alex");
    }

    #[test]
    fn name_prompt_supports_backspace_and_filters_characters() {
        let mut app = app_with_size(temp_store("backspace.json"), 100, 30);
        app.open_name_prompt();
        app.handle_input(AppInput::Text('H'));
        app.handle_input(AppInput::Text('i'));
        app.handle_input(AppInput::Backspace);
        app.handle_input(AppInput::Text('o'));
        app.handle_input(AppInput::Backspace);
        app.handle_input(AppInput::Text('A'));
        app.handle_input(AppInput::Text('💩'));
        assert_eq!(app.name_buffer(), "HA", "backspace then a filtered char");
        app.handle_input(AppInput::Confirm);
        assert_eq!(app.player_name(), "HA");
    }

    #[test]
    fn n_on_the_menu_opens_the_name_prompt_elsewhere_ignored() {
        let mut app = app_with_size(temp_store("rename.json"), 100, 30);
        app.handle_input(AppInput::Rename);
        assert_eq!(app.state, AppState::NamePrompt);

        let mut playing = playing_app();
        playing.handle_input(AppInput::Rename);
        assert_eq!(playing.state, AppState::Playing);
    }

    #[test]
    fn name_prompt_survives_agent_events() {
        let mut app = app_with_size(temp_store("nameagent.json"), 100, 30);
        app.open_name_prompt();
        app.handle_agent_event(C, AgentEvent::NeedsInput);
        app.handle_agent_event(X, AgentEvent::Working);
        assert_eq!(app.state, AppState::NamePrompt);
        app.tick(Duration::from_secs(1));
        assert_eq!(app.state, AppState::NamePrompt);
    }

    #[test]
    fn renamed_player_appears_on_the_board() {
        let mut app = app_with_size(temp_store("renamed.json"), 100, 30);
        app.open_name_prompt();
        app.handle_input(AppInput::Text('A'));
        app.handle_input(AppInput::Confirm);
        assert_eq!(app.player_name(), "A");

        app.open_name_prompt();
        app.handle_input(AppInput::Text('B'));
        app.handle_input(AppInput::Text('o'));
        app.handle_input(AppInput::Text('b'));
        app.handle_input(AppInput::Confirm);
        assert_eq!(app.player_name(), "Bob");
    }

    // ---- daily MVP ----------------------------------------------------------

    #[test]
    fn finishing_a_run_sets_the_daily_mvp_for_today() {
        let mut app = playing_app();
        assert!(!app.has_player_name());
        scored_run(&mut app);
        assert_eq!(app.state, AppState::GameOver);
        assert!(app.mvp_just_set());
        let mvp = app.daily_mvp().expect("daily mvp set");
        assert_eq!(mvp.name, "Anonymous");
        assert_eq!(mvp.game_kind(), GameKind::StackOverflow);
    }

    #[test]
    fn a_lower_score_does_not_dethrone_the_daily_mvp() {
        let mut app = playing_app();
        scored_run(&mut app);
        let mvp_score = app.daily_mvp().unwrap().score;

        app.start_game();
        overflow(&mut app); // immediate miss: score 0
        assert_eq!(app.daily_mvp().unwrap().score, mvp_score);
        assert!(!app.mvp_just_set());
    }
}
