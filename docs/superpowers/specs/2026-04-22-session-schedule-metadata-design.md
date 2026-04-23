# Session And Schedule Metadata Design

## Goal

Make agent-driven and schedule-driven activity visible in the main operator surfaces without forcing the operator to open agent or schedule inspector screens.

## Problem

The runtime already stores most of the useful state:
- current session agent
- schedule origin markers via `scheduled_by`
- schedule state in persisted agent schedules
- run and job state in active status views

But those signals are exposed unevenly:
- session lists only show agent name and a raw schedule marker
- chat headers do not summarize schedule state
- `render_active_run` shows execution detail but not enough session/schedule context
- prompt/session-head style system surfaces also omit agent/schedule metadata

This makes operator workflows slower than necessary because the user has to drill into inspector views to answer simple questions like:
- which agent is active here?
- was this session spawned by a schedule?
- when does that schedule fire next?
- is it enabled?
- what happened on the last schedule run?

## Approach

Add one canonical metadata summary at the app/bootstrap layer and reuse it everywhere else.

The design keeps one runtime path:
- `SessionSummary` becomes the app-level operator summary for a session
- `SessionHead` becomes the prompt/debug-facing rendering of the same essential metadata
- TUI, CLI, and HTTP stay thin over these shared summaries

No separate status formatter, no second prompt path, and no TUI-only metadata object.

## Data Model

Extend `SessionSummary` with a nested optional schedule summary:
- `schedule_id`
- `mode`
- `delivery_mode`
- `enabled`
- `next_fire_at`
- `target_session_id`
- `last_result`
- `last_error`

Keep `scheduled_by` as the simple origin marker for compatibility and quick checks.

Also extend `SessionHead` with:
- current agent display (`name` + `id`)
- optional schedule summary line

The schedule summary is populated only when the session has a schedule origin or an attached schedule summary can be resolved canonically from persisted state.

## Rendering Rules

### Session lists

Show agent more explicitly and compactly:
- `агент=Ассистент (default)`

If a schedule summary exists, append a compact schedule capsule:
- `расписание=pulse enabled next=...`

Do not dump full schedule details into the list view.

### Chat header

Keep the current dense first line for session/runtime settings.
Add one dedicated metadata line for operator-facing session origin:
- current agent
- schedule id
- mode/delivery
- enabled
- next fire
- last result or last error, if present

### Active run status

Before step/process detail, show the session context:
- session title
- agent name and id
- schedule summary if present

This keeps `\статус` useful even when the active run detail is mostly tool/process output.

### System/debug-like surfaces

`SessionHead.render()` should include:
- `Agent: <name> (<id>)`
- optional `Schedule: ...`

That makes prompt/debug bundles more self-explanatory and aligns them with the operator UI.

## HTTP Surface

`SessionSummaryResponse` and any direct session detail response that mirrors summary data should carry the new schedule summary fields so daemon-backed TUI/CLI do not fork formatting logic.

## Testing

Add or update tests for:
- session summary builder resolving schedule metadata from persisted schedules
- `render_active_run` including agent/schedule summary lines
- TUI session header rendering the new metadata
- session list rendering compact schedule metadata
- HTTP summary round-trip for new fields

## Non-Goals

- Replacing the detailed schedule inspector
- Building schedule history UI here
- Changing scheduling semantics
- Adding a second metadata/status API just for TUI
