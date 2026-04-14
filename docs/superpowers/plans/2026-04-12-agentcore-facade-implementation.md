# AgentCore Facade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce a runtime-owned `AgentCore` facade that becomes the canonical orchestration surface for transports and HTTP handlers without rewriting the existing runtime services.

**Architecture:** Add a new facade in `internal/runtime` that composes existing runtime services such as `runtime.API`, `ExecutionService`, and focused service helpers for plans, workers, jobs, approvals, and session actions. Then migrate HTTP API handlers and remaining transport control paths to depend on the facade instead of assembling runtime behavior ad hoc.

**Tech Stack:** Go, existing `internal/runtime`, `internal/api`, `internal/transport/telegram`, stdlib `net/http`, current runtime stores and services.

---

### Task 1: Introduce AgentCore interface and concrete facade

**Files:**
- Create: `internal/runtime/agent_core.go`
- Create: `internal/runtime/agent_core_test.go`
- Modify: `internal/runtime/runtime_api.go`
- Modify: `internal/runtime/execution_service.go`

- [ ] **Step 1: Write failing facade tests**

Cover:
- `StartRun` delegates to execution service
- `Run` and `ListRuns` delegate to runtime API/store-backed views
- `ControlState` and control actions are exposed from one facade

- [ ] **Step 2: Run targeted tests to confirm failure**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime -run 'AgentCore' -count=1`

Expected: FAIL because the facade does not exist yet.

- [ ] **Step 3: Implement minimal facade**

Add:
- `type AgentCore interface`
- `type RuntimeCore struct`
- constructor wiring existing services

- [ ] **Step 4: Re-run targeted runtime tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime -run 'AgentCore' -count=1`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/agent_core.go internal/runtime/agent_core_test.go internal/runtime/runtime_api.go internal/runtime/execution_service.go
git commit -m "refactor(teamD): add runtime AgentCore facade"
```

### Task 2: Move HTTP API handlers onto AgentCore

**Files:**
- Modify: `internal/api/server.go`
- Modify: `internal/api/server_test.go`
- Modify: `cmd/coordinator/bootstrap.go`

- [ ] **Step 1: Write failing handler tests**

Cover:
- runs endpoints call `AgentCore`
- control endpoints call `AgentCore`
- session action endpoints call `AgentCore`

- [ ] **Step 2: Run targeted API tests to confirm failure**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/api -run 'AgentCore|Server' -count=1`

Expected: FAIL or require wiring changes.

- [ ] **Step 3: Rewire API server to depend on the facade**

Keep handlers thin:
- parse request
- call `AgentCore`
- encode response

- [ ] **Step 4: Re-run API tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/api -run 'AgentCore|Server' -count=1`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/api/server.go internal/api/server_test.go cmd/coordinator/bootstrap.go
git commit -m "refactor(teamD): route api handlers through AgentCore"
```

### Task 3: Thin Telegram control paths further

**Files:**
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/control_actions.go`
- Modify: `internal/transport/telegram/session_actions.go`
- Modify: `internal/transport/telegram/telegram_api.go`

- [ ] **Step 1: Write failing transport tests**

Cover:
- Telegram status/cancel path uses the same runtime facade
- session actions go through the same runtime facade

- [ ] **Step 2: Run targeted Telegram tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run 'Control|Session' -count=1`

Expected: FAIL or expose missing wiring.

- [ ] **Step 3: Switch transport control entrypoints to AgentCore**

Do not change message rendering responsibilities.
Do remove direct dependence on scattered runtime entrypoints where possible.

- [ ] **Step 4: Re-run targeted Telegram tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run 'Control|Session' -count=1`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/adapter.go internal/transport/telegram/control_actions.go internal/transport/telegram/session_actions.go internal/transport/telegram/telegram_api.go
git commit -m "refactor(teamD): route telegram control paths through AgentCore"
```

### Task 4: Document the new center of gravity

**Files:**
- Modify: `docs/agent/core-architecture-walkthrough.md`
- Modify: `docs/agent/runtime-api-walkthrough.md`
- Modify: `docs/agent/code-map.md`
- Create: `docs/agent/agentcore.md`

- [ ] **Step 1: Update docs**

Document:
- why `AgentCore` exists
- what it owns
- what it explicitly does not own
- how transports and API handlers use it

- [ ] **Step 2: Verify docs references**

Run:

```bash
rg -n "AgentCore|RuntimeCore|agentcore" docs/agent internal/runtime internal/api internal/transport/telegram
```

Expected: all new references present and consistent.

- [ ] **Step 3: Commit**

```bash
git add docs/agent/core-architecture-walkthrough.md docs/agent/runtime-api-walkthrough.md docs/agent/code-map.md docs/agent/agentcore.md
git commit -m "docs(teamD): document AgentCore facade"
```

### Task 5: Full verification

**Files:**
- No new files expected

- [ ] **Step 1: Run full test suite**

Run:

```bash
GOTMPDIR=$PWD/.tmp/go go test ./... -count=1
```

Expected: PASS

- [ ] **Step 2: Run build**

Run:

```bash
GOTMPDIR=$PWD/.tmp/go go build ./cmd/coordinator
```

Expected: PASS

- [ ] **Step 3: Commit follow-up fixes if needed**

Only if verification reveals necessary adjustments.
