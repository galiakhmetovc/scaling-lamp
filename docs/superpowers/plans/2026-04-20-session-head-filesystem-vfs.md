# Session Head Filesystem And VFS Context Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the canonical `SessionHead` with recent filesystem activity and
shallow workspace-tree context, without adding a second prompt path or a new
persisted projection.

**Architecture:** Keep `SessionHead` derived in the app layer, enrich run-step
detail for filesystem tool completions, and render the new sections inside the
existing synthetic `system` message consumed by `PromptAssembly`.

**Tech Stack:** Rust, `agentd`, `agent-runtime`, current workspace/tool/run
model.

---

## Task 1: Add runtime session-head filesystem/VFS types (`teamD-ctx.3`)

**Files:**
- Modify: `crates/agent-runtime/src/prompt.rs`
- Test: `crates/agent-runtime/src/prompt.rs`

- [ ] **Step 1: Write failing unit tests**

Add tests asserting:
- `SessionHead::render()` includes `Recent Filesystem Activity`
- `SessionHead::render()` includes `Workspace Tree`
- rendered lines are compact and stable

- [ ] **Step 2: Run focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agent-runtime prompt -- --nocapture
```

- [ ] **Step 3: Implement minimal runtime types**

Add bounded runtime structs for:
- filesystem activity rows
- workspace entries

Then extend `SessionHead` and rendering.

- [ ] **Step 4: Run focused tests to verify GREEN**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agent-runtime prompt -- --nocapture
```

## Task 2: Derive canonical filesystem/VFS head data (`teamD-ctx.3`)

**Files:**
- Modify: `cmd/agentd/src/prompting.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] **Step 1: Write failing app test**

Assert that `App::session_head(session_id)` derives:
- recent filesystem activity from session run steps
- shallow workspace entries from the configured workspace root

- [ ] **Step 2: Run focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app 'session_head'
```

- [ ] **Step 3: Implement canonical session-head derivation**

Keep the derivation local to the app/prompting layer and bounded:
- newest-first filesystem slice
- shallow root tree
- no transcript scraping

- [ ] **Step 4: Run focused tests to verify GREEN**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app 'session_head'
```

## Task 3: Preserve filesystem activity through canonical run steps (`teamD-ctx.3`)

**Files:**
- Modify: `cmd/agentd/src/execution.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] **Step 1: Write failing execution test**

Assert that a chat/tool prompt assembled after filesystem tools includes:
- session head first
- filesystem activity lines in that same session head

- [ ] **Step 2: Run focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app 'filesystem'
```

- [ ] **Step 3: Implement stable run-step detail**

When filesystem tools complete, record:

`<tool call summary> -> <tool output summary>`

so session-head derivation can inspect canonical run history without transcript
parsing.

- [ ] **Step 4: Run focused tests to verify GREEN**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app 'filesystem'
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
git add crates/agent-runtime/src/prompt.rs \
  cmd/agentd/src/prompting.rs \
  cmd/agentd/src/bootstrap.rs \
  cmd/agentd/src/execution.rs \
  cmd/agentd/tests/bootstrap_app.rs \
  docs/superpowers/specs/2026-04-20-session-head-filesystem-vfs-design.md \
  docs/superpowers/plans/2026-04-20-session-head-filesystem-vfs.md
git commit -m "feat: add session head filesystem context"
```
