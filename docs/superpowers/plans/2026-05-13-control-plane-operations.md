# Control Plane Operations Dashboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Сделать отдельный web экран для ежедневной эксплуатации роя: активные runs, task registry, delivery routes, Telegram inputs и event bus summary.

**Architecture:** Первый срез read-only и полностью поверх `GET /v1/web/snapshot`. Никаких новых runtime paths.

---

### Task 1: Operations Model

**Files:**
- Create: `apps/web/src/features/operations/operationsModel.ts`
- Create: `apps/web/src/features/operations/operationsModel.test.ts`

- [x] Add active/failed status classifiers for operations.
- [x] Add summary aggregation from `WebSnapshot`.
- [x] Add tests for active/failed counts.

### Task 2: Operations Screen

**Files:**
- Create: `apps/web/src/features/operations/OperationsScreen.tsx`
- Modify: `apps/web/src/App.tsx`
- Modify: `apps/web/src/ui/navigation.ts`
- Modify: `apps/web/src/features/chat/chatCommands.ts`

- [x] Add navigation section `Operations`.
- [x] Add `/ops` chat command.
- [x] Show active runs/tasks and fallback recent rows when nothing is active.
- [x] Reuse existing route/event-bus panels instead of duplicating runtime logic.

### Task 3: Verification

- [x] `corepack pnpm --dir apps/web test`
- [x] `corepack pnpm --dir apps/web build`
