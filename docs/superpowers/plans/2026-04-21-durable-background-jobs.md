# Durable Background Jobs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the canonical job model and persistence so `agentd` can represent durable session-scoped background jobs, then expose minimal current-session visibility in TUI/REPL without adding a second runtime path.

**Architecture:** Expand the existing `JobSpec`/`jobs` storage model rather than introducing a separate queue. Session summary and `\задачи`/`/jobs` read from the same canonical app/store APIs, with daemon-backed TUI using the same endpoints.

**Tech Stack:** Rust, rusqlite, ratatui, tiny_http, serde

---

### Task 1: Red tests for the expanded job domain

**Files:**
- Modify: `crates/agent-runtime/src/mission.rs`

- [ ] **Step 1: Write failing domain tests**

Add tests for:
- a session-scoped non-mission background job validating successfully
- a mission turn requiring `mission_id = Some(..)` and matching payload mission id

- [ ] **Step 2: Run targeted tests to verify they fail**

Run: `cargo test -p agent-runtime mission::tests -- --nocapture`

Expected: FAIL because `JobSpec` and validation still require the legacy shape.

- [ ] **Step 3: Implement the minimal domain changes**

Add:
- `session_id`
- optional `mission_id`
- new job kinds and inputs
- durable metadata fields
- updated validation helpers/builders

- [ ] **Step 4: Re-run the targeted domain tests**

Run: `cargo test -p agent-runtime mission::tests -- --nocapture`

Expected: PASS

### Task 2: Red tests for records, schema, and repository queries

**Files:**
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/repository.rs`
- Modify: `crates/agent-persistence/src/store/schema.rs`
- Modify: `crates/agent-persistence/src/store/execution_repos.rs`
- Modify: `crates/agent-persistence/src/store/tests.rs`

- [ ] **Step 1: Write failing persistence tests**

Add tests for:
- record round-trip of a session-scoped background job
- legacy migration preserving old mission jobs while backfilling `session_id`
- listing active jobs for one session only

- [ ] **Step 2: Run targeted persistence tests to verify they fail**

Run: `cargo test -p agent-persistence store::tests -- --nocapture`

Expected: FAIL because schema, records, and repo APIs still use the old job layout.

- [ ] **Step 3: Implement the minimal persistence changes**

Add schema columns, migration/backfill, record conversion, and repository methods for session-scoped job queries.

- [ ] **Step 4: Re-run targeted persistence tests**

Run: `cargo test -p agent-persistence store::tests -- --nocapture`

Expected: PASS

### Task 3: Red tests for app, daemon transport, and session summary counts

**Files:**
- Modify: `cmd/agentd/src/bootstrap.rs`
- Modify: `cmd/agentd/src/bootstrap/session_ops.rs`
- Modify: `cmd/agentd/src/http/types.rs`
- Modify: `cmd/agentd/src/http/server/sessions.rs`
- Modify: `cmd/agentd/src/http/client/sessions.rs`
- Modify: `cmd/agentd/tests/bootstrap_app.rs`
- Modify: `cmd/agentd/tests/daemon_http.rs`

- [ ] **Step 1: Write failing app/transport tests**

Add tests for:
- session summary background counts
- rendered current-session active job view
- daemon-backed session summary and job view

- [ ] **Step 2: Run targeted tests to verify they fail**

Run: `cargo test -p agentd bootstrap_app daemon_http -- --nocapture`

Expected: FAIL because app summaries and HTTP types do not expose background jobs yet.

- [ ] **Step 3: Implement minimal app/transport support**

Add canonical app methods, session summary counts, and daemon endpoints/client methods.

- [ ] **Step 4: Re-run targeted app/transport tests**

Run: `cargo test -p agentd bootstrap_app daemon_http -- --nocapture`

Expected: PASS

### Task 4: Red tests for TUI and REPL visibility

**Files:**
- Modify: `cmd/agentd/src/tui/backend.rs`
- Modify: `cmd/agentd/src/tui/render.rs`
- Modify: `cmd/agentd/src/tui.rs`
- Modify: `cmd/agentd/src/tui/app.rs`
- Modify: `cmd/agentd/src/cli/repl.rs`
- Modify: `cmd/agentd/tests/tui_app.rs`

- [ ] **Step 1: Write failing UI tests**

Add tests for:
- header rendering with `bg=<total> (run=<running> queued=<queued>)`
- `\задачи` / `/jobs` showing only active jobs for the current session
- REPL aliasing for `/jobs` and `\задачи`

- [ ] **Step 2: Run targeted UI tests to verify they fail**

Run: `cargo test -p agentd tui_app -- --nocapture`

Expected: FAIL because the command and counts are not implemented yet.

- [ ] **Step 3: Implement minimal UI changes**

Keep TUI/REPL thin by calling the new app/backend read APIs only.

- [ ] **Step 4: Re-run targeted UI tests**

Run: `cargo test -p agentd tui_app -- --nocapture`

Expected: PASS

### Task 5: Full verification

**Files:**
- No new files expected beyond the ones above

- [ ] **Step 1: Run formatting**

Run: `cargo fmt --all`

- [ ] **Step 2: Run lints**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

- [ ] **Step 3: Run full tests**

Run: `cargo test --workspace --all-features`

- [ ] **Step 4: Run builds**

Run: `cargo build -p agentd`

Run: `cargo build --release -p agentd`

- [ ] **Step 5: Commit**

```bash
git add crates/agent-runtime/src/mission.rs \
  crates/agent-persistence/src/records.rs \
  crates/agent-persistence/src/repository.rs \
  crates/agent-persistence/src/store/schema.rs \
  crates/agent-persistence/src/store/execution_repos.rs \
  crates/agent-persistence/src/store/tests.rs \
  cmd/agentd/src/bootstrap.rs \
  cmd/agentd/src/bootstrap/session_ops.rs \
  cmd/agentd/src/http/types.rs \
  cmd/agentd/src/http/server/sessions.rs \
  cmd/agentd/src/http/client/sessions.rs \
  cmd/agentd/src/tui/backend.rs \
  cmd/agentd/src/tui/render.rs \
  cmd/agentd/src/tui.rs \
  cmd/agentd/src/tui/app.rs \
  cmd/agentd/src/cli/repl.rs \
  cmd/agentd/tests/bootstrap_app.rs \
  cmd/agentd/tests/daemon_http.rs \
  cmd/agentd/tests/tui_app.rs \
  docs/superpowers/specs/2026-04-21-durable-background-jobs-design.md \
  docs/superpowers/plans/2026-04-21-durable-background-jobs.md
git commit -m "feat: add durable background job model"
```
