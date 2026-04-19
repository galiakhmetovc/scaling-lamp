# teamD Agent Runtime Vision

This branch captures the target architecture for the agent runtime.

It does not describe the current implementation. It describes the system we
would build if we were optimizing for clarity, reliability, and long-term
evolution instead of preserving the current shape.

## Position

The agent should be built as an operating environment for execution, not as a
chat loop with tools attached.

The runtime should be:

- a modular monolith
- daemon-centered
- stateful
- evidence-driven
- recovery-friendly
- explicit about capabilities and side effects

The runtime should not be:

- event-sourced at the execution core
- projection-driven for live state
- split across multiple competing runtime paths
- dependent on UI heuristics to infer what is happening

## Local Smoke

The current greenfield branch includes a minimal live provider smoke path in
`agentd`.

Local `.env` files are ignored by git and autoloaded by `agentd` when matching
process environment variables are not already set. The repo ships
[.env.example](/home/admin/AI-AGENT/data/projects/teamD/.env.example) with the
current z.ai baseline taken from the older smoke branches:

- provider kind: `zai_chat_completions`
- base URL: `https://api.z.ai/api/coding/paas/v4`
- model: `glm-5-turbo`
- key env var: `TEAMD_PROVIDER_API_KEY`

Smoke command:

```bash
cargo run -p agentd -- provider smoke "Say hello in one short sentence."
```

For the current `z.ai` smoke path, `agentd` keeps `thinking={"type":"disabled"}`
for the one-shot `provider smoke` command so the check stays focused on basic
request/response health.

## Current Operator Notes

The current MVP already applies a boot-time recovery pass.

On daemon startup:

- `waiting_approval` runs stay pending
- `queued` runs stay queued
- `running`, `resuming`, `waiting_process`, and `waiting_delegate` runs are
  marked `interrupted`

This is intentionally conservative. The current branch does not yet persist
enough live process/provider state to resume those paths safely after a crash.

Useful commands:

```bash
cargo run -p agentd -- status
cargo run -p agentd -- chat show <session-id>
cargo run -p agentd -- chat send <session-id> "<message>"
cargo run -p agentd -- chat repl <session-id>
cargo run -p agentd -- mission tick
cargo run -p agentd -- mission tick <unix-timestamp>
cargo run -p agentd -- job execute <job-id>
cargo run -p agentd -- job execute <job-id> <unix-timestamp>
cargo run -p agentd -- run show <run-id>
cargo run -p agentd -- approval list <run-id>
cargo run -p agentd -- approval approve <run-id> <approval-id>
```

`run show` now surfaces the stored run error, which includes recovery
interrupt reasons after restart.

The first autonomous operator path is now:

1. `mission tick` to queue due mission-turn jobs
2. `job execute <job-id>` to run one mission-turn job through the configured provider

The normal chat operator path is now:

1. `chat show <session-id>` to inspect transcript history
2. `chat send <session-id> "<message>"` to execute one ordinary chat turn
3. `chat repl <session-id>` to stay inside one terminal chat loop
4. `tui` to work in the chat-first fullscreen terminal UI

`chat repl` now streams provider output directly in the terminal:

- assistant text appears as live deltas
- reasoning appears as a distinct `reasoning: ...` line
- when a turn invokes a tool, the REPL keeps one compact
  `tool: <name> | <status>` line per active tool step and updates it through
  `requested`, `waiting_approval`, `approved`, `running`, and
  `completed`/`failed` instead of printing a long raw event log

Streaming reasoning currently works through:

- OpenAI Responses via `reasoning summary` deltas
- `z.ai` chat completions via streamed `reasoning_content`

The current OpenAI-backed chat path now also supports one bounded model-driven
tool loop for auto-allowed tools:

- exposed automatically: `fs_read`, `fs_list`, `fs_glob`, `fs_search`,
  `web_fetch`, `web_search`
- if a surfaced tool resolves to `ask`, `approval approve <run-id> <approval-id>`
  now resumes the same provider loop instead of leaving the run stranded
- continuation uses `previous_response_id` instead of replaying the whole
  transcript on every tool step
- the loop is bounded and rejects repeated tool-call signatures instead of
  spinning forever
- when a surfaced tool resolves to `ask`, `chat send` now returns
  `status=waiting_approval` together with `run_id` and `approval_id`

## Autonomous Mission Smoke

Minimal operator smoke for the first autonomous path:

```bash
cargo run -p agentd -- session create session-smoke "Smoke Session"
cargo run -p agentd -- mission create mission-smoke session-smoke "Run the autonomous smoke"
cargo run -p agentd -- mission tick
cargo run -p agentd -- job show mission-smoke-mission-turn-<timestamp>
cargo run -p agentd -- job execute mission-smoke-mission-turn-<timestamp>
cargo run -p agentd -- run show run-mission-smoke-mission-turn-<timestamp>
```

If you need deterministic output for testing or demos, pass explicit unix
timestamps to `mission tick` and `job execute`.

## Chat Smoke

Minimal operator smoke for the normal chat path:

```bash
cargo run -p agentd -- session create session-chat "Chat Session"
cargo run -p agentd -- chat show session-chat
cargo run -p agentd -- chat send session-chat "Hello chat"
cargo run -p agentd -- chat show session-chat
```

Tool-calling smoke for the current OpenAI Responses path:

1. Configure `TEAMD_PROVIDER_KIND=openai_responses` with a model that supports
   function calling.
2. Prompt for a fetch/search-style action that stays inside the auto-allowed
   surface, for example:

```bash
cargo run -p agentd -- session create session-tool "Tool Session"
cargo run -p agentd -- chat send session-tool "Fetch https://example.com and summarize it in one line."
cargo run -p agentd -- chat show session-tool
```

Approval-resume smoke for the canonical chat path:

1. Add a permission rule that turns a surfaced tool such as `web_fetch` into
   `ask`.
2. Send a prompt that triggers that tool.
3. Inspect and approve the run:

```bash
cargo run -p agentd -- approval list <run-id>
cargo run -p agentd -- approval approve <run-id> <approval-id>
cargo run -p agentd -- chat show <session-id>
```

The approval command now continues the same model turn and appends the final
assistant reply when the provider returns a completion.

Interactive REPL commands:

- `/help`
- `/show`
- `/approve [approval-id]`
- `/exit`

When the active session hits `waiting_approval`, the REPL prints the pending
`run_id` and `approval_id`. `/approve` with no argument resumes the latest
pending approval for that REPL session; `/approve <approval-id>` overrides it.

## Terminal UI

The current branch now includes a chat-first terminal UI on top of the same
canonical runtime path:

```bash
cargo run -p agentd -- tui
```

The first screen is a session picker. `Enter` opens the selected session, `N`
opens the create-session dialog, `D` opens delete confirmation, and `Esc`
returns to the previous chat if one is already active.

Inside the chat screen:

- the main surface is one lazy scrollable chat timeline
- assistant text streams into the timeline as it arrives
- reasoning is rendered as its own timestamped timeline entry type
- tool activity stays compact as one status row per tool step
- approvals are still command-driven through `/approve [approval-id]`

Supported chat commands in the TUI:

- `/session`
- `/new`
- `/rename`
- `/clear`
- `/approve [approval-id]`
- `/model <name>`
- `/reasoning on|off`
- `/think <level>`
- `/compact`
- `/exit`

Current notes:

- `/clear` is destructive: it deletes the current session after confirmation
  and immediately switches into a new empty session
- `/compact` now runs real canonical context compaction: it summarizes the
  older transcript prefix, persists one context summary per session, and makes
  future chat turns use that summary plus only the uncovered trailing messages
- `/model` now updates the session-level model override that the canonical chat
  execution path actually sends to the provider
- `/think` is currently stored and surfaced in the UI/top bar, but does not yet
  change provider-specific reasoning parameters

## Core Principles

### 1. World State First

The center of the system is not the transcript. The center is the current
execution state of the world: session state, active runs, plans, approvals,
processes, delegates, and artifacts.

Transcript is history. It is important, but it is not the source of truth for
live execution.

### 2. One Canonical Runtime Model

Every active run must have exactly one canonical snapshot. The UI, daemon,
delegates, and operators all read from that same model.

There must be no parallel truths such as:

- TUI-local run flags
- detached approval queues
- inferred tool state from logs
- websocket events treated as final truth

### 3. State, History, and Audit Are Different Things

These concerns must be separated:

- state: current operational data
- history: transcript and user-visible chronology
- audit: append-only debugging and forensic record

Trying to make one structure serve all three creates confusion.

### 4. Capabilities Must Be Typed

Tooling must be exposed as explicit capabilities with typed inputs, structured
results, and known side effects.

There should be no "smart" execution path that guesses intent from ambiguous
text.

### 5. Evidence Before Claims

The agent should not mark work complete because the model believes it is done.
Completion requires evidence: command output, tests, diffs, artifacts, or
operator approval.

## Target System Shape

```text
cmd/
  agentd/              # daemon entrypoint
  agent-cli/           # optional thin client

internal/
  app/                 # composition root
  config/              # config load + validation
  session/             # sessions, prompts, transcript metadata
  run/                 # canonical run engine and lifecycle
  provider/            # model adapters only
  tool/                # tool contracts, registry, dispatch, policy
  workspace/           # files, editor buffers, terminal, artifacts
  plan/                # structured task/plan model
  delegate/            # child agents / bounded delegation
  verify/              # evidence collection and verification
  memory/              # working, project, semantic, episodic memory
  uiapi/               # API consumed by TUI/web/IDE
  stream/              # stream fanout for text and tool output
  store/               # sqlite + filesystem-backed stores
  recovery/            # crash recovery and reconciliation
  audit/               # append-only diagnostic log
```

## Bounded Contexts

### Session

Owns the user-facing conversation container:

- session metadata
- prompt override
- transcript references
- links to active plan and runs

It does not execute tools and it does not own process lifecycle.

### Run

Owns the execution state machine for a single agent run.

It is the most important subsystem in the architecture.

Responsibilities:

- provider turn lifecycle
- tool call sequencing
- approval waits
- async process waits
- delegate waits
- completion, failure, cancellation, interruption

### Tool

Owns the capability layer.

Responsibilities:

- tool definitions
- input validation
- dispatch
- policy checks
- result normalization
- side-effect metadata

### Workspace

Owns the operator and agent working environment:

- file tree access
- file edits
- terminal sessions
- artifact storage

Workspace is adjacent to the run engine, not fused with it.

### Plan

Owns structured planning state:

- goal
- tasks
- dependencies
- acceptance criteria
- status
- ownership

Plan is structured operational data, not markdown as source of truth.

### Delegate

Owns bounded child-agent execution.

Delegation is a controlled runtime primitive, not an unstructured second chat.

### Verify

Owns proof collection:

- test output
- build output
- lint output
- screenshots
- artifacts
- residual risk tracking

## Canonical Entities

### Session

- `id`
- `title`
- `created_at`
- `updated_at`
- `settings`
- `prompt_override`

### Message

- `id`
- `session_id`
- `role`
- `content`
- `attachments`
- `created_at`

### Run

- `id`
- `session_id`
- `status`
- `started_at`
- `updated_at`
- `finished_at`
- `error`
- `result`

### RunSnapshot

- `run`
- `pending_approvals`
- `active_processes`
- `recent_steps`
- `provider_stream`
- `delegate_runs`
- `evidence_refs`

### Approval

- `id`
- `run_id`
- `tool_call_id`
- `reason`
- `status`

### Process

- `id`
- `run_id`
- `kind`
- `status`
- `pid_ref`
- `started_at`
- `exit_code`

### Plan

- `id`
- `session_id`
- `goal`
- `tasks`

### Artifact

- `id`
- `session_id`
- `kind`
- `path`
- `metadata`

## Run Lifecycle

The run engine should expose one explicit state machine:

```text
queued
running
waiting_approval
waiting_process
waiting_delegate
resuming
completed
failed
cancelled
interrupted
```

Rules:

- only the run engine may transition run status
- approvals live inside the run snapshot
- active processes live inside the run snapshot
- delegates are attached to the run snapshot
- UI reads, but does not infer, run state

## Tool Model

Tools should be grouped by capability family.

### Filesystem

- `fs_read`
- `fs_write`
- `fs_patch`
- `fs_list`
- `fs_glob`
- `fs_search`

### Web

- `web_fetch`
- `web_search`

### Structured Process Execution

- `exec_start`
- `exec_wait`
- `exec_kill`

This family is for executable + args execution only.

No shell parsing semantics are allowed here.

If the agent needs a script-like workflow, it must write an explicit script file with
`fs_write` and then invoke that file through `exec_start`. There is no dedicated
shell-specific tool family.

## Permissions

Permissions are project-level and resolve to `allow`, `ask`, or `deny` before a tool
call becomes an approval wait.

Supported default modes:

- `default`: respect each tool's built-in approval policy
- `accept_edits`: allow filesystem edits, still ask for exec-style actions
- `plan`: allow read-only tools, deny mutating tools
- `auto`: allow tools unless a rule overrides them
- `bypass_permissions`: unconditional allow

Rules can override the mode by matching `tool`, `family`, and optional `path_prefix`.

### Planning

- `plan_create`
- `plan_list`
- `plan_update`

### Delegation

- `delegate_start`
- `delegate_send`
- `delegate_wait`
- `delegate_close`

## Memory Model

Memory should be split by purpose.

### Transcript

Raw conversation history.

### Working Memory

Only the active material needed for the current step.

### Task Memory

Decisions and constraints for the current mission.

### Project Memory

Stable knowledge about the codebase:

- architecture
- conventions
- ownership
- recurring hazards

### Long-Term Memory

Operator preferences and reusable patterns that survive across sessions.

The agent should retrieve memory through policy, not blindly dump memory into
the prompt.

## Verification Model

Verification must be a first-class subsystem.

Every meaningful completion claim should be backed by evidence such as:

- tests passing
- build succeeding
- lint succeeding
- browser flow succeeding
- file diff matching expected scope
- artifact generation succeeding

The runtime should support collecting an evidence bundle for each completed task
or run.

## Delegation Model

Subagents should be treated as bounded workers.

Each delegated task should have:

- a clear goal
- bounded context
- write scope
- expected output
- ownership

The parent agent remains responsible for:

- review
- integration
- verification

Delegation should improve throughput, not hide complexity.

## Operator Interface

The ideal operator surface is not just a chat.

The primary views should be:

- goal
- plan
- active run
- approvals
- tool activity
- workspace
- delegates
- evidence
- memory

Chat is just one surface over the runtime, not the runtime itself.

## Persistence

Use simple storage with clear responsibilities:

- SQLite for operational state
- filesystem/blob storage for large artifacts and attachments
- in-memory handles for live streams and processes
- append-only audit log for diagnostics

Do not use event sourcing as the core execution model.

If an append-only record is valuable, keep it as audit, not as the mechanism
required to reconstruct live runtime truth.

## Recovery

Recovery should be explicit.

After restart, the daemon should reconcile:

- active runs
- pending approvals
- live processes
- delegates
- incomplete verification

Runs that cannot be safely resumed should become `interrupted`, not silently
pretend to be healthy.

## Development Order

If building this from scratch, the order should be:

1. session store + transcript store
2. canonical run engine
3. provider adapter layer
4. tool registry and typed tool contracts
5. approval and async process lifecycle
6. verification subsystem
7. thin TUI/web API
8. workspace subsystem
9. delegation subsystem
10. recovery and hardening

## Anti-Goals

This target architecture deliberately rejects:

- event-sourced live runtime state
- multiple long-lived runtime versions in parallel
- shell execution contracts that infer intent from text
- UI-local state as source of truth
- feature growth before runtime clarity
- distributed architecture before local runtime correctness

## Practical Reading Of This Branch

This branch should be treated as an architecture statement.

Use the current implementation only as a source of:

- ideas
- tests
- isolated subsystems worth preserving
- migration reference

Do not preserve architectural decisions just because they already exist.
