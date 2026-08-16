//! Same-terminal Codex runner.
//!
//! WaitState owns the physical terminal and runs stock Codex in a child PTY.
//! Codex stays on the normal screen; the game temporarily uses the alternate
//! screen while hooks report that the agent is working.

use std::error::Error;
use std::ffi::OsString;
use std::io::{self, IsTerminal, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::cursor;
use crossterm::execute;
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::agent::event::AgentEvent;
use crate::agent::integrations::ProviderStatus;
use crate::agent::status::{AgentDisplay, AgentKind, AgentStatus};
use crate::app::{App, AppState};
use crate::config::{HighScoreStore, SettingsStore};
use crate::event::AppInput;
use crate::ipc;
use crate::ui;

const FRAME_TIME: Duration = Duration::from_millis(16);
const OUTPUT_SOFT_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Foreground {
    Codex,
    Game,
}

struct OutputGate {
    game_visible: bool,
    buffered: Vec<u8>,
    overflowed: bool,
}

struct PtyChild {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    gate: Arc<Mutex<OutputGate>>,
    output_done: Arc<AtomicBool>,
}

impl Drop for PtyChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// Runs stock Codex and switches the same terminal to WaitState while it works.
pub fn run(args: &[OsString]) -> Result<(), Box<dyn Error>> {
    if !matches!(super::status_state(), ProviderStatus::Current) {
        return Err(io::Error::other(
            "Codex hooks are not current; run `waitstate codex install` first",
        )
        .into());
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(
            io::Error::other("`waitstate codex run` requires an interactive terminal").into(),
        );
    }

    let (cols, rows) = crossterm::terminal::size()?;
    let socket_path = ipc::server::managed_socket_path();
    let server = ipc::IpcServer::start(socket_path.clone())?
        .ok_or_else(|| io::Error::other("managed WaitState socket is already in use"))?;
    crate::debug_log!("managed Codex socket: {}", socket_path.display());
    let mut session = spawn_codex(args, cols, rows, &socket_path)?;

    crate::tui::install_panic_hook();
    enable_raw_mode()?;
    let _guard = ManagedTerminalGuard;

    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let input = spawn_input_reader();
    let mut app = App::new(HighScoreStore::discover());
    app.set_agent_display(Some(AgentDisplay::Specific(AgentKind::Codex)));
    app.set_terminal_size(cols, rows);

    let mut foreground = Foreground::Codex;
    let mut settings = SettingsStore::discover();
    let mut auto_play = settings.codex_auto_play();
    let mut hidden_until_turn_end = false;
    let mut previous_size = (cols, rows);
    let mut last_frame = Instant::now();

    loop {
        while let Some((kind, event)) = server.try_recv() {
            crate::debug_log!("managed lifecycle {} {event}", kind.id());
            app.handle_agent_event(kind, event);
            match event {
                AgentEvent::Working if auto_play && !hidden_until_turn_end => {
                    prepare_game(&mut app);
                    show_game(&mut terminal, &session.gate)?;
                    foreground = Foreground::Game;
                }
                AgentEvent::NeedsInput | AgentEvent::Completed | AgentEvent::Stopped => {
                    show_codex(&mut terminal, &session.gate)?;
                    foreground = Foreground::Codex;
                    if matches!(event, AgentEvent::Completed | AgentEvent::Stopped) {
                        hidden_until_turn_end = false;
                    }
                }
                AgentEvent::Started | AgentEvent::Working => {}
            }
        }

        while let Ok(bytes) = input.try_recv() {
            crate::debug_log!("managed terminal input: {} bytes", bytes.len());
            match foreground {
                Foreground::Codex => {
                    if bytes.contains(&0x1d) {
                        auto_play = !auto_play;
                        settings.set_codex_auto_play(auto_play);
                        hidden_until_turn_end = false;
                        if auto_play && app.agent_aggregate() == AgentStatus::Working {
                            prepare_game(&mut app);
                            show_game(&mut terminal, &session.gate)?;
                            foreground = Foreground::Game;
                        }
                    } else {
                        session.writer.write_all(&bytes)?;
                        session.writer.flush()?;
                    }
                }
                Foreground::Game => {
                    if handle_game_bytes(&mut app, &bytes) {
                        if bytes.contains(&0x1d) {
                            auto_play = false;
                            settings.set_codex_auto_play(false);
                        } else {
                            hidden_until_turn_end = true;
                        }
                        show_codex(&mut terminal, &session.gate)?;
                        foreground = Foreground::Codex;
                        if bytes.contains(&0x03) {
                            session.writer.write_all(&[0x03])?;
                            session.writer.flush()?;
                        }
                    }
                }
            }
        }

        if session
            .gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .overflowed
        {
            auto_play = false;
            show_codex(&mut terminal, &session.gate)?;
            foreground = Foreground::Codex;
        }

        let size = crossterm::terminal::size()?;
        if size != previous_size {
            previous_size = size;
            session.master.resize(pty_size(size.0, size.1))?;
            app.set_terminal_size(size.0, size.1);
            terminal.autoresize()?;
        }

        if session.child.try_wait()?.is_some() {
            show_codex(&mut terminal, &session.gate)?;
            break;
        }

        if foreground == Foreground::Game {
            let now = Instant::now();
            app.tick(now.duration_since(last_frame));
            last_frame = now;
            terminal.draw(|frame| ui::render(frame, &app))?;
        } else {
            last_frame = Instant::now();
        }

        thread::sleep(FRAME_TIME);
    }

    for _ in 0..50 {
        if session.output_done.load(Ordering::Acquire) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

fn spawn_codex(
    args: &[OsString],
    cols: u16,
    rows: u16,
    socket_path: &std::path::Path,
) -> Result<PtyChild, Box<dyn Error>> {
    let pair = native_pty_system().openpty(pty_size(cols, rows))?;
    let mut command = CommandBuilder::new("codex");
    command.arg("--no-alt-screen");
    command.arg("-c");
    command.arg(format!(
        "shell_environment_policy.set.{}={}",
        ipc::server::SOCKET_ENV,
        serde_json::to_string(&socket_path.to_string_lossy())?
    ));
    command.args(args);
    command.cwd(std::env::current_dir()?);
    command.env(ipc::server::SOCKET_ENV, socket_path.as_os_str());

    let reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;
    let child = pair.slave.spawn_command(command)?;
    drop(pair.slave);

    let gate = Arc::new(Mutex::new(OutputGate {
        game_visible: false,
        buffered: Vec::new(),
        overflowed: false,
    }));
    let output_done = Arc::new(AtomicBool::new(false));
    let thread_gate = Arc::clone(&gate);
    let thread_done = Arc::clone(&output_done);
    thread::Builder::new()
        .name("waitstate-codex-output".into())
        .spawn(move || {
            copy_output(reader, &thread_gate);
            thread_done.store(true, Ordering::Release);
        })?;

    Ok(PtyChild {
        master: pair.master,
        writer,
        child,
        gate,
        output_done,
    })
}

fn copy_output(mut reader: Box<dyn Read + Send>, gate: &Arc<Mutex<OutputGate>>) {
    let mut bytes = [0_u8; 16 * 1024];
    loop {
        let count = match reader.read(&mut bytes) {
            Ok(0) => return,
            Ok(count) => count,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            #[cfg(unix)]
            Err(err) if err.raw_os_error() == Some(libc::EIO) => return,
            Err(_) => return,
        };
        let mut state = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.game_visible {
            state.buffered.extend_from_slice(&bytes[..count]);
            state.overflowed |= state.buffered.len() > OUTPUT_SOFT_LIMIT;
        } else {
            let mut stdout = io::stdout().lock();
            if stdout.write_all(&bytes[..count]).is_err() || stdout.flush().is_err() {
                return;
            }
        }
    }
}

fn show_game(terminal: &mut crate::tui::Tui, gate: &Arc<Mutex<OutputGate>>) -> io::Result<()> {
    let mut state = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.game_visible {
        return Ok(());
    }
    state.game_visible = true;
    if let Err(err) = execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        cursor::Hide,
        Clear(ClearType::All),
        cursor::MoveTo(0, 0)
    ) {
        state.game_visible = false;
        return Err(err);
    }
    Ok(())
}

fn show_codex(terminal: &mut crate::tui::Tui, gate: &Arc<Mutex<OutputGate>>) -> io::Result<()> {
    let mut state = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if !state.game_visible {
        return Ok(());
    }
    execute!(terminal.backend_mut(), LeaveAlternateScreen, cursor::Show)?;
    terminal.backend_mut().flush()?;
    if !state.buffered.is_empty() {
        let mut stdout = io::stdout().lock();
        stdout.write_all(&state.buffered)?;
        stdout.flush()?;
        state.buffered.clear();
    }
    state.game_visible = false;
    state.overflowed = false;
    Ok(())
}

fn prepare_game(app: &mut App) {
    match app.state {
        AppState::Menu | AppState::GameOver => app.start_game(),
        AppState::PausedAgent(_) => app.resume(),
        AppState::Playing | AppState::PausedManual => {}
    }
}

/// Returns true when the supervisor should restore Codex.
fn handle_game_bytes(app: &mut App, bytes: &[u8]) -> bool {
    if bytes.contains(&0x03) || bytes.contains(&0x1b) || bytes.contains(&0x1d) {
        app.pause();
        return true;
    }
    for byte in bytes {
        let input = match byte {
            b' ' => Some(AppInput::Jump),
            b'p' | b'P' => Some(AppInput::TogglePause),
            b'r' | b'R' => Some(AppInput::Restart),
            b'\r' | b'\n' => Some(AppInput::Confirm),
            b'q' | b'Q' => {
                app.pause();
                return true;
            }
            _ => None,
        };
        if let Some(input) = input {
            app.handle_input(input);
        }
    }
    false
}

fn spawn_input_reader() -> Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("waitstate-terminal-input".into())
        .spawn(move || {
            loop {
                match read_stdin_bytes(50) {
                    Ok(Some(bytes)) => {
                        if sender.send(bytes).is_err() {
                            return;
                        }
                    }
                    Ok(None) => continue,
                    Err(_) => return,
                }
            }
        })
        .expect("terminal input thread should start");
    receiver
}

#[cfg(unix)]
fn read_stdin_bytes(timeout_ms: i32) -> io::Result<Option<Vec<u8>>> {
    let mut descriptor = libc::pollfd {
        fd: libc::STDIN_FILENO,
        events: libc::POLLIN,
        revents: 0,
    };
    loop {
        // SAFETY: descriptor points to one valid pollfd for this call.
        let ready = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
        if ready == 0 {
            return Ok(None);
        }
        if ready < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        if descriptor.revents & libc::POLLIN == 0 {
            return Ok(None);
        }
        let mut bytes = vec![0_u8; 4096];
        // SAFETY: bytes owns a writable allocation of bytes.len() bytes.
        let count =
            unsafe { libc::read(libc::STDIN_FILENO, bytes.as_mut_ptr().cast(), bytes.len()) };
        if count < 0 {
            return Err(io::Error::last_os_error());
        }
        if count == 0 {
            return Ok(None);
        }
        bytes.truncate(count as usize);
        return Ok(Some(bytes));
    }
}

#[cfg(not(unix))]
fn read_stdin_bytes(_timeout_ms: i32) -> io::Result<Option<Vec<u8>>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "managed Codex mode is not yet supported on this platform",
    ))
}

fn pty_size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

struct ManagedTerminalGuard;

impl Drop for ManagedTerminalGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, cursor::Show);
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::new(HighScoreStore::load(std::env::temp_dir().join(format!(
            "waitstate_runner_test_{}.json",
            std::process::id()
        ))))
    }

    #[test]
    fn working_starts_or_resumes_the_managed_game() {
        let mut app = app();
        prepare_game(&mut app);
        assert_eq!(app.state, AppState::Playing);
        app.handle_agent_event(AgentKind::Codex, AgentEvent::Completed);
        assert!(matches!(app.state, AppState::PausedAgent(_)));
        prepare_game(&mut app);
        assert_eq!(app.state, AppState::Playing);
    }

    #[test]
    fn escape_preserves_and_hides_the_game() {
        let mut app = app();
        app.start_game();
        assert!(handle_game_bytes(&mut app, &[0x1b]));
        assert_eq!(app.state, AppState::PausedManual);
    }

    #[test]
    fn game_controls_still_reach_the_app() {
        let mut app = app();
        app.start_game();
        assert!(!handle_game_bytes(&mut app, b"p"));
        assert_eq!(app.state, AppState::PausedManual);
        assert!(!handle_game_bytes(&mut app, b"p"));
        assert_eq!(app.state, AppState::Playing);
    }
}
