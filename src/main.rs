mod app;
mod config;
mod event;
mod game;
mod tui;
mod ui;

use std::error::Error;
use std::time::Instant;

fn main() -> Result<(), Box<dyn Error>> {
    tui::install_panic_hook();
    let mut terminal = tui::init()?;
    let _guard = tui::TerminalGuard;
    let result = run(&mut terminal);
    tui::restore()?;
    result
}

fn run(terminal: &mut tui::Tui) -> Result<(), Box<dyn Error>> {
    let mut app = app::App::new(config::HighScoreStore::discover());
    let size = terminal.size()?;
    app.set_terminal_size(size.width, size.height);

    let mut last_frame = Instant::now();
    while !app.should_quit() {
        while let Some(input) = event::next_input()? {
            app.handle_input(input);
            if app.should_quit() {
                break;
            }
        }
        let now = Instant::now();
        app.tick(now.duration_since(last_frame));
        last_frame = now;
        terminal.draw(|frame| ui::render(frame, &app))?;
    }
    Ok(())
}
