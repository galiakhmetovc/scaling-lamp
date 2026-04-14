# Runtime Governance Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce a runtime-owned governance layer for effective session policy resolution and baseline MCP tool execution controls.

**Architecture:** Add a `PolicyResolver` and MCP policy types in `internal/runtime`, then thread them through tool listing and tool execution so transports ask one runtime-owned decision API instead of reassembling policy locally.

**Tech Stack:** Go, `internal/runtime`, `internal/mcp`, `internal/transport/telegram`, stdlib.

---

### Task 1: Add runtime policy resolver and MCP policy contracts

**Files:**
- Create: `internal/runtime/policy_resolver.go`
- Create: `internal/runtime/policy_resolver_test.go`
- Modify: `internal/runtime/types.go`
- Modify: `internal/runtime/session_overrides.go`

- [ ] **Step 1: Write failing resolver tests**
- [ ] **Step 2: Run `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime -run 'PolicyResolver|MCPPolicy' -count=1`**
- [ ] **Step 3: Implement effective policy bundle, MCP policy types, and resolver**
- [ ] **Step 4: Re-run targeted runtime tests**
- [ ] **Step 5: Commit**

### Task 2: Enforce baseline MCP policy in runtime tool exposure and execution

**Files:**
- Modify: `internal/mcp/tools/shell.go`
- Modify: `internal/transport/telegram/provider_tools.go`
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/runtime_support.go`
- Test: `internal/transport/telegram/approvals_test.go`

- [ ] **Step 1: Write failing tests for deny-by-default, approval-needed, and output limits**
- [ ] **Step 2: Run targeted Telegram tests**
- [ ] **Step 3: Implement policy-backed tool filtering and execution checks**
- [ ] **Step 4: Re-run targeted tests**
- [ ] **Step 5: Commit**

### Task 3: Surface effective governance through API/docs

**Files:**
- Modify: `docs/agent/runtime-api-walkthrough.md`
- Modify: `docs/agent/operator-chat.md`
- Modify: `docs/agent/http-api.md`
- Modify: `docs/agent/code-map.md`

- [ ] **Step 1: Update docs to explain effective policy resolution**
- [ ] **Step 2: Verify references with `rg -n 'PolicyResolver|MCP policy|EffectivePolicy' docs/agent internal/runtime internal/transport/telegram`**
- [ ] **Step 3: Commit**

### Task 4: Full verification

**Files:**
- No new files expected

- [ ] **Step 1: Run `GOTMPDIR=$PWD/.tmp/go go test ./... -count=1`**
- [ ] **Step 2: Run `GOTMPDIR=$PWD/.tmp/go go build ./cmd/coordinator`**
- [ ] **Step 3: Close issue after green verification**
