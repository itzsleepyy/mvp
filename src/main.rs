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

use crate::agent::status::{AgentDisplay, AgentKind};
use crate::cli::{Command, IntegrationsCommand, ProviderCommand};

fn main() -> Result<(), Box<dyn Error>> {
    let cli = cli::Cli::parse();
    match cli.command {
        None | Some(Command::Play) => run_tui(
            cli.agent.map(|a| a.into_agent_display()),
            cli.no_auto_resume,
        ),
        Some(Command::AgentEvent { args }) => {
            // Used by agent hook commands. Never prints to stdout, never
            // launches a TUI, and exits 0 even when WaitState is not
            // running: a missing game must never disturb the agent.
            let (kind, event) =
                cli::parse_agent_event_args(&args).map_err(std::io::Error::other)?;
            crate::debug_log!("agent-event {} {event}", kind.id());
            match ipc::send_event(kind, event) {
                Ok(()) => crate::debug_log!("agent-event {} {event}: sent", kind.id()),
                Err(err) => {
                    crate::debug_log!("agent-event {} {event}: not sent ({err})", kind.id())
                }
            }
            Ok(())
        }
        Some(Command::Hook { agent, event }) => {
            // Bridge for hook systems whose protocol requires one JSON
            // object on stdout (Codex, Gemini). "{}" carries no decision,
            // no additional context: purely observational. Always exit 0 —
            // exit 2 blocks Codex turns and Gemini tools, which WaitState
            // must never do.
            let kind = agent.into_agent_kind();
            match ipc::send_event(kind, event) {
                Ok(()) => crate::debug_log!("hook {} {event}: sent", kind.id()),
                Err(err) => crate::debug_log!("hook {} {event}: not sent ({err})", kind.id()),
            }
            println!("{{}}");
            Ok(())
        }
        Some(Command::Claude { command }) => run_provider(AgentKind::ClaudeCode, command),
        Some(Command::Codex { command }) => run_provider(AgentKind::Codex, command),
        Some(Command::Gemini { command }) => run_provider(AgentKind::GeminiCli, command),
        Some(Command::Opencode { command }) => run_provider(AgentKind::OpenCode, command),
        Some(Command::Integrations { command }) => match command {
            None => {
                agent::integrations::run_overview();
                Ok(())
            }
            Some(IntegrationsCommand::Install { all }) => {
                agent::integrations::run_install(all);
                Ok(())
            }
            Some(IntegrationsCommand::Repair) => {
                agent::integrations::run_repair();
                Ok(())
            }
        },
    }
}

/// Dispatches one provider subcommand. Each provider handles its own
/// config format; failures stay local to that provider.
fn run_provider(kind: AgentKind, command: ProviderCommand) -> Result<(), Box<dyn Error>> {
    use crate::agent::{claude, codex, gemini, opencode};
    let to_err = |e: String| -> Box<dyn Error> { std::io::Error::other(e).into() };
    match (kind, command) {
        (AgentKind::ClaudeCode, ProviderCommand::Install) => claude::install().map_err(to_err),
        (AgentKind::ClaudeCode, ProviderCommand::Uninstall) => claude::uninstall().map_err(to_err),
        (AgentKind::ClaudeCode, ProviderCommand::Status) => {
            claude::status();
            Ok(())
        }
        (AgentKind::Codex, ProviderCommand::Install) => codex::install().map_err(to_err),
        (AgentKind::Codex, ProviderCommand::Uninstall) => codex::uninstall().map_err(to_err),
        (AgentKind::Codex, ProviderCommand::Status) => {
            codex::status();
            Ok(())
        }
        (AgentKind::GeminiCli, ProviderCommand::Install) => gemini::install().map_err(to_err),
        (AgentKind::GeminiCli, ProviderCommand::Uninstall) => gemini::uninstall().map_err(to_err),
        (AgentKind::GeminiCli, ProviderCommand::Status) => {
            gemini::status();
            Ok(())
        }
        (AgentKind::OpenCode, ProviderCommand::Install) => opencode::install().map_err(to_err),
        (AgentKind::OpenCode, ProviderCommand::Uninstall) => opencode::uninstall().map_err(to_err),
        (AgentKind::OpenCode, ProviderCommand::Status) => {
            opencode::status();
            Ok(())
        }
    }
}

fn run_tui(agent: Option<AgentDisplay>, no_auto_resume: bool) -> Result<(), Box<dyn Error>> {
    tui::install_panic_hook();
    let mut terminal = tui::init()?;
    let _guard = tui::TerminalGuard;
    let result = run(&mut terminal, agent, no_auto_resume);
    tui::restore()?;
    result
}

fn run(
    terminal: &mut tui::Tui,
    agent: Option<AgentDisplay>,
    no_auto_resume: bool,
) -> Result<(), Box<dyn Error>> {
    let mut app = app::App::new(config::HighScoreStore::discover());
    app.set_agent_display(agent);
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
            while let Some((kind, event)) = server.try_recv() {
                app.handle_agent_event(kind, event);
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
