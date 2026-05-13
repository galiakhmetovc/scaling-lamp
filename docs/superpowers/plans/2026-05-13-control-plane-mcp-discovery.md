# Control Plane MCP Discovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Расширить web control-plane для MCP: оператор должен видеть discovered resources/prompts и уметь читать resource/get prompt без обращения к модели.

**Architecture:** Источник правды — `SharedMcpRegistry`, который уже наполняется runtime discovery. Web endpoints только читают registry и вызывают существующий MCP worker.

---

### Task 1: Backend MCP Discovery API

**Files:**
- Modify: `cmd/agentd/src/bootstrap/mcp_ops.rs`
- Modify: `cmd/agentd/src/http/server/mcp.rs`
- Modify: `cmd/agentd/src/http/server.rs`
- Modify: `cmd/agentd/src/http/types.rs`

- [x] Add structured list views for discovered resources and prompts.
- [x] Add pagination/filtering using existing runtime MCP search limits.
- [x] Add `GET /v1/mcp/resources` and `POST /v1/mcp/resources/read`.
- [x] Add `GET /v1/mcp/prompts` and `POST /v1/mcp/prompts/get`.

### Task 2: Web MCP UI

**Files:**
- Create: `apps/web/src/features/tools/McpResourcesTable.tsx`
- Create: `apps/web/src/features/tools/McpPromptsTable.tsx`
- Modify: `apps/web/src/features/tools/ToolsScreen.tsx`
- Modify: `apps/web/src/api.ts`
- Modify: `apps/web/src/types.ts`

- [x] Add MCP resources/prompts metrics.
- [x] Add tabs for resources and prompts.
- [x] Read selected resource into details modal.
- [x] Get selected prompt into details modal.

### Task 3: Verification

- [x] `cargo fmt --all --check`
- [x] `CARGO_INCREMENTAL=0 cargo clippy -p agentd --lib -- -D warnings`
- [x] `CARGO_INCREMENTAL=0 cargo test -p agentd --lib mcp -- --nocapture`
- [x] `CARGO_INCREMENTAL=0 cargo build -p agentd`
- [x] `corepack pnpm --dir apps/web test`
- [x] `corepack pnpm --dir apps/web build`
