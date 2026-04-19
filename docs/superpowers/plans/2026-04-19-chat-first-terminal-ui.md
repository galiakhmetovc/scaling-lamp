# Chat-First Terminal UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a full-screen Rust terminal UI for the canonical chat runtime with a dedicated session screen, a chat-first workspace, inline streaming/timeline rendering, and command-driven session management.

**Architecture:** Keep one runtime path. The TUI becomes a new operator surface inside `agentd`, backed by the existing `App` and `ExecutionService` boundaries. Session metadata and command behavior move into canonical session/store structures first, then the TUI shell and chat timeline layer sit on top of that state.

**Tech Stack:** Rust, `agentd`, `agent-runtime`, `agent-persistence`, `ratatui`, `crossterm`, existing integration tests in `cmd/agentd/tests`.

---

## File Structure

### Existing files to modify

- `Cargo.toml`
  Add shared workspace dependencies for terminal UI crates.
- `cmd/agentd/Cargo.toml`
  Add `ratatui` and `crossterm`.
- `cmd/agentd/src/cli.rs`
  Keep existing CLI entrypoints, but move chat-REPL-only rendering helpers out once the TUI exists.
- `cmd/agentd/src/bootstrap.rs`
  Add app methods needed by TUI for session listing, deletion, renaming, metadata reads, and compact hook placeholder.
- `cmd/agentd/src/main.rs`
  Continue to start through the same `App`, now with a `tui` command path.
- `cmd/agentd/src/execution.rs`
  Add or expose command/session operations needed by the TUI without duplicating runtime logic.
- `crates/agent-runtime/src/session.rs`
  Extend canonical session settings for model/reasoning/think/compact metadata and helpers.
- `crates/agent-persistence/src/repository.rs`
  Extend repository traits for delete/update/list metadata as needed.
- `crates/agent-persistence/src/records.rs`
  Persist new session metadata in `settings_json`.
- `crates/agent-persistence/src/store.rs`
  Add repository methods for session deletion and any metadata list helpers.
- `README.md`
  Document `agentd tui`.

### New files to create

- `cmd/agentd/src/tui.rs`
  TUI entrypoint and event loop.
- `cmd/agentd/src/tui/app.rs`
  Local TUI app state only: active screen, selected session row, dialogs, scroll, input buffer.
- `cmd/agentd/src/tui/events.rs`
  Input events and high-level UI actions.
- `cmd/agentd/src/tui/render.rs`
  Stateless rendering helpers for top bar, timeline, session list, and dialogs.
- `cmd/agentd/src/tui/screens/session.rs`
  Session screen behavior.
- `cmd/agentd/src/tui/screens/chat.rs`
  Chat screen behavior and command parsing.
- `cmd/agentd/src/tui/timeline.rs`
  Timeline entry model and projection from canonical transcript/run events.
- `cmd/agentd/tests/tui_app.rs`
  Integration tests for the TUI shell and command behavior.

## Task 1: Refactor session state for TUI metadata and destructive actions (`teamD-23t.3`)

**Files:**
- Modify: `crates/agent-runtime/src/session.rs`
- Modify: `crates/agent-persistence/src/repository.rs`
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/store.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] **Step 1: Write failing tests for canonical session metadata**

Add tests that prove:
- session settings can persist current model
- session settings can persist reasoning visibility
- session settings can persist think level
- session settings can persist compactification count
- session rename is persisted
- session delete removes the session and its transcript view from store-facing app methods

- [ ] **Step 2: Run the focused tests to verify they fail**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app tui_like_session_metadata
```

Expected: FAIL because session metadata/delete APIs do not exist yet.

- [ ] **Step 3: Extend canonical session settings**

Add fields to `SessionSettings` for:
- `model: Option<String>`
- `reasoning_visible: bool`
- `think_level: Option<String>`
- `compactifications: u32`

Keep defaults conservative and backward-compatible with existing stored JSON.

- [ ] **Step 4: Add repository support for destructive and rename operations**

Add repository/store methods for:
- delete session
- delete related transcripts/runs/missions/jobs/artifacts as needed for session removal semantics
- rename or generic put/update path as needed
- list sessions with enough metadata for the future session screen

- [ ] **Step 5: Expose app-level methods for TUI command needs**

In `bootstrap.rs`, add methods for:
- listing sessions for UI
- creating a new session with generated id/title
- renaming a session
- deleting a session
- clearing current session into a fresh one
- updating current session settings
- compact placeholder hook

- [ ] **Step 6: Run focused tests to verify green**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app tui_like_session_metadata
```

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/agent-runtime/src/session.rs crates/agent-persistence/src/repository.rs crates/agent-persistence/src/records.rs crates/agent-persistence/src/store.rs cmd/agentd/src/bootstrap.rs cmd/agentd/tests/bootstrap_app.rs
git commit -m "feat: add canonical session metadata for tui"
```

## Task 2: Add the terminal UI shell and screen stack (`teamD-23t.4`)

**Files:**
- Modify: `Cargo.toml`
- Modify: `cmd/agentd/Cargo.toml`
- Modify: `cmd/agentd/src/lib.rs`
- Modify: `cmd/agentd/src/main.rs`
- Modify: `cmd/agentd/src/cli.rs`
- Create: `cmd/agentd/src/tui.rs`
- Create: `cmd/agentd/src/tui/app.rs`
- Create: `cmd/agentd/src/tui/events.rs`
- Create: `cmd/agentd/src/tui/render.rs`
- Create: `cmd/agentd/src/tui/screens/session.rs`
- Create: `cmd/agentd/src/tui/screens/chat.rs`
- Test: `cmd/agentd/tests/tui_app.rs`

- [ ] **Step 1: Write failing tests for TUI shell navigation**

Add tests for:
- starting with session screen when no current session exists
- opening chat screen from a selected session
- opening session screen from chat
- returning to previous chat with `Esc`
- opening create/confirm dialogs from the proper screens

- [ ] **Step 2: Run the focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test tui_app tui_shell_navigation
```

Expected: FAIL because the TUI module and command do not exist.

- [ ] **Step 3: Add terminal UI dependencies and command entrypoint**

Add `ratatui` and `crossterm`, then add `agentd tui` command parsing and a new TUI entrypoint.

- [ ] **Step 4: Implement the screen stack and local UI state**

Implement local-only state for:
- active screen
- current session id
- previous session id
- selected session row
- input buffer
- scroll offset
- dialog state

Do not duplicate canonical transcript/run state.

- [ ] **Step 5: Implement first-pass rendering**

Render:
- top bar
- session list
- chat timeline frame
- input line
- input/confirm dialogs

- [ ] **Step 6: Run focused tests to verify GREEN**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test tui_app tui_shell_navigation
```

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml cmd/agentd/Cargo.toml cmd/agentd/src/lib.rs cmd/agentd/src/main.rs cmd/agentd/src/cli.rs cmd/agentd/src/tui.rs cmd/agentd/src/tui cmd/agentd/tests/tui_app.rs
git commit -m "feat: add terminal ui shell"
```

## Task 3: Wire the chat timeline, commands, and streaming (`teamD-23t.1`)

**Files:**
- Modify: `cmd/agentd/src/bootstrap.rs`
- Modify: `cmd/agentd/src/execution.rs`
- Modify: `cmd/agentd/src/tui.rs`
- Modify: `cmd/agentd/src/tui/app.rs`
- Modify: `cmd/agentd/src/tui/render.rs`
- Modify: `cmd/agentd/src/tui/screens/chat.rs`
- Create: `cmd/agentd/src/tui/timeline.rs`
- Test: `cmd/agentd/tests/tui_app.rs`

- [ ] **Step 1: Write failing tests for chat commands and timeline behavior**

Add tests that prove:
- `/new` creates and switches immediately
- `/rename` updates the current session title
- `/clear` confirms, deletes current session, and switches to a fresh one
- `/approve` targets latest pending approval
- `/approve <approval-id>` overrides
- `/model`, `/reasoning`, `/think`, and `/compact` call the canonical app layer
- user/assistant/tool/reasoning timeline entries carry timestamps
- tool status updates a single timeline entry instead of duplicating rows

- [ ] **Step 2: Run focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test tui_app tui_chat_commands_and_timeline
```

Expected: FAIL because timeline projection and command wiring are incomplete.

- [ ] **Step 3: Build timeline projection on canonical state and events**

Create a timeline entry model that can render:
- persisted transcript lines
- streaming assistant deltas
- reasoning lines when enabled
- inline tool status
- approval notices

- [ ] **Step 4: Wire chat commands into the canonical app boundary**

No ad hoc provider/store calls from widgets. Chat screen actions must go through:
- `App`
- `ExecutionService`
- canonical session/store updates

- [ ] **Step 5: Implement top bar metadata**

Render:
- current session title
- current model
- reasoning visibility
- think level
- context tokens
- compactifications count
- message count

Use best-known values where provider-agnostic precision is unavailable.

- [ ] **Step 6: Run focused tests to verify GREEN**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test tui_app tui_chat_commands_and_timeline
```

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add cmd/agentd/src/bootstrap.rs cmd/agentd/src/execution.rs cmd/agentd/src/tui.rs cmd/agentd/src/tui/app.rs cmd/agentd/src/tui/render.rs cmd/agentd/src/tui/screens/chat.rs cmd/agentd/src/tui/timeline.rs cmd/agentd/tests/tui_app.rs
git commit -m "feat: wire terminal ui chat timeline"
```

## Task 4: Integration coverage, polish, and docs (`teamD-23t.2`)

**Files:**
- Modify: `cmd/agentd/tests/tui_app.rs`
- Modify: `cmd/agentd/tests/bootstrap_app.rs`
- Modify: `README.md`
- Modify: `cmd/agentd/src/cli.rs`
- Modify: `cmd/agentd/src/tui/*`

- [ ] **Step 1: Write failing end-to-end TUI tests**

Cover:
- entering through session screen
- creating/opening/deleting sessions
- streaming assistant text in the chat timeline
- reasoning on/off behavior
- `Esc` returning to the previous chat
- destructive action confirmation behavior

- [ ] **Step 2: Run focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test tui_app tui_end_to_end
```

Expected: FAIL until polish and end-to-end wiring are complete.

- [ ] **Step 3: Refactor any oversized TUI modules**

Keep renderer/state/screens separated. If `tui.rs` or `screens/chat.rs` turns into a god-file, split it before finalizing.

- [ ] **Step 4: Update docs**

Document:
- `agentd tui`
- session workflow
- supported chat commands
- current `/compact` placeholder behavior

- [ ] **Step 5: Run full verification**

Run:
```bash
/home/admin/.cargo/bin/cargo fmt --all
/home/admin/.cargo/bin/cargo fmt --check --all
/home/admin/.cargo/bin/cargo clippy --workspace --all-targets --all-features -- -D warnings
/home/admin/.cargo/bin/cargo test --workspace --all-features
```

Expected: all PASS

- [ ] **Step 6: Commit**

```bash
git add cmd/agentd/tests/tui_app.rs cmd/agentd/tests/bootstrap_app.rs README.md cmd/agentd/src/cli.rs cmd/agentd/src/tui
git commit -m "feat: finish chat-first terminal ui slice"
```
