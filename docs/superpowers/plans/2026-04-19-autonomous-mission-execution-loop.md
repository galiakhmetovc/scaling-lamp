# Autonomous Mission Execution Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first real autonomous execution path that can turn persisted mission state into queued jobs, provider-backed runs, approval pauses, verification evidence, and mission completion.

**Architecture:** Keep the current modular monolith. Add a thin orchestration path on top of existing persistence, scheduler, run engine, provider, tool, and verification modules instead of introducing a second runtime. Persist every meaningful transition through the canonical run snapshot and mission/job records.

**Tech Stack:** Rust, rusqlite, reqwest, serde, local SQLite/file-backed persistence, `bd` for issue tracking.

---

### Task 1: Persistence Read Layer For Execution Ticks

**Files:**
- Modify: `crates/agent-persistence/src/repository.rs`
- Modify: `crates/agent-persistence/src/store.rs`
- Modify: `crates/agent-persistence/src/lib.rs`
- Test: `crates/agent-persistence/src/store.rs`

- [ ] Add failing tests for listing sessions, missions, jobs, and deterministic execution-state reads.
- [ ] Run the targeted persistence tests and confirm they fail for missing APIs.
- [ ] Implement minimal list/query repository methods plus one aggregate execution-state read helper.
- [ ] Re-run targeted tests, then full persistence/workspace tests.
- [ ] Commit the task.

### Task 2: Supervisor Tick Service

**Files:**
- Create: `cmd/agentd/src/execution.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`
- Test: `cmd/agentd/src/bootstrap.rs`

- [ ] Add failing tests for one persisted supervisor tick that queues mission-turn jobs.
- [ ] Implement a thin execution service that loads execution state, runs `SupervisorLoop`, and persists resulting job/mission changes.
- [ ] Re-run targeted tests, then full workspace tests.
- [ ] Commit the task.

### Task 3: Provider-Backed Mission Turn Execution

**Files:**
- Modify: `cmd/agentd/src/execution.rs`
- Modify: `cmd/agentd/src/cli.rs`
- Modify: `crates/agent-runtime/src/provider.rs`
- Modify: `crates/agent-persistence/src/store.rs`
- Test: `cmd/agentd/src/bootstrap.rs`

- [ ] Add failing tests for a queued mission-turn job producing a persisted run and transcript output.
- [ ] Implement run creation, provider call wiring, transcript persistence, and job/run status updates.
- [ ] Re-run targeted tests, then full workspace tests.
- [ ] Commit the task.

### Task 4: Tool, Approval, And Verification Wiring

**Files:**
- Modify: `cmd/agentd/src/execution.rs`
- Modify: `crates/agent-runtime/src/tool.rs`
- Modify: `crates/agent-runtime/src/run.rs`
- Test: `cmd/agentd/src/bootstrap.rs`

- [ ] Add failing tests for approval pauses, resume behavior, and verification evidence through the mission execution loop.
- [ ] Implement typed tool dispatch integration, approval waits, resume path, and evidence recording.
- [ ] Re-run targeted tests, then full workspace tests.
- [ ] Commit the task.

### Task 5: Operator Commands And End-To-End Smoke

**Files:**
- Modify: `cmd/agentd/src/cli.rs`
- Modify: `README.md`
- Test: `cmd/agentd/src/bootstrap.rs`

- [ ] Add failing tests for operator-driven mission tick/execution commands.
- [ ] Implement CLI entrypoints for one mission tick and one mission-turn execution pass.
- [ ] Add operator-facing smoke docs for the full path.
- [ ] Re-run targeted tests, full workspace tests, and live smoke where applicable.
- [ ] Commit the task.
