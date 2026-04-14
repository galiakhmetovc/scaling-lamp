# Transport-Agnostic Runtime Execution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put run start and approval continuation resume behind a runtime-owned execution service and make Telegram and HTTP API use that service instead of transport-local orchestration.

**Architecture:** Add `runtime.ExecutionService` with explicit transport hooks. Keep the prompt/tool loop where it is for now, but centralize start/resume orchestration in runtime. This is a boundary refactor, not a state-machine rewrite.

**Tech Stack:** Go, runtime API, Telegram transport, stdlib HTTP API.

---

### Task 1: Add runtime execution service

- [x] Add `internal/runtime/execution_service.go`
- [x] Add `StartRunRequest`
- [x] Add runtime-level tests for start and approval resume

### Task 2: Switch transports to execution service

- [x] Make API server use runtime-level starter interface
- [x] Make Telegram `Reply` and `Dispatch` use execution service
- [x] Make approval callback resume use execution service

### Task 3: Update docs and verification

- [ ] Update architecture walkthrough docs
- [ ] Run focused tests
- [ ] Run `go test ./...`
- [ ] Rebuild and restart live services
