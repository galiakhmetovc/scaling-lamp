# Daemon Scheduler Design

## Goal

Add a daemon-owned scheduler to the clean-room runtime so the operator can define recurring or one-shot automated jobs without relying on system cron.

The scheduler must:

- persist jobs across daemon restarts
- support two job kinds:
  - `shell`
  - `chat_prompt`
- support three trigger kinds:
  - `interval`
  - `cron`
  - `once_at`
- expose operator commands and live events
- render through a dedicated TUI surface

This is not a generic workflow engine. It is a bounded scheduler for operator-authored automation.

## Why This Exists

The current operator surface supports:

- interactive chat
- tool inspection
- session control
- workspace execution

What it does not support is durable background automation.

Examples the operator should be able to define:

- every 10 minutes run a shell healthcheck
- once at a specific time run a maintenance command
- every hour ask the agent to summarize system state into a target session

System cron is the wrong boundary for this because it is outside daemon state, outside session context, and outside operator surfaces.

The scheduler should live at the same layer as the rest of the operator runtime.

## Scope

First slice includes:

- persisted scheduled jobs
- persisted recent run history
- daemon scheduler loop
- two job kinds:
  - `shell`
  - `chat_prompt`
- three trigger kinds:
  - `interval`
  - `cron`
  - `once_at`
- operator commands:
  - create
  - update
  - delete
  - pause
  - resume
  - run now
  - list
  - inspect history
- TUI `Schedules` tab
- jump-outs from schedule runs into related artifacts and sessions

First slice explicitly excludes:

- DAGs or workflow graphs
- multi-step jobs
- distributed scheduling
- arbitrary retries with branching logic
- calendar UI
- agent-authored job creation

## Core Principle

This scheduler is daemon-owned and operator-authored.

Scheduled jobs are not part of the agent tool loop. They are not created by the model. They do not use the interactive approval UX used for chat-time shell calls.

That is deliberate.

If scheduled jobs flowed through chat approval, they would stall unpredictably and stop being useful. If scheduled jobs were agent-authored, the scheduler would become an uncontrolled automation surface.

## Data Model

### ScheduledJob

`ScheduledJob` is the durable configuration object.

Fields:

- `job_id`
- `title`
- `kind`
  - `shell`
  - `chat_prompt`
- `session_id`
  - optional
- `trigger_type`
  - `interval`
  - `cron`
  - `once_at`
- `trigger_spec`
- `enabled`
- `created_at`
- `updated_at`
- `next_run_at`
- `last_run_at`
- `last_status`
- `last_error`
- `last_result_ref`
- `running`
- `concurrency_policy`

For the first slice, `concurrency_policy` is intentionally narrow:

- `skip_if_running`

This should be the default and only supported behavior initially.

### ScheduledRun

`ScheduledRun` is the execution record.

Fields:

- `run_id`
- `job_id`
- `session_id`
- `started_at`
- `finished_at`
- `status`
  - `running`
  - `ok`
  - `error`
  - `cancelled`
- `summary`
- `artifact_ref`
- `triggered_by`
  - `scheduler`
  - `run_now`
- `result_meta`

`result_meta` should stay compact and structured. Large outputs belong in artifact storage.

## Job Kinds

### Shell

`shell` jobs run a daemon-owned shell execution path.

They are intended for recurring operational commands such as:

- health checks
- status collection
- maintenance commands

Output handling:

- short output goes into run summary/result metadata
- long output offloads to artifact storage
- run history stores `artifact_ref`

### Chat Prompt

`chat_prompt` jobs run a normal agent turn against a target session.

This is the automation path for prompts such as:

- summarize service state every hour
- periodically re-check a system and append conclusions
- run a recurring diagnostic prompt

Rules:

- target session must exist
- result should appear in the normal transcript for that session
- run history should still capture summary and metadata

If the session does not exist, the run fails explicitly.

## Trigger Kinds

### Interval

Examples:

- `5m`
- `1h`
- `24h`

This is the easiest and safest recurring trigger and should be implemented first.

### Cron

Examples:

- `0 * * * *`
- `*/15 * * * *`

Cron support should use a single explicit timezone policy in the first slice:

- scheduler evaluation runs in UTC

This avoids DST ambiguity and hidden local-time behavior.

### Once At

One-shot scheduled execution at a specific timestamp.

After the first run:

- the job should auto-disable

This is preferable to silent deletion because it leaves an inspectable record.

## Daemon Architecture

Add a dedicated scheduler module:

- `internal/runtime/scheduler`

Primary pieces:

- `JobStore`
- `TriggerEvaluator`
- `Runner`
- `Loop`

### JobStore

Stores:

- scheduled jobs
- recent run history

Requirements:

- survives daemon restart
- supports atomic updates
- simple enough for operator-owned state

First-slice storage should be file-backed and local to daemon/operator state. A database is unnecessary at this stage.

### TriggerEvaluator

Responsible for computing:

- whether a job is due
- next run time after completion

It must support:

- `interval`
- `cron`
- `once_at`

### Runner

Single interface:

- `Run(job ScheduledJob) -> ScheduledRun`

Implementations:

- `shellRunner`
- `chatPromptRunner`

### Loop

Background daemon goroutine:

1. load enabled jobs
2. compute due jobs
3. enforce concurrency policy
4. launch runs
5. persist updated job state and run records
6. compute next wake-up

The loop should use a small polling cadence, for example one second, rather than depending on external cron.

## Execution Semantics

### Concurrency

Per-job:

- a job must not run concurrently with itself
- first slice policy is `skip_if_running`

Global daemon limit:

- cap scheduled job concurrency

Recommended first default:

- `max_concurrent_runs = 4`

This protects daemon responsiveness and prevents operator error from creating a run storm.

### Safety Boundary

Scheduled jobs do not use interactive approval.

That is intentional and required for the feature to be useful.

However:

- jobs are created only by the operator
- the scheduler is not exposed as an agent-authored automation path
- all runs are logged
- shell output remains inspectable through run history and artifacts

This is a trusted operator execution surface, not a hidden escalation path for the model.

### Failure Handling

Failure of a run must not disrupt the scheduler loop.

On every run completion:

- `last_status` updates
- `last_error` updates when relevant
- `last_run_at` updates
- `next_run_at` recalculates

For the first slice:

- no automatic retry policy
- operator can use `run_now` if needed

This keeps behavior explicit and avoids hidden retry storms.

## Commands And API

Add daemon/operator commands:

- `schedule.create`
- `schedule.update`
- `schedule.delete`
- `schedule.pause`
- `schedule.resume`
- `schedule.run_now`
- `schedule.list`
- `schedule.get`
- `schedule.history`

Minimum payload for create/update:

- `title`
- `kind`
- `session_id`
- `trigger_type`
- `trigger_spec`
- kind-specific payload:
  - `shell.command`
  - `chat.prompt`
- `enabled`

## Live Events

Add daemon websocket events:

- `schedule.updated`
- `schedule.run.started`
- `schedule.run.finished`

This allows the TUI to update live without forcing manual refresh.

## TUI Surface

Add top-level tab:

- `Schedules`

Suggested layout:

- left: jobs list
- right: detail pane

Details pane shows:

- title
- kind
- session
- trigger
- enabled/paused state
- next run
- last run
- last status
- recent run history

Actions:

- `n` new job
- `e` edit job
- `space` pause/resume
- `r` run now
- `d` delete
- `Enter` inspect history/details

### Create And Edit UX

First slice should use a structured form/editor rather than a complex visual builder.

For `shell`:

- title
- command
- trigger
- enabled

For `chat_prompt`:

- title
- session picker
- multiline prompt
- trigger
- enabled

## Cross-Linking

Scheduled runs should connect to the rest of the operator surface.

From `Schedules`:

- open `Artifacts` when run has `artifact_ref`
- open `Chat` when run targets a session
- open terminal-style output view for shell result summaries later if needed

This keeps scheduler output inside the same operational workspace as the rest of the daemon UI.

## Persistence And Time Rules

All internal timestamps should be stored in UTC.

The UI can render timestamps in human-readable form, but scheduler evaluation itself must stay deterministic.

For the first slice:

- cron evaluation uses UTC
- run timestamps are stored in UTC
- `once_at` is stored as an absolute UTC timestamp

## Rollout Order

Recommended implementation order:

1. scheduler domain model and persistent store
2. trigger evaluator
3. daemon scheduler loop
4. `shell` runner
5. `chat_prompt` runner
6. daemon commands and websocket events
7. TUI `Schedules` tab
8. jump-outs into artifacts and sessions

This keeps the backend authoritative before adding operator UX.

## Testing Strategy

Need coverage for:

- interval scheduling
- cron next-run evaluation
- once-at auto-disable behavior
- persistence across daemon restart
- shell run recording and artifact offload
- chat prompt run recording and transcript integration
- websocket live updates
- TUI create/edit/pause/run-now flows

The scheduler loop itself should be tested with controllable time rather than wall-clock sleeps where possible.

## Summary

The daemon scheduler is a bounded operator automation surface.

It deliberately sits between two extremes:

- stronger than manual operator repetition
- much smaller than a workflow engine

The first slice should deliver durable, inspectable automation with:

- two job kinds
- three trigger kinds
- persistent state
- run history
- TUI control

without introducing workflow graphs, retries, or agent-authored scheduling.
