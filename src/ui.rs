use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use crate::app::{App, AppState};
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
        AppState::Playing | AppState::Paused | AppState::GameOver => render_game(frame, app),
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
        AppState::Paused => render_paused(frame, area),
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
    let line = Line::from(vec![score, Span::raw("   "), speed, Span::raw("   "), best]);
    Paragraph::new(line)
}

fn controls_paragraph() -> Paragraph<'static> {
    let dim = Style::new().fg(Color::DarkGray);
    let line = Line::styled(
        "SPACE jump    P pause    R restart    ESC menu    Q quit",
        dim,
    );
    Paragraph::new(line).alignment(Alignment::Center)
}

fn render_paused(frame: &mut Frame, area: Rect) {
    let rect = centered_fixed(area, 40, 5);
    frame.render_widget(Clear, rect);
    let block = Block::bordered().border_style(Style::new().fg(Color::Yellow));
    let inner = block.inner(rect);
    let lines = vec![
        Line::styled(
            "PAUSED",
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::styled("P resume    ESC menu", Style::new().fg(Color::DarkGray)),
    ];
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
