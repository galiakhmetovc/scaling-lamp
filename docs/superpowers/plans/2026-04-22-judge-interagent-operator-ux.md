# Judge And Inter-Agent Operator UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add direct operator controls for judge/inter-agent messaging and chain continuation in CLI/TUI while reusing the existing canonical runtime path.

**Architecture:** Extend the existing bootstrap/app layer with direct operator methods over `message_agent` and `grant_agent_chain_continuation`, then wire those methods through the existing CLI/TUI backend traits and browser/dialog UX. Render chain metadata from persisted canonical state in status/debug surfaces instead of introducing a second orchestration path.

**Tech Stack:** Rust, agentd bootstrap/app layer, TUI dialogs/browser flow, existing runtime inter-agent state

---

### Task 1: Add Canonical Operator Methods For Inter-Agent Control

**Files:**
- Modify: `cmd/agentd/src/bootstrap/execution_ops.rs`
- Modify: `cmd/agentd/src/bootstrap/context_ops.rs`
- Test: `cmd/agentd/tests/bootstrap_app/interagent.rs`

- [ ] Add failing app-level tests for direct operator send-to-agent and continuation-grant flows.
- [ ] Run targeted interagent tests and verify they fail for missing operator methods.
- [ ] Implement app-layer methods that send an inter-agent message and grant one additional hop using existing canonical state.
- [ ] Run targeted interagent tests and verify they pass.

### Task 2: Wire CLI Commands

**Files:**
- Modify: `cmd/agentd/src/cli/repl.rs`
- Modify: `cmd/agentd/src/http/client/sessions.rs`
- Test: `cmd/agentd/tests/bootstrap_app/repl.rs`

- [ ] Add failing CLI tests for `\\агент написать`, `\\судья`, and `\\цепочка продолжить`.
- [ ] Implement parsing and backend routing through the new app-layer methods.
- [ ] Verify targeted REPL tests pass.

### Task 3: Add TUI Dialogs And Actions

**Files:**
- Modify: `cmd/agentd/src/tui/backend.rs`
- Modify: `cmd/agentd/src/tui/app.rs`
- Modify: `cmd/agentd/src/tui/events.rs`
- Modify: `cmd/agentd/src/tui/screens/inspector.rs`
- Modify: `cmd/agentd/src/tui.rs`
- Modify: `cmd/agentd/src/tui/render.rs`
- Test: `cmd/agentd/tests/tui_app.rs`

- [ ] Add failing TUI tests for agent-message dialog, judge quick-send, and continuation-grant dialog/action.
- [ ] Extend dialog state and backend trait with the new operator actions.
- [ ] Wire key handling and submit flow through existing browser/dialog mechanics.
- [ ] Verify targeted TUI tests pass.

### Task 4: Enrich Status And System Rendering

**Files:**
- Modify: `cmd/agentd/src/bootstrap/session_ops.rs`
- Modify: `cmd/agentd/src/bootstrap/context_ops.rs`
- Test: `cmd/agentd/tests/session_context.rs`

- [ ] Add failing render tests for visible inter-agent chain metadata in status/system output.
- [ ] Implement compact chain-state rendering from canonical persisted state.
- [ ] Verify targeted render tests pass.

### Task 5: Verify End To End

**Files:**
- Modify as needed: touched files above

- [ ] Run `cargo fmt --all`
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] Run `cargo test --workspace --all-features`
- [ ] Run `cargo build -p agentd`
- [ ] Run `cargo build --release -p agentd`

