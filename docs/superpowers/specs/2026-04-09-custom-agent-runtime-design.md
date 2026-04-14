# Custom Agent Runtime Design

Date: 2026-04-09
Status: aligned to shipped runtime on 2026-04-12; worker gRPC IPC explicitly deferred
Implementation stack: Go-only

## Alignment Note

This document started as the pre-implementation runtime design.

As of 2026-04-12, `teamD` no longer matches that early design literally. The shipped system has a canonical
`AgentCore`-centered runtime, an API-first control plane, operator CLI chat, managed workers/jobs, persistent
approvals, plans, artifacts, event streaming, replay, and worker process supervision.

This document is therefore the aligned design for the runtime that actually exists today.

One explicit design decision was made during alignment:

- worker IPC was originally fixed as `gRPC/protobuf`
- the shipped runtime uses `supervised local worker processes + heartbeat/status events`
- for the current runtime phase, the supervised-process contract is canonical
- `gRPC/protobuf` worker IPC is deferred to a future phase rather than treated as current MVP debt

## Goal

Build a self-hosted runtime that starts as a personal system but is structured so it can evolve into a reusable framework. The shipped runtime must support:

- `z.ai` as the primary model provider
- Telegram as a transport, not as orchestration center
- HTTP API as the stable control plane
- CLI as the reference operator client, including interactive chat
- managed local workers and background jobs
- long-term memory, searchable memory, and session working state
- MCP and skills as separate runtime layers
- automatic session compaction
- artifact offload and replay-friendly inspection surfaces

The system is single-owner. It already supports delegation through managed workers, but it is not yet a full mesh runtime in the hot path.

## Non-Goals For MVP

- Multi-tenant isolation
- Peer-to-peer agent collaboration
- Multi-provider routing
- Web UI
- Distributed execution across hosts
- Complex RBAC

## Implementation Stance

The MVP implementation is `Go-only`.

This means:

- the coordinator is a Go service
- workers are implemented in Go
- Telegram, `z.ai`, MCP, memory, and artifact integrations are implemented with Go packages
- skills are represented as Go-managed configuration, prompt layers, and workflow contracts rather than Python plugins

The design must avoid assumptions that require a Python runtime.

## Concrete Runtime Decisions

The following decisions describe the runtime that is actually shipped:

- orchestration center: `AgentCore`
- control plane: `HTTP API + CLI over API + SSE event stream`
- transport model: transports are adapters around the same runtime core
- run orchestration: `ExecutionService`
- storage baseline:
  - runtime lifecycle store: `SQLite or Postgres`
  - semantic memory: `Postgres + pgvector` when enabled
  - artifacts: `local filesystem artifact store`
- worker isolation: `separate OS processes`
- worker supervision: `heartbeat + graceful stop + recovery`
- worker IPC status:
  - current canonical path: local supervised process contract
  - future optional phase: `gRPC/protobuf` worker IPC if later justified
- skills packaging: file-based YAML/JSON definitions and prompt/templates in the repository
- `z.ai` integration: official API only
- Telegram transport mode: long polling
- operator auth boundary: bearer token for API/CLI when configured

## Review-Driven Clarifications

The following areas are additionally fixed before implementation begins:

- worker supervision must include heartbeat, graceful shutdown, crash recovery, and orphan cleanup
- Telegram approvals require an explicit persistence model and finite-state machine
- MCP execution requires a concrete security baseline, not only generic policy language
- testing must include deterministic coordinator-worker integration and provider mocks
- `z.ai` integration must be constrained by documented API limits before scheduler work begins

## Architecture Overview

The runtime is organized as `AgentCore + Narrow Runtime Services + Transports/Clients`.

The canonical orchestration surface is `AgentCore`.

`AgentCore` composes and exposes:

- run start/status/cancel
- approval continuation resume
- sessions and overrides
- approvals
- plans
- jobs
- workers
- events
- control state and control actions

The main runtime services under it are:

- `runtime.API`: store-backed runtime queries and lifecycle primitives
- `ExecutionService`: run orchestration
- `JobsService`: detached background commands
- `WorkersService`: managed local subagents
- `SessionActions`: generic session control actions

`Worker Runtime` is an isolated execution environment for one managed worker. A worker has its own session,
memory scope, event stream, tool context, and lifecycle state. Workers do not communicate directly with each
other in the shipped runtime. All coordination flows through the main runtime.

Supporting services and adapters:

- `provider-zai`: model access, retries, streaming, budgets, model policy
- `transport-telegram`: inbound updates, outbound responses, notifications
- `http-api`: stable control plane surface
- `cli-client`: reference operator client over the HTTP API
- `memory-service`: searchable memory, recall, and working-state persistence
- `skills-runtime`: behavior packs, workflow rules, prompt layering, policies
- `mcp-runtime`: MCP server registry, tool/resource discovery, invocation
- `session-compactor`: threshold-based and lifecycle-based compaction
- `artifact-store`: files, reports, message attachments, snapshots
- `event-log`: append-only operational trace for runs, jobs, workers, approvals, and plans

Recommended implementation shape in Go:

- `cmd/coordinator`: runtime entrypoint
- `cmd/worker`: supervised worker process entrypoint
- `internal/worker`: worker lifecycle and execution loop
- `internal/runtime`: canonical runtime core and control plane services
- `internal/api`: HTTP API surface
- `internal/cli`: CLI client/runtime-facing console logic
- `internal/provider/zai`: `z.ai` adapter
- `internal/transport/telegram`: Telegram adapter
- `internal/memory`: memory interfaces and storage services
- `internal/skills`: prompt layering and workflow behavior
- `internal/mcp`: MCP registry and invocation runtime
- `internal/compaction`: session compaction and checkpoints
- `internal/artifacts`: artifact persistence
- `internal/events`: typed event contracts
- `internal/policy`: permissions and approval policy
- `internal/observability`: logs, traces, metrics
- `proto/worker/v1`: reserved for a future worker IPC phase if `gRPC/protobuf` is later reintroduced

## Core Design Decisions

### Agent Model

The main agent is the runtime itself executing runs for a session.

Managed delegation happens through:

- `jobs`: detached background process execution
- `workers`: managed local subagents with their own session, event stream, and handoff

Workers run as separate OS processes under supervision. This gives stronger lifecycle control, fault isolation,
per-worker state isolation, and a clean path toward future mesh work.

### Swarm Model

The original design assumed a supervisor-led swarm as the main runtime shape.

That is no longer the canonical hot path.

The shipped runtime is:

- single-agent first
- control-plane first
- managed workers/jobs as delegation primitives
- mesh deferred

Direct worker-to-worker communication is still out of scope.

### Provider Strategy

`z.ai` is the primary provider and the only required provider in MVP. The provider contract must still be abstract enough that future providers can be added without changing orchestration or memory behavior.

### Transport Strategy

Telegram is a transport adapter, not orchestration logic. The runtime core works without Telegram.

The current transport/client stack is:

- Telegram adapter
- HTTP API
- CLI over API
- operator chat console over the same control plane

### Skills And MCP

Skills and MCP are separate:

- `skills`: behavior, policy, prompt composition, workflow contracts
- `MCP`: external tools, resources, and server-provided capabilities

Neither should be modeled as a special case inside worker logic. Both are runtime services consumed through clean interfaces.

For MVP, skills are loaded from file-based definitions in the repository. Runtime metadata and execution outcomes may be stored elsewhere, but skill source-of-truth stays in versioned files.

## Memory Model

The shipped runtime has four practical memory layers.

### Session History

Raw transcript and message history for a session or worker session.

### Working State

Short-lived execution state derived from the session:

- checkpoint
- continuity
- current plan
- pending execution state for immediate continuation

This is the primary target for compaction and prompt assembly.

### Searchable Memory

Durable memory documents used for recall/search:

- continuity-derived documents
- selected promoted facts
- optional semantic indexing when Postgres embeddings are enabled

Automatic writes are conservative and policy-gated.

### Artifact-Backed Archive

Large outputs and archived context references:

- offloaded tool results
- archived transcript windows
- replay-linked checkpoint/context references
- worker handoff artifacts

This exists specifically to keep large payloads out of prompts and durable memory bodies.

## Memory Write Policy

Automatic writes must be conservative.

- checkpoint and continuity updates are always eligible for working-state persistence
- continuity promotion into searchable memory is policy-controlled
- checkpoint promotion is conservative by default
- handoff summaries are eligible for promotion
- artifacts are indexed semantically only when they pass size, type, and policy checks
- raw transcripts should not be blindly copied into long-term memory

Memory and replay paths should preserve `archive_refs` and `artifact_refs` whenever possible.

## Run And Worker Lifecycle

Primary run lifecycle:

1. `queued`
2. `running`
3. `waiting_approval` or `cancel_requested` when applicable
4. `completed`, `cancelled`, or `failed`

Worker lifecycle states:

1. `created`
2. `idle`
3. `running`
4. `waiting_approval`
5. `failed`
6. `closed`

Worker process lifecycle is tracked separately:

- `starting`
- `running`
- `stopped`
- `failed`

## Auto-Compaction

Compaction is triggered by both thresholds and lifecycle hooks.

Threshold triggers:

- token/context budget pressure
- accumulated tool output size
- long-running session depth

Lifecycle triggers:

- idle timeout
- handoff
- completion
- failure recovery checkpoint

Compaction output is not a lossy truncation. It is a structured checkpoint containing:

- what happened
- what matters now
- unresolved items
- next actions
- source-of-truth artifacts

## Control Plane Model

The canonical operator surface is no longer Telegram.

The shipped control plane consists of:

- HTTP API
- CLI over API
- SSE event streaming
- operator chat console

The runtime exposes first-class state for:

- runs
- approvals
- sessions and overrides
- plans
- jobs
- workers
- artifacts
- events
- replay

Telegram-specific UI state still exists, but it is a presentation layer rather than the orchestration center.

## Telegram Operational Model

Telegram is both a user channel and a notification bus.

For MVP, the Telegram adapter uses long polling rather than webhooks.

Inbound flow:

1. Telegram adapter receives update
2. Update is normalized into an `InboundEvent`
3. Identity mapping resolves owner/chat/session/worker association
4. Coordinator resumes or creates workflow state

Outbound flow:

1. Coordinator or worker emits `OutboundEvent`
2. Telegram adapter renders it as a user reply, status update, alert, or handoff notification

Telegram notification classes:

- standard reply
- progress/status update
- handoff or completion notice
- warning or error notification
- approval request

### Approval Model

Approvals are first-class runtime state. Pending approvals are persisted with at least:

- approval ID
- worker ID
- session ID
- payload
- current status
- reason
- target type / target id
- decision metadata
- originating Telegram callback metadata when Telegram is involved

The approval FSM for MVP is:

- `pending -> approved`
- `pending -> rejected`
- `pending -> expired`
- `pending -> canceled`

Telegram callbacks must still be handled idempotently using stable Telegram update or callback identifiers.

## z.ai Provider Model

`provider-zai` owns:

- authentication for the official API
- model/profile selection by worker role
- retry and backoff policy
- streaming response handling
- usage accounting and budget signals

Provider-specific details must not leak into worker or coordinator contracts.

No browser-session or scraping fallback is included in MVP.

Before implementation of the provider slice, the team must pin:

- auth method and credential format
- request and streaming endpoints
- supported model families
- rate limits and retryable errors
- context window constraints
- any tool-calling or structured-output support

## Policy And Permissions

The system is single-owner but still needs policy boundaries.

Required policy controls:

- which agents may spawn other agents
- which skills may attach to which workers
- which MCP servers/tools are allowed per agent role
- what can be written to shared semantic memory
- which actions require explicit human approval

## MCP Security Baseline

MCP capability access is deny-by-default.

MVP baseline requirements:

- allowlist servers and tools per worker role
- default tool timeout
- output size limits for returned data
- input validation before invocation
- explicit handling for partial or failed tool responses
- version pinning for configured MCP servers
- separate process execution where the MCP integration requires launching a local server

If a server cannot meet baseline safety requirements, it should not be enabled in MVP.

## Scheduling, Supervision, And Backpressure

The runtime needs controlled concurrency from day one.

Scheduler responsibilities:

- worker queueing
- concurrency caps
- retry policy
- timeout policy
- dead-letter or failed-job handling
- budget-aware throttling for provider usage

Because workers are separate OS processes, supervision also owns:

- process spawn policy
- health checks and heartbeats
- graceful shutdown and force-kill escalation
- crash recovery and restart decisions
- orphaned worker cleanup
- heartbeat timeout handling
- runtime-visible process metadata persistence

Any future `gRPC/protobuf` worker IPC must preserve this supervision behavior instead of replacing it with a thinner transport-only layer.

## Observability

Operational visibility is mandatory in MVP.

Minimum telemetry:

- structured logs
- append-only event log
- traces by task/session/worker
- provider latency and failure reasons
- compaction reasons
- tool and MCP invocation outcomes
- worker process start, exit, crash, and restart events
- approval lifecycle transitions

## Artifact Store

Large artifacts must not live in prompts or KV state.

The artifact store holds:

- generated reports
- uploaded files and Telegram attachments
- compaction snapshots
- handoff packages
- exported task results

Semantic memory stores references into the artifact store rather than duplicating large payloads.

## Failure Model

Recoverable failures:

- transient provider errors
- MCP tool timeouts
- Telegram delivery retries
- interrupted workers with valid checkpoints

Unrecoverable failures:

- corrupted worker state
- missing critical artifacts
- policy violations that require owner action

Recovery should prefer resume-from-checkpoint when the last compacted state is valid.

## Testing Strategy

The runtime must include:

- mock `z.ai` provider tests with deterministic responses
- deterministic worker supervision tests
- control-plane integration tests for approvals, artifacts, and worker visibility
- compaction fixture tests that verify structure and source references
- Telegram flow tests for inbound updates and approval callbacks
- end-to-end smoke coverage through API/CLI or Telegram transport

## MVP Deliverables

The runtime now delivers:

- Go coordinator/runtime core
- isolated Go worker runtime
- `z.ai` provider adapter in Go
- Telegram transport adapter in Go
- HTTP API control plane
- CLI client and operator chat console
- memory service with searchable memory and working state
- skills runtime
- MCP runtime
- session compactor
- event log, replay, and artifact store
- jobs and managed workers as delegation primitives

## Open Questions

- how far to take worker sandboxing beyond process isolation
- future mesh adoption path on top of AgentCore/jobs/workers
- how much of Telegram presentation state should remain adapter-local long term

## Recommended Next Step

Treat this document as the canonical aligned runtime design and continue with explicit follow-up slices:

1. keep runtime docs and tests aligned with the shipped AgentCore/control-plane architecture
2. treat `gRPC/protobuf` worker IPC as a future optional runtime evolution, not as current alignment debt
3. only then decide whether any deferred swarm/mesh assumptions should re-enter the hot path
