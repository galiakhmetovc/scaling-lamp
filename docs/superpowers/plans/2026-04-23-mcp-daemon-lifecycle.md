# MCP Daemon Lifecycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add daemon-managed lifecycle and persisted configuration for `stdio` MCP connectors without introducing a second runtime path.

**Architecture:** Persist connector definitions in `agent-persistence`, keep live runtime status in a shared daemon registry, and let the existing background worker supervise enabled connectors. Use the official `rmcp` Rust SDK as the client transport/protocol layer for child-process `stdio` connectors.

**Tech Stack:** Rust, rusqlite, tiny_http, tokio, rmcp

---

### Task 1: Persist MCP Connector Config

**Files:**
- Modify: `crates/agent-runtime/src/lib.rs`
- Create: `crates/agent-runtime/src/mcp.rs`
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/repository.rs`
- Modify: `crates/agent-persistence/src/store/schema.rs`
- Create: `crates/agent-persistence/src/store/mcp_repos.rs`
- Modify: `crates/agent-persistence/src/store.rs`
- Modify: `crates/agent-persistence/src/lib.rs`
- Test: `crates/agent-persistence/src/store/tests.rs`

- [ ] Write failing persistence round-trip test for MCP connector config.
- [ ] Run the targeted test and verify it fails for missing MCP repository/table support.
- [ ] Add runtime domain types and persistence record/repository support.
- [ ] Run the targeted test and verify it passes.

### Task 2: Add Daemon Runtime Registry

**Files:**
- Create: `cmd/agentd/src/mcp.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`
- Modify: `cmd/agentd/src/execution.rs`
- Test: `cmd/agentd/tests/bootstrap_app/mcp.rs`

- [ ] Write failing app-level test for create/list/update/delete MCP connector config with runtime status defaults.
- [ ] Run the targeted test and verify it fails.
- [ ] Implement shared MCP registry and app lifecycle methods for persisted connectors plus status reads.
- [ ] Run the targeted test and verify it passes.

### Task 3: Supervise `stdio` Connectors

**Files:**
- Modify: `cmd/agentd/Cargo.toml`
- Modify: `cmd/agentd/src/mcp.rs`
- Modify: `cmd/agentd/src/execution/background.rs`
- Test: `cmd/agentd/tests/bootstrap_app/mcp.rs`

- [ ] Write failing test for background worker starting enabled connectors, stopping disabled connectors, and restarting failed connectors.
- [ ] Run the targeted test and verify it fails.
- [ ] Add `tokio` and `rmcp`, implement `stdio` connector workers, and wire supervision into the existing background worker tick.
- [ ] Run the targeted test and verify it passes.

### Task 4: Expose HTTP Lifecycle

**Files:**
- Modify: `cmd/agentd/src/http/types.rs`
- Modify: `cmd/agentd/src/http/server.rs`
- Create: `cmd/agentd/src/http/server/mcp.rs`
- Modify: `cmd/agentd/src/http/client/sessions.rs`
- Modify: `cmd/agentd/tests/daemon_http.rs`

- [ ] Write failing daemon HTTP test for MCP connector lifecycle endpoints.
- [ ] Run the targeted test and verify it fails.
- [ ] Implement HTTP request/response types, server routes, and thin client helpers.
- [ ] Run the targeted test and verify it passes.

### Task 5: Verify Whole Slice

**Files:**
- Modify only if implementation clarified an invariant:
  - `docs/superpowers/specs/2026-04-23-mcp-daemon-lifecycle-design.md`
  - `docs/superpowers/plans/2026-04-23-mcp-daemon-lifecycle.md`

- [ ] Run `cargo fmt --all`
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] Run `cargo test --workspace --all-features`
- [ ] Run `cargo build -p agentd`
- [ ] Run `cargo build --release -p agentd`
