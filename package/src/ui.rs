use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::agent::{AgentDisplay, AgentKind, AgentStatus};
use crate::app::{App, AppState, OnlineLeaderboard, PauseReason};
use crate::game::daily_fix::DailyFix;
use crate::game::daily_pr::{DailyPr, MAX_GUESSES, Mark};
use crate::game::scoring::{format_elapsed, format_score};
use crate::game::stack_overflow::StackOverflow;
use crate::game::{ActiveGame, GameKind};

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
        AppState::Leaderboard => render_leaderboard(frame, app),
        AppState::GameMenu => render_game_menu(frame, app),
        AppState::NamePrompt => render_name_prompt(frame, app),
        AppState::Playing
        | AppState::PausedManual
        | AppState::PausedAgent(_)
        | AppState::GameOver => render_game(frame, app),
    }
}

// ---- menu ----------------------------------------------------------------

/// FIGlet's "Big" style with full-width spacing between letters. Avoiding
/// smushing keeps M, V and P distinct even with unusual terminal fonts.
const LOGO: [&str; 6] = [
    r#" __  __   __      __  _____ "#,
    r#"|  \/  |  \ \    / / |  __ \"#,
    r#"| \  / |   \ \  / /  | |__) |"#,
    r#"| |\/| |    \ \/ /   |  ___/"#,
    r#"| |  | |     \  /    | |"#,
    r#"|_|  |_|      \/     |_|"#,
];
const LOGO_WIDTH: usize = 29;

/// FIGlet's "doh" font, generated for `MVP`. This is intentionally large
/// and uses each letter as its own fill character, making the word readable
/// at a glance. Smaller terminals fall back to [`LOGO`].
const DOH_LOGO: [&str; 16] = [
    "MMMMMMMM               MMMMMMMMVVVVVVVV           VVVVVVVVPPPPPPPPPPPPPPPPP",
    "M:::::::M             M:::::::MV::::::V           V::::::VP::::::::::::::::P",
    "M::::::::M           M::::::::MV::::::V           V::::::VP::::::PPPPPP:::::P",
    "M:::::::::M         M:::::::::MV::::::V           V::::::VPP:::::P     P:::::P",
    "M::::::::::M       M::::::::::M V:::::V           V:::::V  PP::::P     P:::::P",
    "M:::::::::::M     M:::::::::::M  V:::::V         V:::::V   PP::::P     P:::::P",
    "M:::::::M::::M   M::::M:::::::M   V:::::V       V:::::V    PP::::PPPPPP:::::P",
    "M::::::M M::::M M::::M M::::::M    V:::::V     V:::::V     PP:::::::::::::PP",
    "M::::::M  M::::M::::M  M::::::M     V:::::V   V:::::V      PP::::PPPPPPPPP",
    "M::::::M   M:::::::M   M::::::M      V:::::V V:::::V       PP::::PP",
    "M::::::M    M:::::M    M::::::M       V:::::V:::::V        PP::::PP",
    "M::::::M     MMMMM     M::::::M        V:::::::::V         PP::::PP",
    "M::::::M               M::::::M         V:::::::V         PP::::::PP",
    "M::::::M               M::::::M          V:::::V          P::::::::P",
    "M::::::M               M::::::M           V:::V           P::::::::P",
    "MMMMMMMM               MMMMMMMM            VVV            PPPPPPPPPP",
];
const DOH_LOGO_WIDTH: usize = 78;
const DOH_MIN_ROWS: u16 = 34;

fn render_menu(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let wide = area.width as usize >= LOGO_WIDTH;

    let logo = Style::new()
        .fg(Color::LightMagenta)
        .add_modifier(Modifier::BOLD);
    let dim = Style::new().fg(Color::DarkGray);
    let white = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);
    let green = Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);
    let gold = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);

    let mut lines: Vec<Line<'_>> = Vec::new();
    if area.width as usize >= DOH_LOGO_WIDTH && area.height >= DOH_MIN_ROWS {
        lines.extend(
            DOH_LOGO
                .iter()
                .map(|row| Line::styled(format!("{row:<DOH_LOGO_WIDTH$}"), logo)),
        );
    } else if wide {
        lines.extend(
            LOGO.iter()
                .map(|row| Line::styled(format!("{row:<LOGO_WIDTH$}"), logo)),
        );
    } else {
        lines.push(Line::styled("M V P", logo));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled("MOST VALUED PROGRAMMER", dim));
    lines.push(Line::from(""));
    if let Some(mvp) = app.daily_mvp() {
        let score_text = match &mvp.note {
            Some(note) => note.clone(),
            None => format_score(mvp.score),
        };
        lines.push(Line::from(vec![
            Span::styled("MVP OF THE DAY  ", gold),
            Span::styled(&mvp.name, gold),
            Span::styled("  |  ", gold),
            Span::styled(score_text, gold),
            Span::styled("  |  ", gold),
            Span::styled(mvp.game_kind().title(), gold),
        ]));
        lines.push(Line::from(""));
    }
    lines.push(Line::from(vec![
        Span::styled("PLAYING AS ", dim),
        Span::styled(app.player_name(), white),
    ]));
    lines.push(Line::from(""));
    if let Some(status_line) = agent_status_line(app) {
        lines.push(status_line);
        lines.push(Line::from(""));
    }
    if let Some(message) = app.account_message() {
        lines.push(Line::styled(message, dim));
        lines.push(Line::from(""));
    }
    lines.push(Line::styled("[ ENTER ] PLAY", green));
    if app.best_score() > 0 {
        lines.push(Line::from(""));
        lines.push(Line::styled(
            format!("BEST {}", format_score(app.best_score())),
            dim,
        ));
    }

    let menu_areas = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);
    let content = menu_areas[0];
    let footer = menu_areas[1];
    let total = lines.len() as u16;
    let vertical = Layout::vertical([
        Constraint::Length(content.height.saturating_sub(total) / 2),
        Constraint::Length(total),
        Constraint::Min(0),
    ])
    .split(content);
    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, vertical[1]);
    let account = if app.online_username().is_some() {
        "[ I ] ACCOUNT"
    } else {
        "[ I ] SIGN IN"
    };
    frame.render_widget(
        Paragraph::new(format!(
            "[ L ] LEADERBOARD    {account}    [ N ] CHANGE NAME    [ Q ] QUIT"
        ))
        .style(dim)
        .alignment(Alignment::Center),
        footer,
    );
}

fn render_leaderboard(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let title = Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD);
    let white = Style::new().fg(Color::White);
    let dim = Style::new().fg(Color::DarkGray);
    let green = Style::new().fg(Color::Green);
    let yellow = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let mut lines = vec![
        Line::styled("MVP - DAILY LEADERBOARD", title),
        Line::from(""),
    ];

    match app.online_leaderboard() {
        OnlineLeaderboard::NotLoaded | OnlineLeaderboard::Loading => {
            lines.push(Line::styled("Loading global leaderboard...", dim));
        }
        OnlineLeaderboard::Unavailable => {
            lines.push(Line::styled("Global leaderboard unavailable.", yellow));
            lines.push(Line::styled("Local scores are still available.", dim));
            lines.push(Line::from(""));
            lines.push(Line::styled("OFFLINE", dim));
        }
        OnlineLeaderboard::Available(board) => {
            lines.push(Line::styled("  #   Programmer                 MVP", dim));
            let username = app.online_username();
            for entry in &board.entries {
                let mine =
                    username.is_some_and(|name| name.eq_ignore_ascii_case(&entry.user.username));
                let style = if mine { yellow } else { white };
                lines.push(Line::styled(
                    format!(
                        "{:>3}   {:<22} {:>8}",
                        entry.rank,
                        entry.user.username.chars().take(22).collect::<String>(),
                        format_score(entry.points.max(0) as u64)
                    ),
                    style,
                ));
            }
            lines.push(Line::from(""));
            if let Some(name) = username {
                let rank = board
                    .entries
                    .iter()
                    .find(|entry| entry.user.username.eq_ignore_ascii_case(name))
                    .map(|entry| format!("#{}", entry.rank))
                    .unwrap_or_else(|| "outside top 10".to_string());
                lines.push(Line::styled(format!("YOU: {rank}"), yellow));
            } else {
                lines.push(Line::styled(
                    "Sign in with `mvp login` to submit scores.",
                    dim,
                ));
            }
            lines.push(Line::styled("ONLINE", green));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::styled("[ ESC ] BACK", dim));

    let total = lines.len() as u16;
    let vertical = Layout::vertical([
        Constraint::Length(area.height.saturating_sub(total) / 2),
        Constraint::Length(total.min(area.height)),
        Constraint::Min(0),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        vertical[1],
    );
}

// ---- name prompt ---------------------------------------------------------

fn render_name_prompt(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let magenta = Style::new().fg(Color::Magenta);
    let dim = Style::new().fg(Color::DarkGray);
    let white = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);

    let mut lines: Vec<Line<'_>> = Vec::new();
    lines.push(Line::styled(
        "WHO IS THE MVP?",
        magenta.add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("> "),
        Span::styled(format!("{}▌", app.name_buffer()), white),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::styled("YOUR NAME GOES ON TODAY'S MVP BOARD", dim));
    lines.push(Line::styled(
        if app.has_player_name() {
            "[ ENTER ] SAVE    [ ESC ] CANCEL"
        } else {
            "[ ENTER ] SAVE    NAME REQUIRED TO PLAY"
        },
        dim,
    ));

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

// ---- game menu ------------------------------------------------------------

/// The best score for a game, rendered in the game's own units: raw score
/// for Stack Overflow, guess count for The Daily PR, and time for The Daily
/// Fix (normalized board scores are interpreted back).
fn best_line(kind: GameKind, best: u64) -> String {
    if best == 0 {
        return "BEST 0".to_string();
    }
    match kind {
        GameKind::StackOverflow => format!("BEST {}", format_score(best)),
        GameKind::DailyPr => {
            format!("BEST {} GUESSES", 7 - best / 1_000_000)
        }
        GameKind::DailyFix => {
            format!(
                "BEST {}",
                format_elapsed((1_000_000_000 - best) as f64 / 1000.0)
            )
        }
    }
}

fn render_game_menu(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let magenta = Style::new().fg(Color::Magenta);
    let dim = Style::new().fg(Color::DarkGray);
    let white = Style::new().fg(Color::White);
    let selected_style = Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);
    let best_style = Style::new().fg(Color::Yellow);

    let mut lines: Vec<Line<'_>> = Vec::new();
    lines.push(Line::styled(
        "CHOOSE YOUR GAME",
        magenta.add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::from(""));
    for kind in GameKind::ALL {
        let selected = kind == app.selected_kind();
        let marker = if selected { "▶" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(marker, selected_style),
            Span::raw(" "),
            Span::styled(kind.title(), if selected { selected_style } else { white }),
            Span::raw("  "),
            Span::styled(kind.blurb(), dim),
            Span::raw("  "),
            Span::styled(best_line(kind, app.best_score_for(kind)), best_style),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled("↑/↓ SELECT    ENTER PLAY    ESC BACK", dim));

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
    match game {
        ActiveGame::StackOverflow(game) => render_stack_overflow(frame, app, game),
        ActiveGame::DailyPr(game) => render_daily_pr(frame, app, game),
        ActiveGame::DailyFix(game) => render_daily_fix(frame, app, game),
    }
}

/// Draws the shared chrome (bordered frame, HUD, controls) plus the
/// pause/agent/game-over overlays on top of a game's custom body.
fn render_game_frame(
    frame: &mut Frame,
    app: &App,
    title: &'static str,
    body: &dyn Fn(&mut Frame, Rect),
    controls: &'static str,
    hud: Line<'static>,
) {
    let area = frame.area();
    let block = Block::bordered()
        .border_style(Style::new().fg(Color::DarkGray))
        .title(Line::styled(
            format!(" {title} "),
            Style::new().fg(Color::Magenta),
        ));
    let inner = block.inner(area);
    let layout = Layout::vertical([
        Constraint::Length(1), // HUD
        Constraint::Min(1),    // game body
        Constraint::Length(1), // controls
    ])
    .split(inner);

    frame.render_widget(hud_paragraph(hud, app), layout[0]);
    body(frame, layout[1]);
    frame.render_widget(controls_paragraph(controls), layout[2]);
    frame.render_widget(block, area);

    match app.state {
        AppState::PausedManual => render_paused(frame, area, app),
        AppState::PausedAgent(reason) => render_agent_paused(frame, area, app, reason),
        AppState::GameOver => render_game_over(frame, area, app),
        AppState::Playing
        | AppState::Menu
        | AppState::Leaderboard
        | AppState::GameMenu
        | AppState::NamePrompt => {}
    }
}

fn hud_paragraph(mut line: Line<'static>, app: &App) -> Paragraph<'static> {
    if let Some(status) = agent_status_span(app) {
        line.spans.insert(0, Span::raw("   "));
        line.spans.insert(0, status);
    }
    Paragraph::new(line)
}

fn controls_paragraph(text: &'static str) -> Paragraph<'static> {
    let dim = Style::new().fg(Color::DarkGray);
    Paragraph::new(Line::styled(text, dim)).alignment(Alignment::Center)
}

// ---- Stack Overflow --------------------------------------------------------

fn render_stack_overflow(frame: &mut Frame, app: &App, game: &StackOverflow) {
    render_game_frame(
        frame,
        app,
        "STACK OVERFLOW",
        &|frame, area| render_stack_body(frame.buffer_mut(), area, game),
        "SPACE drop    P pause    R restart    ESC menu    Q quit",
        stack_hud(game, app),
    );
}

fn stack_hud(game: &StackOverflow, app: &App) -> Line<'static> {
    let white = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);
    let dim = Style::new().fg(Color::DarkGray);
    let yellow = Style::new().fg(Color::Yellow);
    let combo = if game.perfect_streak() > 0 {
        format!("   COMBO x{}", game.perfect_streak())
    } else {
        String::new()
    };
    Line::from(vec![
        Span::styled(format!("SCORE {}", format_score(game.score())), white),
        Span::raw("   "),
        Span::styled(format!("HEIGHT {}", game.height()), dim),
        Span::raw("   "),
        Span::styled(
            format!(
                "BEST {}",
                format_score(
                    app.best_score_for(GameKind::StackOverflow)
                        .max(game.score())
                )
            ),
            yellow,
        ),
        Span::styled(
            combo,
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
    ])
}

/// Basic ANSI colors cycled by absolute layer depth. These render on every
/// terminal, unlike 256-color indexed shades which silently degrade.
const LAYER_COLORS: [Color; 8] = [
    Color::Red,
    Color::Yellow,
    Color::Green,
    Color::Cyan,
    Color::Blue,
    Color::Magenta,
    Color::LightRed,
    Color::LightCyan,
];

/// The stacker's body: the tower, the sliding block and the direction
/// arrow. The tower scrolls: only the layers near the top are visible.
fn render_stack_body(buf: &mut Buffer, area: Rect, game: &StackOverflow) {
    let all_layers = game.layers();
    let visible = (area.height.saturating_sub(2)) as usize;
    let skip = all_layers.len().saturating_sub(visible);
    let shown = &all_layers[skip..];

    let dim = Style::new().fg(Color::DarkGray);
    let block_style = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);

    // Ground line.
    for x in area.left()..area.right() {
        buf[(x, area.bottom() - 1)].set_symbol("─").set_style(dim);
    }

    // The stack, bottom layers first so the newest sits on top. Depth is
    // counted from the absolute bottom of the tower so the color pattern
    // stays stable while the viewport scrolls.
    let base_row = area.bottom().saturating_sub(2);
    for (i, layer) in shown.iter().enumerate() {
        let row = base_row.saturating_sub(i as u16);
        let depth = skip + i;
        let style = Style::new().fg(LAYER_COLORS[depth % LAYER_COLORS.len()]);
        fill_row(buf, area, row, layer.left, layer.width, "█", style);
    }

    // The sliding block floats one row above the newest layer, rising with
    // the stack until the playfield fills up.
    let block_row = base_row.saturating_sub(shown.len() as u16);
    let top = game.top_layer();
    let offset = game.block_offset().round() as i32;
    let arrow = if game.direction() > 0 { ">" } else { "<" };
    let block_left = top.left + offset;
    if block_row >= area.top() && block_row < area.bottom() {
        fill_row(
            buf,
            area,
            block_row,
            block_left,
            top.width,
            "█",
            block_style,
        );
        // Draw the arrow at the leading edge, just outside the block.
        let arrow_col = if game.direction() > 0 {
            block_left + top.width
        } else {
            block_left - 1
        };
        if arrow_col >= i32::from(area.left()) && arrow_col < i32::from(area.right()) {
            buf[(arrow_col as u16, block_row)]
                .set_symbol(arrow)
                .set_style(block_style);
        }
    }
}

/// Fills `width` cells starting at `left` with a symbol.
fn fill_row(
    buf: &mut Buffer,
    area: Rect,
    row: u16,
    left: i32,
    width: i32,
    symbol: &str,
    style: Style,
) {
    if row < area.top() || row >= area.bottom() {
        return;
    }
    for x in left.max(i32::from(area.left()))..(left + width).min(i32::from(area.right())) {
        buf[(x as u16, row)].set_symbol(symbol).set_style(style);
    }
}

// ---- The Daily PR ----------------------------------------------------------

fn render_daily_pr(frame: &mut Frame, app: &App, game: &DailyPr) {
    render_game_frame(
        frame,
        app,
        "THE DAILY PR",
        &|frame, area| render_pr_body(frame, area, game),
        "TYPE GUESS    ⌫ DELETE    ENTER SUBMIT    ESC MENU",
        pr_hud(game, app),
    );
}

fn pr_hud(game: &DailyPr, app: &App) -> Line<'static> {
    let white = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);
    let dim = Style::new().fg(Color::DarkGray);
    let yellow = Style::new().fg(Color::Yellow);
    let best = app.best_score_for(GameKind::DailyPr);
    let best_text = if best == 0 {
        "BEST —".to_string()
    } else {
        format!("BEST {} GUESSES", 7 - best / 1_000_000)
    };
    Line::from(vec![
        Span::styled(format!("COMMIT #{}", game.commit_number()), white),
        Span::raw("   "),
        Span::styled(format!("GUESS {}/{}", game.guesses(), MAX_GUESSES), dim),
        Span::raw("   "),
        Span::styled(best_text, yellow),
    ])
}

fn mark_symbol(mark: Mark) -> (&'static str, Style) {
    match mark {
        Mark::Correct => (
            "G",
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        Mark::Present => (
            "?",
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Mark::Absent => ("·", Style::new().fg(Color::DarkGray)),
    }
}

fn render_pr_body(frame: &mut Frame, area: Rect, game: &DailyPr) {
    let white = Style::new().fg(Color::White);
    let dim = Style::new().fg(Color::DarkGray);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for guess in game.guesses_used() {
        let word_line = Line::from(
            guess
                .word
                .chars()
                .map(|c| Span::styled(c.to_string(), white))
                .collect::<Vec<_>>(),
        );
        let marks_line = Line::from(
            guess
                .marks
                .iter()
                .map(|m| {
                    let (symbol, style) = mark_symbol(*m);
                    Span::styled(symbol.to_string(), style)
                })
                .collect::<Vec<_>>(),
        );
        lines.push(word_line);
        lines.push(marks_line);
    }
    if game.locked() {
        lines.push(Line::from(""));
    } else {
        lines.push(Line::from(vec![
            Span::raw("> "),
            Span::styled(format!("{}▌", game.input()), white),
            Span::styled(" ".repeat(5 - game.input().chars().count().min(5)), dim),
        ]));
        if let Some(status) = game.status() {
            lines.push(Line::styled(status, Style::new().fg(Color::Red)));
        }
    }

    let width = 20.min(area.width);
    let height = (lines.len() as u16).min(area.height);
    let board_area = centered_fixed(area, width, height);
    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, board_area);
}

// ---- The Daily Fix ---------------------------------------------------------

fn render_daily_fix(frame: &mut Frame, app: &App, game: &DailyFix) {
    render_game_frame(
        frame,
        app,
        "THE DAILY FIX",
        &|frame, area| render_fix_body(frame, area, game),
        "TYPE FIX    ⌫ DELETE    ENTER SUBMIT    ESC MENU",
        fix_hud(game, app),
    );
}

fn fix_hud(game: &DailyFix, app: &App) -> Line<'static> {
    let white = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);
    let dim = Style::new().fg(Color::DarkGray);
    let yellow = Style::new().fg(Color::Yellow);
    let best = app.best_score_for(GameKind::DailyFix);
    let best_text = if best == 0 {
        "BEST —".to_string()
    } else {
        format!(
            "BEST {}",
            format_elapsed((1_000_000_000 - best) as f64 / 1000.0)
        )
    };
    Line::from(vec![
        Span::styled(
            format!("TIME {}", format_elapsed(game.total_seconds())),
            white,
        ),
        Span::raw("   "),
        Span::styled(format!("ATTEMPTS {}", game.attempts()), dim),
        Span::raw("   "),
        Span::styled(best_text, yellow),
    ])
}

fn render_fix_body(frame: &mut Frame, area: Rect, game: &DailyFix) {
    let dim = Style::new().fg(Color::DarkGray);
    let red = Style::new().fg(Color::Red);
    let white = Style::new().fg(Color::White);
    let yellow = Style::new().fg(Color::Yellow);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let bug = game.bug();
    for (i, line) in bug.lines.iter().enumerate() {
        let buggy = i == bug.buggy_line;
        let number = Span::styled(format!("{:>2} ", i + 1), dim);
        let marker = if buggy {
            Span::styled("BUG ▸ ", red)
        } else {
            Span::styled("      ", dim)
        };
        lines.push(Line::from(vec![
            number,
            marker,
            Span::styled(*line, if buggy { red } else { white }),
        ]));
    }
    if game.hint_shown() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("HINT  ", yellow),
            Span::styled(bug.hint, dim),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("fix ▸ "),
        Span::styled(format!("{}▌", game.input()), white),
    ]));

    let width = bug
        .lines
        .iter()
        .map(|line| line.chars().count() + 9)
        .chain(std::iter::once(game.input().chars().count() + 7))
        .chain(game.hint_shown().then(|| bug.hint.chars().count() + 6))
        .max()
        .unwrap_or(1)
        .min(area.width as usize) as u16;
    let height = (lines.len() as u16).min(area.height);
    let code_area = centered_fixed(area, width, height);
    frame.render_widget(Paragraph::new(lines), code_area);
}

// ---- overlays --------------------------------------------------------------

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
fn render_agent_paused(frame: &mut Frame, area: Rect, app: &App, reason: PauseReason) {
    let game_kind = app.game().map(ActiveGame::kind);
    let game_title = game_kind.map(GameKind::title).unwrap_or("Stack Overflow");
    let (status, verb_one, verb_many, detail, restart_hint) = match reason {
        PauseReason::NeedsInput => (
            AgentStatus::NeedsInput,
            "NEEDS YOU",
            "NEED YOU",
            format!("{game_title} paused automatically"),
            "Return to {agents}",
        ),
        PauseReason::Completed => (
            AgentStatus::Completed,
            "FINISHED",
            "FINISHED",
            "Your run has been preserved".to_string(),
            "Return to {agents}",
        ),
        PauseReason::Stopped => (
            AgentStatus::Stopped,
            "SESSION ENDED",
            "SESSIONS ENDED",
            "Your run has been preserved".to_string(),
            "Restart {agents} to resume",
        ),
    };
    // The agents that caused the pause. Prefer the recorded attribution:
    // the pause is sticky, so a completing agent may already be working
    // again while the run is still paused.
    let mut involved = app.pause_involved().to_vec();
    if involved.is_empty() {
        involved = app.agents_with_status(status);
    }
    let involved = involved;

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
        format!(
            "Score {}",
            format_score(app.game().map(ActiveGame::score).unwrap_or(0))
        ),
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

fn render_game_over(frame: &mut Frame, area: Rect, app: &App) {
    let Some(game) = app.game() else {
        return;
    };

    let mut lines: Vec<Line<'static>> = vec![
        Line::styled(
            "GAME OVER",
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
    ];
    match game {
        ActiveGame::StackOverflow(game) => {
            lines.push(Line::styled(
                format!("SCORE {}", format_score(game.score())),
                Style::new().fg(Color::White),
            ));
            lines.push(Line::styled(
                format!(
                    "HEIGHT {}   PERFECTS x{}",
                    game.height(),
                    game.perfect_streak()
                ),
                Style::new().fg(Color::DarkGray),
            ));
            lines.push(Line::styled(
                format!("TIME  {}", format_elapsed(game.elapsed())),
                Style::new().fg(Color::DarkGray),
            ));
        }
        ActiveGame::DailyPr(game) => {
            match game.state() {
                crate::game::daily_pr::PrState::Solved => {
                    lines.push(Line::styled(
                        format!("SOLVED IN {} GUESSES", game.guesses()),
                        Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
                    ));
                }
                crate::game::daily_pr::PrState::Failed => {
                    lines.push(Line::styled(
                        format!("THE WORD WAS {}", game.word()),
                        Style::new().fg(Color::White),
                    ));
                }
                crate::game::daily_pr::PrState::Playing => {}
            }
            lines.push(Line::styled(
                format!("TIME  {}", format_elapsed(game.elapsed())),
                Style::new().fg(Color::DarkGray),
            ));
        }
        ActiveGame::DailyFix(game) => {
            lines.push(Line::styled(
                format!("FIXED IN {}", format_elapsed(game.total_seconds())),
                Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::styled(
                format!(
                    "{} ATTEMPTS   {}",
                    game.attempts(),
                    if game.hint_shown() {
                        "HINT USED"
                    } else {
                        "NO HINT"
                    }
                ),
                Style::new().fg(Color::DarkGray),
            ));
            lines.push(Line::styled(
                game.bug().explainer,
                Style::new().fg(Color::DarkGray),
            ));
        }
    }
    if app.is_new_record() {
        lines.push(Line::styled(
            "NEW BEST!",
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        ));
    }
    if app.mvp_just_set() {
        lines.push(Line::styled(
            "MVP OF THE DAY!",
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
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

// ---- agent indicator -------------------------------------------------------

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

fn render_too_small(frame: &mut Frame, app: &App) {
    let (cols, rows) = app.terminal_size().unwrap_or((0, 0));
    let lines = vec![
        Line::styled(
            "TERMINAL TOO SMALL",
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::from("MVP requires at least 60×20."),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentKind;
    use crate::config::HighScoreStore;
    use crate::event::AppInput;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    fn test_store(name: &str) -> HighScoreStore {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mvp_ui_test_{}_{}_{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        HighScoreStore::load(dir.join("highscore.json"))
    }

    fn app_at(width: u16, height: u16) -> App {
        app_with_store(test_store("ui.json"), width, height)
    }

    fn app_with_store(store: HighScoreStore, width: u16, height: u16) -> App {
        let mut app = App::new(store);
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

    fn start_game_of(app: &mut App, kind: GameKind) {
        app.handle_input(AppInput::Confirm); // Menu → GameMenu
        while app.selected_kind() != kind {
            app.handle_input(AppInput::Down);
        }
        app.handle_input(AppInput::Confirm); // start
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
    fn menu_render_shows_logo_and_controls() {
        let app = app_at(100, 30);
        let buffer = render_buffer(&app, 100, 30);
        let text = all_text(&buffer);
        assert!(text.contains(LOGO[0].trim()), "logo missing:\n{text}");
        assert!(LOGO.iter().all(|line| line.is_ascii()));
        assert_eq!(
            LOGO.iter().map(|line| line.chars().count()).max(),
            Some(LOGO_WIDTH)
        );
        let rows: Vec<_> = text.lines().collect();
        let logo_y = rows
            .iter()
            .position(|row| row.contains(LOGO[0].trim()))
            .expect("logo row");
        let logo_x = rows[logo_y].find('_').expect("first logo cell") as u16;
        let logo_cell = &buffer[(logo_x, logo_y as u16)];
        assert_eq!(logo_cell.fg, Color::LightMagenta);
        assert!(logo_cell.modifier.contains(Modifier::BOLD));
        assert!(text.contains("MOST VALUED PROGRAMMER"));
        assert!(text.contains("PLAY"));
        assert!(text.contains("QUIT"));
        assert!(text.contains("[ I ] SIGN IN"));
    }

    #[test]
    fn leaderboard_renders_online_rank_and_offline_fallback() {
        let mut online = app_at(100, 30);
        let (commands, _received) = std::sync::mpsc::channel();
        online.configure_online(commands, Some("alex".into()));
        online.handle_input(AppInput::Leaderboard);
        let board = serde_json::from_value(serde_json::json!({
            "period": "daily",
            "from": "2026-08-20",
            "through": "2026-08-20",
            "game_id": null,
            "entries": [
                {
                    "rank": 1,
                    "user": {
                        "id": uuid::Uuid::new_v4(),
                        "username": "alice",
                        "display_name": "alice",
                        "avatar_url": null
                    },
                    "points": 9420
                },
                {
                    "rank": 2,
                    "user": {
                        "id": uuid::Uuid::new_v4(),
                        "username": "alex",
                        "display_name": "alex",
                        "avatar_url": null
                    },
                    "points": 9180
                }
            ]
        }))
        .unwrap();
        online.handle_online_event(crate::api::WorkerEvent::Leaderboard(Ok(board)));
        let text = all_text(&render_buffer(&online, 100, 30));
        assert!(text.contains("DAILY LEADERBOARD"));
        assert!(text.contains("alice"));
        assert!(text.contains("YOU: #2"));
        assert!(text.contains("ONLINE"));

        let mut offline = app_at(100, 30);
        offline.handle_input(AppInput::Leaderboard);
        let text = all_text(&render_buffer(&offline, 100, 30));
        assert!(text.contains("Global leaderboard unavailable."));
        assert!(text.contains("Local scores are still available."));
    }

    #[test]
    fn menu_render_uses_doh_logo_when_the_terminal_has_room() {
        let app = app_at(120, 40);
        let buffer = render_buffer(&app, 120, 40);
        let text = all_text(&buffer);
        assert!(text.contains(DOH_LOGO[0]), "doh logo missing:\n{text}");
        assert!(text.contains(DOH_LOGO[15]));
        assert!(DOH_LOGO.iter().all(|line| line.is_ascii()));
        assert_eq!(
            DOH_LOGO.iter().map(|line| line.chars().count()).max(),
            Some(DOH_LOGO_WIDTH)
        );

        let rows: Vec<_> = text.lines().collect();
        let logo_left_edges: Vec<_> = DOH_LOGO
            .iter()
            .map(|logo_row| {
                rows.iter()
                    .find(|row| row.contains(logo_row))
                    .and_then(|row| row.find(logo_row))
                    .expect("doh logo row")
            })
            .collect();
        assert!(
            logo_left_edges.windows(2).all(|edges| edges[0] == edges[1]),
            "doh logo rows must share a left edge: {logo_left_edges:?}"
        );
        let logo_y = rows
            .iter()
            .position(|row| row.contains(DOH_LOGO[0]))
            .expect("doh logo row");
        let logo_x = rows[logo_y].find('M').expect("first doh cell") as u16;
        let logo_cell = &buffer[(logo_x, logo_y as u16)];
        assert_eq!(logo_cell.fg, Color::LightMagenta);
        assert!(logo_cell.modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn menu_render_degrades_to_plain_title_when_narrow() {
        let app = app_at(100, 30);
        // The app gates menus at 60×20, so exercise the narrow branch by
        // drawing the menu directly into a tiny frame.
        let backend = TestBackend::new(10, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render_menu(frame, &app)).unwrap();
        let text = all_text(terminal.backend().buffer());
        assert!(text.contains("M V P"));
        assert!(text.contains("PLAY"));
    }

    #[test]
    fn menu_render_shows_the_player_name_and_mvp_of_the_day() {
        let mut app = app_with_store(test_store("mvpline.json"), 100, 30);
        app.open_name_prompt();
        app.handle_input(AppInput::Text('A'));
        app.handle_input(AppInput::Text('l'));
        app.handle_input(AppInput::Text('e'));
        app.handle_input(AppInput::Text('x'));
        app.handle_input(AppInput::Confirm);
        assert_eq!(app.player_name(), "Alex");

        let buffer = render_buffer(&app, 100, 30);
        let text = all_text(&buffer);
        assert!(text.contains("PLAYING AS Alex"), "menu must show the name");
        let rows: Vec<_> = text.lines().collect();
        let player_y = rows
            .iter()
            .position(|row| row.contains("PLAYING AS Alex"))
            .expect("player line");
        let name_x = rows[player_y].find("Alex").expect("player name") as u16;
        let name_cell = &buffer[(name_x, player_y as u16)];
        assert_eq!(name_cell.fg, Color::White);
        assert!(name_cell.modifier.contains(Modifier::BOLD));
        assert!(rows.last().unwrap().contains("[ I ] SIGN IN"));

        app.start_game();
        app.game_mut()
            .as_mut()
            .unwrap()
            .as_stack_overflow_mut()
            .unwrap()
            .debug_set_block(0.0);
        app.handle_input(AppInput::Jump); // one perfect drop: scores
        app.game_mut()
            .as_mut()
            .unwrap()
            .as_stack_overflow_mut()
            .unwrap()
            .debug_force_overflow();
        app.tick(std::time::Duration::from_millis(16));
        assert_eq!(app.state, AppState::GameOver);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("MVP OF THE DAY!"), "game over must crown");

        app.back_to_menu();
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(
            text.lines().any(|row| {
                row.contains("MVP OF THE DAY")
                    && row.contains("Alex")
                    && row.contains("Stack Overflow")
            }),
            "today's MVP must render on one line"
        );
    }

    #[test]
    fn signed_in_menu_advertises_account_action() {
        let mut app = app_at(100, 30);
        let (commands, _received) = std::sync::mpsc::channel();
        app.configure_online(commands, Some("alex".into()));
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("[ I ] ACCOUNT"));
        assert!(!text.contains("[ I ] SIGN IN"));
    }

    #[test]
    fn name_prompt_render_shows_the_input_line() {
        let mut app = app_at(100, 30);
        app.open_name_prompt();
        app.handle_input(AppInput::Text('A'));
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("WHO IS THE MVP?"));
        assert!(text.contains("A▌"), "typed name must render with cursor");
        assert!(text.contains("NAME REQUIRED TO PLAY"));
        assert!(!text.contains("SKIP"));
    }

    fn type_word(app: &mut App, word: &str) {
        for c in word.chars() {
            app.handle_input(AppInput::Text(c));
        }
        app.handle_input(AppInput::Confirm);
        app.tick(std::time::Duration::from_millis(16));
    }

    #[test]
    fn stack_overflow_render_shows_hud_tower_and_block() {
        let mut app = app_at(100, 30);
        app.start_game();
        let buf = render_buffer(&app, 100, 30);
        let text = all_text(&buf);
        assert!(text.contains("STACK OVERFLOW"));
        assert!(text.contains("SCORE"));
        assert!(text.contains("HEIGHT"));
        assert!(text.contains("BEST"));
        assert!(text.contains("SPACE drop"));
        // The base layer is centered on the playfield's ground row. Body
        // rows run 2..28 of a 30-row terminal, so the ground line sits at
        // row 27 and the base layer at 26.
        let cols = playfield_dims(100, 30).0 as i32;
        let left = (cols - crate::game::stack_overflow::START_WIDTH) / 2;
        let ground = 27;
        assert_eq!(buf[(left as u16, ground)].symbol(), "─", "ground line");
        for x in left..left + crate::game::stack_overflow::START_WIDTH {
            assert_eq!(
                buf[(x as u16, 26)].symbol(),
                "█",
                "tower base at column {x}"
            );
        }
    }

    #[test]
    fn stack_overflow_render_shows_the_sliding_block_and_arrow() {
        let mut app = app_at(100, 30);
        app.start_game();
        app.game_mut()
            .as_mut()
            .unwrap()
            .as_stack_overflow_mut()
            .unwrap()
            .debug_set_block(3.0);
        let buf = render_buffer(&app, 100, 30);
        let text = all_text(&buf);
        assert!(text.contains(">") || text.contains("<"), "direction arrow");
        // The block sits two rows above the ground (one spare row).
        let cols = playfield_dims(100, 30).0 as i32;
        let left = (cols - crate::game::stack_overflow::START_WIDTH) / 2;
        assert_eq!(buf[(left as u16 + 3, 25)].symbol(), "█");
    }

    #[test]
    fn daily_pr_render_shows_the_board_and_input() {
        let mut app = app_at(100, 30);
        start_game_of(&mut app, GameKind::DailyPr);
        let word = app.game().unwrap().as_daily_pr().unwrap().word();
        for c in word.chars() {
            app.handle_input(AppInput::Text(c));
        }
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("THE DAILY PR"));
        assert!(text.contains("COMMIT #"));
        assert!(text.contains("GUESS 0/6"));
        assert!(text.contains("TYPE GUESS"));
        assert!(text.contains(&format!("{}▌", word)), "typed word renders");
        let rows: Vec<_> = text.lines().collect();
        let input_y = rows
            .iter()
            .position(|row| row.contains(&format!("{}▌", word)))
            .expect("Daily PR input row");
        let input_x = rows[input_y].find(&format!("{}▌", word)).unwrap();
        assert!(input_x > 10, "board starts at x={input_x}");
        assert!(input_y > 5, "board starts at y={input_y}:\n{text}");

        type_word(&mut app, &word); // submit: solved
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("G"), "correct letters are marked");
    }

    #[test]
    fn daily_pr_fail_render_reveals_the_word() {
        let mut app = app_at(100, 30);
        start_game_of(&mut app, GameKind::DailyPr);
        let word = app.game().unwrap().as_daily_pr().unwrap().word();
        let filler = crate::game::daily_pr::GUESSES
            .iter()
            .find(|w| **w != word)
            .unwrap();
        for _ in 0..crate::game::daily_pr::MAX_GUESSES {
            type_word(&mut app, filler);
        }
        assert_eq!(app.state, AppState::GameOver);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(
            text.contains(&format!("THE WORD WAS {}", word)),
            "the failed PR reveals its word:\n{text}"
        );
    }

    #[test]
    fn daily_fix_render_shows_snippet_hint_and_input() {
        let mut app = app_at(100, 30);
        start_game_of(&mut app, GameKind::DailyFix);
        let bug = app.game().unwrap().as_daily_fix().unwrap().bug();
        let fix = bug.fix;
        // Two wrong attempts reveal the hint.
        for _ in 0..2 {
            for c in "definitely wrong".chars() {
                app.handle_input(AppInput::Text(c));
            }
            app.handle_input(AppInput::Confirm);
            app.tick(std::time::Duration::from_millis(16));
        }
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("THE DAILY FIX"));
        assert!(text.contains("TIME "));
        assert!(text.contains("ATTEMPTS 2"));
        assert!(text.contains("BUG ▸"), "broken line is marked:\n{text}");
        assert!(text.contains(bug.hint), "hint appears after two misses");
        assert!(text.contains("fix ▸"));
        let rows: Vec<_> = text.lines().collect();
        let code_y = rows
            .iter()
            .position(|row| row.contains("BUG ▸"))
            .expect("buggy code line");
        let code_x = rows[code_y].find("BUG ▸").unwrap();
        assert!(code_x > 10, "code must not hug the left edge");
        assert!(code_y > 5, "code must not hug the top edge");

        for c in fix.chars() {
            app.handle_input(AppInput::Text(c));
        }
        app.handle_input(AppInput::Confirm);
        app.tick(std::time::Duration::from_millis(16));
        assert_eq!(app.state, AppState::GameOver);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(
            text.contains("FIXED IN"),
            "game over shows the time:\n{text}"
        );
        let prefix: String = bug.explainer.chars().take(20).collect();
        assert!(
            text.contains(&prefix),
            "explainer shows the lesson (prefix of {prefix:?}):\n{text}"
        );
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
        app.game_mut()
            .as_mut()
            .unwrap()
            .as_stack_overflow_mut()
            .unwrap()
            .debug_force_overflow();
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
        app.game_mut()
            .as_mut()
            .unwrap()
            .as_stack_overflow_mut()
            .unwrap()
            .debug_force_overflow();
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

    // ---- game menu ----------------------------------------------------------

    #[test]
    fn game_menu_render_lists_all_three_games() {
        let mut app = app_at(100, 30);
        app.handle_input(AppInput::Confirm);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("CHOOSE YOUR GAME"));
        assert!(text.contains("Stack Overflow"));
        assert!(text.contains("don't overflow"));
        assert!(text.contains("The Daily PR"));
        assert!(text.contains("fewest guesses wins"));
        assert!(text.contains("The Daily Fix"));
        assert!(text.contains("fix it fastest"));
        assert!(text.contains("ENTER PLAY"));
        assert!(text.contains("ESC BACK"));
    }

    #[test]
    fn game_menu_marker_follows_the_selection() {
        let mut app = app_at(100, 30);
        app.handle_input(AppInput::Confirm);
        app.handle_input(AppInput::Down);
        assert_eq!(app.selected_kind(), GameKind::DailyPr);
        let text = all_text(&render_buffer(&app, 100, 30));
        assert!(text.contains("▶ The Daily PR"));
        assert!(!text.contains("▶ Stack Overflow"));
    }

    #[test]
    fn best_lines_speak_the_games_own_units() {
        assert_eq!(best_line(GameKind::StackOverflow, 4_820), "BEST 4,820");
        assert_eq!(best_line(GameKind::DailyPr, 6_000_000), "BEST 1 GUESSES");
        assert_eq!(best_line(GameKind::DailyPr, 0), "BEST 0");
        assert_eq!(best_line(GameKind::DailyFix, 999_984_000), "BEST 0:16");
    }
}
