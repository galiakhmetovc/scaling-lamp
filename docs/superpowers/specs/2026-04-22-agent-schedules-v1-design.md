# Agent Schedules V1 Design

## Goal

Define the first operator-usable schedule model for `agentd` so an agent can be launched automatically in a project workspace without introducing a second runtime path.

This slice focuses on:

- two concrete schedule modes:
  - `interval`
  - `after_completion`
- visible scheduled sessions in the normal session list
- schedule-owned fresh-session launches
- schedule-safe execution semantics without interactive approvals
- TUI-first operator UX for listing, creating, toggling, inspecting, and deleting schedules

## Scope

This design covers:

- `AgentSchedule` domain and persistence changes
- scheduler semantics for `interval` and `after_completion`
- auto-approved scheduled execution policy
- session metadata needed to mark schedule-created sessions
- TUI browser behavior for schedules
- minimum CLI/HTTP parity required to preserve the thin-client architecture

This design does not cover:

- trigger-based schedules
- cron syntax
- in-place schedule editing beyond enable/disable in `v1`
- reusing the same session for repeated schedule runs
- hidden schedule-only sessions

## Constraints

- Preserve one canonical runtime path. Scheduled runs must still use the existing session creation, prompt assembly, tool routing, provider loop, background job, wake-up, and transcript paths.
- Keep prompt assembly ordered as:
  1. `SYSTEM.md`
  2. `AGENTS.md`
  3. `SessionHead`
  4. `Plan`
  5. `ContextSummary`
  6. offload refs
  7. uncovered transcript tail
- Scheduled launches must create normal sessions, not a hidden synthetic run type.
- Scheduled sessions remain visible in the main session list with an explicit mark.
- Schedule execution must not block forever on interactive approvals.

## Approaches

### Recommended: One Schedule Entity With Explicit Mode

Use one `AgentSchedule` model with a `mode` field:

- `interval`
- `after_completion`
- reserved later: `trigger`

Benefits:

- preserves one scheduler and one persistence path
- avoids splitting nearly identical schedule entities
- allows `trigger` to be added later without reshaping operator concepts

### Alternative: Separate Interval And Completion-Cadence Entities

Store different schedule types separately.

This makes the first implementation look smaller, but it duplicates rendering, persistence, and scheduler logic while making future expansion harder.

### Alternative: Mission-Only Scheduling Without AgentSchedule Identity

Encode everything as synthetic missions and infer schedule behavior from existing mission fields.

This reuses substrate, but it produces poor operator UX because there is no stable schedule object to inspect, enable/disable, or reason about.

## Data Model

`AgentSchedule` becomes a durable first-class entity with:

- `id`
- `agent_profile_id`
- `workspace_root`
- `prompt`
- `mode`
  - `interval`
  - `after_completion`
- `interval_seconds`
- `next_fire_at`
- `enabled`
- `last_triggered_at`
- `last_finished_at`
- `last_session_id`
- `last_job_id`
- `last_result`
- `last_error`
- `created_at`
- `updated_at`

`trigger` remains reserved but is not operator-creatable in `v1`.

## Schedule Modes

### `interval`

This is wall-clock cadence.

Semantics:

- the schedule tries to fire on a stable rhythm
- the next due time is advanced from the previous schedule cadence, not from worker slippage
- if the worker wakes up late, the system does not launch multiple catch-up runs in a burst
- only one active launch per schedule may exist at a time

This mode is for checks like:

- "every 5 minutes check queue state"
- "every hour re-run workspace review"

### `after_completion`

This is completion-relative cadence.

Semantics:

- a new run is eligible only after the previous run reaches a terminal outcome
- the next fire time is computed from `last_finished_at + interval_seconds`
- no new run starts while the previous run is still active

This mode is for loops like:

- "10 minutes after the previous run finishes, run again"
- "keep polling, but avoid overlap and avoid drift from long executions"

## Launch Semantics

Each schedule fire creates a **fresh new session**.

The session:

- is bound to the schedule's `agent_profile_id`
- uses the schedule's `workspace_root`
- receives the saved `prompt` as its initial incoming message
- runs through the normal chat/runtime/tool loop

`v1` does not reuse a prior scheduled session.

## Scheduled Execution Policy

Scheduled launches use automatic approval semantics.

That means:

- interactive approval flow is not used for schedule-owned turns
- execution proceeds as if auto-approve were enabled for that scheduled launch
- if the run still ends in an unrecoverable error, the schedule records that as a terminal failed result

This avoids schedules stalling forever on operator approvals and matches the expectation that scheduled work is autonomous.

## Session Visibility

Schedule-created sessions must remain visible in the normal session list.

They are not hidden in a separate queue or archive.

They carry an explicit schedule mark so the operator can distinguish them from manual sessions.

Minimum visible metadata:

- that the session was created by a schedule
- which schedule created it

This requires durable session-side metadata for schedule origin, for example:

- `agent_schedule_id`

## TUI UX

The schedule browser should become operational rather than read-only.

### List View

Each row should show:

- `id`
- `agent`
- `mode`
- `enabled/disabled`
- next run summary
- last result summary
- short prompt preview

### Detail View

The detail pane should show:

- full prompt
- `workspace_root`
- `mode`
- `interval_seconds`
- `next_fire_at`
- `last_triggered_at`
- `last_finished_at`
- `last_session_id`
- `last_job_id`
- `last_result`
- `last_error`

### TUI Actions

`v1` needs these actions:

- `Н` create schedule
- `Enter` inspect details
- `П` enable/disable schedule
- `У` delete schedule

Creation should use a dialog/wizard rather than one raw free-form line.

Recommended creation flow:

1. `id`
2. `agent`
3. `mode`
4. `interval_seconds`
5. `prompt`

## CLI And HTTP Surface

The existing command and daemon surface stays thin over the same app/runtime layer.

That means:

- CLI/REPL commands remain available
- daemon HTTP keeps schedule create/show/list/delete endpoints
- TUI continues using the same backend traits and daemon client rather than inventing a private schedule path

## Scheduler Behavior

For both modes:

- at most one active run per schedule
- the scheduler must not enqueue duplicate overlapping launches for the same schedule
- disabled schedules are skipped but retained

Mode-specific updates:

- `interval`
  - advance `next_fire_at` by cadence rules after dispatch
- `after_completion`
  - update `next_fire_at` only after the launched session reaches terminal completion

## Error Handling

- Missing agent:
  - schedule run does not crash scheduler
  - `last_result=failed`
  - `last_error` records the reason
- Missing workspace:
  - same failure handling as above
- Run failure:
  - schedule records `last_result=failed`
  - `last_error` stores the terminal reason
- Approval-demanding behavior should not appear in scheduled runs because the execution policy is auto-approved

## Testing

Required coverage:

- schedule record/domain round-trips with `mode`, `enabled`, and terminal result fields
- `interval` schedules fire on stable cadence without burst catch-up
- `after_completion` schedules do not relaunch until the previous run is terminal
- scheduled launches create fresh sessions, not reused threads
- scheduled launches use auto-approved execution semantics
- schedule-created sessions are marked in session metadata and visible in list rendering
- TUI schedule browser supports create, enable/disable, delete, and detailed inspection
- daemon-backed TUI/CLI paths use the same schedule semantics as direct app calls

## Rollout Order

Recommended implementation order:

1. extend `AgentSchedule` domain, records, and schema
2. extend scheduler/background semantics for `interval` and `after_completion`
3. add session origin metadata for schedule-created sessions
4. update app/CLI/HTTP rendering for richer schedule state
5. upgrade TUI schedule browser to operational UX

## Follow-On Work

After `v1`:

1. trigger-based schedules once the event model exists
2. cron expressions
3. in-place schedule editing
4. filters for manual vs schedule-created sessions
