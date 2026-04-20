# Service-First Daemon, Skills, and MCP Design

## Goal

Turn `agentd` into a service-first daemon that can run on Linux and Windows, keep long-lived state in memory, serve TUI and CLI clients over HTTP/JSON, and become the canonical base for background jobs, schedules, skills, and later MCP.

This design must preserve one canonical runtime path:

- one daemon as the source of truth;
- one session/mission/run model;
- one prompt assembly path;
- one tool execution model.

The daemon must support local usage and remote usage without introducing a separate "remote mode" architecture.

## Scope

### In scope for the first implementation wave

- service-first `agentd daemon`
- HTTP/JSON control plane
- configurable bind host and port
- simple bearer token authentication
- `agentd tui` as a daemon client
- local auto-spawn of daemon from `agentd tui` when the target is local and the daemon is absent
- durable background task runtime inside the daemon
- schedules built on top of the background runtime
- Agent Skills support following the `agentskills` open format
- Russian `\` commands in TUI/REPL, with old `/` commands preserved as aliases

### Explicitly out of scope for the first implementation wave

- Telegram or any other product-specific integration
- distributed remote tool execution
- multi-agent mesh protocol
- advanced auth/roles beyond one bearer token
- full MCP execution

### In scope as the next wave after the daemon + skills wave

- MCP client support
- MCP-backed tools/resources/prompts integrated into the daemon

## Constraints

- The daemon must run on Linux and Windows.
- TUI must be able to talk to a local daemon and a remote daemon.
- All tool execution remains local to the daemon host for now.
- The design must leave clean seams for future remote worker nodes.
- Skills must be discovered from a `skills/` directory rooted at the daemon process working directory.
- The daemon remains the source of truth for process registries, background jobs, schedules, and active sessions.

## User Experience

## Daemon

- Start explicitly:
  - `agentd daemon`
- Bind host and port come from config/env.
- Example production bind:
  - `0.0.0.0:5140`
- Authentication uses one bearer token from config/env.

## TUI

- `agentd tui`
  - first attempts to connect to the daemon over HTTP/JSON
  - if the target is local and the daemon is unavailable, it attempts to spawn a local daemon automatically
- remote connect is explicit:
  - `agentd tui --host 10.6.5.3 --port 5140`

## Commands

Primary TUI/REPL command style becomes Russian commands with `\`.

Examples:

- `\выход`
- `\план`
- `\сессии`
- `\новая`
- `\переименовать`
- `\очистить`
- `\скиллы`
- `\включить <skill>`
- `\выключить <skill>`

Legacy slash commands remain supported as aliases.

## Service Model

`agentd` becomes a dual-role binary:

- `agentd daemon`
  - long-lived service process
- `agentd tui`
  - client UI talking to the daemon

The daemon owns:

- session state
- run state
- process registry
- background jobs
- schedules
- prompt assembly
- skill catalog and activation state
- future MCP connections

The client owns only ephemeral UI state:

- current screen
- input buffers
- cursor/selection
- local rendering state

This preserves the existing "single runtime path" principle and prevents local-vs-remote divergence.

## HTTP/JSON Control Plane

The first transport is HTTP/JSON, not gRPC.

Reasons:

- easier to debug by hand
- easier to support with TUI and CLI
- simpler cross-platform deployment
- enough for the first service-first wave

Suggested API groups:

- `/v1/status`
- `/v1/sessions/*`
- `/v1/chat/*`
- `/v1/runs/*`
- `/v1/approvals/*`
- `/v1/background/*`
- `/v1/schedules/*`
- `/v1/skills/*`

The transport layer should remain thin. Business logic stays in the canonical execution/runtime modules.

## Configuration

Daemon configuration must add:

- `daemon.bind_host`
- `daemon.bind_port`
- `daemon.bearer_token`
- `daemon.skills_dir`

Defaults:

- bind host: `127.0.0.1`
- bind port: stable local default
- bearer token: required for non-local binds; may be optional for strict local-only configs
- skills dir: `./skills` from daemon cwd

Remote client connection uses explicit host and port flags in TUI/CLI.

## Background Runtime

The daemon wave should not invent a second job model.

Instead it should complete and host the already-identified background runtime:

- durable background job model
- worker loop
- progress/log stream
- cancel/retry/recovery

Schedules must sit on top of this same background runtime, not beside it.

This means the existing `teamD-bg.*` and `teamD-cron.*` work becomes part of the daemon implementation path.

## Skills

## Format

Skills follow the `agentskills` format:

- directory per skill
- required `SKILL.md`
- optional `scripts/`
- optional `references/`
- optional `assets/`

References:

- https://agentskills.io/specification
- https://agentskills.io/integrate-skills

## Discovery

The daemon scans:

- `skills/*/SKILL.md`

from the daemon process working directory.

This means:

- local daemon: `./skills` from where the service was started
- remote daemon: the service machine's own `./skills`

No separate global skills path is introduced in the first wave.

## Loading Model

Skills use progressive disclosure:

1. catalog load
   - `name`
   - `description`
2. activation load
   - full `SKILL.md`
3. on-demand resource load
   - `scripts/`
   - `references/`
   - `assets/`

The daemon, not the TUI, owns this loading lifecycle.

## Activation

Two activation paths are required:

- automatic activation by matching user task against skill `name` and `description`
- manual activation overrides per session

Manual commands:

- `\скиллы`
- `\включить <skill>`
- `\выключить <skill>`

Manual activation applies only to the current session, not globally to the daemon.

This gives:

- global discoverability
- local session control
- no surprise cross-session leakage

## Prompt Integration

Active skills must join the canonical prompt path instead of becoming a side channel.

Prompt order becomes:

1. `SYSTEM.md`
2. `AGENTS.md`
3. active skill instructions
4. `SessionHead`
5. `Plan`
6. `ContextSummary`
7. `Offload refs`
8. transcript tail

Only activated skills load into the prompt.

Referenced skill resources should be loaded only when the active skill instructions direct the agent to them.

## MCP

MCP support remains part of the target architecture, but it is intentionally staged after daemon + skills.

The daemon must be designed so MCP can later slot into the same control model:

- daemon manages MCP connections
- daemon exposes MCP-backed capabilities through the canonical runtime path
- TUI remains only a client

But first-wave implementation should not try to deliver MCP execution.

## Security

First-wave security is intentionally simple:

- bearer token for HTTP access
- configurable bind host/port
- no role system
- no complex multi-tenant auth

This is enough for service-first operation without turning the first wave into auth infrastructure work.

## Architecture Boundaries

## Daemon layer

Responsibilities:

- process lifetime
- HTTP server
- auth
- service state ownership
- client request routing

## Runtime layer

Responsibilities:

- sessions
- runs
- prompt assembly
- tools
- approvals
- background jobs
- schedules
- skills activation and loading

## TUI layer

Responsibilities:

- rendering
- input
- command aliases
- remote/local connection handling
- auto-spawn local daemon

The TUI must never become a second execution engine.

## Implementation Phases

### Phase 1: Service-first daemon

- daemon process entrypoint
- HTTP/JSON API
- bearer token auth
- TUI client mode
- local auto-spawn

### Phase 2: Background runtime hosted by daemon

- durable jobs
- worker loop
- cancel/retry/recovery
- schedule execution

### Phase 3: Skills

- discovery and catalog
- auto activation
- manual per-session activation
- prompt integration
- Russian command aliases

### Phase 4: MCP

- separate follow-up wave

## Testing Strategy

- daemon transport tests
- auth tests
- local auto-spawn tests
- TUI client/server integration tests
- background job durability tests
- schedule dispatch tests
- skills discovery parsing tests
- skills activation tests
- prompt assembly tests proving active skills join the canonical prompt path
- Windows + Linux smoke coverage for daemon start and client connect flows

