use std::io::{self, Stdout};

use crossterm::cursor;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Switches the terminal into raw mode with an alternate screen and a hidden
/// cursor. If any step fails, whatever was already switched on is restored.
pub fn init() -> io::Result<Tui> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    if let Err(err) = execute!(stdout, EnterAlternateScreen, cursor::Hide) {
        let _ = disable_raw_mode();
        return Err(err);
    }
    let terminal = match Terminal::new(CrosstermBackend::new(stdout)) {
        Ok(terminal) => terminal,
        Err(err) => {
            let _ = restore();
            return Err(err);
        }
    };
    Ok(terminal)
}

/// Restores the terminal to its normal state. Safe to call more than once.
pub fn restore() -> io::Result<()> {
    let mut stdout = io::stdout();
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen, cursor::Show)
}

/// Installs a panic hook that restores the terminal before unwinding, so a
/// panic never leaves the user's terminal in raw mode.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        default_hook(info);
    }));
}

/// RAII guard that restores the terminal when dropped, as a safety net on top
/// of the explicit restore in `main`.
pub struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = restore();
    }
}
