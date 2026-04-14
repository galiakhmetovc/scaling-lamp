# Clean-Room Runtime Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first extensible skeleton of the clean-room agent without importing legacy runtime code.

**Architecture:** Start from the lowest reusable foundations only: explicit config graph loading, canonical event envelopes, minimal projection interfaces, and an agent builder shell. The first slice must be small but extensible, so new policies, projections, and executors can be added without redesigning the core.

**Tech Stack:** Go, stdlib, `gopkg.in/yaml.v3`

---

## File Structure

- Create: `cmd/agent/main.go`
  Responsibility: parse `--config`, build one agent instance, start it
- Create: `internal/config/types.go`
  Responsibility: root config and module reference types
- Create: `internal/config/loader.go`
  Responsibility: load explicit root config and referenced contract/policy modules
- Create: `internal/config/registry.go`
  Responsibility: register supported module kinds and validate loaded modules
- Create: `internal/runtime/events.go`
  Responsibility: canonical event envelope and event kinds for the skeleton
- Create: `internal/runtime/event_log.go`
  Responsibility: append-only event log interface and in-memory implementation
- Create: `internal/runtime/projections/projection.go`
  Responsibility: common projection interface
- Create: `internal/runtime/projections/session.go`
  Responsibility: minimal `SessionProjection`
- Create: `internal/runtime/projections/run.go`
  Responsibility: minimal `RunProjection`
- Create: `internal/runtime/agent_builder.go`
  Responsibility: assemble config loader, registry, event log, and projections into one agent instance
- Create: `internal/config/loader_test.go`
- Create: `internal/runtime/event_log_test.go`
- Create: `internal/runtime/projections/session_test.go`
- Create: `internal/runtime/projections/run_test.go`
- Create: `internal/runtime/agent_builder_test.go`

## Task 1: Config graph loader

**Files:**
- Create: `internal/config/types.go`
- Create: `internal/config/loader.go`
- Create: `internal/config/registry.go`
- Test: `internal/config/loader_test.go`

- [ ] **Step 1: Write failing tests for loading one root config with explicit module paths**
- [ ] **Step 2: Run `go test ./internal/config -count=1` and verify failure**
- [ ] **Step 3: Implement root config types, module registry, and loader**
- [ ] **Step 4: Re-run `go test ./internal/config -count=1` and verify pass**
- [ ] **Step 5: Commit**

## Task 2: Event log foundation

**Files:**
- Create: `internal/runtime/events.go`
- Create: `internal/runtime/event_log.go`
- Test: `internal/runtime/event_log_test.go`

- [ ] **Step 1: Write failing tests for appending and reading canonical event envelopes**
- [ ] **Step 2: Run `go test ./internal/runtime -run 'EventLog' -count=1` and verify failure**
- [ ] **Step 3: Implement event envelope types and in-memory event log**
- [ ] **Step 4: Re-run `go test ./internal/runtime -run 'EventLog' -count=1` and verify pass**
- [ ] **Step 5: Commit**

## Task 3: Minimal projections

**Files:**
- Create: `internal/runtime/projections/projection.go`
- Create: `internal/runtime/projections/session.go`
- Create: `internal/runtime/projections/run.go`
- Test: `internal/runtime/projections/session_test.go`
- Test: `internal/runtime/projections/run_test.go`

- [ ] **Step 1: Write failing tests for `SessionProjection` and `RunProjection` applying events**
- [ ] **Step 2: Run `go test ./internal/runtime/projections -count=1` and verify failure**
- [ ] **Step 3: Implement projection interface and minimal session/run projections**
- [ ] **Step 4: Re-run `go test ./internal/runtime/projections -count=1` and verify pass**
- [ ] **Step 5: Commit**

## Task 4: Agent builder shell

**Files:**
- Create: `internal/runtime/agent_builder.go`
- Create: `cmd/agent/main.go`
- Test: `internal/runtime/agent_builder_test.go`

- [ ] **Step 1: Write failing tests for building one agent instance from one root config**
- [ ] **Step 2: Run `go test ./internal/runtime -run 'AgentBuilder' -count=1` and verify failure**
- [ ] **Step 3: Implement `AgentBuilder` and `cmd/agent` shell**
- [ ] **Step 4: Re-run targeted builder tests**
- [ ] **Step 5: Run `go build ./cmd/agent`**
- [ ] **Step 6: Commit**

## Task 5: Verification

**Files:**
- Verify created files above

- [ ] **Step 1: Run `go test ./internal/config ./internal/runtime ./internal/runtime/projections -count=1`**
- [ ] **Step 2: Run `go build ./cmd/agent`**
- [ ] **Step 3: Commit final skeleton verification if needed**
