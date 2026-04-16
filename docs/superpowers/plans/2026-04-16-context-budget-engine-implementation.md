# Context Budget Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver Phase 1 of the context budget engine: canonical token accounting, runtime budget snapshot exposure, and summary counter rendering in session head.

**Architecture:** Add a new contract family for context budgeting, normalize provider usage into runtime events/projections, compute session-scoped budget snapshots with exact and estimated fields, expose them through daemon/TUI/web, and render a compact summary count in prompt assembly.

**Tech Stack:** Go, YAML policy graph, runtime projections, daemon websocket/session snapshots, web/TUI rendering, prompt assembly.

---

### Task 1: Add context budget contracts and shipped policies

**Files:**
- Modify: `internal/contracts/contracts.go`
- Modify: `internal/runtime/contract_resolver.go`
- Modify: `internal/runtime/contract_resolver_test.go`
- Create: `config/zai-smoke/contracts/context-budget.yaml`
- Create: `config/zai-smoke/policies/context-budget/accounting.yaml`
- Create: `config/zai-smoke/policies/context-budget/estimation.yaml`
- Create: `config/zai-smoke/policies/context-budget/compaction.yaml`
- Create: `config/zai-smoke/policies/context-budget/summary-display.yaml`
- Modify: `config/zai-smoke/agent.yaml`

- [ ] **Step 1: Write failing contract-resolution tests**

Add tests asserting that `ResolveContracts` loads `ContextBudgetContract` and its shipped params.

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/runtime -run 'TestResolveContracts.*ContextBudget' -count=1`
Expected: FAIL because the contract family does not exist yet.

- [ ] **Step 3: Implement contract types and resolver wiring**

Add the new contract family and ship config/policy files under `config/zai-smoke`.

- [ ] **Step 4: Run contract test to verify it passes**

Run: `go test ./internal/runtime -run 'TestResolveContracts.*ContextBudget' -count=1`
Expected: PASS

### Task 2: Normalize provider usage into a runtime-owned shape

**Files:**
- Modify: `internal/provider/...`
- Modify: `internal/runtime/chat.go`
- Modify: `internal/runtime/tool_loop.go` if required
- Test: `internal/provider/..._test.go`
- Test: `internal/runtime/..._test.go`

- [ ] **Step 1: Write failing normalization tests**

Cover provider responses that include usage and assert the normalized runtime shape is produced.

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/provider ./internal/runtime -run 'Test.*Usage.*Normalized' -count=1`
Expected: FAIL because there is no canonical normalized surface yet.

- [ ] **Step 3: Implement normalization**

Normalize provider usage into one internal structure and attach it to runtime events/results.

- [ ] **Step 4: Run tests to verify they pass**

Run: `go test ./internal/provider ./internal/runtime -run 'Test.*Usage.*Normalized' -count=1`
Expected: PASS

### Task 3: Add context budget projection and session snapshot

**Files:**
- Create: `internal/runtime/projections/context_budget.go`
- Create: `internal/runtime/projections/context_budget_test.go`
- Modify: `internal/runtime/projections/registry.go`
- Modify: `internal/runtime/agent_builder.go` or shipped projection config as needed
- Modify: daemon session snapshot files under `internal/runtime/daemon/...`

- [ ] **Step 1: Write failing projection tests**

Add tests asserting that a sequence of run usage events produces a `ContextBudgetSnapshot` with:
- exact last usage
- estimated current context
- estimated next input
- summary counters defaulting to zero

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/runtime/projections -run 'TestContextBudgetProjection' -count=1`
Expected: FAIL because the projection does not exist yet.

- [ ] **Step 3: Implement projection and daemon exposure**

Add the projection, register it, and include its snapshot in daemon session/bootstrap payloads.

- [ ] **Step 4: Run projection/daemon tests to verify they pass**

Run: `go test ./internal/runtime/projections ./internal/runtime/daemon -run 'TestContextBudgetProjection|Test.*Budget.*SessionSnapshot' -count=1`
Expected: PASS

### Task 4: Add session-head summary counter display

**Files:**
- Modify: `internal/contracts/contracts.go`
- Modify: `internal/promptassembly/executor.go`
- Modify: `internal/promptassembly/executor_test.go`
- Modify: `config/zai-smoke/policies/prompt-assembly/session-head.yaml`

- [ ] **Step 1: Write failing prompt assembly test**

Add a test asserting that when `include_summary_counter` is enabled and the budget snapshot reports summaries, the session head contains a compact line such as `🧠 Summaries: 2`.

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/promptassembly -run 'TestExecutorBuild.*SummaryCounter' -count=1`
Expected: FAIL because session head does not yet render the field.

- [ ] **Step 3: Implement summary-counter rendering**

Extend prompt assembly input/policy to render the counter from the canonical budget snapshot.

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./internal/promptassembly -run 'TestExecutorBuild.*SummaryCounter' -count=1`
Expected: PASS

### Task 5: Wire TUI and web to the canonical budget snapshot

**Files:**
- Modify: `internal/runtime/tui/...`
- Modify: `web/src/...`
- Test: `internal/runtime/tui/..._test.go`
- Test: `web/src/...test.tsx`

- [ ] **Step 1: Write failing UI tests**

Add tests asserting TUI and web consume the canonical budget fields and distinguish exact vs estimated values.

- [ ] **Step 2: Run tests to verify they fail**

Run:
`go test ./internal/runtime/tui -run 'Test.*Budget' -count=1`
`cd web && npm test -- --run`
Expected: FAIL on missing canonical budget fields/rendering.

- [ ] **Step 3: Implement rendering**

Update status bars and related UI surfaces to consume daemon budget snapshot data instead of local ad hoc token counting.

- [ ] **Step 4: Run tests to verify they pass**

Run:
`go test ./internal/runtime/tui -run 'Test.*Budget' -count=1`
`cd web && npm test -- --run`
Expected: PASS

### Task 6: Full verification and ship

**Files:**
- Verify only

- [ ] **Step 1: Run targeted tests**

Run:
`go test ./internal/provider ./internal/runtime ./internal/runtime/projections ./internal/runtime/daemon ./internal/promptassembly ./internal/runtime/tui -count=1`

- [ ] **Step 2: Run full repo tests and build**

Run:
`go test ./internal/... ./cmd/agent -count=1`
`go build ./cmd/agent`

- [ ] **Step 3: Commit**

```bash
git add internal/contracts/contracts.go \
  internal/runtime/contract_resolver.go \
  internal/runtime/contract_resolver_test.go \
  internal/runtime/projections/registry.go \
  internal/promptassembly/executor.go \
  internal/promptassembly/executor_test.go \
  config/zai-smoke/agent.yaml \
  config/zai-smoke/contracts/context-budget.yaml \
  config/zai-smoke/policies/context-budget \
  docs/superpowers/specs/2026-04-16-context-budget-engine-design.md \
  docs/superpowers/plans/2026-04-16-context-budget-engine-implementation.md
git commit -m "feat(teamD): add phase 1 context budget engine"
```
