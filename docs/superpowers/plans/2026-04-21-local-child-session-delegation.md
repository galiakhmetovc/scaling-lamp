# Local Child-Session Delegation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `JobKind::Delegate` as a local child-session delegation job that produces a compact durable result package and wakes the parent session through the existing inbox substrate.

**Architecture:** Extend the existing durable delegation/background substrate rather than creating a second subagent path. Delegate jobs create child sessions with parent linkage, execute child work through the same canonical chat execution path, persist a compact delegation result in the job record, and emit `delegation_result_ready` to the parent session inbox for normal wake-up scheduling.

**Tech Stack:** Rust, `agent-runtime`, `agent-persistence`, `agentd`, SQLite persistence, existing background worker loop and wake-up execution path.

---

## File Structure

- Modify: `crates/agent-runtime/src/session.rs`
  - add optional child-session lineage metadata
- Modify: `crates/agent-runtime/src/mission.rs`
  - expand delegate job input and result shapes
- Modify: `crates/agent-runtime/src/delegation.rs`
  - keep validation and result-package types aligned with job payloads
- Modify: `crates/agent-persistence/src/records.rs`
  - session and job record conversions for delegation fields
- Modify: `crates/agent-persistence/src/store/schema.rs`
  - session schema migration for parent linkage metadata
- Modify: `crates/agent-persistence/src/store/session_mission.rs`
  - persist extended session records
- Modify: `cmd/agentd/src/bootstrap/session_ops.rs`
  - create child sessions with delegation metadata
- Modify: `cmd/agentd/src/execution/background.rs`
  - route `JobKind::Delegate` into a local child-session executor
- Modify: `cmd/agentd/src/execution/chat.rs`
  - package delegated child-session results and write operator-visible transcript entries
- Modify: `cmd/agentd/src/bootstrap.rs`
  - surface lineage in session details or summaries if needed for inspection
- Test: `cmd/agentd/tests/bootstrap_app.rs`
  - delegation integration tests
- Test: `crates/agent-persistence/src/store/tests.rs`
  - schema and record round-trip tests

## Task 1: Extend Session and Job Models for Delegation

**Files:**
- Modify: `crates/agent-runtime/src/session.rs`
- Modify: `crates/agent-runtime/src/mission.rs`
- Modify: `crates/agent-runtime/src/delegation.rs`
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/store/schema.rs`
- Modify: `crates/agent-persistence/src/store/session_mission.rs`
- Test: `crates/agent-persistence/src/store/tests.rs`

- [ ] **Step 1: Write failing tests for session lineage metadata and delegate job payload round-trips**
- [ ] **Step 2: Run targeted tests to verify they fail**
  Run: `cargo test -p agent-persistence delegation_ -- --nocapture`
- [ ] **Step 3: Implement session metadata, delegate job input/result expansion, and persistence updates**
- [ ] **Step 4: Run targeted tests to verify they pass**
  Run: `cargo test -p agent-persistence delegation_ -- --nocapture`
- [ ] **Step 5: Commit**
  Commit message: `feat: add durable delegation session metadata`

## Task 2: Implement Local Child-Session Delegate Execution

**Files:**
- Modify: `cmd/agentd/src/bootstrap/session_ops.rs`
- Modify: `cmd/agentd/src/execution/background.rs`
- Modify: `cmd/agentd/src/execution/chat.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] **Step 1: Write failing integration tests for delegate jobs creating child sessions and completing with compact result packages**
- [ ] **Step 2: Run targeted tests to verify they fail**
  Run: `cargo test -p agentd delegate_job -- --nocapture`
- [ ] **Step 3: Implement child-session creation and local delegate execution through the canonical chat path**
- [ ] **Step 4: Persist durable `JobResult::Delegation` and emit `delegation_result_ready` inbox events**
- [ ] **Step 5: Run targeted tests to verify they pass**
  Run: `cargo test -p agentd delegate_job -- --nocapture`
- [ ] **Step 6: Commit**
  Commit message: `feat: execute delegate jobs through child sessions`

## Task 3: Parent Transcript and Wake-Up Visibility

**Files:**
- Modify: `cmd/agentd/src/execution/chat.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] **Step 1: Write failing tests for parent transcript visibility and compact delegation wake-up summaries**
- [ ] **Step 2: Run targeted tests to verify they fail**
  Run: `cargo test -p agentd delegation_result_ready -- --nocapture`
- [ ] **Step 3: Implement operator-visible parent/child transcript entries without full transcript rehydration**
- [ ] **Step 4: Run targeted tests to verify they pass**
  Run: `cargo test -p agentd delegation_result_ready -- --nocapture`
- [ ] **Step 5: Commit**
  Commit message: `feat: surface local delegation results in session wakeups`

## Task 4: Full Verification

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
  Commit message: `test: verify local child-session delegation`
