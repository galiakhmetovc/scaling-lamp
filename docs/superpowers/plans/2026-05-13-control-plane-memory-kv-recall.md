# Control Plane Memory KV Recall Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Добавить в native web console операторский слой для памяти: Mem0 semantic memory, scoped KV и preview автоматического Memory Recall.

**Architecture:** Web UI не хранит память сам. Все операции идут через thin `agentd` HTTP endpoints, которые вызывают те же `ExecutionService` методы, что и runtime tools `memory_*` и `kv_*`.

**Tech Stack:** Rust `agentd`, React + MUI, TypeScript `node --test`.

---

### Task 1: Backend Structured Memory API

**Files:**
- Create: `cmd/agentd/src/bootstrap/memory_ops.rs`
- Create: `cmd/agentd/src/http/server/memory.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`
- Modify: `cmd/agentd/src/execution.rs`
- Modify: `cmd/agentd/src/execution/memory_recall.rs`
- Modify: `cmd/agentd/src/http/server.rs`
- Modify: `cmd/agentd/src/http/types.rs`

- [x] Expose semantic memory search/list/update/delete through `App`.
- [x] Expose KV list/put/delete through `App`.
- [x] Add recall preview wrapper that resolves either explicit query or latest user message.
- [x] Add structured HTTP responses instead of text-only `/memory` render.

### Task 2: Web Memory Screen

**Files:**
- Create: `apps/web/src/features/memory/MemoryScreen.tsx`
- Create: `apps/web/src/features/memory/memoryModel.ts`
- Create: `apps/web/src/features/memory/memoryModel.test.ts`
- Modify: `apps/web/src/api.ts`
- Modify: `apps/web/src/types.ts`
- Modify: `apps/web/src/App.tsx`
- Modify: `apps/web/src/ui/navigation.ts`

- [x] Add navigation section `Память`.
- [x] Show selected session scope context.
- [x] Add Mem0 list/search/update/delete UI.
- [x] Add scoped KV browse/put/delete UI.
- [x] Add recall preview table and raw JSON preview.
- [x] Add boundary explainer for Mem0, KV and SilverBullet.
- [x] Add `/memory` chat command to open the screen.

### Task 3: Verification

- [x] `cargo fmt --all --check`
- [x] `CARGO_INCREMENTAL=0 cargo clippy -p agentd --lib -- -D warnings`
- [x] `CARGO_INCREMENTAL=0 cargo test -p agentd --lib -- --nocapture`
- [x] `CARGO_INCREMENTAL=0 cargo build -p agentd`
- [x] `corepack pnpm --dir apps/web test`
- [x] `corepack pnpm --dir apps/web build`
