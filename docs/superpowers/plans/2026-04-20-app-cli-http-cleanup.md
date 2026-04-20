# App, CLI, and HTTP Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the overloaded `agentd` app/CLI/HTTP files into focused modules without changing daemon, CLI, REPL, or TUI behavior.

**Architecture:** Keep `App` as the canonical façade, keep daemon HTTP transport unchanged externally, and move parsing/dispatch/rendering/server/client logic into smaller internal modules. This is structural cleanup only.

**Tech Stack:** Rust, `tiny_http`, `reqwest::blocking`, existing integration test suite

---

### Task 1: Split CLI Into Focused Modules

**Files:**
- Create: `cmd/agentd/src/cli/parse.rs`
- Create: `cmd/agentd/src/cli/process.rs`
- Create: `cmd/agentd/src/cli/repl.rs`
- Create: `cmd/agentd/src/cli/render.rs`
- Modify: `cmd/agentd/src/cli.rs`
- Test: `cmd/agentd/tests/daemon_cli.rs`, `cmd/agentd/tests/bootstrap_app.rs`

- [ ] Write or reuse failing coverage proving daemon-backed process CLI still works for `status`, `session create/show/skills`, and `chat repl`.
- [ ] Move command parsing into `cli/parse.rs`.
- [ ] Move process dispatch and daemon routing into `cli/process.rs`.
- [ ] Move REPL backend trait and implementation into `cli/repl.rs`.
- [ ] Move renderer helpers into `cli/render.rs`.
- [ ] Re-run targeted CLI tests.

### Task 2: Split App Operations Out of `bootstrap.rs`

**Files:**
- Create: `cmd/agentd/src/bootstrap/session_ops.rs`
- Create: `cmd/agentd/src/bootstrap/context_ops.rs`
- Create: `cmd/agentd/src/bootstrap/execution_ops.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] Keep `App`, `BootstrapError`, construction, and wiring in `bootstrap.rs`.
- [ ] Move session/skills/preferences/transcript methods into `session_ops.rs`.
- [ ] Move session head, plan, compaction, and approval lookup into `context_ops.rs`.
- [ ] Move chat/mission/approval execution entrypoints into `execution_ops.rs`.
- [ ] Re-run targeted bootstrap/integration tests.

### Task 3: Split HTTP Client and Server Modules

**Files:**
- Create: `cmd/agentd/src/http/client/internal.rs`
- Create: `cmd/agentd/src/http/client/status.rs`
- Create: `cmd/agentd/src/http/client/sessions.rs`
- Create: `cmd/agentd/src/http/client/chat.rs`
- Create: `cmd/agentd/src/http/server/internal.rs`
- Create: `cmd/agentd/src/http/server/status.rs`
- Create: `cmd/agentd/src/http/server/sessions.rs`
- Create: `cmd/agentd/src/http/server/chat.rs`
- Modify: `cmd/agentd/src/http/client.rs`
- Modify: `cmd/agentd/src/http/server.rs`
- Test: `cmd/agentd/tests/daemon_http.rs`, `cmd/agentd/tests/daemon_tui.rs`, `cmd/agentd/tests/daemon_cli.rs`

- [ ] Move transport helpers and response decoding to `internal.rs`.
- [ ] Move status/session/chat operations to dedicated modules on both client and server.
- [ ] Keep route surface unchanged.
- [ ] Re-run targeted daemon HTTP/client tests.

### Task 4: Full Verification and Landing

**Files:**
- Modify only if verification reveals issues

- [ ] Run `cargo fmt --all --check`
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] Run `cargo test --workspace --all-features`
- [ ] Run `cargo build -p agentd`
- [ ] Run `cargo build --release -p agentd`
- [ ] Update beads task with handoff/close note
- [ ] Commit and push

