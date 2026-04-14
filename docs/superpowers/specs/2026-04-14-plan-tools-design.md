# Plan Tools Domain Design

This document defines the clean-room internal planning domain for the agent.

It is a new runtime domain.
It is not a `bd` adapter.
External issue sync may be added later, but is explicitly out of scope for the first slice.

## Goal

Add an internal event-sourced planning system that:

- maintains exactly one active plan at a time
- archives replaced plans instead of deleting them
- exposes plan-management tools to the model through the normal clean-room tool surface
- projects a compact plan summary into the session head

## Scope

First-slice scope:

- internal plan storage and history
- plan-management tools
- plan projections for active plan, archive, and session-head rendering
- tool exposure through existing `ToolContract`
- tool-call safety through existing `ToolExecutionContract`

Out of scope:

- external `bd` synchronization
- deleting plans or tasks
- multiple active plans
- fancy terminal UI for plans

## Domain Boundaries

### Plan Domain

`PlanContract` is responsible for:

- plan lifecycle
- task lifecycle
- status transition rules
- archive policy
- session-head projection format

### Plan Tool Domain

`PlanToolContract` is responsible for:

- which plan-management tools are available to the model
- how those tools are described in the tool catalog
- how tool calls map into plan-domain commands

Important boundary:

- plan tools must be real clean-room tools
- they are not ad hoc helper functions in chat runtime
- they must be exposed through `ToolContract`
- they must pass through `ToolExecutionContract`

## Single Active Plan Rule

The runtime may have only one active plan at a time.

When `init_plan` is called:

1. if an active plan exists, it is archived
2. a new active plan is created

Archive is persistent.
Archive is never silent deletion.

## Data Model

### Plan

- `id`
- `goal`
- `status`
- `created_at`
- `archived_at` optional

### Task

- `id`
- `plan_id`
- `parent_task_id` optional
- `depends_on[]`
- `description`
- `status`
- `order`
- `notes[]`
- `blocked_reason` optional

### Note

- `text`
- `created_at`

## Status Model

### Plan statuses

- `active`
- `archived`

### Task statuses

- `todo`
- `in_progress`
- `done`
- `blocked`
- `cancelled`

## Status Transition Rules

Allowed transitions:

- `todo -> in_progress`
- `todo -> blocked`
- `todo -> cancelled`
- `in_progress -> done`
- `in_progress -> blocked`
- `in_progress -> cancelled`
- `blocked -> todo`
- `blocked -> in_progress`
- `blocked -> cancelled`

Terminal states:

- `done`
- `cancelled`

Additional rules:

- task cannot become `done` while it has children not in terminal states
- task cannot become `cancelled` while it has active children unless they are first transitioned
- task may be explicitly `blocked`
- task may also be temporarily not ready because one or more dependencies are not done
- task cannot depend on itself
- dependency graph must remain acyclic
- there is no delete operation

## Event Model

Aggregate types:

- `plan`
- `plan_task`

Events:

- `plan.created`
- `plan.archived`
- `task.added`
- `task.status_changed`
- `task.note_added`
- `task.edited`

Event payloads should include enough information to rebuild projections without reading current snapshots.

## Projection Model

### ActivePlanProjection

Responsibility:

- current active plan
- current active tasks
- current task notes

### PlanArchiveProjection

Responsibility:

- archived plans metadata
- archived task trees

### PlanHeadProjection

Responsibility:

- compact operator/model-facing rendering input for session head

This projection also computes:

- `ready`
  - task status is `todo`
  - all dependencies are `done`
  - task is not explicitly `blocked`
- `waiting_on_dependencies`
  - task status is `todo`
  - at least one dependency is not `done`

This projection is the source for session-head summary, not ad hoc chat/runtime formatting.

## Session Head Integration

`SessionHeadPolicy.projection_summary` must gain plan awareness through projections, not direct tool/runtime state.

Expected rendered shape is approximately:

```text
🎯 Цель: Рефакторинг модуля авторизации

✅ [t1] Спроектировать новую БД схему
⏳ [t2] Написать мидлварь
  ✅ [t2.1] Парсинг JWT токенов
  🏃 [t2.2] Валидация прав доступа
     📝 Упираемся в то, что роли хранятся в кэше, а не в БД
⬜ [t3] Интегрировать в роуты
🚫 [t4] Написать тесты (Blocked: ждем подтверждение от Васи)
```

Important constraint:

- session head must stay compact
- projection decides what to truncate
- model should see current actionable state, not full archive

Important modeling rule:

- `ready` is a computed projection/view
- it is not a persisted task status in the event model

## Tools

First-slice tool set:

- `init_plan(goal)`
- `add_task(plan_id, description, parent_task_id?)`
- `set_task_status(task_id, new_status, blocked_reason?)`
- `add_task_note(task_id, note_text)`
- `edit_task(task_id, new_description)`

Deliberately not included:

- `delete_task`
- `get_plan`

Reason:

- history must be preserved
- current plan should already be injected into session head

## Tool Exposure

Plan tools must be surfaced through the existing clean-room tool system:

1. plan tool definitions are produced by plan-tool runtime
2. `ToolContract` selects whether they are visible
3. `ToolSerializationPolicy` serializes them into provider request body
4. provider-emitted tool calls pass through `ToolExecutionContract`
5. plan tool executor applies domain commands and emits plan events

This is the critical integration point.

Without it, plan tools would exist only on paper and would not actually be available to the agent.

## Safety Model

Plan tools are internal runtime tools.

Recommended first-slice safety posture:

- access allowlist controlled by `ToolExecutionContract`
- approval policy can be `always_allow` initially
- sandbox policy can be `default_runtime`

This is acceptable because plan tools mutate only internal event-sourced state, not filesystem or shell.

## Storage Model

No separate special-purpose plan database is needed in the first slice.

Plan domain uses:

- existing event log
- existing projection snapshot store

Archive therefore lives in:

- persisted events
- persisted plan projections

## Recommended Implementation Order

1. Add plan event model and projections
2. Add plan command service
3. Add plan-tool definitions and executor
4. Expose plan tools through `ToolContract`
5. Feed `PlanHeadProjection` into `SessionHeadPolicy.projection_summary`
6. Add end-to-end tests where agent receives plan tools and mutates plan state through tool calls

## Known Follow-Up

Future work, not first slice:

- sync adapter from internal plan to `bd`
- import external issue context into plan initialization
- richer task metadata
- operator commands for plan inspection in CLI
- archive compaction policy for old plans
