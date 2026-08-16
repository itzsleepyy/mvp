//! Command-line interface.
//!
//! ```text
//! waitstate                              run the game (default)
//! waitstate play                         run the game
//! waitstate --agent codex                run the game, show Codex status
//! waitstate --agent auto                 run the game, follow live events
//! waitstate agent-event [AGENT] EVENT    send one lifecycle event (used by
//!                                        agent hooks; never launches a TUI,
//!                                        never prints to stdout)
//! waitstate hook AGENT EVENT             bridge for Codex/Gemini hooks:
//!                                        sends the event and prints "{}"
//!                                        (their protocols require JSON
//!                                        output; hidden from help)
//! waitstate claude install|uninstall|status
//! waitstate codex install|uninstall|status
//! waitstate codex run -- [CODEX_ARGS...]  run Codex and the game in one terminal
//! waitstate gemini install|uninstall|status
//! waitstate opencode install|uninstall|status
//! waitstate integrations                 show all integrations at once
//! waitstate integrations install         install for detected agents
//! waitstate integrations install --all   install every supported agent
//! waitstate integrations repair          fix missing/outdated pieces
//! ```

use std::ffi::OsString;

use clap::{Parser, Subcommand, ValueEnum};

use crate::agent::event::AgentEvent;
use crate::agent::status::{AgentDisplay, AgentKind};

#[derive(Debug, Parser)]
#[command(
    name = "waitstate",
    version,
    about = "A competitive terminal arcade for the time between prompts",
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
    /// Send a lifecycle event to the running WaitState instance
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
        command: CodexCommand,
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
}

#[derive(Debug, Subcommand)]
pub enum ProviderCommand {
    /// Merge WaitState hooks/plugin into the agent configuration (idempotent)
    Install,
    /// Remove only WaitState-owned hooks/plugin files
    Uninstall,
    /// Show integration status
    Status,
}

#[derive(Debug, Subcommand)]
pub enum CodexCommand {
    /// Merge WaitState hooks into the Codex configuration (idempotent)
    Install,
    /// Remove only WaitState-owned Codex hooks
    Uninstall,
    /// Show Codex integration status
    Status,
    /// Read or change automatic game display for managed Codex sessions
    AutoPlay {
        #[arg(value_enum, default_value_t = AutoPlayValue::Status)]
        value: AutoPlayValue,
    },
    /// Run Codex and automatically show the game while the agent works
    Run {
        /// Arguments forwarded verbatim to Codex after `--no-alt-screen`
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AutoPlayValue {
    On,
    Off,
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
    /// Repair missing or outdated WaitState integration pieces
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

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_invocation_runs_the_game() {
        let cli = parse(&["waitstate"]);
        assert!(cli.agent.is_none());
        assert!(cli.command.is_none());
        assert!(!cli.no_auto_resume);
    }

    #[test]
    fn no_auto_resume_flag_parses() {
        let cli = parse(&["waitstate", "--no-auto-resume"]);
        assert!(cli.no_auto_resume);
    }

    #[test]
    fn play_command_is_explicit() {
        let cli = parse(&["waitstate", "play"]);
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
            let cli = parse(&["waitstate", "agent-event", name]);
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
            let cli = parse(&["waitstate", "agent-event", name, "working"]);
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
        assert!(Cli::try_parse_from(["waitstate", "agent-event"]).is_err());
    }

    #[test]
    fn hook_command_parses_agent_and_event() {
        let cli = parse(&["waitstate", "hook", "codex", "working"]);
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
        let cli = parse(&["waitstate", "--agent", "claude"]);
        assert_eq!(cli.agent.unwrap().into_agent_kind(), AgentKind::ClaudeCode);
        let cli = parse(&["waitstate", "--agent", "codex"]);
        assert_eq!(cli.agent.unwrap().into_agent_kind(), AgentKind::Codex);
        let cli = parse(&["waitstate", "--agent", "gemini"]);
        assert_eq!(cli.agent.unwrap().into_agent_kind(), AgentKind::GeminiCli);
        let cli = parse(&["waitstate", "--agent", "opencode"]);
        assert_eq!(cli.agent.unwrap().into_agent_kind(), AgentKind::OpenCode);
        let cli = parse(&["waitstate", "--agent", "auto"]);
        assert!(matches!(
            cli.agent.unwrap().into_agent_display(),
            AgentDisplay::Auto
        ));
        assert!(Cli::try_parse_from(["waitstate", "--agent", "warp"]).is_err());
    }

    #[test]
    fn provider_subcommands_parse() {
        for name in ["claude", "codex", "gemini", "opencode"] {
            for verb in ["install", "uninstall", "status"] {
                let cli = parse(&["waitstate", name, verb]);
                let ok = match &cli.command {
                    Some(Command::Claude { command }) => matches!(
                        command,
                        ProviderCommand::Install
                            | ProviderCommand::Uninstall
                            | ProviderCommand::Status
                    ),
                    Some(Command::Codex { command }) => matches!(
                        command,
                        CodexCommand::Install | CodexCommand::Uninstall | CodexCommand::Status
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
        assert!(Cli::try_parse_from(["waitstate", "codex", "explode"]).is_err());
    }

    #[test]
    fn integrations_commands_parse() {
        let cli = parse(&["waitstate", "integrations"]);
        assert!(matches!(
            cli.command,
            Some(Command::Integrations { command: None })
        ));
        let cli = parse(&["waitstate", "integrations", "install"]);
        match cli.command {
            Some(Command::Integrations {
                command: Some(IntegrationsCommand::Install { all }),
            }) => assert!(!all),
            other => panic!("unexpected: {other:?}"),
        }
        let cli = parse(&["waitstate", "integrations", "install", "--all"]);
        match cli.command {
            Some(Command::Integrations {
                command: Some(IntegrationsCommand::Install { all }),
            }) => assert!(all),
            other => panic!("unexpected: {other:?}"),
        }
        let cli = parse(&["waitstate", "integrations", "repair"]);
        assert!(matches!(
            cli.command,
            Some(Command::Integrations {
                command: Some(IntegrationsCommand::Repair)
            })
        ));
    }

    #[test]
    fn codex_run_forwards_trailing_arguments() {
        let cli = parse(&[
            "waitstate",
            "codex",
            "run",
            "--",
            "--model",
            "gpt-5.6-sol",
            "resume",
            "--last",
        ]);
        match cli.command {
            Some(Command::Codex {
                command: CodexCommand::Run { args },
            }) => {
                let values: Vec<_> = args.iter().map(|arg| arg.to_string_lossy()).collect();
                assert_eq!(values, ["--model", "gpt-5.6-sol", "resume", "--last"]);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn codex_auto_play_parses_all_values() {
        for (name, expected) in [
            ("on", AutoPlayValue::On),
            ("off", AutoPlayValue::Off),
            ("status", AutoPlayValue::Status),
        ] {
            let cli = parse(&["waitstate", "codex", "auto-play", name]);
            assert!(matches!(
                cli.command,
                Some(Command::Codex {
                    command: CodexCommand::AutoPlay { value }
                }) if value == expected
            ));
        }
    }
}
