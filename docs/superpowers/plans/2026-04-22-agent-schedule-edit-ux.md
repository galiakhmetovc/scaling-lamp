# Agent Schedule Edit UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add full schedule create, edit, enable, disable, and delete controls in the canonical app/daemon/CLI/TUI path.

**Architecture:** Extend the existing app-level schedule operations with one canonical update/patch flow, then expose that flow through HTTP, CLI, and a form-based TUI browser workflow. Keep all validation and rendering on the existing runtime path and reuse `render_agent_schedule` for browser preview refresh.

**Tech Stack:** Rust, agent runtime/app layer, tiny_http JSON routes, daemon client, ratatui TUI, existing bootstrap and schedule persistence code.

---

## File Map

- Modify: `cmd/agentd/src/bootstrap/agent_ops.rs`
- Modify: `cmd/agentd/src/http/types.rs`
- Modify: `cmd/agentd/src/http/server/agents.rs`
- Modify: `cmd/agentd/src/http/client/sessions.rs`
- Modify: `cmd/agentd/src/cli/repl.rs`
- Modify: `cmd/agentd/src/tui/backend.rs`
- Modify: `cmd/agentd/src/tui/app.rs`
- Modify: `cmd/agentd/src/tui/events.rs`
- Modify: `cmd/agentd/src/tui/render.rs`
- Modify: `cmd/agentd/src/tui/screens/inspector.rs`
- Modify: `cmd/agentd/src/tui.rs`
- Test: `cmd/agentd/tests/bootstrap_app/schedules.rs`
- Test: `cmd/agentd/tests/daemon_cli.rs`
- Test: `cmd/agentd/tests/tui_app.rs`

## Task 1: Add Canonical Schedule Update Semantics In The App Layer

**Files:**
- Modify: `cmd/agentd/src/bootstrap/agent_ops.rs`
- Test: `cmd/agentd/tests/bootstrap_app/schedules.rs`

- [ ] Write a failing app test for editing an existing schedule from interval/fresh-session to after-completion/existing-session with a new prompt and target session.
- [ ] Run the targeted test and verify it fails for the expected missing update behavior.
- [ ] Write a failing app test for quick enable/disable updates.
- [ ] Run the targeted test and verify it fails for the expected missing toggle behavior.
- [ ] Add a schedule patch/update type in the app layer.
- [ ] Implement schedule update logic by loading the current schedule, applying patch fields, rebuilding a validated `AgentSchedule`, and persisting it.
- [ ] Add a small helper for enable/disable as a thin wrapper over the same update path.
- [ ] Re-run the targeted tests and verify they pass.

## Task 2: Expose Schedule Update Over HTTP And Daemon Client

**Files:**
- Modify: `cmd/agentd/src/http/types.rs`
- Modify: `cmd/agentd/src/http/server/agents.rs`
- Modify: `cmd/agentd/src/http/client/sessions.rs`
- Test: `cmd/agentd/tests/daemon_cli.rs`

- [ ] Write a failing daemon-backed test for updating a schedule through the HTTP path.
- [ ] Run the targeted test and verify it fails because no update route exists.
- [ ] Add request payloads for schedule update and enable/disable operations.
- [ ] Add a `PATCH /v1/agent-schedules/{id}` server route.
- [ ] Add matching daemon client methods.
- [ ] Re-run the targeted daemon test and verify it passes.

## Task 3: Extend CLI Schedule Commands

**Files:**
- Modify: `cmd/agentd/src/cli/repl.rs`
- Test: `cmd/agentd/tests/daemon_cli.rs`

- [ ] Write a failing CLI test for `/schedule edit`.
- [ ] Write a failing CLI test for `/schedule enable` and `/schedule disable`.
- [ ] Run the targeted CLI tests and verify they fail on unknown subcommands.
- [ ] Extend `handle_schedule_command` with `изменить|edit`, `включить|enable`, and `выключить|disable`.
- [ ] Add explicit parsers for schedule create/edit field specs.
- [ ] Keep existing `show`, `create`, and `delete` behavior intact.
- [ ] Re-run the targeted CLI tests and verify they pass.

## Task 4: Make The TUI Schedule Browser Operational

**Files:**
- Modify: `cmd/agentd/src/tui/backend.rs`
- Modify: `cmd/agentd/src/tui/app.rs`
- Modify: `cmd/agentd/src/tui/events.rs`
- Modify: `cmd/agentd/src/tui/render.rs`
- Modify: `cmd/agentd/src/tui/screens/inspector.rs`
- Modify: `cmd/agentd/src/tui.rs`
- Test: `cmd/agentd/tests/tui_app.rs`

- [ ] Write a failing TUI test for the schedule browser edit hotkey.
- [ ] Write a failing TUI test for quick enable/disable from the schedule browser.
- [ ] Write a failing TUI test for submitting a create/edit schedule form.
- [ ] Run the targeted TUI tests and verify they fail before implementation.
- [ ] Extend the TUI backend trait with schedule update/toggle methods.
- [ ] Replace the single-line schedule create dialog with form-backed create/edit dialog state.
- [ ] Add browser hotkeys:
  - `Н` create
  - `Р` edit
  - `П` enable/disable
  - `У` delete
- [ ] Refresh browser rows and preview from canonical render methods after save/toggle/delete.
- [ ] Re-run the targeted TUI tests and verify they pass.

## Task 5: Verify End-To-End And Refresh Artifacts

**Files:**
- Modify: any files above as needed

- [ ] Run `cargo fmt --all`
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] Run `cargo test --workspace --all-features`
- [ ] Run `cargo build -p agentd`
- [ ] Run `cargo build --release -p agentd`
- [ ] Refresh local artifacts in `dist/linux-x86_64/agentd` and `dist/agentd-linux-x86_64.tar.gz`
- [ ] Update `beads` status/note for `teamD-agentos.1`
