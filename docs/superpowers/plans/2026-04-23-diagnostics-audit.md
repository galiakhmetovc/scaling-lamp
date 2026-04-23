# Diagnostics Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a canonical structured diagnostics layer on top of `audit/runtime.jsonl`, instrument the riskiest runtime paths, and expose a bounded operator-facing log tail for debugging.

**Architecture:** Keep one canonical audit path and add a small shared diagnostic logger that writes structured JSONL events. Instrument config/daemon/session/storage paths through helper functions, then expose a thin CLI/TUI/debug-bundle surface on top of the same log file.

**Tech Stack:** Rust, serde/serde_json, existing `agent-persistence` audit path, existing CLI/TUI/debug bundle surfaces

---

### Task 1: Shared Diagnostic Logger Foundation

**Files:**
- Create: `cmd/agentd/src/diagnostics.rs`
- Modify: `cmd/agentd/src/lib.rs`
- Test: `cmd/agentd/tests/bootstrap_app/core.rs`

- [ ] **Step 1: Write failing tests for diagnostic event append and bounded tail read**
- [ ] **Step 2: Run the targeted tests to verify they fail**
- [ ] **Step 3: Implement a shared JSONL event writer and tail reader over `data_dir/audit/runtime.jsonl`**
- [ ] **Step 4: Re-run the targeted tests and make them pass**

### Task 2: Config and Daemon Lifecycle Instrumentation

**Files:**
- Modify: `crates/agent-persistence/src/config.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`
- Modify: `cmd/agentd/src/http/client.rs`
- Modify: `cmd/agentd/src/daemon.rs`
- Test: `cmd/agentd/tests/daemon_tui.rs`
- Test: `cmd/agentd/tests/daemon_http.rs`

- [ ] **Step 1: Write/extend failing tests for daemon compatibility/restart diagnostics**
- [ ] **Step 2: Instrument config capture/data_dir resolution events**
- [ ] **Step 3: Instrument daemon status probe, reuse, mismatch, stop, and spawn decisions**
- [ ] **Step 4: Re-run daemon-path tests and confirm green**

### Task 3: Session Route and Storage Hot Path Instrumentation

**Files:**
- Modify: `cmd/agentd/src/http/client/sessions.rs`
- Modify: `cmd/agentd/src/http/server/sessions.rs`
- Modify: `cmd/agentd/src/bootstrap/session_ops.rs`
- Modify: `crates/agent-persistence/src/store/session_mission.rs`
- Modify: `crates/agent-persistence/src/store.rs`
- Test: `cmd/agentd/tests/bootstrap_app/core.rs`

- [ ] **Step 1: Write failing tests for delete/list/open/create diagnostics coverage**
- [ ] **Step 2: Instrument HTTP request start/finish/error for session routes**
- [ ] **Step 3: Instrument storage enumeration/delete paths for transcripts/artifacts**
- [ ] **Step 4: Re-run targeted session tests and confirm green**

### Task 4: Operator-Facing Log Tail and Debug Bundle Integration

**Files:**
- Modify: `cmd/agentd/src/bootstrap/context_ops.rs`
- Modify: `cmd/agentd/src/cli/repl.rs`
- Modify: `cmd/agentd/src/help.rs`
- Modify: `cmd/agentd/src/tui.rs`
- Test: `cmd/agentd/tests/bootstrap_app/core.rs`
- Test: `cmd/agentd/src/tui.rs`

- [ ] **Step 1: Add a bounded log tail render in the app layer**
- [ ] **Step 2: Expose it in CLI/REPL and TUI**
- [ ] **Step 3: Include recent diagnostics tail in debug bundle output**
- [ ] **Step 4: Re-run targeted CLI/TUI/debug bundle tests**

### Task 5: Full Verification and Follow-Up Audit Notes

**Files:**
- Modify: `docs/superpowers/plans/2026-04-23-runtime-audit.md`

- [ ] **Step 1: Run `cargo fmt --all`**
- [ ] **Step 2: Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`**
- [ ] **Step 3: Run `cargo test --workspace --all-features`**
- [ ] **Step 4: Run `cargo build -p agentd`**
- [ ] **Step 5: Run `cargo build --release -p agentd`**
- [ ] **Step 6: Record remaining high-risk follow-up audit targets**
