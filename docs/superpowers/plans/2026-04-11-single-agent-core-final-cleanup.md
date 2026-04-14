# Single-Agent Core Final Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the current single-agent cleanup pass so the codebase is easier to read, easier to test, and comes with a minimal “build your own agent” skeleton.

**Architecture:** Keep the existing runtime behavior intact and improve clarity around three concrete seams: test organization, memory model explanation, and minimal agent scaffolding. Do not change mesh behavior or widen runtime scope. Preserve current live behavior while making the project a better reference implementation.

**Tech Stack:** Go, Telegram transport, runtime store/memory layers, Markdown docs.

---

### Task 1: Split Telegram Adapter Tests By Concern

**Files:**
- Create: `internal/transport/telegram/commands_test.go`
- Create: `internal/transport/telegram/memory_test.go`
- Create: `internal/transport/telegram/runtime_test.go`
- Create: `internal/transport/telegram/tools_test.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Move command-routing and runtime-command tests into focused files**
- [ ] **Step 2: Move memory recall and memory-tool tests into a focused file**
- [ ] **Step 3: Move tool/runtime loop tests into focused files**
- [ ] **Step 4: Keep only shared test helpers in `adapter_test.go`**
- [ ] **Step 5: Run `GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram`**
- [ ] **Step 6: Commit**

### Task 2: Add Minimal Agent Skeleton

**Files:**
- Create: `examples/minimal-agent/README.md`
- Create: `examples/minimal-agent/main.go`
- Create: `examples/minimal-agent/provider.go`
- Create: `examples/minimal-agent/tools.go`
- Create: `examples/minimal-agent/memory.go`
- Create: `examples/minimal-agent/go.mod`
- Modify: `docs/agent/09-build-your-own-agent.md`
- Modify: `docs/agent/code-map.md`

- [ ] **Step 1: Add a tiny standalone example that mirrors the real architecture**
- [ ] **Step 2: Keep the example limited to one fake provider, one fake tool, one tiny memory layer**
- [ ] **Step 3: Explain how each example file maps to the real project**
- [ ] **Step 4: Run `GOTMPDIR=$PWD/.tmp/go go test ./...` to verify the repo still builds cleanly**
- [ ] **Step 5: Run `cd examples/minimal-agent && GOTOOLCHAIN=local go test ./...` and then build/run the example binary locally**
- [ ] **Step 5: Commit**

### Task 3: Simplify Memory Narrative For Newcomers

**Files:**
- Modify: `docs/agent/05-memory-and-recall.md`
- Modify: `docs/agent/core-architecture-walkthrough.md`
- Modify: `docs/agent/request-lifecycle.md`

- [ ] **Step 1: Reframe memory around three concepts: session history, working state, searchable memory**
- [ ] **Step 2: Explain where checkpoint and continuity fit inside that simpler model**
- [ ] **Step 3: Keep policy examples aligned with `internal/config/config.go` and `internal/runtime/memory_policy.go` defaults**
- [ ] **Step 4: Commit**

### Task 4: Final Verification And Rollout

**Files:**
- Modify: `memory/2026-04-11.md`

- [ ] **Step 1: Run `GOTMPDIR=$PWD/.tmp/go go test ./...`**
- [ ] **Step 2: Update the work diary with the final cleanup summary**
- [ ] **Step 3: Close the beads issue and commit any remaining docs-only changes**
