# Delegation Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an explicit routing seam for delegate jobs so local child-session execution stays intact and a future remote A2A executor can plug into the same durable delegation substrate.

**Architecture:** Move delegate execution behind a routing module that resolves a `DelegationExecutorKind` from the current request/policy, then dispatches to either the existing local child-session path or a remote placeholder. Both paths share the same job/result/inbox contract; only the executor backend differs.

**Tech Stack:** Rust, `agentd`, `agent-runtime`, current background worker and inbox wake-up substrate.

---

## File Structure

- Create: `cmd/agentd/src/execution/delegation.rs`
  - routing types and dispatch helpers
- Modify: `cmd/agentd/src/execution.rs`
  - wire new delegation module
- Modify: `cmd/agentd/src/execution/chat.rs`
  - move local delegation logic behind routing seam
- Modify: `cmd/agentd/src/execution/background.rs`
  - use routed delegate execution path
- Test: `cmd/agentd/tests/bootstrap_app.rs`
  - local route regression and remote placeholder behavior

## Task 1: Add Routing Types and Red Tests

**Files:**
- Create: `cmd/agentd/src/execution/delegation.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] **Step 1: Write failing tests for local-vs-remote routing semantics**
- [ ] **Step 2: Run targeted tests to verify they fail**
  Run: `cargo test -p agentd delegate_routing -- --nocapture`
- [ ] **Step 3: Implement routing types and owner-based resolver**
- [ ] **Step 4: Run targeted tests to verify they pass**
  Run: `cargo test -p agentd delegate_routing -- --nocapture`
- [ ] **Step 5: Commit**
  Commit message: `feat: add delegation routing seam`

## Task 2: Dispatch Delegate Jobs Through Routing

**Files:**
- Modify: `cmd/agentd/src/execution/chat.rs`
- Modify: `cmd/agentd/src/execution/background.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] **Step 1: Write failing tests for remote placeholder blocking and unchanged local execution**
- [ ] **Step 2: Run targeted tests to verify they fail**
  Run: `cargo test -p agentd delegate_job -- --nocapture`
- [ ] **Step 3: Route delegate jobs through local or remote dispatch without changing the current result package path**
- [ ] **Step 4: Run targeted tests to verify they pass**
  Run: `cargo test -p agentd delegate_job -- --nocapture`
- [ ] **Step 5: Commit**
  Commit message: `feat: route delegate jobs through executor slots`

## Task 3: Full Verification

**Files:**
- Modify only if verification reveals gaps

- [ ] **Step 1: Run full formatting**
  Run: `cargo fmt --all`
- [ ] **Step 2: Run full lint gate**
  Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] **Step 3: Run full test suite**
  Run: `cargo test --workspace --all-features`
- [ ] **Step 4: Run debug build**
  Run: `cargo build -p agentd`
- [ ] **Step 5: Run release build**
  Run: `cargo build --release -p agentd`
- [ ] **Step 6: Commit any final fixes**
  Commit message: `test: verify delegation routing`
