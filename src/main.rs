mod agent;
mod app;
mod cli;
mod config;
mod event;
mod game;
mod ipc;
mod log;
mod tui;
mod ui;

use std::error::Error;
use std::time::Instant;

use clap::Parser;

use crate::agent::status::AgentKind;
use crate::cli::{ClaudeCommand, Command};

fn main() -> Result<(), Box<dyn Error>> {
    let cli = cli::Cli::parse();
    match cli.command {
        None | Some(Command::Play) => {
            run_tui(cli.agent.map(|a| a.into_agent_kind()), cli.no_auto_resume)
        }
        Some(Command::AgentEvent { event }) => {
            // Used by Claude Code hooks. Never prints to stdout, never
            // launches a TUI, and exits 0 even when WaitState is not
            // running: a missing game must never disturb the agent.
            crate::debug_log!("agent-event {event}");
            match ipc::send_event(event) {
                Ok(()) => crate::debug_log!("agent-event {event}: sent"),
                Err(err) => crate::debug_log!("agent-event {event}: not sent ({err})"),
            }
            Ok(())
        }
        Some(Command::Claude { command }) => match command {
            ClaudeCommand::Install => agent::claude::install()
                .map_err(|e| -> Box<dyn Error> { std::io::Error::other(e).into() }),
            ClaudeCommand::Uninstall => agent::claude::uninstall()
                .map_err(|e| -> Box<dyn Error> { std::io::Error::other(e).into() }),
            ClaudeCommand::Status => {
                agent::claude::status();
                Ok(())
            }
        },
    }
}

fn run_tui(agent: Option<AgentKind>, no_auto_resume: bool) -> Result<(), Box<dyn Error>> {
    tui::install_panic_hook();
    let mut terminal = tui::init()?;
    let _guard = tui::TerminalGuard;
    let result = run(&mut terminal, agent, no_auto_resume);
    tui::restore()?;
    result
}

fn run(
    terminal: &mut tui::Tui,
    agent: Option<AgentKind>,
    no_auto_resume: bool,
) -> Result<(), Box<dyn Error>> {
    let mut app = app::App::new(config::HighScoreStore::discover());
    if let Some(kind) = agent {
        app.set_agent_kind(kind);
    }
    app.set_agent_auto_resume(!no_auto_resume);
    let size = terminal.size()?;
    app.set_terminal_size(size.width, size.height);

    let server = match ipc::IpcServer::start(ipc::socket_path()) {
        Ok(Some(server)) => {
            crate::debug_log!("IPC server listening at {}", server.socket_path().display());
            Some(server)
        }
        Ok(None) => {
            crate::debug_log!("another WaitState instance owns the IPC socket");
            None
        }
        Err(err) => {
            crate::debug_log!("IPC unavailable ({err}); running standalone");
            None
        }
    };

    let mut last_frame = Instant::now();
    while !app.should_quit() {
        // Drain agent events on the main loop so application state stays
        // single-threaded.
        if let Some(server) = &server {
            while let Some(event) = server.try_recv() {
                app.handle_agent_event(event);
            }
        }
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
