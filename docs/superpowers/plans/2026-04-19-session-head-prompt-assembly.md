# Session Head Prompt Assembly Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add canonical session head derivation and a single prompt assembly path on top of the existing context-summary compaction flow.

**Architecture:** Introduce runtime types for `SessionHead` and `PromptAssembly`, derive the head from canonical persisted state through the app layer, and make execution build provider messages only through the assembly path.

**Tech Stack:** Rust, `agentd`, `agent-runtime`, `agent-persistence`, current provider abstraction.

---

## Task 1: Add runtime session-head and prompt-assembly types (`teamD-ctx.2`)

**Files:**
- Create: `crates/agent-runtime/src/prompt.rs`
- Modify: `crates/agent-runtime/src/lib.rs`
- Test: `crates/agent-runtime/src/prompt.rs`

- [ ] **Step 1: Write failing unit tests**

Add tests asserting:
- `SessionHead::render()` emits stable compact lines
- prompt assembly orders `session head -> compact summary -> raw messages`
- prompt assembly omits optional sections when absent

- [ ] **Step 2: Run focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agent-runtime prompt -- --nocapture
```

- [ ] **Step 3: Implement minimal runtime types**

Add:
- `SessionHead`
- `PromptAssemblyInput`
- `PromptAssembly`

- [ ] **Step 4: Run focused tests to verify GREEN**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agent-runtime prompt -- --nocapture
```

## Task 2: Add canonical app/session-head derivation (`teamD-ctx.2`)

**Files:**
- Modify: `cmd/agentd/src/bootstrap.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] **Step 1: Write failing app test**

Assert that `App::session_head(session_id)` derives:
- title
- counts
- summary coverage
- pending approval count
- last user / last assistant previews

- [ ] **Step 2: Run focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app 'session_head'
```

- [ ] **Step 3: Implement canonical session-head derivation**

Keep the builder local to the app layer for this slice, but use runtime prompt
types instead of ad hoc strings.

- [ ] **Step 4: Run focused tests to verify GREEN**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app 'session_head'
```

## Task 3: Route chat execution through prompt assembly (`teamD-ctx.2`)

**Files:**
- Modify: `cmd/agentd/src/execution.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] **Step 1: Write failing execution test**

Assert that compacted chat requests include:
- session head system message first
- compact summary system message second
- only uncovered raw transcript messages after that

- [ ] **Step 2: Run focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app 'prompt_assembly'
```

- [ ] **Step 3: Implement the canonical prompt assembly path**

Replace the current ad hoc message shaping with one prompt assembly call.

- [ ] **Step 4: Run focused tests to verify GREEN**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app 'prompt_assembly'
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
  crates/agent-runtime/src/lib.rs \
  cmd/agentd/src/bootstrap.rs \
  cmd/agentd/src/execution.rs \
  cmd/agentd/tests/bootstrap_app.rs \
  docs/superpowers/specs/2026-04-19-session-head-prompt-assembly-design.md \
  docs/superpowers/plans/2026-04-19-session-head-prompt-assembly.md
git commit -m "feat: add session head prompt assembly"
```
