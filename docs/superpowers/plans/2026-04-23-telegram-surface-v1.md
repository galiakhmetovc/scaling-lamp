# Telegram Surface V1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first working Telegram operator surface slice: `agentd telegram run` with pairing, private-chat session bindings, English slash commands for session navigation, normal text -> canonical chat turn, and rate-limited progress/final response delivery.

**Architecture:** Keep Telegram as a thin daemon-backed surface. Add Telegram config and durable SQLite state in `agent-persistence`, then add a separate `agentd telegram run` process using `teloxide` only for polling/Telegram API transport. Route private-chat updates into the existing `App`/daemon operations; do not introduce a second execution loop, prompt path, or Telegram-specific runtime semantics.

**Tech Stack:** Rust, teloxide, reqwest, rusqlite, existing `agentd` daemon/client path, existing diagnostics/config/audit infrastructure

---

### Task 1: Add Telegram Config and Durable State Foundation

**Files:**
- Modify: `crates/agent-persistence/src/config.rs`
- Modify: `crates/agent-persistence/src/config/tests.rs`
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/repository.rs`
- Modify: `crates/agent-persistence/src/store.rs`
- Modify: `crates/agent-persistence/src/store/schema.rs`
- Create: `crates/agent-persistence/src/store/telegram_repos.rs`
- Modify: `crates/agent-persistence/src/store/tests.rs`
- Modify: `crates/agent-persistence/src/lib.rs`
- Modify: `config.example.toml`

- [ ] **Step 1: Write failing config tests for a new `telegram` section and `.env` token override**
- [ ] **Step 2: Run the targeted config tests and verify they fail**
- [ ] **Step 3: Add `TelegramConfig` to `AppConfig` with defaults that respect the 80% Telegram headroom policy**
- [ ] **Step 4: Write failing store tests for Telegram pairing tokens, chat bindings, and update cursor CRUD**
- [ ] **Step 5: Run the targeted store tests and verify they fail**
- [ ] **Step 6: Add Telegram records, repository traits, schema migrations, and store repo methods**
- [ ] **Step 7: Re-run the targeted config/store tests and verify they pass**

### Task 2: Add CLI Commands for Telegram Pairing and Process Entry

**Files:**
- Modify: `cmd/agentd/src/lib.rs`
- Modify: `cmd/agentd/src/cli.rs`
- Modify: `cmd/agentd/src/cli/parse.rs`
- Modify: `cmd/agentd/src/cli/process.rs`
- Modify: `cmd/agentd/src/cli/render.rs`
- Modify: `cmd/agentd/src/cli/tests.rs`
- Modify: `cmd/agentd/src/help.rs`
- Create: `cmd/agentd/src/telegram.rs`

- [ ] **Step 1: Write failing CLI parse/render tests for `agentd telegram run`, `agentd telegram pair <key>`, and `agentd telegram pairings`**
- [ ] **Step 2: Run the targeted CLI tests and verify they fail**
- [ ] **Step 3: Add Telegram command variants and process wiring without implementing the worker yet**
- [ ] **Step 4: Implement CLI pairing operations over the new persistence repos**
- [ ] **Step 5: Re-run the targeted CLI tests and verify they pass**

### Task 3: Add Telegram Transport Layer and Fake Telegram API Test Harness

**Files:**
- Modify: `cmd/agentd/Cargo.toml`
- Create: `cmd/agentd/src/telegram/client.rs`
- Create: `cmd/agentd/src/telegram/render.rs`
- Create: `cmd/agentd/src/telegram/polling.rs`
- Create: `cmd/agentd/tests/telegram_surface.rs`

- [ ] **Step 1: Write failing transport tests around a fake Telegram Bot API server**
- [ ] **Step 2: Cover `getUpdates`, `sendMessage`, `editMessageText`, `setMyCommands`, and basic file metadata paths in the fake server**
- [ ] **Step 3: Run the targeted transport tests and verify they fail**
- [ ] **Step 4: Add `teloxide` dependency and a small Telegram client wrapper that isolates polling/send/edit/register operations**
- [ ] **Step 5: Implement renderer helpers that enforce Telegram soft caps for text and captions**
- [ ] **Step 6: Re-run the targeted transport tests and verify they pass**

### Task 4: Implement Pairing Flow, Private-Chat Routing, and Session Bindings

**Files:**
- Create: `cmd/agentd/src/telegram/router.rs`
- Create: `cmd/agentd/src/telegram/backend.rs`
- Modify: `cmd/agentd/src/http/client.rs`
- Modify: `cmd/agentd/src/http/client/sessions.rs`
- Test: `cmd/agentd/tests/telegram_surface.rs`

- [ ] **Step 1: Write failing integration tests for `/start` on unpaired private chats**
- [ ] **Step 2: Add expectations for durable chat binding resolution and auto-created default session**
- [ ] **Step 3: Run the targeted Telegram integration tests and verify they fail**
- [ ] **Step 4: Implement Telegram router behavior for:**
- [ ] private chat `/start`
- [ ] pairing-required rejection for all other inputs
- [ ] `/new`, `/sessions`, `/use`, `/help`
- [ ] plain text routing into the selected private-chat session
- [ ] **Step 5: Implement Telegram backend methods by calling the existing daemon/app session and chat operations**
- [ ] **Step 6: Re-run the targeted Telegram integration tests and verify they pass**

### Task 5: Execute Canonical Chat Turns with Throttled Progress Delivery

**Files:**
- Modify: `cmd/agentd/src/telegram.rs`
- Modify: `cmd/agentd/src/telegram/router.rs`
- Modify: `cmd/agentd/src/telegram/render.rs`
- Modify: `cmd/agentd/src/telegram/client.rs`
- Test: `cmd/agentd/tests/telegram_surface.rs`

- [ ] **Step 1: Write failing integration tests for long-running chat turns producing:**
- [ ] start acknowledgement
- [ ] progress updates no more often than every 30 seconds
- [ ] final response delivery
- [ ] **Step 2: Write a failing test that long replies are chunked or offloaded instead of exceeding Telegram soft caps**
- [ ] **Step 3: Run the targeted progress/render tests and verify they fail**
- [ ] **Step 4: Implement a Telegram run loop that consumes updates and forwards private-chat text turns into the canonical chat path**
- [ ] **Step 5: Implement observer-driven progress rendering using existing `ChatExecutionEvent` without exposing a second execution loop**
- [ ] **Step 6: Add per-chat send budgeting and prefer status-message edits over new messages when possible**
- [ ] **Step 7: Re-run the targeted progress/render tests and verify they pass**

### Task 6: Register Telegram Commands and Emit Diagnostics

**Files:**
- Modify: `cmd/agentd/src/telegram.rs`
- Modify: `cmd/agentd/src/telegram/client.rs`
- Modify: `cmd/agentd/src/diagnostics.rs`
- Test: `cmd/agentd/tests/telegram_surface.rs`

- [ ] **Step 1: Write failing tests for Telegram command registration at worker startup**
- [ ] **Step 2: Write failing tests for operator-readable diagnostic events on polling errors and runtime-unavailable conditions**
- [ ] **Step 3: Run the targeted registration/diagnostic tests and verify they fail**
- [ ] **Step 4: Register the Telegram command set with Bot API on worker startup**
- [ ] **Step 5: Emit structured diagnostics for update handling, pairing outcomes, and daemon connectivity failures**
- [ ] **Step 6: Re-run the targeted registration/diagnostic tests and verify they pass**

### Task 7: Verify the Full Slice and Record Follow-Up Scope

**Files:**
- Modify if implementation clarifies constraints:
  - `docs/superpowers/specs/2026-04-23-telegram-surface-design.md`
  - `docs/current/03-surfaces.md`
  - `docs/current/07-config.md`
  - `docs/current/09-operator-cheatsheet.md`

- [ ] **Step 1: Run `cargo fmt --all`**
- [ ] **Step 2: Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`**
- [ ] **Step 3: Run `cargo test --workspace --all-features`**
- [ ] **Step 4: Run `cargo build -p agentd`**
- [ ] **Step 5: Run `cargo build --release -p agentd`**
- [ ] **Step 6: Record remaining follow-up scope for phase 2:**
- [ ] groups and shared session semantics
- [ ] files upload/download beyond the first minimal path
- [ ] inter-agent + `session_wait`
- [ ] full Telegram command parity with TUI/CLI
