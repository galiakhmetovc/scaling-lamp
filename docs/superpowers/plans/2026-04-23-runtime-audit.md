# Runtime Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Audit the operator-critical `agentd` runtime paths, reproduce real defects, fix them with targeted tests, and remove unnecessarily heavy or fragile flows without introducing a second runtime path.

**Architecture:** Work in phases from the outer operator surface inward: first session/TUI/daemon flows, then persistence and recovery paths, then provider/runtime execution, then scheduler/interagent/MCP/update surfaces. Each phase must produce reproducible failing tests, minimal fixes, and full verification before moving deeper.

**Tech Stack:** Rust, `agentd`, `agent-runtime`, `agent-persistence`, `tiny_http`, `reqwest`, `rusqlite`, TUI/CLI daemon-backed operator flows.

---

### Task 1: Audit Session/TUI/Daemon Critical Paths

**Files:**
- Modify: `cmd/agentd/src/bootstrap.rs`
- Modify: `cmd/agentd/src/bootstrap/session_ops.rs`
- Modify: `cmd/agentd/src/http/server/sessions.rs`
- Modify: `cmd/agentd/src/tui.rs`
- Modify: `crates/agent-persistence/src/repository.rs`
- Modify: `crates/agent-persistence/src/store/session_mission.rs`
- Test: `cmd/agentd/tests/bootstrap_app/core.rs`
- Test: `cmd/agentd/tests/daemon_tui.rs`

- [ ] Write failing tests for session open/create/delete behavior when unrelated or stale session data exists.
- [ ] Write failing tests for heavy summary paths that should not require full transcript payload hydration.
- [ ] Refactor single-session summary and list-summary paths to use lightweight aggregation only.
- [ ] Verify TUI startup, open, create, delete, clear, and rename flows do not depend on heavyweight summary assembly.
- [ ] Run targeted tests for bootstrap, daemon/TUI, and session-context behavior.

### Task 2: Audit Persistence and Recovery Paths

**Files:**
- Modify: `crates/agent-persistence/src/store.rs`
- Modify: `crates/agent-persistence/src/store/session_mission.rs`
- Modify: `crates/agent-persistence/src/store/context_repos.rs`
- Modify: `cmd/agentd/src/execution/memory.rs`
- Modify: `cmd/agentd/src/execution/provider_loop.rs`
- Test: `crates/agent-persistence/src/store/tests.rs`
- Test: `cmd/agentd/tests/bootstrap_app/context.rs`

- [ ] Audit stale payload, orphan payload, backup restore, and archive bundle recovery behavior.
- [ ] Add failing tests for partial payload loss and mixed healthy/stale session state.
- [ ] Fix only the confirmed recovery gaps in storage and offload/archive paths.
- [ ] Verify store open/reconcile flows do not make healthy sessions unusable because of unrelated stale payloads.

### Task 3: Audit Runtime Provider and Execution Paths

**Files:**
- Modify: `cmd/agentd/src/execution/provider_loop.rs`
- Modify: `crates/agent-runtime/src/provider.rs`
- Modify: `crates/agent-runtime/src/tool.rs`
- Modify: `cmd/agentd/src/http/client/internal.rs`
- Test: `cmd/agentd/tests/bootstrap_app/chat.rs`
- Test: `crates/agent-runtime/tests/provider_contract.rs`

- [ ] Audit retry, timeout, empty terminal response, repeated tool polling, and long-running process paths.
- [ ] Add failing tests for any reproduced bad operator experience before changing code.
- [ ] Remove unnecessary blocking or overly short timeout behavior where the current path is clearly wrong.
- [ ] Verify provider and exec behavior with full workspace tests.

### Task 4: Audit Scheduler, Interagent, MCP, and Background Paths

**Files:**
- Modify: `cmd/agentd/src/execution/background.rs`
- Modify: `cmd/agentd/src/execution/delegate_jobs.rs`
- Modify: `cmd/agentd/src/mcp.rs`
- Modify: `cmd/agentd/src/bootstrap/mcp_ops.rs`
- Test: `cmd/agentd/tests/bootstrap_app/background.rs`
- Test: `cmd/agentd/tests/bootstrap_app/interagent.rs`
- Test: `cmd/agentd/tests/bootstrap_app/chat.rs`

- [ ] Inspect due schedule delivery, one-shot continuation, parent/child wakeups, MCP connector lifecycle, and restart handling.
- [ ] Add failing tests only for reproduced high-signal defects.
- [ ] Apply targeted fixes that preserve the canonical runtime path and daemon-backed supervision.
- [ ] Re-run subsystem tests before moving on.

### Task 5: Audit Update and Release-Safety Paths

**Files:**
- Modify: `cmd/agentd/src/about.rs`
- Modify: `cmd/agentd/src/cli.rs`
- Modify: `cmd/agentd/src/cli/parse.rs`
- Modify: `cmd/agentd/src/cli/process.rs`
- Modify: `cmd/agentd/src/http/server/status.rs`
- Test: `cmd/agentd/src/about.rs`
- Test: `cmd/agentd/src/cli/tests.rs`

- [ ] Audit version/update CLI entrypoints, daemon interaction, and staged binary replacement behavior.
- [ ] Add failing tests for any release regression found during operator use.
- [ ] Fix only verified regressions and keep update flow on the canonical app/runtime path.

### Task 6: Verification and Patch Release

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Optional docs: `COMPARISON.md`, relevant runtime audit notes if behavior changed materially

- [ ] Run `cargo fmt --all`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --workspace --all-features`.
- [ ] Run `cargo build -p agentd`.
- [ ] Run `cargo build --release -p agentd`.
- [ ] Cut a patch release only after the current audit slice is verified end-to-end.
