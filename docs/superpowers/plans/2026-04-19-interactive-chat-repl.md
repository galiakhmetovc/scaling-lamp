# Interactive Chat REPL Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a terminal-native interactive chat mode on top of the canonical Rust runtime so operators can hold one session open, send repeated turns, inspect transcript state, and approve pending tool calls without dropping into one-command-per-turn usage.

**Architecture:** Extend `agentd` with a `chat repl <session-id>` command that runs a thin stdin/stdout loop over the existing `chat send`, `chat show`, and `approval approve` execution paths. Keep all state transitions inside the current `ExecutionService` and CLI helpers so the REPL is only a surface layer, not a second runtime.

**Tech Stack:** Rust, existing `agentd` CLI/bootstrap tests, canonical run/approval persistence, standard library stdin/stdout I/O

---

### Task 1: Cover REPL command parsing and transcript flow

**Files:**
- Modify: `cmd/agentd/src/cli.rs`
- Test: `cmd/agentd/src/bootstrap.rs`

- [ ] **Step 1: Write the failing tests**
  Add CLI/bootstrap tests that drive `chat repl <session-id>` with scripted input and assert that:
  - normal chat turns print assistant replies inline
  - `/show` prints the stored transcript
  - `/exit` terminates cleanly

- [ ] **Step 2: Run targeted tests to verify they fail**

  Run: `/home/admin/.cargo/bin/cargo test -p agentd repl_`

  Expected: FAIL because `chat repl` is not implemented yet.

- [ ] **Step 3: Write the minimal implementation**
  Add a `ChatRepl` CLI command and a small REPL runner that reads stdin line-by-line, dispatches normal lines through the current `chat send` path, and handles `/show`, `/help`, and `/exit`.

- [ ] **Step 4: Run targeted tests to verify they pass**

  Run: `/home/admin/.cargo/bin/cargo test -p agentd repl_`

  Expected: PASS

- [ ] **Step 5: Commit**

  ```bash
  git add cmd/agentd/src/cli.rs cmd/agentd/src/bootstrap.rs
  git commit -m "feat: add interactive chat repl"
  ```

### Task 2: Cover approval-aware REPL behavior

**Files:**
- Modify: `cmd/agentd/src/cli.rs`
- Test: `cmd/agentd/src/bootstrap.rs`

- [ ] **Step 1: Write the failing tests**
  Add tests that prove:
  - a turn that hits `waiting_approval` prints `run_id` and `approval_id`
  - `/approve <approval-id>` resumes the pending run for the active session
  - the final assistant reply is printed back into the REPL transcript flow

- [ ] **Step 2: Run targeted tests to verify they fail**

  Run: `/home/admin/.cargo/bin/cargo test -p agentd repl_approval`

  Expected: FAIL because approval commands are not wired into the REPL yet.

- [ ] **Step 3: Write the minimal implementation**
  Track the current session’s latest pending run inside the REPL loop and wire `/approve <approval-id>` to the existing `approval approve <run-id> <approval-id>` path.

- [ ] **Step 4: Run targeted tests to verify they pass**

  Run: `/home/admin/.cargo/bin/cargo test -p agentd repl_approval`

  Expected: PASS

- [ ] **Step 5: Commit**

  ```bash
  git add cmd/agentd/src/cli.rs cmd/agentd/src/bootstrap.rs
  git commit -m "feat: add approval-aware repl flow"
  ```

### Task 3: Document and verify the operator path

**Files:**
- Modify: `README.md`
- Test: `cmd/agentd/tests/chat_smoke.rs` or `cmd/agentd/src/bootstrap.rs`

- [ ] **Step 1: Write the failing smoke/assertion**
  Add or extend a smoke-style test that proves the documented `chat repl` path works for a basic scripted session.

- [ ] **Step 2: Run targeted tests to verify they fail**

  Run: `/home/admin/.cargo/bin/cargo test -p agentd chat_smoke`

  Expected: FAIL until docs-backed coverage is present.

- [ ] **Step 3: Write the minimal implementation**
  Document `chat repl`, slash commands, and the approval flow in `README.md`.

- [ ] **Step 4: Run full verification**

  Run:
  - `/home/admin/.cargo/bin/cargo fmt --check`
  - `/home/admin/.cargo/bin/cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `/home/admin/.cargo/bin/cargo test --workspace --all-features`

  Expected: PASS

- [ ] **Step 5: Commit**

  ```bash
  git add README.md cmd/agentd/src/bootstrap.rs cmd/agentd/src/cli.rs
  git commit -m "docs: cover interactive chat repl"
  ```
