# Operator Chat Console Design

## Goal

Add `teamd-agent chat <chat_id> <session_id>` as a human-readable operator console over the existing runtime control plane.

The console must let an operator:
- talk to the agent in one session
- see live run progress through SSE events
- see plan updates, workers, jobs, approvals, artifacts, and memory/compaction signals
- take inline actions like approve/reject/cancel without leaving the chat loop

## Non-Goals

- No full-screen TUI
- No new runtime execution path
- No hidden chain-of-thought display
- No second event model beside `runtime_events`

## Architecture

`teamd-agent chat` stays API-first.

It uses existing surfaces:
- `POST /api/runs`
- `GET /api/runs/{id}`
- `GET /api/events/stream`
- `GET /api/approvals`
- `POST /api/approvals/{id}/approve`
- `POST /api/approvals/{id}/reject`
- `GET /api/workers/{id}/handoff`
- `GET /api/artifacts/{ref}`
- `GET /api/artifacts/{ref}/content`
- `GET /api/plans`

The CLI owns only:
- REPL loop
- minimal line editing and tab completion
- human-readable event rendering
- inline operator commands

The runtime remains unchanged as the source of truth.

## User Experience

### Entry

Command:

```bash
teamd-agent chat 1001 1001:default
```

The console prints:
- session header
- short help
- prompt `you> `

### Input Model

Two input classes exist:

1. Plain text
- treated as a user message
- starts a new run in the given `session_id`

2. Slash commands
- handled locally by CLI
- do not create a run

Supported commands in first version:
- `/help`
- `/status`
- `/plan`
- `/plans`
- `/approve <approval_id>`
- `/reject <approval_id>`
- `/handoff <worker_id>`
- `/artifact <ref>`
- `/cancel`
- `/quit`

### Minimal Completion

Use minimal terminal completion only.

Required:
- tab-complete slash command names
- tab-complete pending approval ids after `/approve` and `/reject`
- tab-complete known worker ids after `/handoff`
- tab-complete known artifact refs after `/artifact`

Not required:
- rich multiline editor
- fuzzy completion
- syntax coloring

### Human-Readable Rendering

Render event stream into readable sections:

- `you:` user prompt
- `assistant:` final visible reply
- `tool:` tool start/result/failure
- `approval:` pending/approved/rejected
- `worker:` spawned, running, handoff created
- `job:` created, started, completed, failed, cancelled
- `plan:` created, updated, item started/completed
- `memory:` recall, compaction, checkpoint, artifact offload
- `system:` run started/completed/cancelled/errors

The console must prefer concise summaries over raw JSON.

## Event Handling

For each submitted message:
- create run with `runs start`
- subscribe to `events/stream` filtered by `run_id`
- print lifecycle until terminal state
- then return to prompt

If no stream event explains the final answer well enough, the console fetches `runs status` and prints the final assistant-visible summary available from the run path.

## Plan Visibility

The agent’s task plan must be visible in chat.

Required:
- when `plan.created` or `plan.updated` arrives, print a readable plan block
- `/plan` shows the current run plan if one is active
- `/plans` shows recent plans for the current session

This reuses existing persisted plans; no separate chat-only plan state exists.

## Approvals

Approvals stay API-driven.

Required:
- show pending approvals inline when related events arrive
- `/approve <id>` and `/reject <id>` call existing approval endpoints
- after decision, continue showing resulting events in the same console

## Long-Running Work

Jobs and workers are first-class in the console.

Required:
- show job lifecycle from events
- show worker lifecycle from events
- show worker handoff summaries through `/handoff`
- show artifact refs and allow direct `/artifact <ref>` inspection

This allows long tasks to stay visible without turning chat into a raw log stream.

## Error Handling

The console must distinguish:
- bad local command usage
- API request failure
- run failure
- cancellation
- approval rejection
- stream disconnect

On stream disconnect:
- print a system message
- fall back to `runs status`
- return to prompt without crashing the console

## Files To Touch

Primary:
- `cmd/coordinator/cli.go`
- `cmd/coordinator/cli_test.go`
- `internal/cli/client.go`
- `internal/cli/client_test.go`

Possible helper additions:
- `internal/cli/chat_console.go`
- `internal/cli/chat_console_test.go`

Docs:
- `docs/agent/cli.md`
- `docs/agent/http-api.md`
- `docs/agent/core-architecture-walkthrough.md`

## Rollout Order

1. CLI chat skeleton with local commands and polling fallback
2. SSE-backed live event rendering
3. approvals/plan/artifact/handoff commands
4. minimal completion
5. docs and smoke tests
