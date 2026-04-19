# Streaming Chat REPL Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stream assistant text and a single live-updating tool status line into `agentd chat repl` while preserving the existing canonical chat runtime and approval loop.

**Architecture:** Extend the provider layer with typed streaming events, extend execution with observer callbacks for provider and tool lifecycle transitions, and keep `chat repl` as a thin renderer that redraws one active tool-status line instead of creating a second runtime path.

**Tech Stack:** Rust, reqwest blocking HTTP, SSE parsing for `z.ai`, existing `agentd` bootstrap tests, canonical run persistence

---

### Task 1: Add failing provider and REPL streaming tests

**Files:**
- Modify: `crates/agent-runtime/src/provider.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`

- [ ] **Step 1: Write the failing tests**
  Add tests that prove:
  - `z.ai` streaming yields text deltas and a final response
  - `chat repl` renders streamed assistant text
  - `chat repl` keeps one final tool status line after approval and completion

- [ ] **Step 2: Run targeted tests to verify they fail**

  Run: `/home/admin/.cargo/bin/cargo test -p agent-runtime stream_ && /home/admin/.cargo/bin/cargo test -p agentd repl_stream`

  Expected: FAIL because provider streaming and REPL rendering are not implemented yet.

- [ ] **Step 3: Write minimal implementation to satisfy the first failures**
  Add typed streaming support for `z.ai` only and enough REPL rendering hooks to satisfy the tests.

- [ ] **Step 4: Run targeted tests to verify they pass**

  Run: `/home/admin/.cargo/bin/cargo test -p agent-runtime stream_ && /home/admin/.cargo/bin/cargo test -p agentd repl_stream`

  Expected: PASS

- [ ] **Step 5: Commit**

  ```bash
  git add crates/agent-runtime/src/provider.rs cmd/agentd/src/bootstrap.rs
  git commit -m "feat: add z.ai streaming events"
  ```

### Task 2: Wire streaming events through execution and REPL

**Files:**
- Modify: `cmd/agentd/src/execution.rs`
- Modify: `cmd/agentd/src/cli.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`

- [ ] **Step 1: Write the failing tests**
  Add tests that prove:
  - execution emits tool lifecycle updates through approval resume
  - REPL redraws a single tool-status line instead of appending a burst of tool log lines
  - non-streaming fallback still completes normally

- [ ] **Step 2: Run targeted tests to verify they fail**

  Run: `/home/admin/.cargo/bin/cargo test -p agentd repl_stream`

  Expected: FAIL because execution does not yet surface typed streaming/tool events.

- [ ] **Step 3: Write minimal implementation**
  Add an execution observer interface, route provider deltas and tool lifecycle transitions through it, and render them in `chat repl`.

- [ ] **Step 4: Run targeted tests to verify they pass**

  Run: `/home/admin/.cargo/bin/cargo test -p agentd repl_stream`

  Expected: PASS

- [ ] **Step 5: Commit**

  ```bash
  git add cmd/agentd/src/execution.rs cmd/agentd/src/cli.rs cmd/agentd/src/bootstrap.rs
  git commit -m "feat: stream chat repl status updates"
  ```

### Task 3: Document, verify, and smoke test the operator path

**Files:**
- Modify: `README.md`
- Modify: `cmd/agentd/src/bootstrap.rs`

- [ ] **Step 1: Extend operator docs**
  Document streamed text, the single updating tool-status line, and the approval behavior in `README.md`.

- [ ] **Step 2: Run full verification**

  Run:
  - `/home/admin/.cargo/bin/cargo fmt --check`
  - `/home/admin/.cargo/bin/cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `/home/admin/.cargo/bin/cargo test --workspace --all-features`

  Expected: PASS

- [ ] **Step 3: Run a live smoke against `z.ai`**

  Run a scripted `chat repl` turn against real `z.ai`, including a tool+approval round-trip when permissions force `ask`.

- [ ] **Step 4: Commit**

  ```bash
  git add README.md cmd/agentd/src/bootstrap.rs
  git commit -m "docs: cover streaming chat repl"
  ```
