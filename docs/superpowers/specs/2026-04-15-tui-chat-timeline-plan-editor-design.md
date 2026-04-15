# TUI Chat Timeline And Plan Editor Design

## Goal

Bring the TUI to an operator-usable state by:

- rendering tool and plan activity directly into the main `Chat` history as persistent markdown lines
- keeping full plan state and editing in the dedicated `Plan` tab
- adding pane-local scrolling everywhere

This keeps the main chat readable while preserving full operational detail in dedicated panes.

## Decisions

### 1. Chat becomes a markdown timeline

The `Chat` tab will stop being a plain transcript viewport with a separate status lane.

Instead it will render a session-scoped timeline composed of:

- user messages
- assistant live stream block
- assistant final markdown block
- tool lifecycle entries rendered as short markdown lines
- plan mutation entries rendered as short markdown lines

These entries are persistent from the TUI point of view: when a session is resumed, the timeline is rebuilt from persistent runtime events and transcript state.

Examples:

```md
- Tool: `fs_list` — `.`
- Tool result: `fs_list` returned 18 entries
- Plan: created `Refactor auth middleware`
- Task: added `Audit current middleware`
```

### 2. Full plan stays in `Plan`

The `Plan` tab becomes the only place that renders the full task tree.

The `Chat` tab only shows one-line plan mutation events. It does not show the whole plan.

This keeps the main conversation readable and avoids duplicating the plan tree in two places.

### 3. Plan editing is form-based

The `Plan` tab will support editing through a keyboard/mouse form flow, not free-form markdown editing.

Supported actions:

- create plan
- add task
- edit task description
- edit task dependencies
- set task status
- add task note

All edits must go through the existing event-sourced plan domain and emit the same `plan.*` / `task.*` events as model-driven plan tools.

### 4. Scrolling must be pane-local everywhere

Scrolling is required in:

- `Chat`
- `Plan`
- `Tools`
- `Settings` form pane
- `Settings` raw YAML pane

Each pane owns its own viewport or scroll state. There is no global page scroll model.

### 5. Streaming remains live

Streaming stays enabled in `Chat`.

During a running turn:

- the current assistant response is shown as a live stream block
- tool and plan events appear as timeline items between rounds

After the turn completes:

- the assistant block is finalized using the existing markdown renderer

### 6. Source of truth

The TUI continues to use:

- persistent runtime events and projections as the source of truth for durable state
- ephemeral UI bus events for live stream updates

No TUI-only hidden plan state should be introduced.

## Architecture

### Chat timeline model

Introduce a TUI-local timeline item model derived from runtime state:

- `timeline_user_message`
- `timeline_assistant_message`
- `timeline_assistant_stream`
- `timeline_tool_event`
- `timeline_plan_event`

The timeline is session-scoped.

The builder merges:

- transcript projection messages
- plan/tool runtime events from the event log
- current ephemeral stream buffer from the UI bus

### Plan editor model

The `Plan` tab gets two areas:

- left: scrollable task tree / plan summary
- right: form editor for selected plan or task

The editor performs actions by calling existing plan domain services through explicit TUI commands. It does not mutate projections directly.

### Tool log model

`Tools` remains a dedicated log/detail view, but it is no longer the only place to understand tool activity. The `Chat` timeline gets short markdown summaries; `Tools` keeps richer operational detail.

## Event Mapping

### Chat timeline entries

Timeline lines are derived from persistent events:

- `message.recorded`
- `tool.call.started`
- `tool.call.completed`
- `plan.created`
- `plan.archived`
- `task.added`
- `task.edited`
- `task.status_changed`
- `task.note_added`

The timeline entry text is derived at render/build time, not stored as separate duplicate event payloads.

### Plan editor writes

Operator edits from the `Plan` tab must emit normal domain events with:

- source identifying TUI operator actions
- actor identifying the current agent/config

## UX Rules

### Chat

- live stream stays visible
- tool and plan rows are short markdown list-style lines
- final assistant markdown is rendered normally
- no separate duplicated plan block in `Chat`

### Plan

- full plan tree visible
- selected node editable through form controls
- scrolling works independently from the rest of the screen

### Tools

- live log continues
- selection/details can remain follow-up if needed, but scrolling must work now

### Settings

- existing form/raw modes stay
- scrolling must work reliably inside both panes

## Constraints

- keep `--chat` entrypoint as the TUI
- keep non-interactive fallback for tests and scripted stdin
- do not break session-scoped plan semantics introduced in the current TUI baseline

## Testing

Add or expand tests for:

- timeline rebuild from session transcript plus tool/plan events
- plan events from one session not appearing in another session timeline
- default and switched pane scrolling behavior
- plan editor actions emitting correct domain events
- rendered chat history including short tool/plan markdown lines

## Out Of Scope

Not in this slice:

- visual mouse-heavy drag-and-drop plan editing
- full TUI tool manual execution
- arbitrary YAML schema-aware settings editing beyond current known fields
