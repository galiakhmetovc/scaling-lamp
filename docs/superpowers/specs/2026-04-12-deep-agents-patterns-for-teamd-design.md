# Deep Agents Patterns For teamD Design

## Goal

Adopt the strongest long-running-agent ideas from the Deep Agents article without importing its framework model into `teamD`.

The target is not "make teamD look like Deep Agents". The target is:

- keep the current transport-agnostic runtime
- strengthen long-running task execution
- reduce prompt bloat from large tool outputs
- make workers return cleaner, more inspectable results
- preserve the teaching-first architecture of the project

## Scope

This design adds three subsystems:

1. persistent plan state
2. artifact offload for large tool outputs
3. worker handoff contract

This design does not cover:

- LangChain or LangGraph integration
- virtual filesystem as a general abstraction layer
- distributed mesh
- browser UI

## Why These Three

The article highlights five ideas:

- built-in planning
- filesystem-backed context offload
- subagents
- automatic context compression
- long-term memory

`teamD` already has meaningful pieces of:

- context compression
- long-term memory
- managed workers

The missing leverage is:

- first-class planning state
- explicit large-output offload into artifacts
- structured worker handoff back to the parent run

Those three directly improve long-running work without forcing a framework rewrite.

## Design Summary

The next runtime layer should look like this:

- `run` owns user-facing execution
- `plan` tracks the active multi-step intention of the run
- `artifacts` hold large tool outputs outside prompt history
- `worker` executes isolated delegated work
- `handoff` returns only the useful result of a worker to the parent

Compaction and memory continue to exist, but now they can reference:

- plan items
- artifact refs
- worker handoff summaries

instead of trying to carry large transient content in raw transcript text.

## 1. Persistent Plan State

## Purpose

Give `run` and `worker` a first-class structured plan that survives prompt turns and can be inspected through API and CLI.

This is the `teamD` analogue of `write_todos`, but as runtime state instead of prompt text.

## Data Model

New entities:

- `PlanRecord`
- `PlanItem`

Suggested fields:

- `plan_id`
- `owner_type` = `run | worker`
- `owner_id`
- `title`
- `status` = `pending | in_progress | completed | cancelled`
- `position`
- `notes`
- `created_at`
- `updated_at`

Suggested storage:

- `runtime_plans`
- `runtime_plan_items`

## Runtime Contract

The model does not edit plans by emitting arbitrary prose. It updates plan state through a tool or runtime-owned write path.

Minimum operations:

- create plan
- replace plan items
- mark item in progress
- mark item completed
- append note

## API Surface

- `GET /api/plans?owner_type=run&owner_id=<id>`
- `GET /api/plans/{id}`
- `POST /api/plans/{id}/items`
- `POST /api/plans/{id}/items/{item_id}/start`
- `POST /api/plans/{id}/items/{item_id}/complete`

## CLI Surface

- `teamd-agent plans show <plan_id>`
- `teamd-agent plans list <owner_type> <owner_id>`

## Why It Matters

Without this, long tasks only exist as:

- user text
- assistant text
- maybe continuity summary

That is too weak for long-lived execution. A persisted plan gives the runtime a stable skeleton for work.

## 2. Artifact Offload For Large Tool Outputs

## Purpose

When a tool returns a large result, `teamD` should avoid injecting the whole body into prompt history.

Instead:

- persist the full result as an artifact
- keep a short preview in transcript
- attach a stable `artifact_ref`
- let the agent explicitly read it later if needed

This is the highest-leverage practical idea from the article.

## Trigger Policy

Introduce an explicit offload policy:

- `max_inline_chars`
- `max_inline_lines`
- `enabled_for_tools`
- `preview_lines`

Suggested default behavior:

- small outputs stay inline
- large outputs are offloaded automatically
- transcript keeps:
  - tool name
  - artifact ref
  - byte/char count
  - short preview

## Data Model

Reuse existing `internal/artifacts` instead of inventing a new virtual filesystem.

Persist metadata:

- `artifact_ref`
- `owner_type`
- `owner_id`
- `kind` = `tool_output`
- `tool_name`
- `created_at`
- `content_type`
- `size_bytes`

## Runtime Behavior

Tool execution path becomes:

1. tool returns raw output
2. runtime checks offload policy
3. if output is large:
   - persist full output as artifact
   - write short transcript payload with preview + ref
4. if output is small:
   - keep it inline

## API Surface

- `GET /api/artifacts/{ref}`
- `GET /api/artifacts/{ref}/content`
- artifact refs also appear in run/job/worker responses and events

## Why It Matters

This strengthens three parts of the system at once:

- prompt assembly
- compaction quality
- debugging of tool-heavy runs

It is better than blunt truncation because the full result remains recoverable.

## 3. Worker Handoff Contract

## Purpose

A worker should not return "everything it did". It should return a structured handoff to its parent.

This is how we keep parent context clean without losing useful output.

## Handoff Shape

New structured result:

- `summary`
- `artifacts`
- `promoted_facts`
- `open_questions`
- `recommended_next_step`

Optional later:

- `confidence`
- `policy_flags`

## Data Model

New entity:

- `WorkerHandoff`

Suggested fields:

- `worker_id`
- `parent_run_id`
- `summary`
- `artifacts_json`
- `promoted_facts_json`
- `open_questions_json`
- `recommended_next_step`
- `created_at`

## Runtime Behavior

When worker work finishes:

1. worker transcript stays local
2. worker memory stays local by default
3. worker emits one structured handoff
4. parent receives handoff as the canonical result
5. only explicitly promoted facts may flow into shared memory

## Why It Matters

This is the clean equivalent of "subagent returns a summary".

It prevents:

- shared memory pollution
- parent prompt bloat
- unclear provenance of facts

## Relationship To Existing Systems

### Compaction

Compaction should prefer:

- plan summaries
- handoff summaries
- artifact refs

over copying large raw tool outputs.

### Memory

Memory promotion should treat worker handoff as higher-quality material than raw worker transcript.

Default rule:

- worker local transcript is not shared memory
- worker handoff may be promoted
- worker promoted facts may be promoted

### Events

New events:

- `plan.created`
- `plan.updated`
- `plan.item_started`
- `plan.item_completed`
- `artifact.offloaded`
- `worker.handoff_created`

## Rollout Order

The order matters.

### Phase 1

Artifact offload.

Reason:

- fastest quality win
- directly helps prompt pressure
- easiest to validate with tests and traces

### Phase 2

Persistent plan state.

Reason:

- makes long tasks inspectable
- provides a structured backbone for workers and future mesh

### Phase 3

Worker handoff contract.

Reason:

- cleanly upgrades worker usefulness
- depends on clearer artifact and planning semantics

## Non-Goals

Do not do these in this phase:

- import Deep Agents or LangGraph abstractions
- build a general virtual filesystem
- rewrite compaction from scratch
- rewrite memory model around file paths
- start mesh work

## Recommendation

Adopt the design in this exact order:

1. artifact offload
2. persistent plans
3. worker handoff

This gives `teamD` the strongest Deep Agents ideas while preserving its current adult architecture: one runtime core, one API, one CLI, multiple transports, and explicitly owned state.
