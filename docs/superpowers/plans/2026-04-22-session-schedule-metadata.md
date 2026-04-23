# Session And Schedule Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose agent and schedule metadata consistently in session lists, chat headers, active run status, and daemon-backed session summaries.

**Architecture:** Extend the canonical session summary/session head data models with a small optional schedule metadata block, populate it once in bootstrap summary builders, and reuse that data in TUI, CLI, and HTTP renderers. Keep detailed schedule views unchanged.

**Tech Stack:** Rust, agentd bootstrap/session rendering, daemon HTTP types, ratatui rendering, existing workspace test suite.

---

### Task 1: Extend canonical summary models

**Files:**
- Modify: `cmd/agentd/src/bootstrap.rs`
- Modify: `crates/agent-runtime/src/prompt.rs`

- [ ] Add a nested session schedule summary struct to `SessionSummary`
- [ ] Add agent identity fields and optional schedule summary fields to `SessionHead`
- [ ] Keep serde derives and existing compatibility fields intact

### Task 2: Populate metadata once in summary builders

**Files:**
- Modify: `cmd/agentd/src/bootstrap.rs`
- Modify: `cmd/agentd/src/prompting.rs`

- [ ] Write failing tests for summary building with schedule-backed sessions
- [ ] Resolve persisted schedule state from `scheduled_by`
- [ ] Populate the new `SessionSummary.schedule` block
- [ ] Populate `SessionHead` agent/schedule metadata from the same canonical state

### Task 3: Expose metadata through daemon HTTP

**Files:**
- Modify: `cmd/agentd/src/http/types.rs`
- Modify: `cmd/agentd/src/http/server/sessions.rs`
- Modify: `cmd/agentd/src/http/client/sessions.rs`

- [ ] Extend session summary response types with schedule metadata
- [ ] Keep conversion code thin over `SessionSummary`
- [ ] Add/adjust tests for response round-trip if needed

### Task 4: Render metadata in operator surfaces

**Files:**
- Modify: `cmd/agentd/src/bootstrap/session_ops.rs`
- Modify: `cmd/agentd/src/tui/render.rs`

- [ ] Add failing tests for `render_active_run`
- [ ] Add a compact session/schedule metadata block before run step/process detail
- [ ] Update TUI session list rendering to show explicit agent identity and compact schedule capsule
- [ ] Update chat header rendering to show a dedicated metadata line

### Task 5: Verify and polish fixtures

**Files:**
- Modify: `cmd/agentd/tests/session_context.rs`
- Modify: `cmd/agentd/tests/tui_app.rs`
- Modify: any touched fixtures constructing `SessionSummary`

- [ ] Update existing fixtures to include new optional fields
- [ ] Run targeted tests for session context and TUI rendering
- [ ] Run full verification suite
