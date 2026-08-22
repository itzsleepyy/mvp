# Coding Agent Integrations

MVP supports Claude Code, Codex, Gemini CLI, and OpenCode. Each adapter converts native lifecycle signals into the same local event model.

## Commands

```bash
mvp integrations
mvp integrations install
mvp integrations install --all
mvp integrations repair

mvp claude install|status|uninstall
mvp codex install|status|uninstall
mvp gemini install|status|uninstall
mvp opencode install|status|uninstall
```

Install and repair operations are idempotent. JSON hook installers merge MVP handlers into existing files, back up files before modification, reject malformed configuration, and preserve unrelated settings. OpenCode uses an MVP-owned plugin file and never replaces foreign plugins.

## Event Mapping

| MVP event | Claude Code | Codex | Gemini CLI | OpenCode |
| --- | --- | --- | --- | --- |
| `Started` | `SessionStart` | `SessionStart` | `SessionStart` | `session.created` |
| `Working` | `UserPromptSubmit`, `PostToolUse` | `UserPromptSubmit`, `PostToolUse` | `BeforeAgent`, `BeforeTool`, `AfterTool` | busy/tool events |
| `NeedsInput` | `Notification` | `PermissionRequest` | `Notification` | `permission.ask` |
| `Completed` | `Stop` | `Stop` | `AfterAgent` | idle status |
| `Stopped` | `SessionEnd` | `SessionEnd` | `SessionEnd` | `session.deleted` |

## Attention Model

MVP tracks each connected agent separately and derives one display state:

```text
NeedsInput > Working > Completed > Stopped > Idle
```

A manual pause is never overridden by an agent. Completion pauses require manual resume unless another agent starts working or a fresh session begins.

## Privacy

Integrations send only an agent identifier and lifecycle event name over local IPC. MVP does not read prompts, source code, tool arguments, model output, repository contents, or conversation history.

## Limitations

- Claude permission notifications may arrive after the prompt appears.
- Codex hooks must be reviewed and trusted in Codex before first use.
- Gemini hooks are synchronous and use a bounded bridge timeout.
- OpenCode spawns the local `mvp` bridge per event and ignores it when MVP is not running.
- Multiple sessions from one provider currently appear as one logical agent stream.

Legacy compatibility identifiers remain in ownership checks so hooks and plugins installed before the MVP rebrand can be upgraded or removed without touching foreign configuration.
