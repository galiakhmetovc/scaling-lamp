# Telegram Temporary Status Lifecycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Telegram turns use a temporary status message with typing heartbeat, live tool/error counters, and delayed cleanup on next user message or 30-minute TTL.

**Architecture:** Keep one canonical runtime path. The execution layer emits richer `ChatExecutionEvent::ToolStatus` events, and the Telegram router owns a single temporary status message per chat. Final assistant output is sent as a separate message, while the status message becomes stale and is deleted on the next inbound user message or after TTL cleanup.

**Tech Stack:** Rust, teloxide, SQLite persistence via `agent-persistence`, existing Telegram router/client/tests.

---

### Task 1: Persist temporary Telegram status message state

**Files:**
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/repository.rs`
- Modify: `crates/agent-persistence/src/store/schema.rs`
- Modify: `crates/agent-persistence/src/store/telegram_repos.rs`
- Test: `crates/agent-persistence/src/store/tests.rs`

- [x] Add a dedicated persisted record for per-chat temporary status messages.
- [x] Add repository methods to put/get/list/delete the record.
- [x] Add schema bootstrap + migration for the new table.
- [x] Write store round-trip tests, then run them red/green.

### Task 2: Enrich execution events for exact tool counters

**Files:**
- Modify: `cmd/agentd/src/execution.rs`
- Modify: `cmd/agentd/src/execution/provider_loop.rs`
- Modify: `cmd/agentd/src/tui/worker.rs`
- Modify: `cmd/agentd/src/tui.rs`
- Modify: `cmd/agentd/src/cli/repl.rs`
- Test: existing execution / bootstrap / telegram tests that pattern-match `ToolStatus`

- [x] Extend `ChatExecutionEvent::ToolStatus` with a stable `tool_call_id`.
- [x] Update all emitters and consumers to preserve current behavior.
- [x] Add or update tests proving the event shape still works across runtime surfaces.

### Task 3: Add Telegram client primitives for status UX

**Files:**
- Modify: `cmd/agentd/src/telegram/client.rs`
- Test: `cmd/agentd/tests/telegram_surface.rs`

- [x] Add `send_html`, `edit_html`, `delete_message`, and `send_typing` coverage as needed.
- [x] Add transport tests for `sendChatAction` / `deleteMessage`.

### Task 4: Rework Telegram router status lifecycle

**Files:**
- Modify: `cmd/agentd/src/telegram/router.rs`
- Test: `cmd/agentd/tests/telegram_surface.rs`

- [x] Write failing tests for:
  - active temporary status message creation
  - final answer sent as a new message
  - status becoming stale after success
  - stale status deletion on next user message
  - stale status deletion after TTL
  - status retained and marked stale on error
- [x] Implement temporary status record lifecycle in the router.
- [x] Replace final `edit`-into-answer flow with `send final response` + `mark stale`.

### Task 5: Add typing heartbeat and compact status renderer

**Files:**
- Modify: `cmd/agentd/src/telegram/router.rs`
- Modify: `cmd/agentd/src/telegram/render.rs` or a new helper local to the router if cleaner
- Test: `cmd/agentd/tests/telegram_surface.rs`

- [x] Add `typing` heartbeat while a turn is active, best-effort and low frequency.
- [x] Replace `Working... / Phase: drafting / Phase: tool` plain text with compact HTML status blocks.
- [x] Show exact counters: total tool calls and failed tool calls.
- [x] Update only on meaningful state changes to avoid noisy edits.

### Task 6: Document and verify

**Files:**
- Modify: `docs/current/telegram/01-install-and-configure.md`
- Modify: any nearby Telegram architecture / config docs that describe progress updates

- [x] Document temporary status lifecycle, typing heartbeat, and cleanup policy.
- [x] Run:
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo test --workspace --all-features`
  - `cargo build -p agentd`
  - `cargo build --release -p agentd`
