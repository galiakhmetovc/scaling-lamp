# Single-Agent Runtime API And CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn `teamD` into a transport-agnostic single-agent runtime with one Go binary, an HTTP API as the primary control surface, a CLI that talks to that API, and a clean path for a later web UI, while keeping the system understandable as a teaching project for building your own agent.

**Architecture:** Keep one server process and one runtime core. Move run lifecycle, approvals, memory, policy, and session state behind a stable HTTP API. Telegram becomes one client of that API, CLI becomes another client, and a future web UI reuses the same API without duplicating orchestration logic. The implementation must stay explicit and beginner-readable: clear domain types, narrow files, stable interfaces, and documentation that explains not only what the code does but why the boundaries exist.

**Tech Stack:** Go, stdlib `net/http`, existing runtime/memory/telegram packages, Postgres/SQLite backends already supported in repo, systemd user units for live deployment.

---

## Hard Requirements

- This must read like a teaching project, not just a patched production codebase.
- This must also be production-grade enough that we do not knowingly build throwaway layers we expect to rewrite immediately.
- Do not hide essential behavior behind magic helpers, overly implicit config, or transport-specific shortcuts.
- Prefer explicit domain types and explicit state transitions over loosely coupled maps and stringly-typed control flow.
- Every major subsystem must have matching documentation for a newcomer:
  - what it is
  - why it exists
  - what files implement it
  - what data goes in and out
  - how to test it
- Every API introduced here must be designed as if the later Web UI will depend on it for years.
- Avoid “temporary” fallback layers unless they are truly required for compatibility and clearly isolated.

## Teaching-First Design Rules

- The runtime core must be understandable without reading Telegram internals.
- The API layer must be understandable without reading storage internals.
- The CLI must be understandable without reading runtime internals.
- Memory, approvals, sessions, and runs must each have one obvious owner in code.
- Each major state machine must be documented with a compact step list or diagram.
- If a file gets too clever, split it instead of documenting around the complexity.

## File Map

### New/Expanded Runtime-Core Files

- Create: `internal/api/server.go`
  - Own the HTTP router, server wiring, and route registration.
- Create: `internal/api/handlers_runs.go`
  - Run lifecycle endpoints: create run, inspect run, cancel run.
- Create: `internal/api/handlers_approvals.go`
  - Approval list/approve/reject endpoints.
- Create: `internal/api/handlers_sessions.go`
  - Session and memory inspection endpoints.
- Create: `internal/api/handlers_runtime.go`
  - Runtime config and session override endpoints.
- Create: `internal/api/types.go`
  - Shared request/response DTOs for API and CLI client.
- Create: `internal/api/errors.go`
  - Stable API error format.
- Create: `internal/runtime/session_overrides.go`
  - Session-scoped runtime and memory policy overrides.
- Create: `internal/runtime/approval_fsm.go`
  - Explicit approval state transitions and resume metadata.
- Create: `internal/runtime/approval_store.go`
  - Persistent storage interface for approvals/resume state.
- Create: `internal/runtime/types.go`
  - Canonical domain types for runs, approvals, sessions, and overrides.

### Existing Files To Modify

- Modify: `cmd/coordinator/main.go`
  - Make `serve` mode the canonical server entrypoint.
- Modify: `cmd/coordinator/bootstrap.go`
  - Wire HTTP API server and expose runtime dependencies through one app struct.
- Modify: `internal/runtime/runtime_api.go`
  - Expand from thin run wrapper into real transport-agnostic facade.
- Modify: `internal/runtime/run_manager.go`
  - Support resuming approved runs and explicit API-driven lifecycle.
- Modify: `internal/runtime/store.go`
  - Add interfaces for approval persistence and session overrides.
- Modify: `internal/runtime/postgres_store.go`
  - Persist approvals and session overrides.
- Modify: `internal/runtime/sqlite_store.go`
  - Persist approvals and session overrides for local mode.
- Modify: `internal/approvals/service.go`
  - Either reduce to pure FSM helper or fold into runtime approval layer.
- Modify: `internal/transport/telegram/*.go`
  - Gradually move from direct runtime orchestration to API client mode or shared runtime facade usage.
- Modify: `internal/config/config.go`
  - Add API listen address and auth/config flags.
- Modify: `scripts/teamd-agentctl`
  - Ensure service startup uses the new unified server mode.

### CLI Files

- Create: `cmd/teamd/main.go`
  - Multi-command binary entrypoint if split from current `cmd/coordinator`.
  - Alternative if keeping current layout: add subcommands under `cmd/coordinator`.
- Create: `internal/cli/client.go`
  - HTTP client for runtime API.
- Create: `internal/cli/cmd_runs.go`
  - `runs start/status/cancel`.
- Create: `internal/cli/cmd_approvals.go`
  - `approvals list/approve/reject`.
- Create: `internal/cli/cmd_sessions.go`
  - `sessions show/reset`.
- Create: `internal/cli/cmd_memory.go`
  - `memory search/read`.
- Create: `internal/cli/cmd_runtime.go`
  - `runtime show`, `runtime override`, `memory policy`.

### Documentation

- Modify: `docs/agent/01-overview.md`
- Modify: `docs/agent/request-lifecycle.md`
- Modify: `docs/agent/core-architecture-walkthrough.md`
- Create: `docs/agent/http-api.md`
- Create: `docs/agent/cli.md`
- Create: `docs/agent/runtime-api-walkthrough.md`
- Create: `docs/agent/approvals.md`
- Modify: `docs/agent/code-map.md`

### Tests

- Create: `internal/api/server_test.go`
- Create: `internal/api/handlers_runs_test.go`
- Create: `internal/api/handlers_approvals_test.go`
- Create: `internal/api/handlers_runtime_test.go`
- Create: `internal/runtime/session_overrides_test.go`
- Create: `internal/runtime/approval_fsm_test.go`
- Create: `internal/cli/client_test.go`
- Create: `tests/integration/api_runtime_test.go`

---

### Task 1: Define The Stable Runtime API Surface

**Files:**
- Create: `internal/runtime/types.go`
- Create: `internal/api/types.go`
- Create: `internal/api/errors.go`
- Modify: `internal/runtime/runtime_api.go`
- Test: `internal/api/server_test.go`

- [ ] **Step 1: Write failing tests for the API contract**

Write tests that describe the core DTOs and error shape:
- create run request/response
- run status response
- approval response
- session override response

- [ ] **Step 2: Run tests to verify missing contract**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/api -run TestAPITypes -v`
Expected: FAIL because the package or types do not exist yet.

- [ ] **Step 3: Implement minimal API DTOs and error envelope**

Add stable structs for:
- `CreateRunRequest`
- `CreateRunResponse`
- `RunStatusResponse`
- `ApprovalRecordResponse`
- `SessionOverrideResponse`
- `APIError`

Also add canonical runtime domain structs in `internal/runtime/types.go` so the system has one obvious data vocabulary.

- [ ] **Step 4: Expand runtime facade**

Make `internal/runtime/runtime_api.go` the single surface for:
- prepare/start/cancel run
- inspect active run
- list pending approvals
- approve/reject request
- read/write session overrides

- [ ] **Step 5: Run focused tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/api ./internal/runtime -run 'TestAPITypes|TestRuntimeAPI' -v`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/api/types.go internal/api/errors.go internal/runtime/runtime_api.go
git commit -m "refactor(teamD): define stable runtime api surface"
```

### Task 2: Persist Approvals And Build Explicit Approval FSM

**Files:**
- Create: `internal/runtime/approval_fsm.go`
- Create: `internal/runtime/approval_store.go`
- Modify: `internal/runtime/store.go`
- Modify: `internal/runtime/postgres_store.go`
- Modify: `internal/runtime/sqlite_store.go`
- Modify: `internal/approvals/service.go`
- Test: `internal/runtime/approval_fsm_test.go`

- [ ] **Step 1: Write failing FSM tests**

Cover:
- pending -> approved
- pending -> rejected
- repeated callback idempotency
- invalid transition rejection
- reload from store preserves status

- [ ] **Step 2: Run tests to verify they fail**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime -run TestApprovalFSM -v`
Expected: FAIL

- [ ] **Step 3: Implement approval state machine**

State should include:
- approval id
- tool/action metadata
- session id
- run id
- status
- callback update id dedupe
- optional resume token / tool payload snapshot

- [ ] **Step 4: Persist approvals in both stores**

Add storage methods:
- `SaveApproval`
- `GetApproval`
- `ListPendingApprovals(sessionID)`
- `ApplyApprovalDecision`

- [ ] **Step 5: Rewire service to use explicit FSM**

Reduce `internal/approvals/service.go` to a thin in-memory helper or fold logic into runtime approval layer, but avoid duplicated transition rules.

- [ ] **Step 6: Run focused tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/approvals ./internal/runtime -run 'TestApprovalFSM|TestService' -v`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add internal/runtime internal/approvals
git commit -m "feat(teamD): persist approvals with explicit runtime fsm"
```

### Task 3: Add Session-Scoped Runtime And Memory Overrides

**Files:**
- Create: `internal/runtime/session_overrides.go`
- Modify: `internal/runtime/store.go`
- Modify: `internal/runtime/postgres_store.go`
- Modify: `internal/runtime/sqlite_store.go`
- Modify: `internal/transport/telegram/runtime_commands.go`
- Test: `internal/runtime/session_overrides_test.go`

- [ ] **Step 1: Write failing tests for session overrides**

Cover:
- set override for one session
- other sessions unaffected
- memory policy override merges with defaults
- runtime config override merges with defaults

- [ ] **Step 2: Run tests to verify missing behavior**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime -run TestSessionOverrides -v`
Expected: FAIL

- [ ] **Step 3: Implement persistent session override model**

Support:
- runtime request config override
- memory policy override
- action policy override (at least approval-required tools)

- [ ] **Step 4: Expose override reads in current Telegram commands**

Add read-only visibility first if needed, then set/reset commands behind explicit syntax.

- [ ] **Step 5: Run focused tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime ./internal/transport/telegram -run 'TestSessionOverrides|TestRuntimeCommands' -v`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/runtime internal/transport/telegram
git commit -m "feat(teamD): add session scoped runtime overrides"
```

### Task 4: Build HTTP API Server On Top Of Runtime Core

**Files:**
- Create: `internal/api/server.go`
- Create: `internal/api/handlers_runs.go`
- Create: `internal/api/handlers_approvals.go`
- Create: `internal/api/handlers_sessions.go`
- Create: `internal/api/handlers_runtime.go`
- Modify: `cmd/coordinator/bootstrap.go`
- Modify: `internal/config/config.go`
- Test: `internal/api/handlers_runs_test.go`
- Test: `internal/api/handlers_approvals_test.go`
- Test: `internal/api/handlers_runtime_test.go`

- [ ] **Step 1: Write failing handler tests**

Cover endpoints:
- `POST /api/runs`
- `GET /api/runs/{id}`
- `POST /api/runs/{id}/cancel`
- `GET /api/approvals`
- `POST /api/approvals/{id}/approve`
- `POST /api/approvals/{id}/reject`
- `GET /api/runtime`
- `PATCH /api/runtime/sessions/{session_id}`

- [ ] **Step 2: Run handler tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/api -run TestRunHandlers -v`
Expected: FAIL

- [ ] **Step 3: Implement HTTP router and handlers**

Use stdlib `net/http`. Build a clean handler layout with explicit DTO conversion and error mapping.

Do not cut corners by letting handlers reach deep into transport or storage internals.

For security posture in this slice:
- local-only bind by default
- make auth extension points obvious in code
- do not ship a fake abstraction that will be rewritten immediately

- [ ] **Step 4: Wire server into bootstrap**

Add config:
- `TEAMD_API_ENABLED`
- `TEAMD_API_LISTEN_ADDR`

Ensure the same binary can run:
- Telegram poller
- HTTP API
- both together

- [ ] **Step 5: Run focused handler tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/api -v`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/api cmd/coordinator/bootstrap.go internal/config/config.go
git commit -m "feat(teamD): add http api over single agent runtime"
```

### Task 5: Auto-Resume Approved Runs

**Files:**
- Modify: `internal/runtime/run_manager.go`
- Modify: `internal/runtime/runtime_api.go`
- Modify: `internal/runtime/approval_fsm.go`
- Modify: `internal/transport/telegram/telegram_api.go`
- Test: `tests/integration/api_runtime_test.go`

- [ ] **Step 1: Write failing integration test**

Scenario:
- create run
- guarded tool requests approval
- approve request
- same run resumes and completes

- [ ] **Step 2: Run test to verify failure**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./tests/integration -run TestApprovedRunResumes -v`
Expected: FAIL

- [ ] **Step 3: Implement resume metadata and continuation**

Store enough information to continue:
- run id
- pending tool call
- tool payload
- session id

Prefer resuming from runtime-managed continuation point, not replaying the whole prompt blindly.

- [ ] **Step 4: Wire approval endpoints to resume path**

Approval endpoint should:
- mark decision
- resume the waiting run if the status becomes approved

- [ ] **Step 5: Run integration tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./tests/integration -run TestApprovedRunResumes -v`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/runtime internal/transport/telegram tests/integration
git commit -m "feat(teamD): resume guarded runs after approval"
```

### Task 6: Build CLI As An HTTP API Client

**Files:**
- Create: `internal/cli/client.go`
- Create: `internal/cli/cmd_runs.go`
- Create: `internal/cli/cmd_approvals.go`
- Create: `internal/cli/cmd_sessions.go`
- Create: `internal/cli/cmd_memory.go`
- Create: `internal/cli/cmd_runtime.go`
- Create or Modify: `cmd/teamd/main.go` or `cmd/coordinator/main.go`
- Test: `internal/cli/client_test.go`

- [ ] **Step 1: Write failing CLI client tests**

Cover:
- create run request
- cancel run request
- list approvals
- approve request
- memory search request

- [ ] **Step 2: Run tests to verify missing client**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/cli -v`
Expected: FAIL

- [ ] **Step 3: Implement HTTP client**

No runtime logic in CLI layer. It should only speak the HTTP API.

- [ ] **Step 4: Implement subcommands**

Minimum commands:
- `teamd serve`
- `teamd runs start`
- `teamd runs status`
- `teamd runs cancel`
- `teamd approvals list`
- `teamd approvals approve`
- `teamd approvals reject`
- `teamd runtime show`
- `teamd memory search`

- [ ] **Step 5: Run focused tests and one manual smoke test**

Run:
- `GOTMPDIR=$PWD/.tmp/go go test ./internal/cli -v`
- `GOTMPDIR=$PWD/.tmp/go go build ./cmd/...`

Manual:
- start API locally
- use CLI to create/cancel one run

- [ ] **Step 6: Commit**

```bash
git add internal/cli cmd
git commit -m "feat(teamD): add cli over runtime api"
```

### Task 7: Move Telegram Closer To API-Client Semantics

**Files:**
- Modify: `internal/transport/telegram/*.go`
- Test: `internal/transport/telegram/*test.go`

- [ ] **Step 1: Write focused tests for Telegram against stable runtime API**

Assert that transport behavior does not depend on transport-owned lifecycle state beyond UI state.

- [ ] **Step 2: Run transport tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -v`

- [ ] **Step 3: Reduce direct orchestration knowledge**

Telegram should mainly:
- create/display runs
- show status
- send user input
- show approvals
- call runtime API/facade methods

- [ ] **Step 4: Re-run tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram
git commit -m "refactor(teamD): align telegram transport with runtime api"
```

### Task 8: Documentation And Operator Walkthrough

**Files:**
- Create: `docs/agent/http-api.md`
- Create: `docs/agent/cli.md`
- Create: `docs/agent/runtime-api-walkthrough.md`
- Create: `docs/agent/approvals.md`
- Modify: `docs/agent/01-overview.md`
- Modify: `docs/agent/request-lifecycle.md`
- Modify: `docs/agent/core-architecture-walkthrough.md`
- Modify: `docs/agent/code-map.md`

- [ ] **Step 1: Document the new mental model**

Explain for beginners:
- one Go binary
- one runtime core
- one HTTP API
- many clients (Telegram, CLI, later Web UI)

- [ ] **Step 2: Document exact API endpoints**

List requests/responses and when each is used.

- [ ] **Step 2.1: Document approvals and resume flow**

Explain:
- when approval is created
- what is persisted
- what happens on approve
- what happens on reject
- how resume works
- how to inspect stuck approvals

- [ ] **Step 2.2: Document the runtime vocabulary**

Write one compact glossary for:
- run
- session
- approval
- checkpoint
- continuity
- searchable memory
- session override

- [ ] **Step 3: Document CLI commands**

Show examples for:
- start run
- cancel run
- inspect approvals
- memory search

- [ ] **Step 4: Run docs-linked verification**

Run:
- `GOTMPDIR=$PWD/.tmp/go go test ./...`
- `rg -n \"http api|cli|approval|runtime api\" docs/agent`

- [ ] **Step 5: Commit**

```bash
git add docs/agent
git commit -m "docs(teamD): document runtime api and cli architecture"
```

---

## Acceptance Criteria

- One Go binary can run the single-agent runtime and expose HTTP API.
- HTTP API becomes the primary control surface for run lifecycle and approvals.
- CLI uses HTTP API only; it does not call runtime internals directly.
- Guarded tool approvals are persisted and survive restart.
- Approved guarded runs can resume without manual replay.
- Session-scoped runtime/memory/action policy overrides are supported.
- Telegram remains functional but no longer owns the main runtime lifecycle.
- Docs explain the architecture clearly to a new engineer.
- The architecture is explicit enough that a newcomer can understand:
  - where a run starts
  - where approval lives
  - where memory lives
  - where the API boundary is
  - how CLI reaches the runtime
- No major subsystem in this slice relies on temporary shortcuts that would force an immediate rewrite in the Web UI phase.

## Non-Goals

- Mesh evolution
- Multi-agent orchestration
- Web UI implementation itself
- Full auth/RBAC platform
- Background scheduler platform beyond what current runtime already needs

## Recommended Execution Order

1. Task 1
2. Task 2
3. Task 3
4. Task 4
5. Task 5
6. Task 6
7. Task 7
8. Task 8

This order matters. API before CLI, approvals before auto-resume, and runtime facade before transport cleanup.
