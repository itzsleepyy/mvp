//! Command-line interface.
//!
//! ```text
//! mvp                              run the game (default)
//! mvp play                         run the game
//! mvp --agent codex                run the game, show Codex status
//! mvp --agent auto                 run the game, follow live events
//! mvp agent-event [AGENT] EVENT    send one lifecycle event (used by
//!                                        agent hooks; never launches a TUI,
//!                                        never prints to stdout)
//! mvp hook AGENT EVENT             bridge for Codex/Gemini hooks:
//!                                        sends the event and prints "{}"
//!                                        (their protocols require JSON
//!                                        output; hidden from help)
//! mvp claude install|uninstall|status
//! mvp codex install|uninstall|status
//! mvp gemini install|uninstall|status
//! mvp opencode install|uninstall|status
//! mvp integrations                 show all integrations at once
//! mvp integrations install         install for detected agents
//! mvp integrations install --all   install every supported agent
//! mvp integrations repair          fix missing/outdated pieces
//! mvp login                        sign in with GitHub
//! mvp logout                       revoke the local online session
//! mvp whoami                       show local and online identity
//! mvp profile                      show implemented online statistics
//! mvp leaderboard [BOARD]          show a global leaderboard
//! ```

use clap::{Parser, Subcommand, ValueEnum};

use crate::agent::event::AgentEvent;
use crate::agent::status::{AgentDisplay, AgentKind};

#[derive(Debug, Parser)]
#[command(
    name = "mvp",
    version,
    about = "Most Valued Programmer — a competitive terminal arcade for the time between prompts",
    long_about = None
)]
pub struct Cli {
    /// Coding agent to display in the UI ("claude", "codex", "gemini",
    /// "opencode" or "auto"). Without this the agent indicator appears once
    /// the first lifecycle event arrives.
    #[arg(long, value_name = "AGENT")]
    pub agent: Option<AgentKindArg>,

    /// Never auto-resume a run the agent paused; the developer always
    /// resumes those runs manually with ENTER.
    #[arg(long)]
    pub no_auto_resume: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the game (the default command)
    Play,
    /// Send a lifecycle event to the running MVP instance
    AgentEvent {
        /// The lifecycle event, optionally prefixed by the agent:
        /// `agent-event working` or `agent-event codex working`
        #[arg(value_name = "AGENT|EVENT", num_args = 1..=2, required = true)]
        args: Vec<String>,
    },
    /// Internal bridge for hooks that require JSON output (Codex, Gemini)
    #[command(hide = true)]
    Hook {
        /// The originating agent
        #[arg(value_name = "AGENT")]
        agent: AgentKindArg,
        /// The lifecycle event
        #[arg(value_parser = clap::value_parser!(AgentEvent))]
        event: AgentEvent,
    },
    /// Manage the Claude Code integration
    Claude {
        #[command(subcommand)]
        command: ProviderCommand,
    },
    /// Manage the Codex integration
    Codex {
        #[command(subcommand)]
        command: ProviderCommand,
    },
    /// Manage the Gemini CLI integration
    Gemini {
        #[command(subcommand)]
        command: ProviderCommand,
    },
    /// Manage the OpenCode integration
    Opencode {
        #[command(subcommand)]
        command: ProviderCommand,
    },
    /// Show or manage every supported integration at once
    Integrations {
        #[command(subcommand)]
        command: Option<IntegrationsCommand>,
    },
    /// Sign in to MVP with GitHub Device Flow
    Login,
    /// Revoke the MVP session and remove it from the credential store
    Logout,
    /// Show the local player and linked GitHub identity
    Whoami,
    /// Show online ranks and game statistics
    Profile,
    /// Show a global MVP leaderboard (daily by default)
    Leaderboard {
        /// Board to display
        #[arg(value_enum, default_value_t = LeaderboardKind::Daily)]
        board: LeaderboardKind,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum LeaderboardKind {
    #[default]
    Daily,
    Weekly,
    AllTime,
    StackOverflow,
    DailyPr,
    DailyFix,
}

#[derive(Debug, Subcommand)]
pub enum ProviderCommand {
    /// Merge MVP hooks/plugin into the agent configuration (idempotent)
    Install,
    /// Remove only MVP-owned hooks/plugin files
    Uninstall,
    /// Show integration status
    Status,
}

#[derive(Debug, Subcommand)]
pub enum IntegrationsCommand {
    /// Install integrations for detected agents (or every agent with --all)
    Install {
        /// Also install integrations for agents that are not installed
        #[arg(long)]
        all: bool,
    },
    /// Repair missing or outdated MVP integration pieces
    Repair,
}

/// CLI spelling of an agent kind (`--agent codex`).
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum AgentKindArg {
    Claude,
    Codex,
    Gemini,
    Opencode,
    Auto,
}

impl AgentKindArg {
    pub fn into_agent_kind(self) -> AgentKind {
        match self {
            Self::Claude => AgentKind::ClaudeCode,
            Self::Codex => AgentKind::Codex,
            Self::Gemini => AgentKind::GeminiCli,
            Self::Opencode => AgentKind::OpenCode,
            Self::Auto => unreachable!("auto is a display mode, not an agent"),
        }
    }

    pub fn into_agent_display(self) -> AgentDisplay {
        match self {
            Self::Auto => AgentDisplay::Auto,
            other => AgentDisplay::Specific(other.into_agent_kind()),
        }
    }
}

/// Parses `agent-event` arguments: `[AGENT] EVENT`. The agent defaults to
/// claude so hooks installed before multi-agent support keep working.
pub fn parse_agent_event_args(args: &[String]) -> Result<(AgentKind, AgentEvent), String> {
    match args {
        [event] => Ok((
            AgentKind::ClaudeCode,
            event.parse::<AgentEvent>().map_err(|e| e.to_string())?,
        )),
        [agent, event] => {
            let kind = agent
                .parse::<AgentKind>()
                .map_err(|_| format!("unknown agent {agent:?}"))?;
            let event = event.parse::<AgentEvent>().map_err(|e| e.to_string())?;
            Ok((kind, event))
        }
        _ => Err("expected: agent-event [AGENT] EVENT".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_invocation_runs_the_game() {
        let cli = parse(&["mvp"]);
        assert!(cli.agent.is_none());
        assert!(cli.command.is_none());
        assert!(!cli.no_auto_resume);
    }

    #[test]
    fn cli_name_and_help_use_mvp_branding() {
        let mut command = Cli::command();
        assert_eq!(command.get_name(), "mvp");
        let help = command.render_long_help().to_string();
        assert!(help.contains("Most Valued Programmer"));
        assert!(!help.to_ascii_lowercase().contains("waitstate"));
    }

    #[test]
    fn no_auto_resume_flag_parses() {
        let cli = parse(&["mvp", "--no-auto-resume"]);
        assert!(cli.no_auto_resume);
    }

    #[test]
    fn play_command_is_explicit() {
        let cli = parse(&["mvp", "play"]);
        assert!(matches!(cli.command, Some(Command::Play)));
    }

    #[test]
    fn agent_event_parses_every_event_name() {
        for (name, event) in [
            ("started", AgentEvent::Started),
            ("working", AgentEvent::Working),
            ("needs-input", AgentEvent::NeedsInput),
            ("completed", AgentEvent::Completed),
            ("stopped", AgentEvent::Stopped),
        ] {
            let cli = parse(&["mvp", "agent-event", name]);
            match cli.command {
                Some(Command::AgentEvent { args }) => {
                    assert_eq!(
                        parse_agent_event_args(&args),
                        Ok((AgentKind::ClaudeCode, event))
                    );
                }
                other => panic!("unexpected command for {name}: {other:?}"),
            }
        }
    }

    #[test]
    fn agent_event_accepts_an_explicit_agent() {
        for (name, kind) in [
            ("claude", AgentKind::ClaudeCode),
            ("codex", AgentKind::Codex),
            ("gemini", AgentKind::GeminiCli),
            ("opencode", AgentKind::OpenCode),
        ] {
            let cli = parse(&["mvp", "agent-event", name, "working"]);
            match cli.command {
                Some(Command::AgentEvent { args }) => {
                    assert_eq!(
                        parse_agent_event_args(&args),
                        Ok((kind, AgentEvent::Working))
                    );
                }
                other => panic!("unexpected command for {name}: {other:?}"),
            }
        }
    }

    #[test]
    fn agent_event_rejects_unknown_names() {
        assert!(parse_agent_event_args(&["explode".into()]).is_err());
        assert!(parse_agent_event_args(&["warp".into(), "working".into()]).is_err());
        assert!(parse_agent_event_args(&["codex".into(), "explode".into()]).is_err());
        assert!(parse_agent_event_args(&[]).is_err());
        assert!(parse_agent_event_args(&["a".into(), "b".into(), "c".into()]).is_err());
        // Clap itself still requires at least one argument.
        assert!(Cli::try_parse_from(["mvp", "agent-event"]).is_err());
    }

    #[test]
    fn hook_command_parses_agent_and_event() {
        let cli = parse(&["mvp", "hook", "codex", "working"]);
        match cli.command {
            Some(Command::Hook { agent, event }) => {
                assert_eq!(agent.into_agent_kind(), AgentKind::Codex);
                assert_eq!(event, AgentEvent::Working);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn agent_flag_parses_every_agent_and_auto() {
        let cli = parse(&["mvp", "--agent", "claude"]);
        assert_eq!(cli.agent.unwrap().into_agent_kind(), AgentKind::ClaudeCode);
        let cli = parse(&["mvp", "--agent", "codex"]);
        assert_eq!(cli.agent.unwrap().into_agent_kind(), AgentKind::Codex);
        let cli = parse(&["mvp", "--agent", "gemini"]);
        assert_eq!(cli.agent.unwrap().into_agent_kind(), AgentKind::GeminiCli);
        let cli = parse(&["mvp", "--agent", "opencode"]);
        assert_eq!(cli.agent.unwrap().into_agent_kind(), AgentKind::OpenCode);
        let cli = parse(&["mvp", "--agent", "auto"]);
        assert!(matches!(
            cli.agent.unwrap().into_agent_display(),
            AgentDisplay::Auto
        ));
        assert!(Cli::try_parse_from(["mvp", "--agent", "warp"]).is_err());
    }

    #[test]
    fn provider_subcommands_parse() {
        for name in ["claude", "codex", "gemini", "opencode"] {
            for verb in ["install", "uninstall", "status"] {
                let cli = parse(&["mvp", name, verb]);
                let ok = match &cli.command {
                    Some(Command::Claude { command }) => matches!(
                        command,
                        ProviderCommand::Install
                            | ProviderCommand::Uninstall
                            | ProviderCommand::Status
                    ),
                    Some(Command::Codex { command }) => matches!(
                        command,
                        ProviderCommand::Install
                            | ProviderCommand::Uninstall
                            | ProviderCommand::Status
                    ),
                    Some(Command::Gemini { command }) => matches!(
                        command,
                        ProviderCommand::Install
                            | ProviderCommand::Uninstall
                            | ProviderCommand::Status
                    ),
                    Some(Command::Opencode { command }) => matches!(
                        command,
                        ProviderCommand::Install
                            | ProviderCommand::Uninstall
                            | ProviderCommand::Status
                    ),
                    _ => false,
                };
                assert!(ok, "{name} {verb} should parse");
            }
        }
        assert!(Cli::try_parse_from(["mvp", "codex", "explode"]).is_err());
    }

    #[test]
    fn integrations_commands_parse() {
        let cli = parse(&["mvp", "integrations"]);
        assert!(matches!(
            cli.command,
            Some(Command::Integrations { command: None })
        ));
        let cli = parse(&["mvp", "integrations", "install"]);
        match cli.command {
            Some(Command::Integrations {
                command: Some(IntegrationsCommand::Install { all }),
            }) => assert!(!all),
            other => panic!("unexpected: {other:?}"),
        }
        let cli = parse(&["mvp", "integrations", "install", "--all"]);
        match cli.command {
            Some(Command::Integrations {
                command: Some(IntegrationsCommand::Install { all }),
            }) => assert!(all),
            other => panic!("unexpected: {other:?}"),
        }
        let cli = parse(&["mvp", "integrations", "repair"]);
        assert!(matches!(
            cli.command,
            Some(Command::Integrations {
                command: Some(IntegrationsCommand::Repair)
            })
        ));
    }

    #[test]
    fn online_commands_parse() {
        assert!(matches!(
            parse(&["mvp", "login"]).command,
            Some(Command::Login)
        ));
        assert!(matches!(
            parse(&["mvp", "logout"]).command,
            Some(Command::Logout)
        ));
        assert!(matches!(
            parse(&["mvp", "whoami"]).command,
            Some(Command::Whoami)
        ));
        assert!(matches!(
            parse(&["mvp", "profile"]).command,
            Some(Command::Profile)
        ));
        assert!(matches!(
            parse(&["mvp", "leaderboard"]).command,
            Some(Command::Leaderboard {
                board: LeaderboardKind::Daily
            })
        ));
        assert!(matches!(
            parse(&["mvp", "leaderboard", "stack-overflow"]).command,
            Some(Command::Leaderboard {
                board: LeaderboardKind::StackOverflow
            })
        ));
    }
}
