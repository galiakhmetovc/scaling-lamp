# Agent Schedules V1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add usable `interval` and `after_completion` schedules with `fresh_session` or `existing_session` delivery, visible scheduled sessions, schedule-safe autonomous execution, and operational TUI schedule management.

**Architecture:** Extend the existing `AgentSchedule` persistence and background scheduler instead of adding a new automation path. Keep scheduled launches on the canonical session/runtime loop by either creating normal fresh sessions or targeting a fixed existing session with schedule-origin metadata and auto-approved execution semantics, then expose the richer state through the existing app, daemon client, CLI, and TUI browser layers.

**Tech Stack:** Rust, rusqlite/SQLite, existing background scheduler and inbox substrate, ratatui TUI, daemon HTTP client/server.

---

## File Map

### Existing files to modify

- Modify: `crates/agent-runtime/src/agent.rs`
  Extend `AgentSchedule` with `mode`, `delivery_mode`, optional `target_session_id`, enabled/status fields, and helper semantics for cadence updates.

- Modify: `crates/agent-persistence/src/records.rs`
  Extend `AgentScheduleRecord` and any session-side record needed to persist schedule-origin session metadata.

- Modify: `crates/agent-persistence/src/store/schema.rs`
  Add schedule columns for mode/enabled/result metadata and migrate existing schedules forward.

- Modify: `crates/agent-persistence/src/store/agent_repos.rs`
  Round-trip the richer schedule fields and preserve workspace scoping.

- Modify: `crates/agent-persistence/src/store/tests.rs`
  Migration and repository coverage for the richer schedule model.

- Modify: `crates/agent-runtime/src/session.rs`
  Add minimal schedule-origin metadata for sessions created by schedules and for schedule-origin messages injected into existing sessions.

- Modify: `cmd/agentd/src/bootstrap/agent_ops.rs`
  Create/list/show/toggle schedules with richer rendering and the new mode semantics.

- Modify: `cmd/agentd/src/execution/background.rs`
  Dispatch `interval` and `after_completion` schedules correctly across `fresh_session` and `existing_session`, prevent overlap, and update terminal state fields.

- Modify: `cmd/agentd/src/bootstrap/session_ops.rs`
  Create or target schedule-owned session delivery paths with visible schedule origin metadata.

- Modify: `cmd/agentd/src/http/types.rs`
  Extend schedule payloads for `mode`, `delivery_mode`, `target_session_id`, and `enabled`.

- Modify: `cmd/agentd/src/http/server/agents.rs`
  Accept and render the richer schedule payloads.

- Modify: `cmd/agentd/src/http/client/sessions.rs`
  Mirror create/show/list/toggle schedule operations for daemon-backed clients.

- Modify: `cmd/agentd/src/cli/repl.rs`
  Keep command parity for richer schedule rendering and toggling.

- Modify: `cmd/agentd/src/tui/backend.rs`
  Add schedule toggle support to the TUI backend trait.

- Modify: `cmd/agentd/src/tui/events.rs`
  Add schedule-browser actions for enable/disable and creation wizard steps if needed.

- Modify: `cmd/agentd/src/tui/app.rs`
  Add schedule browser state/actions for enable/disable and schedule-creation dialog flow.

- Modify: `cmd/agentd/src/tui/screens/inspector.rs`
  Wire the new schedule-browser hotkeys.

- Modify: `cmd/agentd/src/tui/render.rs`
  Render richer schedule rows/details and mark both schedule-created sessions and schedule-origin messages in the session/chat headers.

- Modify: `cmd/agentd/src/tui.rs`
  Dispatch new schedule actions and schedule creation/toggle flows.

- Modify: `cmd/agentd/tests/bootstrap_app/agents.rs`
  App-level schedule creation/render tests, including `existing_session` delivery metadata.

- Modify: `cmd/agentd/tests/bootstrap_app/background.rs`
  Scheduler semantics tests for `interval`, `after_completion`, and delivery-mode interactions.

- Modify: `cmd/agentd/tests/daemon_cli.rs`
  Daemon-backed CLI schedule command tests.

- Modify: `cmd/agentd/tests/daemon_tui.rs`
  Daemon-backed TUI behavior for schedule-created sessions and visibility.

- Modify: `cmd/agentd/tests/tui_app.rs`
  TUI browser behavior tests for create/toggle/delete/detail flows.

## Task 1: Extend AgentSchedule Domain And Persistence

**Files:**
- Modify: `crates/agent-runtime/src/agent.rs`
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/store/schema.rs`
- Modify: `crates/agent-persistence/src/store/agent_repos.rs`
- Modify: `crates/agent-persistence/src/store/tests.rs`

- [ ] Add failing runtime tests for `AgentSchedule`:
  - `mode=interval`
  - `mode=after_completion`
  - `delivery_mode=fresh_session`
  - `delivery_mode=existing_session`
  - disabled schedules
  - terminal result fields
- [ ] Add failing persistence tests for record round-trips with:
  - `mode`
  - `delivery_mode`
  - `target_session_id`
  - `enabled`
  - `last_finished_at`
  - `last_result`
  - `last_error`
- [ ] Add a failing migration test that loads legacy interval-only schedule rows and backfills sensible defaults.
- [ ] Implement `AgentScheduleMode` and richer schedule fields in `crates/agent-runtime/src/agent.rs`.
- [ ] Implement record conversions in `crates/agent-persistence/src/records.rs`.
- [ ] Extend SQLite schema and migration logic in `crates/agent-persistence/src/store/schema.rs`.
- [ ] Update repository reads/writes in `crates/agent-persistence/src/store/agent_repos.rs`.
- [ ] Run targeted tests:
  - `cargo test -p agent-runtime agent::tests -- --nocapture`
  - `cargo test -p agent-persistence store::tests -- --nocapture`
- [ ] Run full verification commands.
- [ ] Commit:
  - `git add crates/agent-runtime/src/agent.rs crates/agent-persistence/src/records.rs crates/agent-persistence/src/store/schema.rs crates/agent-persistence/src/store/agent_repos.rs crates/agent-persistence/src/store/tests.rs`
  - `git commit -m "feat: extend agent schedules with modes and status"`

## Task 2: Add Schedule-Origin Session Metadata

**Files:**
- Modify: `crates/agent-runtime/src/session.rs`
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/store/schema.rs`
- Modify: `cmd/agentd/src/bootstrap/session_ops.rs`
- Modify: `cmd/agentd/tests/bootstrap_app/agents.rs`

- [ ] Add a failing app-level test that schedule-created sessions persist visible schedule-origin metadata.
- [ ] Add a failing app-level test that an `existing_session` schedule injects a message marked as `расписание: <id>`.
- [ ] Add a failing session-summary/render test that schedule-created sessions can be marked in the normal session list.
- [ ] Implement minimal immutable session metadata for schedule origin plus schedule-origin message metadata for existing-session delivery.
- [ ] Thread that metadata through both fresh-session creation and existing-session injection paths.
- [ ] Run targeted tests:
  - `cargo test -p agentd bootstrap_app::agents -- --nocapture`
- [ ] Run full verification commands.
- [ ] Commit:
  - `git add crates/agent-runtime/src/session.rs crates/agent-persistence/src/records.rs crates/agent-persistence/src/store/schema.rs cmd/agentd/src/bootstrap/session_ops.rs cmd/agentd/tests/bootstrap_app/agents.rs`
  - `git commit -m "feat: mark sessions created by schedules"`

## Task 3: Implement Scheduler Semantics For Modes And Delivery

**Files:**
- Modify: `cmd/agentd/src/execution/background.rs`
- Modify: `cmd/agentd/src/bootstrap/agent_ops.rs`
- Modify: `cmd/agentd/tests/bootstrap_app/background.rs`

- [ ] Add failing background tests for `interval`:
  - stable cadence
  - no burst catch-up
  - no duplicate overlap
- [ ] Add a failing background test that `interval + existing_session` skips a tick when the target session is busy.
- [ ] Add failing background tests for `after_completion`:
  - no relaunch while previous run is active
  - next fire computed from `last_finished_at + interval_seconds`
- [ ] Add a failing background test that `after_completion + existing_session` only observes completion for runs launched by that exact schedule.
- [ ] Add a failing background test that deleting `target_session_id` causes a replacement session to be created and rebound.
- [ ] Add a failing test that schedule errors update `last_result` and `last_error` without crashing the scheduler.
- [ ] Implement cadence updates, active-run suppression, replacement-session rebinding, and existing-session queue/skip rules in `cmd/agentd/src/execution/background.rs`.
- [ ] Update app renderers in `cmd/agentd/src/bootstrap/agent_ops.rs` to show:
  - mode
  - delivery mode
  - enabled
  - next run
  - last result
- [ ] Run targeted tests:
  - `cargo test -p agentd bootstrap_app::background -- --nocapture`
- [ ] Run full verification commands.
- [ ] Commit:
  - `git add cmd/agentd/src/execution/background.rs cmd/agentd/src/bootstrap/agent_ops.rs cmd/agentd/tests/bootstrap_app/background.rs`
  - `git commit -m "feat: add interval and after-completion schedule semantics"`

## Task 4: Enforce Schedule-Safe Auto-Approved Execution

**Files:**
- Modify: `cmd/agentd/src/execution/background.rs`
- Modify: `cmd/agentd/tests/bootstrap_app/background.rs`
- Modify: `cmd/agentd/tests/daemon_tui.rs`

- [ ] Add a failing test that a schedule-owned launch does not pause in interactive approval flow.
- [ ] Add a failing test that schedule-owned failures become terminal failed results instead of waiting forever.
- [ ] Add a failing test that `existing_session` schedule delivery also bypasses interactive approval flow.
- [ ] Implement schedule launch preferences so both fresh-session and existing-session scheduled runs execute in auto-approve mode.
- [ ] Ensure schedule-owned terminal results feed back into schedule state updates.
- [ ] Run targeted tests:
  - `cargo test -p agentd bootstrap_app::background -- --nocapture`
  - `cargo test -p agentd daemon_tui -- --nocapture`
- [ ] Run full verification commands.
- [ ] Commit:
  - `git add cmd/agentd/src/execution/background.rs cmd/agentd/tests/bootstrap_app/background.rs cmd/agentd/tests/daemon_tui.rs`
  - `git commit -m "feat: auto-approve scheduled launches"`

## Task 5: Upgrade CLI And Daemon Schedule Surface

**Files:**
- Modify: `cmd/agentd/src/http/types.rs`
- Modify: `cmd/agentd/src/http/server/agents.rs`
- Modify: `cmd/agentd/src/http/client/sessions.rs`
- Modify: `cmd/agentd/src/cli/repl.rs`
- Modify: `cmd/agentd/tests/daemon_cli.rs`

- [ ] Add failing CLI/daemon tests for:
  - richer schedule create payloads with mode
  - richer schedule create payloads with delivery mode and `target_session_id`
  - list/show rendering with mode/enabled/result fields
  - schedule enable/disable command
- [ ] Extend HTTP request/response types for `mode`, `delivery_mode`, `target_session_id`, and `enabled`.
- [ ] Add app/daemon/CLI command handling for toggling schedules on and off.
- [ ] Keep slash aliases as compatibility aliases while documenting Russian command forms.
- [ ] Run targeted tests:
  - `cargo test -p agentd daemon_cli -- --nocapture`
- [ ] Run full verification commands.
- [ ] Commit:
  - `git add cmd/agentd/src/http/types.rs cmd/agentd/src/http/server/agents.rs cmd/agentd/src/http/client/sessions.rs cmd/agentd/src/cli/repl.rs cmd/agentd/tests/daemon_cli.rs`
  - `git commit -m "feat: expose richer schedule commands over cli and daemon"`

## Task 6: Make The TUI Schedule Browser Operational

**Files:**
- Modify: `cmd/agentd/src/tui/backend.rs`
- Modify: `cmd/agentd/src/tui/events.rs`
- Modify: `cmd/agentd/src/tui/app.rs`
- Modify: `cmd/agentd/src/tui/screens/inspector.rs`
- Modify: `cmd/agentd/src/tui/render.rs`
- Modify: `cmd/agentd/src/tui.rs`
- Modify: `cmd/agentd/tests/tui_app.rs`

- [ ] Add failing TUI tests for schedule browser actions:
  - `Н` create
  - `П` enable/disable
  - `У` delete
  - `Enter` details
- [ ] Add a failing TUI render test that schedule rows show:
  - agent
  - mode
  - delivery mode
  - enabled
  - next
  - last result
- [ ] Add a failing TUI test for dialog-based schedule creation with:
  - id
  - agent
  - mode
  - delivery mode
  - interval
  - prompt
- [ ] Add a failing TUI test that `existing_session` creation accepts a concrete `target_session_id`.
- [ ] Implement backend trait support for schedule toggling.
- [ ] Implement TUI action/event/state wiring for create/toggle/delete/detail flows.
- [ ] Implement dialog/wizard rendering and parsing for schedule creation.
- [ ] Render schedule-created sessions with an explicit schedule mark in session/chat views and render schedule-origin messages in existing sessions as `расписание: <id>`.
- [ ] Run targeted tests:
  - `cargo test -p agentd tui_app -- --nocapture`
- [ ] Run full verification commands.
- [ ] Commit:
  - `git add cmd/agentd/src/tui/backend.rs cmd/agentd/src/tui/events.rs cmd/agentd/src/tui/app.rs cmd/agentd/src/tui/screens/inspector.rs cmd/agentd/src/tui/render.rs cmd/agentd/src/tui.rs cmd/agentd/tests/tui_app.rs`
  - `git commit -m "feat: add operational schedule management to tui"`

## Task 7: Final Integration And Regression Sweep

**Files:**
- Modify: `docs/superpowers/specs/2026-04-22-agent-schedules-v1-design.md` (only if implementation clarifies an invariant)
- Modify: `docs/superpowers/plans/2026-04-22-agent-schedules-v1.md` (only if implementation clarifies execution order)
- Modify: `cmd/agentd/tests/daemon_tui.rs`
- Modify: `cmd/agentd/tests/tui_app.rs`

- [ ] Run a daemon-backed manual smoke:
  - create one `interval` schedule
  - create one `after_completion` schedule
  - create one `existing_session` schedule bound to a concrete session
  - verify fresh sessions are visible with schedule marks
  - verify existing-session delivery lands in the bound session with a schedule label
  - verify one scheduled run does not overlap itself
  - verify schedule detail view updates `last_*` fields
- [ ] Update operator help or spec text if real behavior forced a clarified invariant.
- [ ] Run the full verification suite:
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo test --workspace --all-features`
  - `cargo build -p agentd`
  - `cargo build --release -p agentd`
- [ ] Commit:
  - `git add docs/superpowers/specs/2026-04-22-agent-schedules-v1-design.md docs/superpowers/plans/2026-04-22-agent-schedules-v1.md cmd/agentd/tests/daemon_tui.rs cmd/agentd/tests/tui_app.rs`
  - `git commit -m "docs: finalize agent schedules v1"`
