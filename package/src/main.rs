mod agent;
mod api;
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
use std::future::Future;
use std::time::Instant;

use clap::Parser;

use crate::agent::status::{AgentDisplay, AgentKind};
use crate::api::{
    ApiClient, ApiWorker, AuthFlow, GameId, KeyringCredentialStore, Leaderboard,
    LeaderboardRequest, Session, SessionManager, WorkerCommand,
};
use crate::cli::{Command, IntegrationsCommand, LeaderboardKind, ProviderCommand};

fn main() -> Result<(), Box<dyn Error>> {
    let cli = cli::Cli::parse();
    match cli.command {
        None | Some(Command::Play) => run_tui(
            cli.agent.map(|a| a.into_agent_display()),
            cli.no_auto_resume,
        ),
        Some(Command::AgentEvent { args }) => {
            // Used by agent hook commands. Never prints to stdout, never
            // launches a TUI, and exits 0 even when MVP is not
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
            // exit 2 blocks Codex turns and Gemini tools, which MVP
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
        Some(Command::Login) => run_online(run_login()),
        Some(Command::Logout) => run_online(run_logout()),
        Some(Command::Whoami) => run_online(run_whoami()),
        Some(Command::Profile) => run_online(run_profile()),
        Some(Command::Leaderboard { board }) => run_online(run_leaderboard(board)),
    }
}

fn run_online<F>(future: F) -> Result<(), Box<dyn Error>>
where
    F: Future<Output = Result<(), Box<dyn Error>>>,
{
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(future)
}

async fn run_login() -> Result<(), Box<dyn Error>> {
    let client = ApiClient::from_env()?;
    let credentials = KeyringCredentialStore::for_origin(client.origin());
    let manager = SessionManager::new(client, credentials);
    if let Some(session) = manager.load()? {
        println!("Already signed in as GitHub @{}.", session.user.username);
        return Ok(());
    }

    let flow = manager.start_github(false).await?;
    println!("Open {}", flow.verification_uri);
    println!("Enter code: {}", flow.user_code);
    if open::that(&flow.verification_uri).is_err() {
        println!("The browser could not be opened automatically.");
    }
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(flow.expires_in);
    let mut delay = std::time::Duration::from_secs(flow.interval.max(1));
    loop {
        tokio::time::sleep(delay).await;
        if tokio::time::Instant::now() >= deadline {
            return Err(
                std::io::Error::other("GitHub login expired; run `mvp login` again").into(),
            );
        }
        match manager.poll_once(&flow).await? {
            AuthFlow::Pending { retry_after } => delay = retry_after,
            AuthFlow::Complete(session) => {
                println!("Signed in as GitHub @{}.", session.user.username);
                return Ok(());
            }
        }
    }
}

async fn run_logout() -> Result<(), Box<dyn Error>> {
    let client = ApiClient::from_env()?;
    let credentials = KeyringCredentialStore::for_origin(client.origin());
    let manager = SessionManager::new(client, credentials);
    let session = manager.load().ok().flatten();
    let revoked = manager.logout(session.as_ref()).await?;
    if revoked {
        println!("Signed out.");
    } else {
        println!("Local session removed. Server revocation could not be confirmed.");
    }
    Ok(())
}

fn require_session(
    manager: &SessionManager<KeyringCredentialStore>,
) -> Result<Session, Box<dyn Error>> {
    manager
        .load()?
        .ok_or_else(|| std::io::Error::other("Not signed in. Run `mvp login` first.").into())
}

async fn run_whoami() -> Result<(), Box<dyn Error>> {
    let client = ApiClient::from_env()?;
    let credentials = KeyringCredentialStore::for_origin(client.origin());
    let manager = SessionManager::new(client.clone(), credentials);
    let local_name = config::HighScoreStore::discover().player_name().to_string();
    println!("{local_name}");
    let Some(session) = manager.load()? else {
        println!("GitHub: not signed in");
        println!("Global rank: unranked");
        return Ok(());
    };
    let profile = client.me(&session).await?;
    println!("GitHub: @{}", profile.username);
    println!("Global rank: {}", format_rank(profile.stats.global_rank));
    Ok(())
}

async fn run_profile() -> Result<(), Box<dyn Error>> {
    let client = ApiClient::from_env()?;
    let credentials = KeyringCredentialStore::for_origin(client.origin());
    let manager = SessionManager::new(client.clone(), credentials);
    let session = require_session(&manager)?;
    let profile = client.me(&session).await?;
    let local_name = config::HighScoreStore::discover().player_name().to_string();
    println!("{local_name}");
    println!("GitHub @{}", profile.username);
    println!();
    println!("Daily rank       {}", format_rank(profile.stats.daily_rank));
    println!(
        "Weekly rank      {}",
        format_rank(profile.stats.weekly_rank)
    );
    println!(
        "Global rank      {}",
        format_rank(profile.stats.global_rank)
    );
    println!(
        "Best Stack       {}",
        format_number(profile.stats.best_stack.max(0) as u64)
    );
    println!("Daily PR streak  {}", profile.stats.daily_pr_streak);
    println!(
        "Daily Fix        {}/7 this week",
        profile.stats.daily_fix_this_week
    );
    Ok(())
}

async fn run_leaderboard(board: LeaderboardKind) -> Result<(), Box<dyn Error>> {
    let client = ApiClient::from_env()?;
    let request = leaderboard_request(board, 50);
    let leaderboard = client.leaderboard(request).await?;
    let credentials = KeyringCredentialStore::for_origin(client.origin());
    let username = SessionManager::new(client, credentials)
        .load()
        .ok()
        .flatten()
        .map(|session| session.user.username);
    print_leaderboard(board, &leaderboard, username.as_deref());
    Ok(())
}

fn leaderboard_request(board: LeaderboardKind, limit: u8) -> LeaderboardRequest {
    match board {
        LeaderboardKind::Daily => LeaderboardRequest::Daily { date: None, limit },
        LeaderboardKind::Weekly => LeaderboardRequest::Weekly { limit },
        LeaderboardKind::AllTime => LeaderboardRequest::AllTime { limit },
        LeaderboardKind::StackOverflow => LeaderboardRequest::Game {
            game: GameId::StackOverflow,
            limit,
        },
        LeaderboardKind::DailyPr => LeaderboardRequest::Game {
            game: GameId::DailyPr,
            limit,
        },
        LeaderboardKind::DailyFix => LeaderboardRequest::Game {
            game: GameId::DailyFix,
            limit,
        },
    }
}

fn print_leaderboard(board: LeaderboardKind, leaderboard: &Leaderboard, username: Option<&str>) {
    let title = match board {
        LeaderboardKind::Daily => "Daily",
        LeaderboardKind::Weekly => "Weekly",
        LeaderboardKind::AllTime => "All-Time",
        LeaderboardKind::StackOverflow => "Stack Overflow",
        LeaderboardKind::DailyPr => "The Daily PR",
        LeaderboardKind::DailyFix => "The Daily Fix",
    };
    println!("MVP - {title} Leaderboard");
    println!();
    println!("  #   Programmer                 MVP");
    for entry in &leaderboard.entries {
        println!(
            "{:>3}   {:<22} {:>10}",
            entry.rank,
            entry.user.username.chars().take(22).collect::<String>(),
            format_number(entry.points.max(0) as u64)
        );
    }
    if let Some(username) = username {
        let rank = leaderboard
            .entries
            .iter()
            .find(|entry| entry.user.username.eq_ignore_ascii_case(username))
            .map(|entry| format_rank(Some(entry.rank)))
            .unwrap_or_else(|| "outside top 50".to_string());
        println!();
        println!("You: {rank}");
    }
}

fn format_rank(rank: Option<u32>) -> String {
    rank.map_or_else(|| "unranked".to_string(), |rank| format!("#{rank}"))
}

fn format_number(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            output.push(',');
        }
        output.push(character);
    }
    output
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
    let online = start_online_worker();
    tui::install_panic_hook();
    let mut terminal = tui::init()?;
    let _guard = tui::TerminalGuard;
    let result = run(&mut terminal, agent, no_auto_resume, online);
    tui::restore()?;
    result
}

fn run(
    terminal: &mut tui::Tui,
    agent: Option<AgentDisplay>,
    no_auto_resume: bool,
    mut online: Option<(ApiWorker, Option<String>)>,
) -> Result<(), Box<dyn Error>> {
    let mut app = app::App::new(config::HighScoreStore::discover());
    app.set_agent_display(agent);
    app.set_agent_auto_resume(!no_auto_resume);
    if let Some((worker, username)) = &online {
        app.configure_online(worker.commands(), username.clone());
        if username.is_some() {
            let _ = worker.commands().send(WorkerCommand::RetryPending);
        }
    }
    let size = terminal.size()?;
    app.set_terminal_size(size.width, size.height);
    if !app.has_player_name() {
        app.open_name_prompt();
    }

    let server = match ipc::IpcServer::start(ipc::socket_path()) {
        Ok(Some(server)) => {
            crate::debug_log!("IPC server listening at {}", server.socket_path().display());
            Some(server)
        }
        Ok(None) => {
            crate::debug_log!("another MVP instance owns the IPC socket");
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
        if let Some((worker, _)) = &online {
            while let Ok(event) = worker.try_event() {
                app.handle_online_event(event);
            }
        }
        while let Some(input) = event::next_input(app.text_mode())? {
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
    if let Some((worker, _)) = online.take() {
        worker.shutdown();
    }
    Ok(())
}

fn start_online_worker() -> Option<(ApiWorker, Option<String>)> {
    let client = match ApiClient::from_env() {
        Ok(client) => client,
        Err(error) => {
            crate::debug_log!("online API disabled: {error}");
            return None;
        }
    };
    let credentials = KeyringCredentialStore::for_origin(client.origin());
    let session = SessionManager::new(client.clone(), credentials)
        .load()
        .unwrap_or_else(|error| {
            crate::debug_log!("online session unavailable: {error}");
            None
        });
    let username = session
        .as_ref()
        .map(|session| session.user.username.clone());
    match ApiWorker::spawn_default(client, session) {
        Ok(worker) => Some((worker, username)),
        Err(error) => {
            crate::debug_log!("online worker unavailable: {error}");
            None
        }
    }
}
