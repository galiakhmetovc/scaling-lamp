# MCP Runtime Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose live MCP tools, resources, and prompts through the canonical provider/runtime tool path without adding a second chat or prompt assembly path.

**Architecture:** Extend the daemon MCP registry to keep discovery snapshots for running stdio connectors, merge discovered MCP tools into the normal provider tool list with deterministic safe names, and add four bounded built-in MCP retrieval tools for resources/prompts. Keep execution inside the existing provider loop and execution service.

**Tech Stack:** Rust, rmcp, serde_json, tiny_http, rusqlite

---

### Task 1: Add Canonical MCP Tool Types

**Files:**
- Modify: `crates/agent-runtime/src/tool.rs`
- Modify: `crates/agent-runtime/src/tool/tests.rs`
- Modify: `crates/agent-runtime/src/agent.rs`
- Modify: `cmd/agentd/src/agents.rs`

- [ ] Write failing tests for MCP utility tool parsing and agent allowlist support for dynamic MCP tools.
- [ ] Run the targeted tests and verify they fail.
- [ ] Add MCP tool family/types:
  - built-in utilities for resource/prompt search/read
  - generic internal dynamic MCP call shape
  - outputs and summaries/model-output support
  - allowlist support for MCP dynamic capability
- [ ] Run the targeted tests and verify they pass.

### Task 2: Extend the MCP Registry with Discovery and Live Calls

**Files:**
- Modify: `cmd/agentd/src/mcp.rs`
- Modify: `cmd/agentd/tests/bootstrap_app/mcp.rs`

- [ ] Write failing tests for MCP registry discovery snapshots and mock MCP call/read/get behavior.
- [ ] Run the targeted tests and verify they fail.
- [ ] Extend the live MCP registry to keep:
  - capability-aware discovery snapshots
  - live client handles for running connectors
  - deterministic safe-name mapping for dynamic MCP tools
- [ ] Add registry operations for:
  - listing discovered tools/resources/prompts
  - invoking one dynamic MCP tool
  - reading one MCP resource
  - fetching one MCP prompt
- [ ] Run the targeted tests and verify they pass.

### Task 3: Merge Dynamic MCP Tools into the Provider Surface

**Files:**
- Modify: `cmd/agentd/src/execution/provider_loop.rs`
- Test: `cmd/agentd/tests/bootstrap_app/chat.rs`

- [ ] Write failing tests for deterministic MCP tool exposure in provider requests and capability-aware MCP resource/prompt utilities.
- [ ] Run the targeted tests and verify they fail.
- [ ] Replace the old static-only tool assembly with a combined provider-tool surface:
  - built-in canonical tools
  - dynamic MCP tools from running connectors
- [ ] Keep artifact gating and agent allowlist behavior intact.
- [ ] Run the targeted tests and verify they pass.

### Task 4: Execute MCP Tools Through the Existing Provider Loop

**Files:**
- Modify: `cmd/agentd/src/execution/provider_loop.rs`
- Modify: `cmd/agentd/src/execution/mcp.rs`
- Test: `cmd/agentd/tests/bootstrap_app/chat.rs`

- [ ] Write failing tests for:
  - executing one dynamic MCP tool call from a provider response
  - executing `mcp_search_resources`
  - executing `mcp_read_resource`
  - executing `mcp_search_prompts`
  - executing `mcp_get_prompt`
- [ ] Run the targeted tests and verify they fail.
- [ ] Implement MCP execution using the same provider-loop flow as built-ins:
  - resolve dynamic MCP tool names through the registry snapshot
  - derive permission policy from MCP annotations
  - emit the same tool transcript/status events
  - return outputs through the normal provider continuation path
- [ ] Run the targeted tests and verify they pass.

### Task 5: Verify Whole Slice and Sync Beads

**Files:**
- Modify only if the implementation clarified an invariant:
  - `docs/superpowers/specs/2026-04-23-mcp-runtime-surface-design.md`
  - `docs/superpowers/plans/2026-04-23-mcp-runtime-surface.md`

- [ ] Run `cargo fmt --all`
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] Run `cargo test --workspace --all-features`
- [ ] Run `cargo build -p agentd`
- [ ] Run `cargo build --release -p agentd`
- [ ] Mark `teamD-mcp.2` complete in `beads`
