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

- [ ] Write failing test that `App::tool_catalog()` returns built-in tools and discovered MCP tools.
- [ ] Implement `ToolCatalogView` and `ToolCatalogItemView`.
- [ ] Add `GET /v1/tools/catalog`.

### Task 2: Web Operator Screen

**Files:**
- Create: `apps/web/src/features/tools/toolCatalog.ts`
- Create: `apps/web/src/features/tools/toolCatalog.test.ts`
- Create: `apps/web/src/features/tools/ToolsScreen.tsx`
- Modify: `apps/web/src/api.ts`
- Modify: `apps/web/src/types.ts`
- Modify: `apps/web/src/App.tsx`

- [ ] Write failing test for catalog family grouping and stats.
- [ ] Implement grouping helpers.
- [ ] Replace raw recent calls table section with tabs: catalog, recent calls, MCP connectors.
- [ ] Show allowed/blocked status for selected session agent when available.

### Task 3: Verification

- [ ] `corepack pnpm --dir apps/web test`
- [ ] `corepack pnpm --dir apps/web build`
- [ ] `cargo fmt --all`
- [ ] `cargo test -p agentd --test bootstrap_app mcp`
- [ ] `cargo build -p agentd`
- [ ] Commit and push.
