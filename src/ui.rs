use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use crate::agent::{AgentDisplay, AgentKind, AgentStatus};
use crate::app::{App, AppState, PauseReason};
use crate::game::obstacle::ObstacleKind;
use crate::game::scoring::format_score;
use crate::game::{self, GameRenderState, StackJump};

/// Vertical space consumed by chrome: top/bottom borders (2), HUD (1) and
/// controls footer (1). Everything else is playfield.
const CHROME_ROWS: u16 = 4;

/// The playfield size derived from the terminal size. Shared by the app (for
/// viewport logic) and the renderer (for layout).
pub fn playfield_dims(term_cols: u16, term_rows: u16) -> (u16, u16) {
    let cols = term_cols.saturating_sub(2).max(1);
    let rows = term_rows.saturating_sub(CHROME_ROWS).max(1);
    (cols, rows)
}

pub fn render(frame: &mut Frame, app: &App) {
    if app.too_small() {
        render_too_small(frame, app);
        return;
    }
    match app.state {
        AppState::Menu => render_menu(frame, app),
        AppState::Playing
        | AppState::PausedManual
        | AppState::PausedAgent(_)
        | AppState::GameOver => render_game(frame, app),
    }
}

// ---- menu ----------------------------------------------------------------

const LOGO: [&str; 6] = [
    "██╗    ██╗ █████╗ ██╗████████╗███████╗████████╗ █████╗ ████████╗███████╗",
    "██║    ██║██╔══██╗██║╚══██╔══╝██╔════╝╚══██╔══╝██╔══██╗╚══██╔══╝██╔════╝",
    "██║ █╗ ██║███████║██║   ██║   ███████╗   ██║   ███████║   ██║   █████╗",
    "██║███╗██║██╔══██║██║   ██║   ╚════██║   ██║   ██╔══██║   ██║   ██╔══╝",
    "╚███╔███╔╝██║  ██║██║   ██║   ███████║   ██║   ██║  ██║   ██║   ███████╗",
    " ╚══╝╚══╝ ╚═╝  ╚═╝╚═╝   ╚═╝   ╚══════╝   ╚═╝   ╚═╝  ╚═╝   ╚═╝   ╚══════╝",
];
const LOGO_WIDTH: usize = 74;

fn render_menu(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let wide = area.width as usize >= LOGO_WIDTH;

    let magenta = Style::new().fg(Color::Magenta);
    let dim = Style::new().fg(Color::DarkGray);
    let green = Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);

    let mut lines: Vec<Line<'_>> = Vec::new();
    if wide {
        lines.extend(LOGO.iter().map(|row| Line::styled(*row, magenta)));
    } else {
        lines.push(Line::styled(
            "W A I T S T A T E",
            magenta.add_modifier(Modifier::BOLD),
        ));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled("ARCADE FOR THE AGENTIC ERA", dim));
    lines.push(Line::from(""));
    if let Some(status_line) = agent_status_line(app) {
        lines.push(status_line);
        lines.push(Line::from(""));
    }
    lines.push(Line::styled("[ ENTER ] PLAY", green));
    lines.push(Line::from(""));
    lines.push(Line::styled("Q — QUIT", dim));
    if app.best_score() > 0 {
        lines.push(Line::from(""));
        lines.push(Line::styled(
            format!("BEST {}", format_score(app.best_score())),
            dim,
        ));
    }

    let total = lines.len() as u16;
    let vertical = Layout::vertical([
        Constraint::Length(area.height.saturating_sub(total) / 2),
        Constraint::Length(total),
        Constraint::Min(0),
    ])
    .split(area);
    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, vertical[1]);
}

// ---- game ----------------------------------------------------------------

fn render_game(frame: &mut Frame, app: &App) {
    let Some(game) = app.game() else {
        render_menu(frame, app);
        return;
    };
    let area = frame.area();

    let block = Block::bordered()
        .border_style(Style::new().fg(Color::DarkGray))
        .title(Line::styled(
            " STACK JUMP ",
            Style::new().fg(Color::Magenta),
        ));
    let inner = block.inner(area);
    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .split(inner);

    frame.render_widget(hud_paragraph(game, app), layout[0]);
    frame.render_widget(
        GameView {
            state: game.render_state(),
        },
        layout[1],
    );
    frame.render_widget(controls_paragraph(), layout[2]);
    frame.render_widget(block, area);

    match app.state {
        AppState::PausedManual => render_paused(frame, area, app),
        AppState::PausedAgent(reason) => render_agent_paused(frame, area, app, reason, game),
        AppState::GameOver => render_game_over(frame, area, game, app),
        AppState::Playing | AppState::Menu => {}
    }
}

fn hud_paragraph(game: &StackJump, app: &App) -> Paragraph<'static> {
    let score = Span::styled(
        format!("SCORE {:06}", game.score()),
        Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
    );
    let speed = Span::styled(
        format!("SPEED x{:.1}", game.speed_multiplier()),
        Style::new().fg(Color::DarkGray),
    );
    let best = Span::styled(
        format!("BEST {:06}", app.best_score().max(game.score())),
        Style::new().fg(Color::Yellow),
    );
    let mut line = Line::from(vec![score, Span::raw("   "), speed, Span::raw("   "), best]);
    if let Some(status) = agent_status_span(app) {
        line.spans.insert(0, Span::raw("   "));
        line.spans.insert(0, status);
    }
    Paragraph::new(line)
}

/// The small `Codex • Working` indicator shown in the HUD, pause overlays
/// and menu. Returns `None` while no agent has ever reported in.
fn agent_status_span(app: &App) -> Option<Span<'static>> {
    let specific = match app.display_preference() {
        Some(AgentDisplay::Specific(kind)) => Some(kind),
        None | Some(AgentDisplay::Auto) => None,
    };
    if let Some(kind) = specific {
        let status = app
            .agents()
            .get(&kind)
            .map(|s| s.status)
            .unwrap_or(AgentStatus::Disconnected);
        return (status != AgentStatus::Disconnected).then(|| span_for(kind, status));
    }

    let connected = connected_agents(app);
    if connected.is_empty() {
        return None;
    }
    if connected.len() == 1 {
        let (kind, status) = connected[0];
        return Some(span_for(kind, status));
    }
    // Multi-agent mode: a compact aggregate line.
    let status = app.agent_aggregate();
    Some(Span::styled(
        format!(
            "{} agents {} {}",
            connected.len(),
            status_dot(status),
            status.label()
        ),
        status_style(status),
    ))
}

fn span_for(kind: AgentKind, status: AgentStatus) -> Span<'static> {
    Span::styled(
        format!("{} {} {}", kind.name(), status_dot(status), status.label()),
        status_style(status),
    )
}

fn status_dot(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Working => "●",
        _ => "○",
    }
}

fn status_style(status: AgentStatus) -> Style {
    match status {
        AgentStatus::Working => Style::new().fg(Color::Green),
        AgentStatus::NeedsInput => Style::new().fg(Color::Yellow),
        AgentStatus::Completed => Style::new().fg(Color::Cyan),
        AgentStatus::Disconnected | AgentStatus::Idle | AgentStatus::Stopped => {
            Style::new().fg(Color::DarkGray)
        }
    }
}

/// The non-disconnected agents with their statuses, sorted by kind.
fn connected_agents(app: &App) -> Vec<(AgentKind, AgentStatus)> {
    let mut list: Vec<(AgentKind, AgentStatus)> = app
        .agents()
        .iter()
        .filter(|(_, s)| s.status != AgentStatus::Disconnected)
        .map(|(k, s)| (*k, s.status))
        .collect();
    list.sort_unstable_by_key(|(k, _)| *k);
    list
}

/// A full sentence for the menu, e.g. "Codex is working" or
/// "2 agents are working". `None` while no agent has ever reported in.
fn agent_status_line(app: &App) -> Option<Line<'static>> {
    let connected = connected_agents(app);
    let specific = match app.display_preference() {
        Some(AgentDisplay::Specific(kind)) => Some(kind),
        None | Some(AgentDisplay::Auto) => None,
    };

    let (name, status) = if let Some(kind) = specific {
        let status = app
            .agents()
            .get(&kind)
            .map(|s| s.status)
            .unwrap_or(AgentStatus::Disconnected);
        (kind.name(), status)
    } else if connected.len() == 1 {
        (connected[0].0.name(), connected[0].1)
    } else if connected.len() > 1 {
        let text = match app.agent_aggregate() {
            AgentStatus::Idle => format!("{} agents are idle", connected.len()),
            AgentStatus::Working => format!("{} agents are working", connected.len()),
            AgentStatus::NeedsInput => format!("{} agents need your input", connected.len()),
            AgentStatus::Completed => format!("{} agents finished", connected.len()),
            AgentStatus::Stopped => format!("{} agent sessions ended", connected.len()),
            AgentStatus::Disconnected => return None,
        };
        let style = match app.agent_aggregate() {
            AgentStatus::Working => Style::new().fg(Color::Green),
            AgentStatus::NeedsInput => Style::new().fg(Color::Yellow),
            _ => Style::new().fg(Color::DarkGray),
        };
        return Some(Line::styled(text, style));
    } else {
        return None;
    };

    if status == AgentStatus::Disconnected {
        return None;
    }
    let text = match status {
        AgentStatus::Idle => format!("{name} is idle"),
        AgentStatus::Working => format!("{name} is working"),
        AgentStatus::NeedsInput => format!("{name} needs your input"),
        AgentStatus::Completed => format!("{name} finished"),
        AgentStatus::Stopped => format!("{name} session ended"),
        AgentStatus::Disconnected => return None,
    };
    Some(Line::styled(text, status_style(status)))
}

fn controls_paragraph() -> Paragraph<'static> {
    let dim = Style::new().fg(Color::DarkGray);
    let line = Line::styled(
        "SPACE jump    P pause    R restart    ESC menu    Q quit",
        dim,
    );
    Paragraph::new(line).alignment(Alignment::Center)
}

fn render_paused(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines = vec![
        Line::styled(
            "PAUSED",
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::styled("P resume    ESC menu", Style::new().fg(Color::DarkGray)),
    ];
    if let Some(status) = agent_status_span(app) {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::raw("   "), status, Span::raw("   ")]));
    }
    let height = lines.len() as u16 + 2;
    let rect = centered_fixed(area, 40, height);
    frame.render_widget(Clear, rect);
    let block = Block::bordered().border_style(Style::new().fg(Color::Yellow));
    let inner = block.inner(rect);
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    frame.render_widget(block, rect);
}

/// The agent-pause overlay: visually distinct from manual pause so the
/// developer instantly knows *why* the game stopped and *which* agent is
/// involved. With multiple agents the overlay names them all.
fn render_agent_paused(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    reason: PauseReason,
    game: &StackJump,
) {
    let (status, verb_one, verb_many, detail, restart_hint) = match reason {
        PauseReason::NeedsInput => (
            AgentStatus::NeedsInput,
            "NEEDS YOU",
            "NEED YOU",
            "Stack Jump paused automatically",
            "Return to {agents}",
        ),
        PauseReason::Completed => (
            AgentStatus::Completed,
            "FINISHED",
            "FINISHED",
            "Your run has been preserved",
            "Return to {agents}",
        ),
        PauseReason::Stopped => (
            AgentStatus::Stopped,
            "SESSION ENDED",
            "SESSIONS ENDED",
            "Your run has been preserved",
            "Restart {agents} to resume",
        ),
    };
    let involved = app.agents_with_status(status);

    // Title: "CODEX NEEDS YOU" for one agent, "2 AGENTS NEED YOU" for many.
    let title = if involved.len() == 1 {
        format!("{} {verb_one}", involved[0].upper_name())
    } else {
        format!("{} AGENTS {verb_many}", involved.len())
    };

    let mut lines = vec![
        Line::styled(
            title,
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::styled(detail, Style::new().fg(Color::DarkGray)),
    ];
    if involved.len() > 1 {
        lines.push(Line::styled(
            involved
                .iter()
                .map(|k| k.name())
                .collect::<Vec<_>>()
                .join(", "),
            Style::new().fg(Color::DarkGray),
        ));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        format!("Score {}", format_score(game.score())),
        Style::new().fg(Color::White),
    ));
    lines.push(Line::from(""));

    let hint_agents = if involved.len() == 1 {
        involved[0].name().to_string()
    } else {
        "your coding agents".to_string()
    };
    lines.push(Line::styled(
        restart_hint.replace("{agents}", &hint_agents),
        Style::new().fg(Color::DarkGray),
    ));
    if let Some(status) = agent_status_span(app) {
        lines.push(Line::from(vec![Span::raw("   "), status, Span::raw("   ")]));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        "[ENTER] resume run    [ESC] menu",
        Style::new().fg(Color::Green),
    ));

    let height = lines.len() as u16 + 2;
    let rect = centered_fixed(area, 44, height);
    frame.render_widget(Clear, rect);
    let block = Block::bordered().border_style(Style::new().fg(Color::Cyan));
    let inner = block.inner(rect);
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    frame.render_widget(block, rect);
}

fn render_game_over(frame: &mut Frame, area: Rect, game: &StackJump, app: &App) {
    let mut lines: Vec<Line<'static>> = vec![
        Line::styled(
            "GAME OVER",
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::styled(
            format!("SCORE {}", format_score(game.score())),
            Style::new().fg(Color::White),
        ),
        Line::styled(
            format!("BEST  {}", format_score(app.best_score())),
            Style::new().fg(Color::Yellow),
        ),
        Line::styled(
            format!("TIME  {}", format_elapsed(game.elapsed())),
            Style::new().fg(Color::DarkGray),
        ),
    ];
    if app.is_new_record() {
        lines.push(Line::styled(
            "NEW BEST!",
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(status) = agent_status_span(app) {
        lines.push(Line::from(vec![Span::raw("   "), status, Span::raw("   ")]));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        "[R] PLAY AGAIN",
        Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::styled("[ESC] MENU", Style::new().fg(Color::DarkGray)));

    let height = lines.len() as u16 + 2;
    let rect = centered_fixed(area, 34, height);
    frame.render_widget(Clear, rect);
    let block = Block::bordered().border_style(Style::new().fg(Color::Red));
    let inner = block.inner(rect);
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    frame.render_widget(block, rect);
}

// ---- shared helpers --------------------------------------------------------

fn centered_fixed(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + (area.width - width) / 2;
    let y = area.y + (area.height - height) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// Formats a duration in seconds as `M:SS` (e.g. 75.4 -> "1:15").
fn format_elapsed(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn render_too_small(frame: &mut Frame, app: &App) {
    let (cols, rows) = app.terminal_size().unwrap_or((0, 0));
    let lines = vec![
        Line::styled(
            "TERMINAL TOO SMALL",
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::from("WaitState requires at least 60×20."),
        Line::from(format!("Current size: {cols}×{rows}")),
        Line::from("Resize the terminal to keep playing."),
    ];
    let area = frame.area();
    let total = lines.len() as u16;
    let vertical = Layout::vertical([
        Constraint::Length(area.height.saturating_sub(total) / 2),
        Constraint::Length(total),
        Constraint::Min(0),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        vertical[1],
    );
}

// ---- game view widget -------------------------------------------------------

/// Renders the playfield from a plain-data [`GameRenderState`]. All mapping
/// from world coordinates to cells lives here, keeping game logic free of
/// terminal concerns.
struct GameView<'a> {
    state: GameRenderState<'a>,
}

impl Widget for GameView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let ground_style = Style::new().fg(Color::DarkGray);
        let player_style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
        let player = self.state.player;
        let origin_col = game::player_col(area.width);

        // Ground line along the bottom row of the playfield.
        for x in area.left()..area.right() {
            buf[(x, area.bottom() - 1)]
                .set_symbol("─")
                .set_style(ground_style);
        }

        // Obstacles, then the player on top.
        for obstacle in self.state.obstacles {
            let style = match obstacle.kind {
                ObstacleKind::Small => Style::new().fg(Color::Yellow),
                ObstacleKind::Large => Style::new().fg(Color::Red),
            };
            draw_world_rect(buf, area, origin_col, &obstacle.rect(), style);
        }
        draw_world_rect(buf, area, origin_col, &player.rect(), player_style);
    }
}

/// Fills every cell of `area` intersected by `rect` (world units, origin at
/// `origin_col`, ground at the bottom row) with a solid block.
fn draw_world_rect(
    buf: &mut Buffer,
    area: Rect,
    origin_col: u16,
    rect: &game::collision::Rect,
    style: Style,
) {
    let left = origin_col as f64 + rect.x;
    let right = left + rect.w;
    let area_left = i64::from(area.left());
    let area_right = i64::from(area.right());
    let area_top = i64::from(area.top());
    let area_bottom = i64::from(area.bottom());

    for world_x in (left.floor() as i64)..(right.ceil() as i64) {
        let x = area_left + world_x;
        if !(area_left..area_right).contains(&x) {
            continue;
        }
        for world_y in (rect.y.floor() as i64)..((rect.y + rect.h).ceil() as i64) {
            if world_y < 0 {
                continue;
            }
            let y = area_bottom - 1 - world_y;
            if !(area_top..area_bottom).contains(&y) {
                continue;
            }
            buf[(x as u16, y as u16)].set_symbol("█").set_style(style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentKind;
    use crate::config::HighScoreStore;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    fn test_store(name: &str) -> HighScoreStore {
        let mut path = std::env::temp_dir();
        path.push(format!("waitstate_ui_test_{}_{}", std::process::id(), name));
        let _ = std::fs::remove_file(&path);
        HighScoreStore::load(path)
    }

    fn app_at(width: u16, height: u16) -> App {
        let mut app = App::new(test_store("ui.json"));
        app.set_terminal_size(width, height);
        app
    }

    fn render_buffer(app: &App, width: u16, height: u16) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn all_text(buf: &Buffer) -> String {
        let mut text = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    #[test]
    fn playfield_dims_leave_room_for_chrome() {
        assert_eq!(playfield_dims(80, 24), (78, 20));
        assert_eq!(playfield_dims(60, 20), (58, 16));
    }

    #[test]
    fn playfield_dims_never_go_below_one() {
        assert_eq!(playfield_dims(1, 1), (1, 1));
        assert_eq!(playfield_dims(0, 0), (1, 1));
    }

    #[test]
    fn elapsed_is_formatted_as_minutes_seconds() {
        assert_eq!(format_elapsed(0.0), "0:00");
        assert_eq!(format_elapsed(42.7), "0:42");
        assert_eq!(format_elapsed(75.4), "1:15");
    }

    #[test]
    fn menu_render_shows_logo_and_controls() {
        let app = app_at(100, 30);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("ARCADE FOR THE AGENTIC ERA"));
        assert!(text.contains("PLAY"));
        assert!(text.contains("QUIT"));
    }

    #[test]
    fn menu_render_degrades_to_plain_title_when_narrow() {
        let app = app_at(70, 30);
        let text = all_text(&render_buffer(&app, 70, 30));
        assert!(text.contains("W A I T S T A T E"));
        assert!(text.contains("PLAY"));
    }

    #[test]
    fn playing_render_shows_hud_ground_and_player() {
        let mut app = app_at(100, 30);
        app.start_game();
        let buf = render_buffer(&app, 100, 30);
        let text = all_text(&buf);
        assert!(text.contains("SCORE"));
        assert!(text.contains("BEST"));
        assert!(text.contains("STACK JUMP"));
        assert!(text.contains("─"));
        // Player block standing on the ground row (playfield bottom) at the
        // anchored column (playfield x + player column).
        let player_col = usize::from(game::player_col(98)) + 1;
        assert_eq!(buf[(player_col as u16, 27)].symbol(), "█");
        assert_eq!(buf[(player_col as u16, 26)].symbol(), "█");
        assert_eq!(buf[(player_col as u16 + 1, 27)].symbol(), "█");
        assert_eq!(buf[(player_col as u16, 27)].fg, Color::Cyan);
    }

    #[test]
    fn paused_render_shows_overlay() {
        let mut app = app_at(100, 30);
        app.start_game();
        app.pause();
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("PAUSED"));
    }

    #[test]
    fn agent_pause_render_is_distinct_from_manual_pause() {
        let mut app = app_at(100, 30);
        app.start_game();
        app.handle_agent_event(AgentKind::ClaudeCode, crate::agent::AgentEvent::NeedsInput);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("CLAUDE NEEDS YOU"));
        assert!(text.contains("paused automatically"));
        assert!(text.contains("Return to Claude Code"));
        assert!(text.contains("resume run"));
        assert!(!text.contains("PAUSED"));
    }

    #[test]
    fn codex_pause_render_names_codex() {
        let mut app = app_at(100, 30);
        app.start_game();
        app.handle_agent_event(AgentKind::Codex, crate::agent::AgentEvent::NeedsInput);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("CODEX NEEDS YOU"));
        assert!(text.contains("Return to Codex"));
        assert!(!text.contains("CLAUDE"));
    }

    #[test]
    fn gemini_and_opencode_pause_overlays_use_their_names() {
        for (kind, name) in [
            (AgentKind::GeminiCli, "GEMINI NEEDS YOU"),
            (AgentKind::OpenCode, "OPENCODE NEEDS YOU"),
        ] {
            let mut app = app_at(100, 30);
            app.start_game();
            app.handle_agent_event(kind, crate::agent::AgentEvent::NeedsInput);
            let text = all_text(&render_buffer(&app, 100, 30));
            assert!(text.contains(name), "expected {name} in:\n{text}");
        }
    }

    #[test]
    fn multi_agent_pause_overlay_names_all_agents() {
        let mut app = app_at(100, 30);
        app.start_game();
        app.handle_agent_event(AgentKind::ClaudeCode, crate::agent::AgentEvent::NeedsInput);
        app.handle_agent_event(AgentKind::Codex, crate::agent::AgentEvent::NeedsInput);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("2 AGENTS NEED YOU"));
        assert!(text.contains("Claude Code, Codex"));
        assert!(text.contains("your coding agents"));
    }

    #[test]
    fn completed_pause_render_shows_finished() {
        let mut app = app_at(100, 30);
        app.start_game();
        app.handle_agent_event(AgentKind::ClaudeCode, crate::agent::AgentEvent::Completed);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("CLAUDE FINISHED"));
        assert!(text.contains("Your run has been preserved"));
    }

    #[test]
    fn session_end_pause_render_shows_ended() {
        let mut app = app_at(100, 30);
        app.start_game();
        app.handle_agent_event(AgentKind::ClaudeCode, crate::agent::AgentEvent::Stopped);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("CLAUDE SESSION ENDED"));
    }

    #[test]
    fn hud_shows_agent_indicator_when_agent_reported_in() {
        let mut app = app_at(100, 30);
        app.start_game();
        app.handle_agent_event(AgentKind::ClaudeCode, crate::agent::AgentEvent::Working);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("Claude Code"));
        assert!(text.contains("Working"));
    }

    #[test]
    fn hud_shows_multi_agent_indicator_when_two_agents_connected() {
        let mut app = app_at(100, 30);
        app.start_game();
        app.handle_agent_event(AgentKind::ClaudeCode, crate::agent::AgentEvent::Working);
        app.handle_agent_event(AgentKind::Codex, crate::agent::AgentEvent::Working);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("2 agents"));
        assert!(text.contains("Working"));
    }

    #[test]
    fn hud_has_no_agent_indicator_when_disconnected() {
        let mut app = app_at(100, 30);
        app.start_game();
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(!text.contains("Claude"));
    }

    #[test]
    fn menu_shows_claude_working_hint() {
        let mut app = app_at(100, 30);
        app.handle_agent_event(AgentKind::ClaudeCode, crate::agent::AgentEvent::Working);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("Claude Code is working"));
        assert!(text.contains("PLAY"));
    }

    #[test]
    fn menu_has_no_agent_line_when_disconnected() {
        let app = app_at(100, 30);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(!text.contains("Claude"));
    }

    #[test]
    fn game_over_panel_shows_agent_line() {
        let mut app = app_at(100, 30);
        app.start_game();
        app.spawn_test_obstacle();
        app.tick(std::time::Duration::from_millis(16));
        assert_eq!(app.state, AppState::GameOver);
        app.handle_agent_event(AgentKind::ClaudeCode, crate::agent::AgentEvent::Completed);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("GAME OVER"));
        assert!(text.contains("Finished"));
    }

    #[test]
    fn game_over_render_shows_panel() {
        let mut app = app_at(100, 30);
        app.start_game();
        app.spawn_test_obstacle();
        app.tick(std::time::Duration::from_millis(16));
        assert_eq!(app.state, AppState::GameOver);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("GAME OVER"));
        assert!(text.contains("PLAY AGAIN"));
        assert!(text.contains("MENU"));
    }

    #[test]
    fn too_small_render_shows_warning() {
        let mut app = app_at(42, 14);
        app.start_game();
        let text = all_text(&render_buffer(&app, 42, 14));
        assert!(text.contains("TERMINAL TOO SMALL"));
        assert!(text.contains("42×14"));
    }
}
