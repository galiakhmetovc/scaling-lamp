# Control Plane Skills MCP Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Сделать операторский экран Skills/Tools/MCP поверх реальных данных teamD runtime.

**Architecture:** `agentd` остаётся canonical runtime. Web UI читает catalog tools, MCP connectors, session skills и agent profile files через существующий `/api/agentd` proxy, без отдельного runtime path.

**Tech Stack:** Rust `agentd`, React + MUI, TypeScript `node --test`.

---

### Task 1: Backend Tool Catalog

**Files:**
- Create: `cmd/agentd/src/bootstrap/tool_ops.rs`
- Create: `cmd/agentd/src/http/server/tools.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`
- Modify: `cmd/agentd/src/http/server.rs`
- Test: `cmd/agentd/tests/bootstrap_app/mcp.rs`

- [x] Write failing test that `App::tool_catalog()` returns built-in tools and discovered MCP tools.
- [x] Implement `ToolCatalogView` and `ToolCatalogItemView`.
- [x] Add `GET /v1/tools/catalog`.

### Task 2: Web Operator Screen

**Files:**
- Create: `apps/web/src/features/tools/toolCatalog.ts`
- Create: `apps/web/src/features/tools/toolCatalog.test.ts`
- Create: `apps/web/src/features/tools/ToolsScreen.tsx`
- Modify: `apps/web/src/api.ts`
- Modify: `apps/web/src/types.ts`
- Modify: `apps/web/src/App.tsx`

- [x] Write failing test for catalog family grouping and stats.
- [x] Implement grouping helpers.
- [x] Replace raw recent calls table section with tabs: catalog, recent calls, MCP connectors.
- [x] Show allowed/blocked status for selected session agent when available.

### Task 3: Verification

- [x] `corepack pnpm --dir apps/web test`
- [x] `corepack pnpm --dir apps/web build`
- [x] `cargo fmt --all`
- [x] `cargo test -p agentd --test bootstrap_app mcp`
- [x] `cargo build -p agentd`
- [x] Commit and push.
