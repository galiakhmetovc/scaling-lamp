# Session Wake-Up and Delegation Substrate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a daemon-hosted background worker loop plus durable session inbox wake-up events, so background jobs can wake sessions without polling and so later local/remote delegation can reuse the same substrate.

**Architecture:** Extend the current session-scoped durable jobs model with a new durable inbox-event model. The daemon worker loop leases and executes jobs, persists progress/heartbeats, emits session inbox events on meaningful transitions, and schedules canonical wake-up turns only when the target session is idle. This remains one runtime path: background jobs feed inbox events, inbox events feed canonical session turns, and future local/remote delegation will ride on the same mechanism.

**Tech Stack:** Rust, `agentd`, `agent-runtime`, `agent-persistence`, existing SQLite persistence, existing daemon HTTP/JSON, current execution and scheduler seams.

---

## File Structure

- Modify: `crates/agent-runtime/src/mission.rs`
  - extend durable job result/progress helpers if needed by the worker loop
- Create: `crates/agent-runtime/src/inbox.rs`
  - session inbox event domain model
- Modify: `crates/agent-persistence/src/records.rs`
  - inbox event record conversions
- Modify: `crates/agent-persistence/src/repository.rs`
  - inbox repository traits
- Modify: `crates/agent-persistence/src/store/schema.rs`
  - inbox table and migration
- Create: `crates/agent-persistence/src/store/inbox_repos.rs`
  - inbox repository implementation
- Modify: `crates/agent-persistence/src/store.rs`
  - wire new inbox repo module
- Modify: `cmd/agentd/src/execution/supervisor.rs`
  - daemon-oriented worker tick and wake-up dispatch
- Modify: `cmd/agentd/src/execution/chat.rs`
  - canonical wake-up turn entrypoint consuming inbox events
- Create: `cmd/agentd/src/execution/background.rs`
  - job worker loop and inbox emission
- Modify: `cmd/agentd/src/daemon.rs`
  - host background worker loop inside daemon process
- Modify: `cmd/agentd/src/bootstrap/execution_ops.rs`
  - app-level APIs for worker tick and inbox processing
- Modify: `cmd/agentd/tests/bootstrap_app.rs`
  - execution and wake-up integration tests
- Modify: `cmd/agentd/tests/daemon_http.rs`
  - daemon integration tests for worker loop behavior
- Modify: `crates/agent-persistence/src/store/tests.rs`
  - inbox schema/migration tests

## Task 1: Add Durable Session Inbox Events

**Files:**
- Create: `crates/agent-runtime/src/inbox.rs`
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/repository.rs`
- Modify: `crates/agent-persistence/src/store/schema.rs`
- Create: `crates/agent-persistence/src/store/inbox_repos.rs`
- Modify: `crates/agent-persistence/src/store.rs`
- Test: `crates/agent-persistence/src/store/tests.rs`

- [ ] **Step 1: Write failing persistence tests for session inbox events**
- [ ] **Step 2: Run the targeted tests to verify they fail**
  Run: `cargo test -p agent-persistence inbox_ -- --nocapture`
- [ ] **Step 3: Implement inbox domain model, record conversions, schema, migration, and repo methods**
- [ ] **Step 4: Run targeted persistence tests to verify they pass**
  Run: `cargo test -p agent-persistence inbox_ -- --nocapture`
- [ ] **Step 5: Commit**
  Commit message: `feat: add durable session inbox events`

## Task 2: Add Background Worker Tick and Job Lifecycle Persistence

**Files:**
- Create: `cmd/agentd/src/execution/background.rs`
- Modify: `cmd/agentd/src/execution/supervisor.rs`
- Modify: `cmd/agentd/src/bootstrap/execution_ops.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] **Step 1: Write failing tests for queued job pickup, progress persistence, and terminal transitions**
- [ ] **Step 2: Run targeted tests to verify they fail**
  Run: `cargo test -p agentd background_worker -- --nocapture`
- [ ] **Step 3: Implement daemon-side worker tick that leases queued jobs, updates heartbeat/progress, and persists terminal state**
- [ ] **Step 4: Emit inbox events on completion, failure, block, and selected progress transitions**
- [ ] **Step 5: Run targeted tests to verify they pass**
  Run: `cargo test -p agentd background_worker -- --nocapture`
- [ ] **Step 6: Commit**
  Commit message: `feat: add daemon background worker loop`

## Task 3: Add Canonical Session Wake-Up Scheduling

**Files:**
- Modify: `cmd/agentd/src/execution/chat.rs`
- Modify: `cmd/agentd/src/execution/supervisor.rs`
- Modify: `cmd/agentd/src/bootstrap/execution_ops.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] **Step 1: Write failing tests for idle-session wake-up, busy-session deferral, and single-consumer inbox semantics**
- [ ] **Step 2: Run targeted tests to verify they fail**
  Run: `cargo test -p agentd wakeup_ -- --nocapture`
- [ ] **Step 3: Implement wake-up turn entrypoint that consumes queued inbox events through the canonical chat execution path**
- [ ] **Step 4: Persist system transcript entries for operator-visible wake-up events**
- [ ] **Step 5: Run targeted tests to verify they pass**
  Run: `cargo test -p agentd wakeup_ -- --nocapture`
- [ ] **Step 6: Commit**
  Commit message: `feat: add session wake-up scheduling`

## Task 4: Host Background Worker and Wake-Up Loop Inside the Daemon

**Files:**
- Modify: `cmd/agentd/src/daemon.rs`
- Modify: `cmd/agentd/tests/daemon_http.rs`

- [ ] **Step 1: Write failing daemon tests for hosted worker and wake-up execution**
- [ ] **Step 2: Run targeted tests to verify they fail**
  Run: `cargo test -p agentd daemon_background -- --nocapture`
- [ ] **Step 3: Implement daemon-owned worker thread/tick loop using the canonical app/execution APIs**
- [ ] **Step 4: Run targeted daemon tests to verify they pass**
  Run: `cargo test -p agentd daemon_background -- --nocapture`
- [ ] **Step 5: Commit**
  Commit message: `feat: host background wake-up runtime in daemon`

## Task 5: Full Verification

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
  Commit message: `test: verify background wake-up substrate`
