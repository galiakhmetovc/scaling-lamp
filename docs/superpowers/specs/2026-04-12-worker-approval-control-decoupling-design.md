# Worker Approval Control And Runtime Decoupling

## Goal

Finish the remaining control-plane work that still leaks transport assumptions into runtime behavior.

This slice has two concrete outcomes:

1. Worker approvals must be first-class operator state in CLI chat and API/events.
2. Prompt/context and control/status assembly must move out of Telegram-specific code into runtime-owned helpers.

## Problems

### Worker approvals are not operator-visible enough

Today a worker can enter `waiting_approval`, but the operator chat does not surface that state clearly enough.

The operator currently sees:

- the main run is still active
- a later assistant explanation that a worker is blocked

This is too late and too indirect. The control plane should surface the approval request as soon as it exists.

### Prompt/context assembly is still transport-scoped

Prompt context injection currently lives in:

- `internal/transport/telegram/prompt_context.go`

That means runtime execution still depends on Telegram for:

- workspace system context
- memory recall block
- skills catalog block
- active skills block

These are runtime concerns, not Telegram concerns.

### Control/status assembly is still fragmented

Approval actions, status rendering, and active child state are split between:

- generic runtime state
- CLI operator chat
- Telegram-only UI state

The operator needs one canonical runtime view that explains:

- active run
- waiting approvals
- active workers
- active jobs
- plans and artifacts

## Design

### 1. First-class worker approval events

Runtime emits explicit events for worker approval waits:

- `worker.approval_requested`

Payload must include:

- `worker_id`
- `approval_id`
- `tool`
- `reason`
- `run_id`

This event becomes the canonical source for operator chat, CLI status, and future Web UI.

### 2. Runtime-owned prompt context assembler

Move transport-agnostic prompt fragments into runtime:

- workspace context
- automatic memory recall
- skills catalog
- active skills

Telegram should only provide transport-specific formatting or transport-specific user interaction. It should not own prompt-context composition.

### 3. Runtime-owned control snapshot

Add one runtime helper that assembles a control snapshot for an active run or session:

- active run status
- pending approvals
- active workers and their statuses
- active jobs and their statuses

CLI chat `/status` should render this snapshot.
Telegram may render the same underlying snapshot differently later.

### 4. Thin Telegram boundary

Telegram remains responsible for:

- polling and update normalization
- callback transport delivery
- message rendering
- Telegram-specific status cards

It should stop owning:

- prompt/context logic
- approval state semantics
- generic control/status semantics

## Testing

Add failing tests first for:

1. worker approval event emission
2. worker approval rendering in CLI chat
3. `/status` showing waiting worker approval
4. runtime prompt context assembly independent of Telegram

Then implement minimal code to pass.

## Non-goals

- no worker process supervision
- no mesh changes
- no Web UI
- no full replacement of Telegram UI state machinery
