# Provider Timeout Decision Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace fatal provider-timeout handling with a persisted timeout-decision flow that auto-continues once after 5 minutes and only fails after a second unattended timeout.

**Architecture:** Add runtime-owned timeout-decision state and actions, teach execution service to pause and resume the same run on timeout, then wire Telegram UI to render and resolve those decisions. Keep all semantics in runtime/API so Telegram remains a thin transport surface.

**Tech Stack:** Go, runtime store/API, Telegram adapter, existing run manager/events, persisted Postgres/SQLite stores, Go tests.

---

### Task 1: Add timeout-decision types and store contract

**Files:**
- Modify: `internal/runtime/types.go`
- Modify: `internal/runtime/runtime_api.go`
- Modify: `internal/runtime/sqlite_store.go`
- Modify: `internal/runtime/postgres_store.go`
- Test: `internal/runtime/runtime_api_test.go`

- [ ] Write failing tests for storing and reading timeout decisions.
- [ ] Run the targeted tests and verify they fail for missing timeout-decision support.
- [ ] Add runtime types, store interface methods, and SQLite/Postgres persistence.
- [ ] Run targeted runtime tests and verify they pass.
- [ ] Commit.

### Task 2: Pause runs on provider timeout instead of failing immediately

**Files:**
- Modify: `internal/runtime/execution_service.go`
- Modify: `internal/runtime/conversation_engine.go`
- Modify: `internal/runtime/error_model.go`
- Test: `internal/runtime/execution_service_test.go`

- [ ] Write failing tests for first provider-timeout creating a pending timeout decision and leaving the run actionable.
- [ ] Run the targeted execution-service tests and verify they fail for current fatal behavior.
- [ ] Implement timeout interception and pending-decision creation.
- [ ] Run targeted tests and verify they pass.
- [ ] Commit.

### Task 3: Add auto-continue-once and second-timeout failure path

**Files:**
- Modify: `internal/runtime/execution_service.go`
- Modify: `internal/runtime/runtime_api.go`
- Test: `internal/runtime/execution_service_test.go`

- [ ] Write failing tests for one auto-continue after 5 minutes and final failure after a second unattended timeout.
- [ ] Run the targeted tests and verify they fail.
- [ ] Implement auto-continue scheduling and second-timeout finalization.
- [ ] Run targeted runtime tests and verify they pass.
- [ ] Commit.

### Task 4: Expose timeout-decision actions through API/control plane

**Files:**
- Modify: `internal/api/server.go`
- Modify: `internal/api/types.go`
- Modify: `internal/cli/client.go`
- Test: `internal/api/server_test.go`
- Test: `internal/cli/client_test.go`

- [ ] Write failing tests for timeout-decision action endpoints.
- [ ] Run targeted API/CLI tests and verify they fail.
- [ ] Implement generic actions: `continue`, `retry_round`, `cancel`, `fail`.
- [ ] Run targeted API/CLI tests and verify they pass.
- [ ] Commit.

### Task 5: Wire Telegram status and callbacks

**Files:**
- Modify: `internal/transport/telegram/run_lifecycle.go`
- Modify: `internal/transport/telegram/telegram_api.go`
- Modify: `internal/transport/telegram/ui_helpers.go`
- Modify: `internal/transport/telegram/immediate_updates.go`
- Test: `internal/transport/telegram/runtime_memory_test.go`
- Test: `internal/transport/telegram/adapter_test.go`

- [ ] Write failing tests for Telegram-visible timeout decision state and callback handling.
- [ ] Run targeted Telegram tests and verify they fail.
- [ ] Implement renderer and callback mapping over generic runtime actions.
- [ ] Run targeted Telegram tests and verify they pass.
- [ ] Commit.

### Task 6: Verify end-to-end and document behavior

**Files:**
- Modify: `docs/agent/approvals.md`
- Modify: `docs/agent/state-machines.md`
- Modify: `docs/agent/07-traces-status-and-observability.md`

- [ ] Run full verification: `go test ./... -count=1` and `go build ./cmd/coordinator`.
- [ ] Update docs for provider-timeout decision flow and auto-continue semantics.
- [ ] Re-run verification after doc/code touchpoints if needed.
- [ ] Commit.
