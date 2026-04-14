# Custom Agent Runtime MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first working Go-only version of the custom agent runtime with a coordinator, separate worker processes, `z.ai`, Telegram long polling, Postgres/pgvector-backed memory, file-based skills, MCP, and auto-compaction.

**Architecture:** A single coordinator service owns orchestration, event routing, scheduling, and policy. Workers run as separate OS processes and communicate with the coordinator over gRPC using versioned protobuf contracts. External integrations are adapters around the core, not embedded into worker logic.

**Tech Stack:** Go 1.24+, standard library concurrency/context, gRPC/protobuf, Telegram Bot API client with long polling, official `z.ai` API over HTTP, Postgres, pgvector, local filesystem artifact storage, structured logging, OpenTelemetry-compatible tracing interfaces.

---

## Pre-Implementation Constraints

Before coding beyond the initial contracts, confirm:

- exact `z.ai` API endpoints, auth format, streaming semantics, and rate limits
- Postgres schema baseline for workers, sessions, events, approvals, and memory metadata
- approval callback payload format for Telegram
- MCP server allowlist and timeout defaults

## File Structure

Planned repository structure for the MVP:

- `go.mod`
- `cmd/coordinator/main.go`
- `cmd/worker/main.go`
- `proto/worker/v1/worker.proto`
- `internal/events/types.go`
- `internal/events/bus.go`
- `internal/coordinator/service.go`
- `internal/coordinator/router.go`
- `internal/coordinator/scheduler.go`
- `internal/worker/runtime.go`
- `internal/worker/lifecycle.go`
- `internal/worker/checkpoint.go`
- `internal/worker/grpcserver.go`
- `internal/provider/provider.go`
- `internal/provider/zai/client.go`
- `internal/provider/zai/stream.go`
- `internal/transport/transport.go`
- `internal/transport/telegram/adapter.go`
- `internal/memory/memory.go`
- `internal/memory/session_store.go`
- `internal/memory/semantic_store.go`
- `internal/memory/kv_store.go`
- `internal/skills/runtime.go`
- `internal/skills/prompts.go`
- `skills/`
- `internal/mcp/runtime.go`
- `internal/mcp/registry.go`
- `internal/compaction/service.go`
- `internal/artifacts/store.go`
- `internal/policy/policy.go`
- `internal/observability/logging.go`
- `internal/observability/tracing.go`
- `internal/config/config.go`
- `internal/approvals/service.go`
- `tests/integration/coordinator_flow_test.go`
- `tests/integration/telegram_flow_test.go`
- `tests/integration/swarm_flow_test.go`
- `tests/integration/approval_flow_test.go`

### Task 1: Bootstrap Go Module And Runtime Skeleton

**Files:**
- Create: `go.mod`
- Create: `cmd/coordinator/main.go`
- Create: `cmd/worker/main.go`
- Create: `internal/config/config.go`
- Create: `internal/events/types.go`
- Create: `internal/coordinator/service.go`
- Test: `tests/integration/coordinator_flow_test.go`

- [ ] **Step 1: Write the failing integration test for service bootstrap**

```go
func TestCoordinatorBootsWithEmptyConfig(t *testing.T) {
    cfg := config.TestConfig()
    svc, err := coordinator.New(cfg)
    if err != nil {
        t.Fatalf("unexpected error: %v", err)
    }
    if svc == nil {
        t.Fatal("expected coordinator service")
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./tests/integration -run TestCoordinatorBootsWithEmptyConfig -v`
Expected: FAIL because module/packages do not exist yet

- [ ] **Step 3: Write minimal implementation**

Create:
- `go.mod` for the new module
- `config.TestConfig()`
- `coordinator.New(cfg)` returning a non-nil service
- minimal `main.go` entrypoints for coordinator and worker processes
- config wiring for Postgres DSN, artifact root, and provider credentials

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./tests/integration -run TestCoordinatorBootsWithEmptyConfig -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add go.mod cmd/coordinator/main.go cmd/worker/main.go internal/config/config.go internal/events/types.go internal/coordinator/service.go tests/integration/coordinator_flow_test.go
git commit -m "feat: bootstrap go coordinator skeleton"
```

### Task 2: Define Typed Event Model, Protobuf Contracts, And Coordinator Routing

**Files:**
- Create: `proto/worker/v1/worker.proto`
- Create: `internal/events/bus.go`
- Create: `internal/coordinator/router.go`
- Modify: `internal/events/types.go`
- Modify: `internal/coordinator/service.go`
- Test: `tests/integration/coordinator_flow_test.go`

- [ ] **Step 1: Write the failing test for inbound event routing**

```go
func TestCoordinatorRoutesInboundEvent(t *testing.T) {
    svc := testCoordinator(t)
    evt := events.InboundEvent{Source: "test", SessionID: "s1", Text: "hello"}

    result, err := svc.HandleInbound(context.Background(), evt)
    if err != nil {
        t.Fatalf("unexpected error: %v", err)
    }
    if result.SessionID != "s1" {
        t.Fatalf("expected session s1, got %q", result.SessionID)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./tests/integration -run TestCoordinatorRoutesInboundEvent -v`
Expected: FAIL because `HandleInbound` and typed events are incomplete

- [ ] **Step 3: Write minimal implementation**

Add:
- typed inbound/outbound/system event structs
- worker gRPC protobuf definitions
- in-memory event bus abstraction
- coordinator routing for inbound events
- response contract that returns session/workflow identifiers

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./tests/integration -run TestCoordinatorRoutesInboundEvent -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add proto/worker/v1/worker.proto internal/events/types.go internal/events/bus.go internal/coordinator/router.go internal/coordinator/service.go tests/integration/coordinator_flow_test.go
git commit -m "feat: add typed event routing and worker proto"
```

### Task 3: Implement Worker Lifecycle, Process Management, And Scheduler

**Files:**
- Create: `internal/coordinator/scheduler.go`
- Create: `internal/worker/runtime.go`
- Create: `internal/worker/lifecycle.go`
- Create: `internal/worker/grpcserver.go`
- Modify: `internal/coordinator/service.go`
- Test: `tests/integration/coordinator_flow_test.go`

- [ ] **Step 1: Write the failing test for worker state transitions**

```go
func TestWorkerLifecycleTransitions(t *testing.T) {
    runtime := worker.NewRuntime(worker.TestDeps())
    id, err := runtime.Start(context.Background(), worker.Spec{Role: "supervisor"})
    if err != nil {
        t.Fatalf("unexpected error: %v", err)
    }
    state := runtime.State(id)
    if state != worker.StateRunning {
        t.Fatalf("expected running state, got %v", state)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./tests/integration -run TestWorkerLifecycleTransitions -v`
Expected: FAIL because worker runtime does not exist yet

- [ ] **Step 3: Write minimal implementation**

Implement:
- worker states: created, hydrating, running, waiting, handoff, compacting, completed, failed
- scheduler entry points for spawn/resume/stop
- separate process launch for workers
- worker gRPC server bootstrap
- heartbeat reporting from worker to coordinator
- graceful shutdown and force-kill escalation path
- in-memory worker registry and state inspection

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./tests/integration -run TestWorkerLifecycleTransitions -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/coordinator/scheduler.go internal/worker/runtime.go internal/worker/lifecycle.go internal/worker/grpcserver.go internal/coordinator/service.go tests/integration/coordinator_flow_test.go
git commit -m "feat: add worker lifecycle and process scheduler"
```

### Task 4: Add Postgres/Pgvector Memory Contracts And Session Compaction

**Files:**
- Create: `internal/memory/memory.go`
- Create: `internal/memory/session_store.go`
- Create: `internal/memory/semantic_store.go`
- Create: `internal/memory/kv_store.go`
- Create: `internal/worker/checkpoint.go`
- Create: `internal/compaction/service.go`
- Test: `tests/integration/coordinator_flow_test.go`

- [ ] **Step 1: Write the failing test for compaction output**

```go
func TestCompactionProducesStructuredCheckpoint(t *testing.T) {
    svc := compaction.New(compaction.TestDeps())
    out, err := svc.Compact(context.Background(), compaction.Input{
        SessionID: "s1",
        Transcript: []string{"user: ping", "agent: pong"},
    })
    if err != nil {
        t.Fatalf("unexpected error: %v", err)
    }
    if out.WhatMattersNow == "" {
        t.Fatal("expected structured summary field")
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./tests/integration -run TestCompactionProducesStructuredCheckpoint -v`
Expected: FAIL because compaction service does not exist yet

- [ ] **Step 3: Write minimal implementation**

Implement:
- memory interfaces for session, semantic, and KV state
- in-memory test stores
- Postgres-backed repository contracts
- pgvector semantic storage contract
- checkpoint document type
- compaction service that emits structured summaries

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./tests/integration -run TestCompactionProducesStructuredCheckpoint -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/memory/memory.go internal/memory/session_store.go internal/memory/semantic_store.go internal/memory/kv_store.go internal/worker/checkpoint.go internal/compaction/service.go tests/integration/coordinator_flow_test.go
git commit -m "feat: add memory contracts and compaction"
```

### Task 5: Add z.ai Provider Adapter

**Files:**
- Create: `internal/provider/provider.go`
- Create: `internal/provider/zai/client.go`
- Create: `internal/provider/zai/stream.go`
- Modify: `internal/worker/runtime.go`
- Modify: `internal/config/config.go`
- Test: `tests/integration/coordinator_flow_test.go`

- [ ] **Step 1: Write the failing test for provider-backed worker execution**

```go
func TestWorkerCallsProvider(t *testing.T) {
    runtime := worker.NewRuntime(worker.TestDepsWithFakeProvider())
    reply, err := runtime.RunPrompt(context.Background(), worker.PromptInput{
        WorkerID: "w1",
        Messages: []string{"hello"},
    })
    if err != nil {
        t.Fatalf("unexpected error: %v", err)
    }
    if reply.Text == "" {
        t.Fatal("expected provider reply")
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./tests/integration -run TestWorkerCallsProvider -v`
Expected: FAIL because provider abstraction is missing

- [ ] **Step 3: Write minimal implementation**

Implement:
- provider interface
- fake test provider
- `z.ai` client skeleton with official API auth config, request/response types, and streaming stub
- explicit error classification for auth, rate-limit, network, and provider failures
- worker runtime integration with provider contract

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./tests/integration -run TestWorkerCallsProvider -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/provider/provider.go internal/provider/zai/client.go internal/provider/zai/stream.go internal/worker/runtime.go internal/config/config.go tests/integration/coordinator_flow_test.go
git commit -m "feat: add zai provider adapter"
```

### Task 6: Add Telegram Transport Adapter

**Files:**
- Create: `internal/transport/transport.go`
- Create: `internal/transport/telegram/adapter.go`
- Create: `internal/approvals/service.go`
- Modify: `internal/coordinator/service.go`
- Test: `tests/integration/telegram_flow_test.go`
- Test: `tests/integration/approval_flow_test.go`

- [ ] **Step 1: Write the failing test for Telegram update normalization**

```go
func TestTelegramAdapterNormalizesUpdate(t *testing.T) {
    adapter := telegram.New(telegram.TestDeps())
    evt, err := adapter.Normalize(telegram.TestMessageUpdate("hello"))
    if err != nil {
        t.Fatalf("unexpected error: %v", err)
    }
    if evt.Text != "hello" {
        t.Fatalf("expected hello, got %q", evt.Text)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./tests/integration -run TestTelegramAdapterNormalizesUpdate -v`
Expected: FAIL because Telegram adapter is not implemented

- [ ] **Step 3: Write minimal implementation**

Implement:
- transport interface
- Telegram long polling loop
- Telegram update normalization
- outbound reply rendering
- approval callback routing with idempotent handling
- persisted pending approval records
- coordinator integration path for transport-originated inbound events

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./tests/integration -run 'TestTelegramAdapterNormalizesUpdate|TestTelegramApprovalCallbackFlow' -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/transport.go internal/transport/telegram/adapter.go internal/approvals/service.go internal/coordinator/service.go tests/integration/telegram_flow_test.go tests/integration/approval_flow_test.go
git commit -m "feat: add telegram transport adapter"
```

### Task 7: Add Skills Runtime And MCP Runtime

**Files:**
- Create: `internal/skills/runtime.go`
- Create: `internal/skills/prompts.go`
- Create: `skills/README.md`
- Create: `internal/mcp/runtime.go`
- Create: `internal/mcp/registry.go`
- Modify: `internal/worker/runtime.go`
- Test: `tests/integration/coordinator_flow_test.go`

- [ ] **Step 1: Write the failing test for worker hydration with skills and MCP**

```go
func TestWorkerHydratesSkillsAndMCP(t *testing.T) {
    runtime := worker.NewRuntime(worker.TestDepsWithCapabilities())
    id, err := runtime.Start(context.Background(), worker.Spec{Role: "researcher"})
    if err != nil {
        t.Fatalf("unexpected error: %v", err)
    }
    snap := runtime.Snapshot(id)
    if len(snap.Skills) == 0 || len(snap.MCPServers) == 0 {
        t.Fatal("expected hydrated skills and MCP context")
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./tests/integration -run TestWorkerHydratesSkillsAndMCP -v`
Expected: FAIL because capability runtimes are missing

- [ ] **Step 3: Write minimal implementation**

Implement:
- file-based skill bundle loading interfaces
- prompt layering contract
- MCP registry and invocation contract
- allowlist enforcement and default timeout policy
- output size limits and failed-response handling
- worker hydration of skills and MCP descriptors

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./tests/integration -run TestWorkerHydratesSkillsAndMCP -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/skills/runtime.go internal/skills/prompts.go skills/README.md internal/mcp/runtime.go internal/mcp/registry.go internal/worker/runtime.go tests/integration/coordinator_flow_test.go
git commit -m "feat: add skills and mcp runtimes"
```

### Task 8: Add Supervisor-Led Swarm Flow

**Files:**
- Modify: `internal/coordinator/service.go`
- Modify: `internal/coordinator/scheduler.go`
- Modify: `internal/worker/runtime.go`
- Create: `tests/integration/swarm_flow_test.go`

- [ ] **Step 1: Write the failing swarm orchestration test**

```go
func TestSupervisorDelegatesToSpecialistWorkers(t *testing.T) {
    svc := testCoordinator(t)
    result, err := svc.RunGoal(context.Background(), "research and summarize")
    if err != nil {
        t.Fatalf("unexpected error: %v", err)
    }
    if result.WorkerCount < 2 {
        t.Fatalf("expected supervisor plus specialists, got %d", result.WorkerCount)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./tests/integration -run TestSupervisorDelegatesToSpecialistWorkers -v`
Expected: FAIL because swarm orchestration is missing

- [ ] **Step 3: Write minimal implementation**

Implement:
- supervisor role
- worker spawn API for specialist roles
- result collection and handoff contract
- prevention of direct worker-to-worker coordination

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./tests/integration -run TestSupervisorDelegatesToSpecialistWorkers -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/coordinator/service.go internal/coordinator/scheduler.go internal/worker/runtime.go tests/integration/swarm_flow_test.go
git commit -m "feat: add supervisor-led swarm flow"
```

### Task 9: Add Observability And Artifact Store

**Files:**
- Create: `internal/artifacts/store.go`
- Create: `internal/observability/logging.go`
- Create: `internal/observability/tracing.go`
- Modify: `internal/coordinator/service.go`
- Modify: `internal/compaction/service.go`
- Test: `tests/integration/coordinator_flow_test.go`

- [ ] **Step 1: Write the failing test for artifact-linked compaction**

```go
func TestCompactionLinksArtifactReferences(t *testing.T) {
    svc := compaction.New(compaction.TestDeps())
    out, err := svc.Compact(context.Background(), compaction.Input{
        SessionID: "s1",
        ArtifactRefs: []string{"artifact://report-1"},
    })
    if err != nil {
        t.Fatalf("unexpected error: %v", err)
    }
    if len(out.SourceArtifacts) == 0 {
        t.Fatal("expected artifact references in checkpoint")
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./tests/integration -run TestCompactionLinksArtifactReferences -v`
Expected: FAIL because artifact store and observability links are missing

- [ ] **Step 3: Write minimal implementation**

Implement:
- artifact store interface
- checkpoint references to source artifacts
- structured logger
- trace correlation identifiers on coordinator and worker paths
- worker supervision events in logs/traces
- approval lifecycle events in logs/traces

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./tests/integration -run TestCompactionLinksArtifactReferences -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/artifacts/store.go internal/observability/logging.go internal/observability/tracing.go internal/coordinator/service.go internal/compaction/service.go tests/integration/coordinator_flow_test.go
git commit -m "feat: add observability and artifact store"
```

## Notes

- Prefer narrow interfaces and typed domain objects over generic maps.
- Keep all cancellation and timeout handling explicit through `context.Context`.
- Start with in-memory test doubles for provider, memory, transport, and MCP services before binding real infrastructure.
- Keep production backends aligned to the fixed MVP choices: Postgres, pgvector, filesystem artifacts, gRPC workers, file-based skills, `z.ai` official API, Telegram long polling.
- Use `bufconn` or an equivalent in-memory gRPC harness for coordinator-worker integration tests.
- Add a mock HTTP server for `z.ai` tests to verify retry, backoff, and rate-limit handling deterministically.
