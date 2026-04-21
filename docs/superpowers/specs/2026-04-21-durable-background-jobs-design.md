# Durable Background Jobs Design

## Goal

Extend the canonical job model so `agentd` can persist long-running, session-scoped background work without introducing a second queue or a second execution path.

## Scope

This slice covers:

- durable background job domain shape
- persistence schema and repository support
- app-level read APIs for current-session background jobs
- minimal TUI/REPL visibility:
  - session header background counts
  - `\задачи` and `/jobs` for active jobs in the current session

This slice does not cover:

- worker loop execution
- heartbeats being actively updated by a daemon worker
- schedules
- cancellation execution semantics

## Constraints

- Preserve the existing mission/scheduler path.
- Do not create a second queue or a second background runtime.
- Keep TUI/CLI thin over the same app/runtime layer.
- Keep daemon-backed TUI using the same app APIs via HTTP/JSON.

## Domain Model

`JobSpec` becomes explicitly session-scoped:

- `session_id: String` becomes required
- `mission_id: Option<String>` becomes optional
- existing mission jobs keep working
- future session-only background jobs use `mission_id = None`

New durable fields are added to `JobSpec`:

- `attempt_count: u32`
- `max_attempts: u32`
- `lease_owner: Option<String>`
- `lease_expires_at: Option<i64>`
- `heartbeat_at: Option<i64>`
- `cancel_requested_at: Option<i64>`
- `last_progress_message: Option<String>`

New job kinds are introduced for the background runtime substrate:

- `ChatTurn`
- `ApprovalContinuation`

Existing kinds stay:

- `MissionTurn`
- `Verification`
- `Delegate`
- `Maintenance`

## Validation Rules

- `kind` must match `input.kind()`
- `MissionTurn` input requires `mission_id = Some(..)` and the same mission id in the payload
- non-mission job kinds may use `mission_id = None`

## Persistence

`jobs` table is extended to include:

- `session_id`
- nullable `mission_id`
- durable background fields listed above

Migration must preserve legacy rows:

- backfill `session_id` from the referenced mission when possible
- keep mission jobs intact
- preserve legacy ordering and ids

Repository support is extended with current-session queries:

- list jobs for a session
- list active jobs for a session

“Active” means:

- `queued`
- `running`
- `blocked`

Completed, failed, and cancelled jobs are not shown by default in the UI command.

## App Layer

Add canonical read helpers:

- `session_background_jobs(session_id)` -> structured active jobs
- `render_session_background_jobs(session_id)` -> human-readable text

`SessionSummary` is extended with:

- `background_job_count`
- `running_background_job_count`
- `queued_background_job_count`

These counts are current-session only.

## TUI / REPL

TUI session header shows:

- `bg=<total> (run=<running> queued=<queued>)`

Command surface:

- primary: `\задачи`
- alias: `/jobs`

Default output shows only active jobs for the current session with:

- `job id`
- `kind`
- `status`
- `queued/started since`
- `last progress`

REPL gets the same command aliasing and renderer through the same app API.

## Daemon Transport

To keep daemon-backed TUI thin and canonical, HTTP/JSON exposes current-session background job reads through the same app methods. No background execution loop is added in this slice.

## Testing

Required coverage:

- domain validation for mission and non-mission jobs
- record round-trip for new `JobSpec`
- store migration compatibility for legacy jobs
- session-scoped list queries
- TUI header rendering with background counts
- `\задачи` / `/jobs` showing active current-session jobs only
