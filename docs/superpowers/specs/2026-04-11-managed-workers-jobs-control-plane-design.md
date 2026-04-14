# Managed Workers, Jobs, And Runtime Control Plane Design

## Goal

Turn `teamD` from an API-first single-agent runtime into a transport-agnostic local control plane that can:

- run restart-safe background jobs
- supervise managed worker sessions with their own LLM loop
- expose both through one runtime event model
- keep Telegram, CLI, and a future web UI as clients of the same core API
- avoid introducing mesh or peer routing at this stage

## Scope

This design covers:

- runtime event plane
- background jobs subsystem
- managed workers subsystem
- unified error model
- approval and policy integration for jobs/workers
- CLI and API surfaces for all of the above
- tool exposure so the main agent can delegate to jobs and workers

This design does **not** cover:

- distributed mesh
- peer discovery
- cross-node scheduling
- browser UI implementation

## Design Summary

The next platform layer has three first-class control-plane objects:

- `runs` — existing agent requests
- `jobs` — supervised background process executions
- `workers` — supervised local subagent runtimes with inbox/outbox and their own LLM loop

`jobs` and `workers` are separate. A worker may internally start jobs, but a worker is not modeled as a job.

All three emit events into one runtime event plane. API, CLI, Telegram, and future UI consume state from this plane instead of reconstructing lifecycle from transport-specific behavior.

## Why Jobs And Workers Must Be Separate

Trying to model workers as a thin layer on top of jobs looks simpler but collapses two different abstractions:

- a job is process execution with logs, stdout/stderr, lifecycle, and cancellation
- a worker is an agent session with inbox/outbox, memory, tools, approvals, and LLM reasoning

If these are conflated, the job model becomes polluted with agent-specific fields and the worker model loses clarity.

The adult design is:

- jobs = background execution primitive
- workers = managed local subagent runtime
- event plane = shared observability/control substrate

## Core Runtime Model

### 1. Runtime Event Plane

Every state transition for runs, jobs, workers, and approvals must emit a typed event.

Event classes:

- `run.created`
- `run.started`
- `run.waiting_approval`
- `run.completed`
- `run.failed`
- `run.cancelled`
- `job.created`
- `job.started`
- `job.stdout`
- `job.stderr`
- `job.completed`
- `job.failed`
- `job.cancelled`
- `worker.created`
- `worker.started`
- `worker.message`
- `worker.tool_call`
- `worker.waiting_approval`
- `worker.completed`
- `worker.failed`
- `worker.closed`
- `approval.created`
- `approval.decided`

Requirements:

- monotonically ordered per entity
- stable event cursor for polling
- enough metadata to reconstruct lifecycle from the API alone
- transport-neutral payloads

First delivery mode:

- cursor-based polling endpoints

Later:

- streaming endpoint can be layered on the same event store

### 2. Unified Error Model

Every API-visible failure must map into a stable typed error envelope.

Error classes:

- `validation_error`
- `not_found`
- `conflict`
- `policy_denied`
- `approval_required`
- `runtime_unavailable`
- `provider_error`
- `tool_error`
- `job_error`
- `worker_error`
- `timeout`
- `cancelled`
- `internal_error`

Each error must say:

- code
- message
- entity type
- entity id if available
- retryability

This is required before jobs/workers, otherwise API behavior becomes inconsistent across subsystems.

## Background Jobs Subsystem

## Purpose

Jobs are long-running or detached executions under runtime supervision.

Capabilities:

- start a process with arguments and environment policy
- capture stdout/stderr
- persist lifecycle
- cancel gracefully
- recover interrupted jobs after restart

### Data Model

New runtime entities:

- `JobRecord`
- `JobEvent`
- `JobLogChunk`

Suggested persistent state:

- `runtime_jobs`
- `runtime_job_events`
- `runtime_job_logs`

`JobRecord` fields:

- `job_id`
- `kind`
- `owner_run_id`
- `owner_worker_id`
- `chat_id`
- `session_id`
- `command`
- `args_json`
- `cwd`
- `status`
- `started_at`
- `ended_at`
- `exit_code`
- `failure_reason`
- `cancel_requested`

### API Surface

- `POST /api/jobs`
- `GET /api/jobs/{id}`
- `GET /api/jobs/{id}/events`
- `GET /api/jobs/{id}/logs`
- `POST /api/jobs/{id}/cancel`

### Runtime Behavior

- jobs run detached from request transports
- cancellation uses context + process signal flow
- restart recovery marks orphaned jobs or reattaches based on recoverable state
- logs are append-only and cursor-readable

## Managed Workers Subsystem

## Purpose

Workers are supervised local subagents without mesh.

They are real local agent runtimes:

- own LLM loop
- same tool surface as main agent by default
- own local memory/session state
- inbox/outbox
- explicit closure and supervision

They are not distributed peers and do not participate in routing/discovery.

### Data Model

New runtime entities:

- `WorkerRecord`
- `WorkerMessage`
- `WorkerEvent`
- `WorkerHandoff`

Suggested persistent state:

- `runtime_workers`
- `runtime_worker_messages`
- `runtime_worker_events`
- `runtime_worker_memory`

`WorkerRecord` fields:

- `worker_id`
- `owner_run_id`
- `owner_chat_id`
- `owner_session_id`
- `worker_session_id`
- `title`
- `status`
- `created_at`
- `updated_at`
- `closed_at`
- `failure_reason`

### Memory Model

Worker memory is separate from shared project memory by default.

Rules:

- worker transcript and working state remain local
- worker facts do not auto-promote into shared memory
- explicit promotion bridge writes selected facts or final handoff into shared memory

This keeps project memory clean and preserves provenance.

### API Surface

- `POST /api/workers`
- `GET /api/workers/{id}`
- `GET /api/workers/{id}/messages`
- `GET /api/workers/{id}/events`
- `POST /api/workers/{id}/messages`
- `POST /api/workers/{id}/close`

`wait` is non-blocking:

- caller provides optional cursor
- API returns current status plus new messages/events since cursor
- in-progress is a normal state, not an error

### Runtime Behavior

- worker spawn creates local worker session state
- worker message appends inbox message and schedules/continues its LLM loop
- worker close is explicit lifecycle closure, not hard delete
- worker can internally start jobs
- worker approvals and policies use the same baseline as the main agent

## Tool Exposure

The main agent must be able to call these as runtime tools:

- `job_start`
- `job_status`
- `job_cancel`
- `agent_spawn`
- `agent_message`
- `agent_wait`

These tools are orchestration tools, not transport features.

They must call the same runtime API/service that CLI and HTTP use.

## Policy And Approval Integration

Policy must apply consistently to:

- main agent runs
- background jobs
- managed workers
- worker-started jobs

Requirements:

- policy snapshot per run/job/worker
- approvals persist and are auditable
- approval reasons are explicit
- denials are exposed through unified error model

## CLI Contract

CLI remains an API client only.

New commands:

- `teamd-agent jobs start ...`
- `teamd-agent jobs show <id>`
- `teamd-agent jobs logs <id>`
- `teamd-agent jobs cancel <id>`
- `teamd-agent workers spawn ...`
- `teamd-agent workers show <id>`
- `teamd-agent workers message <id> <text>`
- `teamd-agent workers wait <id> [cursor]`
- `teamd-agent workers close <id>`

## Phase Order

The implementation order matters:

1. runtime event plane
2. unified error model
3. background jobs subsystem
4. managed workers subsystem
5. tool exposure
6. policy/approval maturity
7. CLI polish

This order keeps the system debuggable and avoids building jobs/workers on unstable contracts.

## Success Criteria

- jobs survive restart with correct state recovery
- workers can be spawned, messaged, inspected, and closed without mesh
- API exposes lifecycle via transport-agnostic endpoints
- CLI uses API only
- main agent can delegate via `job_*` and `agent_*` tools
- worker local memory stays isolated unless explicitly promoted
- approvals and errors behave consistently across runs, jobs, and workers
