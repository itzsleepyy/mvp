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
    /// N — open the name prompt from the menu.
    Rename,
    /// A printable character typed into the name prompt.
    Text(char),
    /// Backspace in the name prompt.
    Backspace,
    /// Q or Ctrl+C — quit the application.
    Quit,
    /// Terminal was resized.
    Resize(u16, u16),
}

/// Poll interval drives the render loop at roughly 60 FPS.
const POLL_INTERVAL: Duration = Duration::from_millis(16);

/// Waits briefly for the next terminal event. Returns `None` when the poll
/// window elapses so the caller can tick the simulation and render a frame.
///
/// With `text_mode` set (name prompt), printable characters become
/// [`AppInput::Text`], so names like "Sam" or "Hugh" are typeable even
/// though S, H and P mean something in a game.
pub fn next_input(text_mode: bool) -> std::io::Result<Option<AppInput>> {
    if !event::poll(POLL_INTERVAL)? {
        return Ok(None);
    }
    Ok(match event::read()? {
        Event::Key(key) => key_to_input(key, text_mode),
        Event::Resize(cols, rows) => Some(AppInput::Resize(cols, rows)),
        _ => None,
    })
}

fn key_to_input(key: event::KeyEvent, text_mode: bool) -> Option<AppInput> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(AppInput::Quit);
    }
    if key.kind == KeyEventKind::Release {
        return None;
    }
    if text_mode {
        return match key.code {
            KeyCode::Enter => Some(AppInput::Confirm),
            KeyCode::Esc => Some(AppInput::Back),
            KeyCode::Backspace => Some(AppInput::Backspace),
            KeyCode::Char(c) if c.is_control() => None,
            KeyCode::Char(c) => Some(AppInput::Text(c)),
            _ => None,
        };
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
        KeyCode::Char('n') | KeyCode::Char('N') => press.then_some(AppInput::Rename),
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
            key_to_input(key(KeyCode::Enter, KeyModifiers::NONE), false),
            Some(AppInput::Confirm)
        );
    }

    #[test]
    fn space_is_jump() {
        assert_eq!(
            key_to_input(key(KeyCode::Char(' '), KeyModifiers::NONE), false),
            Some(AppInput::Jump)
        );
    }

    #[test]
    fn arrows_are_discrete_navigation() {
        assert_eq!(
            key_to_input(key(KeyCode::Up, KeyModifiers::NONE), false),
            Some(AppInput::Up)
        );
        assert_eq!(
            key_to_input(key(KeyCode::Down, KeyModifiers::NONE), false),
            Some(AppInput::Down)
        );
        assert_eq!(key_to_input(release(KeyCode::Up), false), None);
        assert_eq!(key_to_input(release(KeyCode::Down), false), None);
    }

    #[test]
    fn h_and_s_drive_twenty_one() {
        assert_eq!(
            key_to_input(key(KeyCode::Char('h'), KeyModifiers::NONE), false),
            Some(AppInput::Hit)
        );
        assert_eq!(
            key_to_input(key(KeyCode::Char('H'), KeyModifiers::NONE), false),
            Some(AppInput::Hit)
        );
        assert_eq!(
            key_to_input(key(KeyCode::Char('s'), KeyModifiers::NONE), false),
            Some(AppInput::Stand)
        );
        assert_eq!(
            key_to_input(key(KeyCode::Char('S'), KeyModifiers::NONE), false),
            Some(AppInput::Stand)
        );
    }

    #[test]
    fn p_is_toggle_pause() {
        assert_eq!(
            key_to_input(key(KeyCode::Char('p'), KeyModifiers::NONE), false),
            Some(AppInput::TogglePause)
        );
        assert_eq!(
            key_to_input(key(KeyCode::Char('P'), KeyModifiers::NONE), false),
            Some(AppInput::TogglePause)
        );
    }

    #[test]
    fn r_is_restart() {
        assert_eq!(
            key_to_input(key(KeyCode::Char('r'), KeyModifiers::NONE), false),
            Some(AppInput::Restart)
        );
    }

    #[test]
    fn n_is_rename() {
        assert_eq!(
            key_to_input(key(KeyCode::Char('n'), KeyModifiers::NONE), false),
            Some(AppInput::Rename)
        );
        assert_eq!(
            key_to_input(key(KeyCode::Char('N'), KeyModifiers::NONE), false),
            Some(AppInput::Rename)
        );
    }

    #[test]
    fn esc_is_back() {
        assert_eq!(
            key_to_input(key(KeyCode::Esc, KeyModifiers::NONE), false),
            Some(AppInput::Back)
        );
    }

    #[test]
    fn q_is_quit() {
        assert_eq!(
            key_to_input(key(KeyCode::Char('q'), KeyModifiers::NONE), false),
            Some(AppInput::Quit)
        );
    }

    #[test]
    fn ctrl_c_is_quit() {
        assert_eq!(
            key_to_input(key(KeyCode::Char('c'), KeyModifiers::CONTROL), false),
            Some(AppInput::Quit)
        );
    }

    #[test]
    fn plain_c_is_ignored() {
        assert_eq!(
            key_to_input(key(KeyCode::Char('c'), KeyModifiers::NONE), false),
            None
        );
    }

    #[test]
    fn releases_are_ignored() {
        assert_eq!(key_to_input(release(KeyCode::Char('p')), false), None);
        assert_eq!(key_to_input(release(KeyCode::Char(' ')), false), None);
        assert_eq!(key_to_input(release(KeyCode::Esc), false), None);
    }

    // ---- text mode (name prompt) ------------------------------------------

    #[test]
    fn text_mode_maps_printable_chars_to_text() {
        assert_eq!(
            key_to_input(key(KeyCode::Char('s'), KeyModifiers::NONE), true),
            Some(AppInput::Text('s')),
            "S must be typeable even though it stands in a game"
        );
        assert_eq!(
            key_to_input(key(KeyCode::Char('A'), KeyModifiers::NONE), true),
            Some(AppInput::Text('A'))
        );
        assert_eq!(
            key_to_input(key(KeyCode::Char(' '), KeyModifiers::NONE), true),
            Some(AppInput::Text(' ')),
            "names may contain spaces"
        );
    }

    #[test]
    fn text_mode_keeps_enter_esc_backspace_and_quit() {
        assert_eq!(
            key_to_input(key(KeyCode::Enter, KeyModifiers::NONE), true),
            Some(AppInput::Confirm)
        );
        assert_eq!(
            key_to_input(key(KeyCode::Esc, KeyModifiers::NONE), true),
            Some(AppInput::Back)
        );
        assert_eq!(
            key_to_input(key(KeyCode::Backspace, KeyModifiers::NONE), true),
            Some(AppInput::Backspace)
        );
        assert_eq!(
            key_to_input(key(KeyCode::Char('c'), KeyModifiers::CONTROL), true),
            Some(AppInput::Quit)
        );
    }

    #[test]
    fn text_mode_ignores_control_and_navigation_keys() {
        assert_eq!(
            key_to_input(key(KeyCode::Up, KeyModifiers::NONE), true),
            None
        );
        assert_eq!(
            key_to_input(key(KeyCode::Char('\n'), KeyModifiers::NONE), true),
            None
        );
        assert_eq!(key_to_input(release(KeyCode::Char('a')), true), None);
    }
}
