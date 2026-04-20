# Canonical Planning Tool Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a persisted session plan plus typed `plan_read` and `plan_write`
tools on the canonical runtime path.

**Architecture:** Persist a canonical `PlanSnapshot` in `agent-persistence`,
surface it in `PromptAssembly` as a synthetic system message, and route typed
planning tools through the existing `ExecutionService` provider tool loop.

**Tech Stack:** Rust, `agent-runtime`, `agent-persistence`, `agentd`,
OpenAI-compatible tool calling.

---

## Task 1: Add runtime planning domain and prompt rendering (`teamD-ctx.4`)

**Files:**
- Modify: `crates/agent-runtime/src/plan.rs`
- Modify: `crates/agent-runtime/src/prompt.rs`
- Test: `crates/agent-runtime/src/plan.rs`
- Test: `crates/agent-runtime/src/prompt.rs`

- [ ] **Step 1: Write failing unit tests**

Add tests asserting:
- `PlanSnapshot` renders stable compact system text
- prompt assembly orders `session head -> plan -> compact summary -> transcript`
- empty plans are omitted from prompt assembly

- [ ] **Step 2: Run focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agent-runtime plan -- --nocapture
/home/admin/.cargo/bin/cargo test -p agent-runtime prompt -- --nocapture
```

- [ ] **Step 3: Implement minimal runtime types**

Add:
- `PlanItemStatus`
- `PlanItem`
- `PlanSnapshot`
- prompt assembly support for optional plan messages

- [ ] **Step 4: Run focused tests to verify GREEN**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agent-runtime plan -- --nocapture
/home/admin/.cargo/bin/cargo test -p agent-runtime prompt -- --nocapture
```

## Task 2: Add persisted plan storage (`teamD-ctx.4`)

**Files:**
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/repository.rs`
- Modify: `crates/agent-persistence/src/store.rs`
- Test: `crates/agent-persistence/src/records.rs`
- Test: `crates/agent-persistence/src/store.rs`

- [ ] **Step 1: Write failing persistence tests**

Add tests asserting:
- `PlanSnapshot <-> PlanRecord` round-trips
- store bootstraps `plans` table and round-trips plan records

- [ ] **Step 2: Run focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agent-persistence plan -- --nocapture
```

- [ ] **Step 3: Implement minimal persistence**

Add:
- `PlanRecord`
- `PlanRepository`
- `plans` table and schema validation
- store read/write methods

- [ ] **Step 4: Run focused tests to verify GREEN**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agent-persistence plan -- --nocapture
```

## Task 3: Add planning tools and permission behavior (`teamD-ctx.4`)

**Files:**
- Modify: `crates/agent-runtime/src/tool.rs`
- Modify: `crates/agent-runtime/src/permission.rs`
- Test: `crates/agent-runtime/src/tool.rs`
- Test: `crates/agent-runtime/src/permission.rs`

- [ ] **Step 1: Write failing tool and permission tests**

Add tests asserting:
- tool catalog exposes `plan` family, `plan_read`, `plan_write`
- `plan_write` survives `plan` permission mode
- tool schemas and summaries stay stable

- [ ] **Step 2: Run focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agent-runtime tool -- --nocapture
/home/admin/.cargo/bin/cargo test -p agent-runtime permission -- --nocapture
```

- [ ] **Step 3: Implement planning tool types**

Add:
- `ToolFamily::Planning`
- `ToolName::PlanRead`, `ToolName::PlanWrite`
- typed input/output/schema/summary/model-output handling
- permission-mode adjustment for planning tools

- [ ] **Step 4: Run focused tests to verify GREEN**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agent-runtime tool -- --nocapture
/home/admin/.cargo/bin/cargo test -p agent-runtime permission -- --nocapture
```

## Task 4: Route planning through canonical app/execution path (`teamD-ctx.4`)

**Files:**
- Modify: `cmd/agentd/src/bootstrap.rs`
- Modify: `cmd/agentd/src/execution.rs`
- Possibly create: `cmd/agentd/src/planning.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] **Step 1: Write failing app/execution tests**

Add tests asserting:
- app can read/write persisted plan snapshots
- chat prompt assembly includes the plan system message
- a provider-driven chat turn can use `plan_write` and `plan_read` in the same
  canonical tool loop

- [ ] **Step 2: Run focused tests to verify RED**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app plan -- --nocapture
```

- [ ] **Step 3: Implement canonical plan execution**

Add:
- app-layer plan accessors
- prompt assembly wiring for plan snapshots
- session-scoped plan tool execution inside `ExecutionService`

- [ ] **Step 4: Run focused tests to verify GREEN**

Run:
```bash
/home/admin/.cargo/bin/cargo test -p agentd --test bootstrap_app plan -- --nocapture
```

## Task 5: Verify and commit

- [ ] **Step 1: Run repo verification**

```bash
/home/admin/.cargo/bin/cargo fmt --all
/home/admin/.cargo/bin/cargo clippy --workspace --all-targets --all-features -- -D warnings
/home/admin/.cargo/bin/cargo test --workspace --all-features
```

- [ ] **Step 2: Commit**

```bash
git add crates/agent-runtime/src/plan.rs \
  crates/agent-runtime/src/prompt.rs \
  crates/agent-runtime/src/tool.rs \
  crates/agent-runtime/src/permission.rs \
  crates/agent-persistence/src/records.rs \
  crates/agent-persistence/src/repository.rs \
  crates/agent-persistence/src/store.rs \
  cmd/agentd/src/bootstrap.rs \
  cmd/agentd/src/execution.rs \
  cmd/agentd/tests/bootstrap_app.rs \
  docs/superpowers/specs/2026-04-20-canonical-planning-tool-surface-design.md \
  docs/superpowers/plans/2026-04-20-canonical-planning-tool-surface.md
git commit -m "feat: add canonical planning tool surface"
```
