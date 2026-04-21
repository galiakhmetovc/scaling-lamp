# Remote A2A Delegation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a configured-peer A2A adapter so delegate jobs can execute on a remote daemon and complete through a callback-driven result package path.

**Architecture:** Extend the durable background job substrate with `waiting_external` and callback metadata, then add daemon HTTP endpoints for delegation acceptance and completion callbacks. The existing delegate routing seam stays canonical: local child sessions and remote A2A use the same job shape, result package, and inbox wake-up flow.

**Tech Stack:** Rust, `agentd`, `agent-runtime`, `agent-persistence`, `tiny_http`, `reqwest`, existing background worker and inbox wake-up substrate.

---

## File Structure

- Modify: `crates/agent-runtime/src/mission.rs`
  - add `waiting_external` job status and callback metadata
- Modify: `crates/agent-persistence/src/config.rs`
  - add daemon A2A peer/public URL config
- Modify: `crates/agent-persistence/src/records.rs`
  - serialize/deserialize new job callback metadata
- Modify: `crates/agent-persistence/src/store/schema.rs`
  - migrate jobs schema for callback metadata
- Modify: `crates/agent-persistence/src/store/session_mission.rs`
  - select/insert new jobs columns
- Create: `cmd/agentd/src/a2a.rs`
  - remote peer client and request/response helpers
- Modify: `cmd/agentd/src/http/types.rs`
  - A2A delegation create/callback payloads
- Modify: `cmd/agentd/src/http/server.rs`
  - route A2A endpoints
- Create: `cmd/agentd/src/http/server/a2a.rs`
  - inbound delegation accept and completion callback handlers
- Modify: `cmd/agentd/src/bootstrap/execution_ops.rs`
  - app methods for accepting remote delegation and applying callbacks
- Modify: `cmd/agentd/src/execution/background.rs`
  - callback delivery pass and skip local wake-up for callback-backed remote child jobs
- Modify: `cmd/agentd/src/execution/chat.rs`
  - remote executor implementation for `a2a:<peer-id>`
- Modify: `cmd/agentd/src/execution/delegation.rs`
  - keep routing stable while using peer-aware remote executor
- Test: `cmd/agentd/tests/daemon_http.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

## Task 1: Add Durable A2A Config and Job Metadata

**Files:**
- Modify: `crates/agent-runtime/src/mission.rs`
- Modify: `crates/agent-persistence/src/config.rs`
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/store/schema.rs`
- Modify: `crates/agent-persistence/src/store/session_mission.rs`

- [ ] **Step 1: Write failing tests for A2A config round-trip and waiting-external job persistence**
- [ ] **Step 2: Run targeted tests to verify they fail**
  Run: `cargo test -p agent-persistence a2a -- --nocapture`
- [ ] **Step 3: Add peer config, public base URL, waiting-external status, and callback metadata**
- [ ] **Step 4: Run targeted tests to verify they pass**
  Run: `cargo test -p agent-persistence a2a -- --nocapture`
- [ ] **Step 5: Commit**
  Commit message: `feat: add durable a2a config and job metadata`

## Task 2: Add Daemon A2A HTTP Contract

**Files:**
- Create: `cmd/agentd/src/a2a.rs`
- Modify: `cmd/agentd/src/http/types.rs`
- Modify: `cmd/agentd/src/http/server.rs`
- Create: `cmd/agentd/src/http/server/a2a.rs`
- Modify: `cmd/agentd/src/bootstrap/execution_ops.rs`
- Test: `cmd/agentd/tests/daemon_http.rs`

- [ ] **Step 1: Write failing daemon HTTP tests for delegation acceptance and completion callback**
- [ ] **Step 2: Run targeted tests to verify they fail**
  Run: `cargo test -p agentd daemon_http_a2a -- --nocapture`
- [ ] **Step 3: Implement A2A request/response types, remote peer client, and HTTP endpoints**
- [ ] **Step 4: Run targeted tests to verify they pass**
  Run: `cargo test -p agentd daemon_http_a2a -- --nocapture`
- [ ] **Step 5: Commit**
  Commit message: `feat: add daemon a2a transport`

## Task 3: Wire Remote Executor Into Canonical Delegation Flow

**Files:**
- Modify: `cmd/agentd/src/execution/background.rs`
- Modify: `cmd/agentd/src/execution/chat.rs`
- Modify: `cmd/agentd/src/execution/delegation.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] **Step 1: Write failing integration tests for remote delegate acceptance and callback-driven completion**
- [ ] **Step 2: Run targeted tests to verify they fail**
  Run: `cargo test -p agentd a2a_delegate -- --nocapture`
- [ ] **Step 3: Implement remote delegate dispatch, callback-backed remote child jobs, and parent wake-up reuse**
- [ ] **Step 4: Run targeted tests to verify they pass**
  Run: `cargo test -p agentd a2a_delegate -- --nocapture`
- [ ] **Step 5: Commit**
  Commit message: `feat: add remote a2a delegation executor`

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
  Commit message: `test: verify remote a2a delegation`
