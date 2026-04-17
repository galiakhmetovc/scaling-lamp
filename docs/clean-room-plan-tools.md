# Clean-Room Plan Tools

This document describes the current internal plan-tools domain in `rewrite/clean-room-root`.

It is the runtime-backed internal planning system for the agent.
It is not a `bd` adapter.

## What Exists Today

Current domain pieces:

- event-sourced plan lifecycle
- event-sourced task lifecycle
- active-plan, archive, and head projections
- plan-management tools exposed through the normal tool surface
- execution of allowed plan tool calls through `ToolExecutionContract`
- compact plan rendering inside the session head

## Current Rule

Only one active plan may exist at a time.

When `init_plan` runs:

1. the current active plan is archived
2. a new active plan is created

Archive is persisted through the event log and projection snapshots.

## Current Tool Set

Currently exposed built-in plan tools:

- `init_plan`
- `add_task`
- `set_task_status`
- `add_task_note`
- `edit_task`
- `plan_snapshot`
- `plan_lint`

Current built-in tool schema support:

- `add_task.depends_on[]`
- `edit_task.new_depends_on[]`
- read-only snapshot and lint tools with empty parameter objects

## Current Execution Path

Current runtime path for a provider-emitted plan tool call:

1. provider returns `tool_calls`
2. provider client parses them and evaluates them through `ToolExecutionContract`
3. runtime tool loop executes allowed plan tools through `internal/runtime/plans.Service`
4. resulting plan events are appended to the shared event log
5. projections update active plan, archive, and head state
6. runtime sends tool-result messages back through the provider loop
7. provider returns the final assistant message

Important boundary:

- provider client does not mutate plan state directly
- plan mutation happens in runtime through the plan-domain service

## Current Projections

Current projections used by this domain:

- `active_plan`
- `plan_archive`
- `plan_head`

`plan_head` computes:

- `ready`
- `waiting_on_dependencies`
- `blocked`
- recent task notes

`ready` is a computed view.
It is not a persisted task status.

## Current Session Head Behavior

Current shipped `zai-smoke` behavior:

- session head is rendered at outbound `messages[0]`
- it includes:
  - static title
  - `session_id`
  - last user message
  - last assistant message
  - compact active plan summary when a plan exists

## Current Shipped Config

Current shipped `zai-smoke` config enables:

- `PlanToolContract`
- `ToolContract` allowlist for the five plan tools
- `ToolExecutionContract` allowlist for the same plan tools plus read-only `plan_snapshot` and `plan_lint`
- `plan_head` projection in runtime

So the live `zai-smoke` agent can now:

- expose plan tools to the model
- allow those plan tools through the gate
- execute them against the internal plan domain

## Current Limits

Not implemented yet:

- sync between internal plan domain and `bd`
- non-plan tool execution backends
- explicit CLI plan inspection commands
- richer archive browsing
