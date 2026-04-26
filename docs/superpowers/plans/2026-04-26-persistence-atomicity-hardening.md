# Persistence Atomicity Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the remaining proven multi-statement SQLite consistency races in the persistence layer without introducing a second runtime path or broad storage redesign.

**Architecture:** Keep the existing repository interfaces and storage layout intact. Add narrow `BEGIN IMMEDIATE` transactions around the proven multi-step SQLite mutation paths, and cover each path with regression tests that fail before the fix and pass after it.

**Tech Stack:** Rust, rusqlite, SQLite WAL/runtime store, cargo test

---

### Task 1: Persist the scope and test targets

**Files:**
- Create: `docs/superpowers/plans/2026-04-26-persistence-atomicity-hardening.md`
- Modify: `crates/agent-persistence/src/store/tests.rs`

- [ ] **Step 1: Record the exact target paths**

Target only these proven paths:
- `put_telegram_user_pairing`
- `delete_knowledge_source`
- `delete_session`

- [ ] **Step 2: Keep `context_offload` out of this change**

Do not fold `context_offload` into this patch unless a failing test proves the same SQLite race class. That path mixes DB updates and payload staging and should be treated separately.

### Task 2: Add failing regression tests first

**Files:**
- Modify: `crates/agent-persistence/src/store/tests.rs`

- [ ] **Step 1: Write a concurrency regression for telegram pairing**

Add a test that opens two runtime stores against the same scaffold, coordinates concurrent `put_telegram_user_pairing` calls for the same `telegram_user_id` with different tokens, and proves the old `DELETE` + `INSERT` path can fail with a `UNIQUE` violation.

- [ ] **Step 2: Run the new pairing test and confirm it fails**

Run: `cargo test -p agent-persistence put_telegram_user_pairing_serializes_concurrent_replacements -- --exact --nocapture`

Expected before fix: failure due to `UNIQUE constraint failed` or equivalent concurrent write error in the old path.

- [ ] **Step 3: Write a delete atomicity regression for knowledge sources**

Add a test that proves `delete_knowledge_source` removes the source row, search docs, and FTS rows as one logical unit, with no leftover FTS entries after completion.

- [ ] **Step 4: Write a delete atomicity regression for sessions**

Add a test that proves `delete_session` removes the session row and session-search FTS rows together, while payload cleanup still succeeds after the DB mutation.

### Task 3: Implement minimal transactional fixes

**Files:**
- Modify: `crates/agent-persistence/src/store/telegram_repos.rs`
- Modify: `crates/agent-persistence/src/store/memory_repos.rs`
- Modify: `crates/agent-persistence/src/store/session_mission.rs`

- [ ] **Step 1: Fix telegram pairing**

Wrap `put_telegram_user_pairing` in `rusqlite::Transaction::new_unchecked(..., TransactionBehavior::Immediate)` and keep the current semantics (`DELETE` conflicting rows, then `INSERT`) inside one transaction.

- [ ] **Step 2: Fix knowledge source deletion**

Make `delete_knowledge_source` use one immediate transaction:
1. delete related FTS rows,
2. delete the `knowledge_sources` row,
3. rely on `ON DELETE CASCADE` for `knowledge_search_docs`,
4. commit.

- [ ] **Step 3: Fix session deletion**

Make the DB portion of `delete_session` use one immediate transaction:
1. delete related `session_search_fts` rows,
2. delete the `sessions` row,
3. rely on `ON DELETE CASCADE` for session-owned rows,
4. commit,
5. then perform payload file cleanup after the committed DB delete.

### Task 4: Verify and document

**Files:**
- Modify: `docs/current/06-storage-recovery-and-diagnostics.md`

- [ ] **Step 1: Update storage diagnostics docs**

Add a short note that the persistence layer now serializes the known multi-statement rebuild/delete paths (`session_search`, `knowledge_search`, `telegram pairing`, `session delete`, `knowledge source delete`) with immediate transactions to avoid transient uniqueness races and partial FTS cleanup windows.

- [ ] **Step 2: Run full verification**

Run:
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `cargo build -p agentd`
- `cargo build --release -p agentd`

- [ ] **Step 3: Commit and deploy**

Commit the changes, push to `origin/master`, deploy to the remote server with the existing deploy flow, restart affected services if needed, and confirm the installed `/opt/teamd/bin/agentd` commit matches the pushed revision.
