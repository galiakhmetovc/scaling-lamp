# Session Wake-Up and Delegation Substrate Design

## Goal

Extend the canonical durable background job runtime so background work can wake a session without model-side polling, and define the substrate that later supports local child-session delegation and remote A2A delegation without introducing a second runtime path.

## Scope

This slice covers:

- durable background worker loop semantics
- durable session inbox events produced by background jobs
- canonical wake-up scheduling from inbox events into new session turns
- daemon-hosted execution of background jobs
- the architectural relationship between background jobs, local child-session delegation, and future remote A2A delegation

This slice does not cover:

- local child-session delegation execution
- remote A2A transport
- recurring schedule dispatch
- new UI beyond minimal current-session background visibility

## Constraints

- Preserve one canonical runtime path for chat, approvals, background jobs, and wake-ups.
- Do not reintroduce polling by the model.
- Do not treat wake-up events as fake user messages.
- Keep prompt assembly ordered as:
  1. `SYSTEM.md`
  2. `AGENTS.md`
  3. `SessionHead`
  4. `Plan`
  5. `ContextSummary`
  6. offload refs
  7. uncovered transcript tail
- Keep TUI/CLI thin over the same app/runtime and daemon HTTP/JSON path.

## Approaches Considered

### A. Polling Turns From the Model

Have the model periodically call back into the daemon to check whether background work finished.

This is rejected because it wastes tokens, creates empty turns, and makes wake-up timing dependent on model behavior rather than daemon-owned durable state.

### B. Side-Channel Notifications Outside the Session Engine

Push background completion directly into the UI and let the user decide whether to resume the agent.

This is rejected because it splits orchestration into a visible UI path and a hidden background path, which would drift from the canonical session/run model.

### C. Recommended: Durable Session Inbox Events + Daemon Wake-Up

Background jobs emit durable session inbox events such as completion, failure, approval-needed, or external-input-received. The daemon scheduler consumes those events and starts a canonical session turn when the session is idle. If the session is busy, the event stays queued until a safe boundary.

This preserves one runtime path, avoids polling, and naturally becomes the substrate for later delegation.

## Core Model

### Background Jobs

Background jobs remain the durable execution substrate introduced in `teamD-bg.1`.

They now gain active worker semantics:

- a queued job can be leased by the daemon worker
- the worker updates:
  - `status`
  - `attempt_count`
  - `lease_owner`
  - `lease_expires_at`
  - `heartbeat_at`
  - `last_progress_message`
- a job can finish as:
  - `completed`
  - `failed`
  - `cancelled`
  - `blocked`

### Session Inbox Events

Add a new durable model for session wake-up inputs. An inbox event is not a transcript message and not a run. It is a queued piece of session input owned by the daemon.

Each inbox event includes:

- `id`
- `session_id`
- `job_id: Option<String>`
- `kind`
- `payload_json`
- `status`
- `created_at`
- `available_at`
- `claimed_at`
- `processed_at`
- `error: Option<String>`

Initial inbox event kinds:

- `job_completed`
- `job_failed`
- `job_progressed`
- `job_blocked`
- `approval_needed`
- `external_input_received`
- `delegation_result_ready` (reserved for a later slice)

Status lifecycle:

- `queued`
- `claimed`
- `processed`
- `failed`

### Wake-Up Scheduling

The daemon owns wake-up decisions.

Rules:

- if a session has queued inbox events and no active run, the daemon may start a new canonical turn
- if a session already has an active run, inbox events remain queued
- wake-up uses the same chat execution path as any other canonical turn
- the wake-up turn consumes one or more inbox events and converts them into canonical runtime context for that turn

The wake-up turn is not represented as a synthetic user message. Instead:

- the daemon adds a system-visible transcript entry describing the event
- the session turn receives the event as structured runtime input

## Transcript and Operator Visibility

When an inbox event wakes the session, the timeline should show a system event such as:

- `background job completed`
- `background job failed`
- `background job requires approval`

This is for operator visibility only. The source of truth remains the inbox event record.

## Worker Loop

`teamD-bg.2` adds a daemon-hosted worker loop with three responsibilities:

1. lease and execute queued background jobs
2. persist progress, heartbeats, completion, and failure
3. emit durable session inbox events when background state should wake a session

The worker loop does not directly update the UI. It only updates canonical store state.

## Recovery and Cancellation

The daemon worker loop must support:

- lease expiry and re-acquisition after daemon restart or crash
- idempotent transition from `queued -> running -> terminal`
- explicit cancel requests through existing durable job fields
- no silent loss of job completion or failure notifications

If a job finishes after the daemon restarts, the worker must still emit the appropriate inbox event exactly once from the durable state transition.

## Relation to Local Subagents and Remote A2A

### Subagents

A local subagent is not a separate orchestration mechanism. It is a future specialization of background delegation:

- parent session creates a delegation background job
- daemon executes it by creating a child session
- child session runs independently through the same runtime
- child session returns a compact result package and artifact refs
- daemon writes a `delegation_result_ready` inbox event into the parent session

### A2A

Remote A2A is the same delegation concept with a different executor backend:

- local delegation executor: spawn child session in the same daemon
- remote delegation executor: send delegation request over A2A to another daemon

Background jobs remain the orchestration substrate for both.

## Daemon Boundaries

The daemon becomes responsible for:

- job leasing and worker execution
- inbox event creation
- wake-up scheduling
- ensuring only one canonical turn consumes a given inbox event

TUI, REPL, and CLI remain clients:

- they render timeline/system entries
- they read current job state
- they do not drive wake-up orchestration themselves

## Testing

Required coverage for this slice:

- worker loop leases and runs queued background jobs
- progress, heartbeat, and completion persist durably
- job completion emits exactly one inbox event
- queued inbox events wake idle sessions
- busy sessions do not lose queued inbox events
- daemon restart preserves leases, recoverability, and pending inbox events
- daemon-backed TUI/CLI continue to read current-session jobs from canonical app APIs

## Follow-On Work

After this slice lands:

1. local child-session delegation job and result package
2. delegation routing with local executor and future remote executor slot
3. remote A2A delegation adapter
4. recurring schedules dispatching into the same background job and inbox wake-up path
