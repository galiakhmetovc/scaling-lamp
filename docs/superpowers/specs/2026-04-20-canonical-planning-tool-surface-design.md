# Canonical Planning Tool Surface Design

## Goal

Add structured planning state and typed planning tools on the same canonical
runtime path as chat and tool execution, so the model can inspect and update a
session plan without relying on transcript prose or UI-local state.

This slice should:

- persist a canonical `PlanSnapshot` per session
- expose typed `plan_read` and `plan_write` tools
- surface the current plan back into prompt assembly as a synthetic system
  message

## Non-Goals

- no TUI planning UI in this slice
- no mission/job scheduler integration in this slice
- no second prompt path for plans
- no transcript scraping for plan state
- no incremental plan mutation tool family beyond the first minimal surface

## Canonical Runtime Shape

Introduce a real domain model in `agent-runtime`:

- `PlanSnapshot`
  - `session_id`
  - `items`
  - `updated_at`
- `PlanItem`
  - `id`
  - `content`
  - `status`
- `PlanItemStatus`
  - `pending`
  - `in_progress`
  - `completed`

The plan is canonical persisted state. It is not reconstructed from transcript
messages.

## Tool Surface

Add a new tool family:

- `plan`

Add two typed tools:

- `plan_read`
- `plan_write`

### `plan_read`

Reads the current plan for the active session.

It is:

- read-only
- non-destructive
- no approval by default

### `plan_write`

Atomically replaces the full current plan for the active session.

This intentionally avoids a first-wave mutation surface like
`plan_add_step`/`plan_complete_step`/`plan_remove_step`. A full replacement
tool keeps the state transition simple and deterministic.

It is:

- not read-only
- not destructive to the external workspace
- no approval by default

## Permission Model

Planning tools live inside the canonical permission model.

`plan_write` should remain allowed in `plan` mode, even though it is not
read-only. Otherwise the dedicated `plan` mode would paradoxically block the
planning surface itself.

That means:

- `plan_read` is allowed
- `plan_write` is allowed
- non-planning non-read-only tools remain denied in `plan` mode

## Persistence

Add a canonical persisted record in `agent-persistence`:

- `PlanRecord`
  - `session_id`
  - `items_json`
  - `updated_at`

Add a `plans` table keyed by `session_id`.

The plan should cascade with session deletion.

## Prompt Assembly

Extend canonical prompt assembly order to:

1. `SessionHead`
2. `PlanSnapshot` synthetic system message, if present and non-empty
3. `ContextSummary`, if present
4. uncovered transcript tail

This keeps planning state visible to every normal chat turn through the same
prompt assembly path already used for context summary and filesystem/VFS state.

## Rendering Rules

`PlanSnapshot` should render compactly and stably, for example:

```text
Plan:
- [pending] inspect-runtime: Inspect runtime seams
- [in_progress] add-store: Add persisted plan storage
- [completed] prompt-head: Extend prompt assembly
```

If the plan is empty, omit the synthetic system message entirely.

## Execution Surface

Planning tools must run through the same `ExecutionService` loop as the other
model-facing tools.

This means:

- they participate in provider tool-calling
- they use the same permission resolution path
- they record stable tool-completion run steps
- they do not require a separate UI or app-local side channel

Planning tools are session-scoped, so execution should use the active session
context instead of asking the model to pass `session_id`.

## Testing

Required coverage:

- plan domain renders stable system text
- persistence round-trips `PlanSnapshot`
- prompt assembly orders `session head -> plan -> context summary -> transcript`
- permission `plan` mode still allows `plan_write`
- a provider-driven chat turn can use `plan_write`/`plan_read` on the canonical
  tool loop and persist the resulting plan
