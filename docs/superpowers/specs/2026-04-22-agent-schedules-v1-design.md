# Agent Schedules V1 Design

## Goal

Define the first operator-usable schedule model for `agentd` so an agent can be launched automatically in a project workspace without introducing a second runtime path.

This slice focuses on:

- two concrete schedule modes:
  - `interval`
  - `after_completion`
- two delivery modes:
  - `fresh_session`
  - `existing_session`
- visible scheduled sessions in the normal session list
- schedule-owned fresh-session and fixed-session launches
- schedule-safe execution semantics without interactive approvals
- TUI-first operator UX for listing, creating, toggling, inspecting, and deleting schedules

## Scope

This design covers:

- `AgentSchedule` domain and persistence changes
- scheduler semantics for `interval` and `after_completion`
- auto-approved scheduled execution policy
- session metadata needed to mark schedule-created sessions and schedule-origin messages
- TUI browser behavior for schedules
- minimum CLI/HTTP parity required to preserve the thin-client architecture

This design does not cover:

- trigger-based schedules
- cron syntax
- in-place schedule editing beyond enable/disable in `v1`
- heuristic retargeting to an arbitrary "latest matching" session
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
- Scheduled execution must use normal visible sessions and normal queued messages, not a hidden synthetic run type.
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
- `delivery_mode`
  - `fresh_session`
  - `existing_session`
- `target_session_id`
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

`target_session_id` is used only for `delivery_mode=existing_session`.

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

## Delivery Modes

### `fresh_session`

Each fire creates a brand new session.

This is the default behavior and works for both schedule modes.

### `existing_session`

Each fire targets one specific persisted session via `target_session_id`.

Rules:

- the target session must already belong to the same agent profile as the schedule
- if the target session has been deleted, the scheduler creates a new session and rewrites `target_session_id` to the new session id
- the scheduler never auto-retargets to an arbitrary "latest matching session"

## Launch Semantics

Each schedule fire resolves its target according to `delivery_mode`.

For `fresh_session`, runtime creates a new session.

For `existing_session`, runtime targets the stored `target_session_id`.

In both cases, the effective session:

- is bound to the schedule's `agent_profile_id`
- uses the schedule's `workspace_root`
- receives the saved `prompt` as its initial incoming message
- runs through the normal chat/runtime/tool loop

If `delivery_mode=existing_session` and the stored session no longer exists:

- runtime creates a new replacement session
- persists the replacement as the new `target_session_id`
- continues future launches against that replacement session

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

Scheduled input shown inside an existing session should be visibly marked as schedule-origin, for example:

- `расписание: <id>`

## TUI UX

The schedule browser should become operational rather than read-only.

### List View

Each row should show:

- `id`
- `agent`
- `mode`
- `delivery_mode`
- `enabled/disabled`
- next run summary
- last result summary
- short prompt preview

### Detail View

The detail pane should show:

- full prompt
- `workspace_root`
- `mode`
- `delivery_mode`
- `interval_seconds`
- `next_fire_at`
- `last_triggered_at`
- `last_finished_at`
- `last_session_id`
- `last_job_id`
- `last_result`
- `last_error`
- `target_session_id`

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
4. `delivery_mode`
5. `interval_seconds`
6. `prompt`
7. optional `target_session_id` when `delivery_mode=existing_session`

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

Mode and delivery interactions:

- `interval + fresh_session`
  - normal fresh-session cadence
- `interval + existing_session`
  - if the target session is currently busy with an active run, the current tick is skipped
- `after_completion + fresh_session`
  - the next fire time is based on the terminal completion of the previous schedule-owned run
- `after_completion + existing_session`
  - completion is tracked only for runs launched by this exact schedule in that target session
  - manual operator turns and other schedules in the same session do not satisfy completion for this schedule
  - if the target session is busy, the scheduled message may be queued into that same session rather than dropped

## Error Handling

- Missing agent:
  - schedule run does not crash scheduler
  - `last_result=failed`
  - `last_error` records the reason
- Missing workspace:
  - same failure handling as above
- Deleted `target_session_id` for `existing_session`:
  - runtime creates a replacement session
  - rewrites `target_session_id`
- Run failure:
  - schedule records `last_result=failed`
  - `last_error` stores the terminal reason
- Approval-demanding behavior should not appear in scheduled runs because the execution policy is auto-approved

## Testing

Required coverage:

- schedule record/domain round-trips with `mode`, `enabled`, and terminal result fields
- schedule record/domain round-trips with `delivery_mode` and optional `target_session_id`
- `interval` schedules fire on stable cadence without burst catch-up
- `after_completion` schedules do not relaunch until the previous run is terminal
- `existing_session` schedules target a concrete session id
- deleted `target_session_id` causes replacement-session creation and schedule rebinding
- `interval + existing_session` skips ticks while the target session is busy
- `after_completion + existing_session` tracks only schedule-owned completion and may queue work into the target session
- scheduled launches create fresh sessions only when `delivery_mode=fresh_session`
- scheduled launches use auto-approved execution semantics
- schedule-created sessions are marked in session metadata and visible in list rendering
- scheduled messages in an existing session are visibly marked as `расписание: <id>`
- TUI schedule browser supports create, enable/disable, delete, and detailed inspection
- daemon-backed TUI/CLI paths use the same schedule semantics as direct app calls

## Rollout Order

Recommended implementation order:

1. extend `AgentSchedule` domain, records, and schema
2. extend scheduler/background semantics for `interval`, `after_completion`, `fresh_session`, and `existing_session`
3. add session origin metadata for schedule-created sessions and scheduled messages
4. update app/CLI/HTTP rendering for richer schedule state
5. upgrade TUI schedule browser to operational UX

## Follow-On Work

After `v1`:

1. trigger-based schedules once the event model exists
2. cron expressions
3. in-place schedule editing
4. filters for manual vs schedule-created sessions
