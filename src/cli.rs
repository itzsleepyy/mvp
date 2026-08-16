//! Command-line interface.
//!
//! ```text
//! waitstate                              run the game (default)
//! waitstate play                         run the game
//! waitstate --agent claude               run the game, show Claude status
//! waitstate agent-event <event>          send one lifecycle event (used by
//!                                        Claude Code hooks; never launches
//!                                        a TUI, never prints to stdout)
//! waitstate claude install               merge WaitState hooks into the
//!                                        Claude Code settings (with backup)
//! waitstate claude uninstall             remove only WaitState-owned hooks
//! waitstate claude status                show integration status
//! ```

use clap::{Parser, Subcommand};

use crate::agent::event::AgentEvent;
use crate::agent::status::AgentKind;

#[derive(Debug, Parser)]
#[command(
    name = "waitstate",
    version,
    about = "A competitive terminal arcade for the time between prompts",
    long_about = None
)]
pub struct Cli {
    /// Coding agent to display in the UI (e.g. "claude"). Without this the
    /// agent indicator appears once the first lifecycle event arrives.
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
        /// The lifecycle event (working, needs-input, completed, started, stopped)
        #[arg(value_parser = clap::value_parser!(AgentEvent))]
        event: AgentEvent,
    },
    /// Manage the Claude Code integration
    Claude {
        #[command(subcommand)]
        command: ClaudeCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum ClaudeCommand {
    /// Merge WaitState hooks into the Claude Code settings (idempotent)
    Install,
    /// Remove only WaitState-owned hooks from the Claude Code settings
    Uninstall,
    /// Show integration status
    Status,
}

/// CLI spelling of an agent kind (`--agent claude`).
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum AgentKindArg {
    Claude,
}

impl AgentKindArg {
    pub fn into_agent_kind(self) -> AgentKind {
        match self {
            Self::Claude => AgentKind::ClaudeCode,
        }
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
                Some(Command::AgentEvent { event: parsed }) => assert_eq!(parsed, event),
                other => panic!("unexpected command for {name}: {other:?}"),
            }
        }
    }

    #[test]
    fn agent_event_rejects_unknown_names() {
        assert!(Cli::try_parse_from(["waitstate", "agent-event", "explode"]).is_err());
    }

    #[test]
    fn agent_flag_parses_claude() {
        let cli = parse(&["waitstate", "--agent", "claude"]);
        assert_eq!(cli.agent.unwrap().into_agent_kind(), AgentKind::ClaudeCode);
        assert!(Cli::try_parse_from(["waitstate", "--agent", "codex"]).is_err());
    }

    #[test]
    fn claude_subcommands_parse() {
        let cli = parse(&["waitstate", "claude", "install"]);
        assert!(matches!(
            cli.command,
            Some(Command::Claude {
                command: ClaudeCommand::Install
            })
        ));
        let cli = parse(&["waitstate", "claude", "uninstall"]);
        assert!(matches!(
            cli.command,
            Some(Command::Claude {
                command: ClaudeCommand::Uninstall
            })
        ));
        let cli = parse(&["waitstate", "claude", "status"]);
        assert!(matches!(
            cli.command,
            Some(Command::Claude {
                command: ClaudeCommand::Status
            })
        ));
    }
}
