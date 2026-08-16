use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

/// Input events as understood by the application. Keyboard mapping happens
/// here so game logic never depends on crossterm types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppInput {
    /// ENTER — confirm selection / start game.
    Confirm,
    /// SPACE — jump.
    Jump,
    /// H — hit (Twenty One).
    Hit,
    /// S — stand (Twenty One).
    Stand,
    /// ↑ — navigate menus; jumps while playing.
    Up,
    /// ↓ — navigate menus.
    Down,
    /// P — pause or resume.
    TogglePause,
    /// R — restart after a game over.
    Restart,
    /// ESC — back to menu.
    Back,
    /// Q or Ctrl+C — quit the application.
    Quit,
    /// Terminal was resized.
    Resize(u16, u16),
}

/// Poll interval drives the render loop at roughly 60 FPS.
const POLL_INTERVAL: Duration = Duration::from_millis(16);

/// Waits briefly for the next terminal event. Returns `None` when the poll
/// window elapses so the caller can tick the simulation and render a frame.
pub fn next_input() -> std::io::Result<Option<AppInput>> {
    if !event::poll(POLL_INTERVAL)? {
        return Ok(None);
    }
    Ok(match event::read()? {
        Event::Key(key) => key_to_input(key),
        Event::Resize(cols, rows) => Some(AppInput::Resize(cols, rows)),
        _ => None,
    })
}

fn key_to_input(key: event::KeyEvent) -> Option<AppInput> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(AppInput::Quit);
    }
    if key.kind == KeyEventKind::Release {
        return None;
    }
    // Repeats are accepted for continuous actions (holding SPACE re-jumps on
    // landing); discrete actions only respond to an initial press.
    let press = key.kind == KeyEventKind::Press;
    match key.code {
        KeyCode::Enter => Some(AppInput::Confirm),
        KeyCode::Char(' ') => Some(AppInput::Jump),
        KeyCode::Char('h') | KeyCode::Char('H') => press.then_some(AppInput::Hit),
        KeyCode::Char('s') | KeyCode::Char('S') => press.then_some(AppInput::Stand),
        KeyCode::Up => press.then_some(AppInput::Up),
        KeyCode::Down => press.then_some(AppInput::Down),
        KeyCode::Char('p') | KeyCode::Char('P') => press.then_some(AppInput::TogglePause),
        KeyCode::Char('r') | KeyCode::Char('R') => press.then_some(AppInput::Restart),
        KeyCode::Esc => press.then_some(AppInput::Back),
        KeyCode::Char('q') | KeyCode::Char('Q') => press.then_some(AppInput::Quit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventState, KeyModifiers};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn release(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn enter_is_confirm() {
        assert_eq!(
            key_to_input(key(KeyCode::Enter, KeyModifiers::NONE)),
            Some(AppInput::Confirm)
        );
    }

    #[test]
    fn space_is_jump() {
        assert_eq!(
            key_to_input(key(KeyCode::Char(' '), KeyModifiers::NONE)),
            Some(AppInput::Jump)
        );
    }

    #[test]
    fn arrows_are_discrete_navigation() {
        assert_eq!(
            key_to_input(key(KeyCode::Up, KeyModifiers::NONE)),
            Some(AppInput::Up)
        );
        assert_eq!(
            key_to_input(key(KeyCode::Down, KeyModifiers::NONE)),
            Some(AppInput::Down)
        );
        assert_eq!(key_to_input(release(KeyCode::Up)), None);
        assert_eq!(key_to_input(release(KeyCode::Down)), None);
    }

    #[test]
    fn h_and_s_drive_twenty_one() {
        assert_eq!(
            key_to_input(key(KeyCode::Char('h'), KeyModifiers::NONE)),
            Some(AppInput::Hit)
        );
        assert_eq!(
            key_to_input(key(KeyCode::Char('H'), KeyModifiers::NONE)),
            Some(AppInput::Hit)
        );
        assert_eq!(
            key_to_input(key(KeyCode::Char('s'), KeyModifiers::NONE)),
            Some(AppInput::Stand)
        );
        assert_eq!(
            key_to_input(key(KeyCode::Char('S'), KeyModifiers::NONE)),
            Some(AppInput::Stand)
        );
    }

    #[test]
    fn p_is_toggle_pause() {
        assert_eq!(
            key_to_input(key(KeyCode::Char('p'), KeyModifiers::NONE)),
            Some(AppInput::TogglePause)
        );
        assert_eq!(
            key_to_input(key(KeyCode::Char('P'), KeyModifiers::NONE)),
            Some(AppInput::TogglePause)
        );
    }

    #[test]
    fn r_is_restart() {
        assert_eq!(
            key_to_input(key(KeyCode::Char('r'), KeyModifiers::NONE)),
            Some(AppInput::Restart)
        );
    }

    #[test]
    fn esc_is_back() {
        assert_eq!(
            key_to_input(key(KeyCode::Esc, KeyModifiers::NONE)),
            Some(AppInput::Back)
        );
    }

    #[test]
    fn q_is_quit() {
        assert_eq!(
            key_to_input(key(KeyCode::Char('q'), KeyModifiers::NONE)),
            Some(AppInput::Quit)
        );
    }

    #[test]
    fn ctrl_c_is_quit() {
        assert_eq!(
            key_to_input(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(AppInput::Quit)
        );
    }

    #[test]
    fn plain_c_is_ignored() {
        assert_eq!(
            key_to_input(key(KeyCode::Char('c'), KeyModifiers::NONE)),
            None
        );
    }

    #[test]
    fn releases_are_ignored() {
        assert_eq!(key_to_input(release(KeyCode::Char('p'))), None);
        assert_eq!(key_to_input(release(KeyCode::Char(' '))), None);
        assert_eq!(key_to_input(release(KeyCode::Esc)), None);
    }
}
