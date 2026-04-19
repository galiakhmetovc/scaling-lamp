# Context Summary Compaction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the first real knowledge-layer slice: canonical context summary compaction and prompt reuse for future chat turns.

**Architecture:** Add a dedicated persisted `ContextSummary` state keyed by `session_id`, expose it through `App`, build compaction through the existing provider abstraction, and make canonical chat execution prepend the stored summary while trimming the covered raw transcript prefix.

**Tech Stack:** Rust, `agentd`, `agent-runtime`, `agent-persistence`, current provider abstraction, current TUI command path.

---

## Task 1: Add canonical context summary persistence and app surface (`teamD-ctx.1`)

**Files:**
- Modify: `crates/agent-persistence/src/repository.rs`
- Modify: `crates/agent-persistence/src/store.rs`
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] **Step 1: Write failing persistence/app tests**

Add tests asserting that:
- a compacted session persists a context summary
- compactification increments the canonical session counter
- compacting with too little history is a no-op

- [ ] **Step 2: Run focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app 'compact_session'
```

- [ ] **Step 3: Implement context summary record and app methods**

Add:
- dedicated persistence record keyed by `session_id`
- get/put methods in store/repository
- `App::compact_session`
- `App::context_summary`

- [ ] **Step 4: Run focused tests to verify GREEN**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app 'compact_session'
```

## Task 2: Make canonical chat execution use compacted prompt state (`teamD-ctx.1`)

**Files:**
- Modify: `cmd/agentd/src/execution.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] **Step 1: Write failing execution tests**

Assert that when a summary covers the older prefix, provider requests include:
- one synthetic summary system message
- only uncovered trailing transcript messages

- [ ] **Step 2: Run focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app 'uses_the_context_summary'
```

- [ ] **Step 3: Implement compacted prompt assembly for the current chat path**

Keep it local to execution for this slice:
- load summary state
- synthesize summary system message
- trim covered raw transcript prefix

- [ ] **Step 4: Run focused tests to verify GREEN**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app 'uses_the_context_summary'
```

## Task 3: Replace `/compact` placeholder in TUI (`teamD-ctx.1`)

**Files:**
- Modify: `cmd/agentd/src/tui.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`
- Modify: `cmd/agentd/tests/tui_app.rs`

- [ ] **Step 1: Write failing TUI test**

Assert that `/compact` creates persisted summary state instead of only bumping the
counter.

- [ ] **Step 2: Run focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test tui_app 'compact'
```

- [ ] **Step 3: Wire `/compact` into the canonical compaction path**

Update TUI to call the real app method and refresh summary-derived metadata.

- [ ] **Step 4: Run focused tests to verify GREEN**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test tui_app 'compact'
```

## Task 4: Verify and commit

- [ ] **Step 1: Run repo verification**

```bash
/home/admin/.cargo/bin/cargo fmt --all
/home/admin/.cargo/bin/cargo clippy --workspace --all-targets --all-features -- -D warnings
/home/admin/.cargo/bin/cargo test --workspace --all-features
```

- [ ] **Step 2: Commit**

```bash
git add crates/agent-persistence/src/repository.rs \
  crates/agent-persistence/src/store.rs \
  crates/agent-persistence/src/records.rs \
  cmd/agentd/src/bootstrap.rs \
  cmd/agentd/src/execution.rs \
  cmd/agentd/src/tui.rs \
  cmd/agentd/tests/bootstrap_app.rs \
  cmd/agentd/tests/tui_app.rs \
  docs/superpowers/specs/2026-04-19-context-summary-compaction-design.md \
  docs/superpowers/plans/2026-04-19-context-summary-compaction.md
git commit -m "feat: add context summary compaction"
```
