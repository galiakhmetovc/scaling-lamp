# Managed Workers, Jobs, And Runtime Control Plane Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add restart-safe background jobs, managed local workers, a shared runtime event plane, and stable API/CLI/tool surfaces without introducing mesh.

**Architecture:** Build this as a transport-agnostic runtime control plane. Start with runtime-owned events and errors, then add jobs as the first supervised detached subsystem, then layer managed workers with their own local LLM loop and memory isolation. Keep HTTP API and CLI as the only external control surfaces.

**Tech Stack:** Go, existing runtime/API/CLI layers, Postgres/SQLite runtime stores, existing provider/tool runtime, systemd live services.

---

## File Map

### Runtime core

- Create: `internal/runtime/events.go`
- Create: `internal/runtime/error_model.go`
- Create: `internal/runtime/jobs_service.go`
- Create: `internal/runtime/workers_service.go`
- Create: `internal/runtime/workers_memory.go`
- Modify: `internal/runtime/store.go`
- Modify: `internal/runtime/types.go`
- Modify: `internal/runtime/postgres_store.go`
- Modify: `internal/runtime/sqlite_store.go`
- Modify: `internal/runtime/runtime_api.go`

### API

- Create: `internal/api/jobs_handlers.go`
- Create: `internal/api/workers_handlers.go`
- Modify: `internal/api/server.go`
- Modify: `internal/api/types.go`
- Modify: `internal/api/errors.go`

### CLI

- Create: `internal/cli/jobs.go`
- Create: `internal/cli/workers.go`
- Modify: `internal/cli/client.go`
- Modify: `cmd/coordinator/cli.go`

### Transport/tool wiring

- Modify: `internal/transport/telegram/provider_tools.go`
- Modify: `internal/transport/telegram/memory_tools.go`
- Create: `internal/transport/telegram/delegation_tools.go`

### Tests

- Create: `internal/runtime/jobs_service_test.go`
- Create: `internal/runtime/workers_service_test.go`
- Create: `internal/runtime/events_test.go`
- Create: `internal/runtime/error_model_test.go`
- Create: `internal/api/jobs_handlers_test.go`
- Create: `internal/api/workers_handlers_test.go`
- Create: `internal/cli/jobs_test.go`
- Create: `internal/cli/workers_test.go`

### Docs

- Create: `docs/agent/jobs.md`
- Create: `docs/agent/workers.md`
- Modify: `docs/agent/http-api.md`
- Modify: `docs/agent/cli.md`
- Modify: `docs/agent/code-map.md`
- Modify: `docs/agent/core-architecture-walkthrough.md`

## Task 1: Add Runtime Event Plane

- [ ] **Step 1: Write failing event store tests**
- [ ] **Step 2: Add event types to `internal/runtime/types.go`**
- [ ] **Step 3: Extend `RunLifecycleStore` with event persistence/query**
- [ ] **Step 4: Implement event persistence in Postgres store**
- [ ] **Step 5: Implement event persistence in SQLite store**
- [ ] **Step 6: Emit events from run lifecycle paths**
- [ ] **Step 7: Run `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime -run Event -v`**
- [ ] **Step 8: Commit**

## Task 2: Add Unified Error Model

- [ ] **Step 1: Write failing tests for API/runtime error mapping**
- [ ] **Step 2: Create `internal/runtime/error_model.go`**
- [ ] **Step 3: Extend API error envelope to carry code/entity/retryability**
- [ ] **Step 4: Map runtime/provider/tool failures into stable codes**
- [ ] **Step 5: Run `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime ./internal/api -run 'Error|API' -v`**
- [ ] **Step 6: Commit**

## Task 3: Implement Background Jobs Store And Service

- [ ] **Step 1: Write failing store tests for jobs, logs, and events**
- [ ] **Step 2: Add job types and store interfaces**
- [ ] **Step 3: Add Postgres schema and CRUD for jobs/logs/events**
- [ ] **Step 4: Add SQLite schema and CRUD for jobs/logs/events**
- [ ] **Step 5: Create `internal/runtime/jobs_service.go`**
- [ ] **Step 6: Implement detached process execution with stdout/stderr capture**
- [ ] **Step 7: Implement cancel and restart-safe recovery**
- [ ] **Step 8: Run `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime -run Job -v`**
- [ ] **Step 9: Commit**

## Task 4: Expose Jobs Through HTTP API And CLI

- [ ] **Step 1: Write failing API handler tests for job endpoints**
- [ ] **Step 2: Add `/api/jobs` endpoints**
- [ ] **Step 3: Add CLI client methods for jobs**
- [ ] **Step 4: Add `teamd-agent jobs ...` commands**
- [ ] **Step 5: Smoke test against local API**
- [ ] **Step 6: Commit**

## Task 5: Implement Managed Workers Store And Service

- [ ] **Step 1: Write failing store/service tests for worker lifecycle**
- [ ] **Step 2: Add worker types and store interfaces**
- [ ] **Step 3: Implement worker persistence in Postgres and SQLite**
- [ ] **Step 4: Create `internal/runtime/workers_service.go`**
- [ ] **Step 5: Implement worker inbox/outbox and message cursors**
- [ ] **Step 6: Implement worker-owned local LLM loop**
- [ ] **Step 7: Implement worker close semantics**
- [ ] **Step 8: Run `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime -run Worker -v`**
- [ ] **Step 9: Commit**

## Task 6: Add Worker Memory Isolation And Promotion Bridge

- [ ] **Step 1: Write failing tests for worker-local memory isolation**
- [ ] **Step 2: Implement local worker memory/session storage**
- [ ] **Step 3: Add explicit promotion bridge to shared memory**
- [ ] **Step 4: Verify no automatic shared-memory pollution**
- [ ] **Step 5: Run `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime ./internal/memory -run Worker -v`**
- [ ] **Step 6: Commit**

## Task 7: Expose Workers Through HTTP API And CLI

- [ ] **Step 1: Write failing API handler tests for worker endpoints**
- [ ] **Step 2: Add `/api/workers` endpoints**
- [ ] **Step 3: Add CLI client methods for workers**
- [ ] **Step 4: Add `teamd-agent workers ...` commands**
- [ ] **Step 5: Smoke test worker spawn/message/wait/close**
- [ ] **Step 6: Commit**

## Task 8: Add Delegation Tools

- [ ] **Step 1: Write failing tests for `job_*` and `agent_*` tools**
- [ ] **Step 2: Add runtime-owned delegation tool implementations**
- [ ] **Step 3: Wire them into provider tool surface**
- [ ] **Step 4: Verify main agent can call them without Telegram-specific shortcuts**
- [ ] **Step 5: Run `GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run 'Job|Agent' -v`**
- [ ] **Step 6: Commit**

## Task 9: Approval And Policy Maturity For Jobs/Workers

- [ ] **Step 1: Write failing tests for approval/policy snapshots**
- [ ] **Step 2: Persist policy snapshot per run/job/worker**
- [ ] **Step 3: Extend approval reasons and audit visibility**
- [ ] **Step 4: Verify restart-safe continuation paths still hold**
- [ ] **Step 5: Run `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime ./internal/approvals -v`**
- [ ] **Step 6: Commit**

## Task 10: CLI Polish And Docs

- [ ] **Step 1: Add machine-friendly output and clearer usage messages where needed**
- [ ] **Step 2: Document jobs/workers/event model**
- [ ] **Step 3: Update HTTP API and CLI guides**
- [ ] **Step 4: Update code map and architecture walkthrough**
- [ ] **Step 5: Run `GOTMPDIR=$PWD/.tmp/go go test ./...`**
- [ ] **Step 6: Build coordinator and restart live services**
- [ ] **Step 7: Commit**

## Exit Criteria

- jobs are restart-safe and observable through API/CLI
- workers are managed local subagents with inbox/outbox and local memory
- main agent can delegate through runtime tools
- event model is shared across runs/jobs/workers
- errors are stable and transport-agnostic
- Telegram remains a client of runtime, not the owner of these subsystems
