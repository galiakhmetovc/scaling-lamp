# Session Head Filesystem Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a policy-driven filesystem summary to the prompt session head with recent activity and a bounded depth-1 tree.

**Architecture:** Extend prompt assembly input with a bounded filesystem snapshot, teach the runtime to build that snapshot from filesystem activity plus workspace root enumeration, and render the new lines through `SessionHeadParams`. Keep all limits and enablement in policy.

**Tech Stack:** Go, YAML config policies, prompt assembly executor tests, runtime contract resolution tests.

---

### Task 1: Extend session-head contracts and shipped policy

**Files:**
- Modify: `internal/contracts/contracts.go`
- Modify: `config/zai-smoke/policies/prompt-assembly/session-head.yaml`
- Test: `internal/runtime/contract_resolver_test.go`

- [ ] **Step 1: Write the failing contract-resolution test**

Add/extend a test in `internal/runtime/contract_resolver_test.go` that resolves a session-head policy containing the new filesystem params and asserts they are loaded into `SessionHeadParams`.

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/runtime -run TestResolveContracts.*SessionHead.* -count=1`
Expected: FAIL because the new YAML fields are not present in `SessionHeadParams`.

- [ ] **Step 3: Add the new params to `SessionHeadParams` and shipped policy**

Add the filesystem-related fields to `internal/contracts/contracts.go` and wire concrete values into `config/zai-smoke/policies/prompt-assembly/session-head.yaml`.

- [ ] **Step 4: Run the contract test to verify it passes**

Run: `go test ./internal/runtime -run TestResolveContracts.*SessionHead.* -count=1`
Expected: PASS

### Task 2: Add failing prompt assembly tests for filesystem sections

**Files:**
- Modify: `internal/promptassembly/executor_test.go`
- Modify: `internal/promptassembly/executor.go`

- [ ] **Step 1: Write the failing tests**

Add tests in `internal/promptassembly/executor_test.go` for:
- recent filesystem activity grouped into compact lines
- depth-1 tree rendering with bounded entries
- disabled filesystem blocks omitted

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/promptassembly -run 'TestExecutorBuild.*Filesystem' -count=1`
Expected: FAIL because prompt assembly input/executor do not yet support filesystem sections.

### Task 3: Add bounded filesystem snapshot input and rendering

**Files:**
- Modify: `internal/promptassembly/executor.go`
- Modify: `internal/promptassembly/executor_test.go`

- [ ] **Step 1: Extend prompt assembly input with filesystem snapshot types**

Define a compact input structure for:
- recent grouped activity
- tree entries

- [ ] **Step 2: Implement minimal rendering**

Teach `buildSessionHead` to append filesystem lines after the plan summary, using only the new policy params and bounded input snapshot.

- [ ] **Step 3: Run prompt assembly tests to verify they pass**

Run: `go test ./internal/promptassembly -run 'TestExecutorBuild.*Filesystem' -count=1`
Expected: PASS

### Task 4: Build runtime filesystem session-head snapshot

**Files:**
- Modify: `internal/runtime/chat.go`
- Modify: `internal/runtime/tool_loop.go`
- Modify: `internal/runtime/projections` or add a focused runtime helper if needed
- Test: `internal/runtime/smoke_test.go` or a focused runtime test file

- [ ] **Step 1: Write the failing runtime test**

Add a focused test that executes filesystem tool activity, assembles prompt messages, and asserts the session head receives:
- recent touched files grouped by action
- bounded tree depth=1 from filesystem root

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/runtime -run 'TestAgent.*Filesystem.*SessionHead' -count=1`
Expected: FAIL because runtime does not build or pass the filesystem snapshot.

- [ ] **Step 3: Implement runtime snapshot collection**

Build a bounded filesystem head snapshot from:
- recent filesystem tool messages / activity in the current prompt state
- workspace root enumeration for the shallow tree

Pass it through `promptassembly.Input`.

- [ ] **Step 4: Run runtime tests to verify they pass**

Run: `go test ./internal/runtime -run 'TestAgent.*Filesystem.*SessionHead' -count=1`
Expected: PASS

### Task 5: Full verification and cleanup

**Files:**
- Verify only

- [ ] **Step 1: Run targeted tests**

Run:
`go test ./internal/promptassembly ./internal/runtime -count=1`

Expected: PASS

- [ ] **Step 2: Run full repo tests and build**

Run:
`go test ./internal/... ./cmd/agent -count=1`
`go build ./cmd/agent`

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add internal/contracts/contracts.go \
  config/zai-smoke/policies/prompt-assembly/session-head.yaml \
  internal/promptassembly/executor.go \
  internal/promptassembly/executor_test.go \
  internal/runtime/chat.go \
  internal/runtime/tool_loop.go \
  internal/runtime/contract_resolver_test.go \
  docs/superpowers/specs/2026-04-16-session-head-filesystem-design.md \
  docs/superpowers/plans/2026-04-16-session-head-filesystem-implementation.md
git commit -m "feat(teamD): add filesystem summary to session head"
```
