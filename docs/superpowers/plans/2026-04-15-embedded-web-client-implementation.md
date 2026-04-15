# Embedded Web Client Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the daemon Phase 1 embedded shell with a production embedded React/Vite web client that mirrors the TUI tabs over the existing daemon API.

**Architecture:** Keep the daemon as the only source of truth. Add a frontend source tree built by Vite into deterministic static assets under `internal/runtime/daemon/assets`, and consume the existing bootstrap and websocket command protocol from a typed browser client.

**Tech Stack:** React, TypeScript, Vite, embedded Go assets, existing daemon HTTP/WebSocket protocol

---

### Task 1: Frontend Build Pipeline

**Files:**
- Create: `web/package.json`
- Create: `web/tsconfig.json`
- Create: `web/vite.config.ts`
- Create: `web/index.html`
- Create: `web/src/main.tsx`
- Modify: `internal/runtime/daemon/assets/*` (generated production build output)
- Test: `npm --prefix web run build`

- [ ] Add a minimal React/Vite project with deterministic output names.
- [ ] Configure Vite to build into `internal/runtime/daemon/assets`.
- [ ] Run `npm --prefix web run build` and verify generated assets replace the current shell.

### Task 2: Web Client Transport Layer

**Files:**
- Create: `web/src/lib/client.ts`
- Create: `web/src/lib/types.ts`
- Test: `npm --prefix web run build`

- [ ] Implement typed bootstrap fetch.
- [ ] Implement typed websocket subscription and command request/response handling.
- [ ] Keep transport paths sourced from `/config.js`.

### Task 3: Sessions And Chat Tabs

**Files:**
- Create: `web/src/App.tsx`
- Create: `web/src/components/layout/*`
- Create: `web/src/components/chat/*`
- Test: `npm --prefix web run build`

- [ ] Render tab shell matching `Sessions / Chat / Plan / Tools / Settings`.
- [ ] Render session list from bootstrap and websocket-driven refresh.
- [ ] Render chat timeline, queued drafts, status bar, and `/btw` branch blocks from daemon data.
- [ ] Send chat and `/btw` through websocket commands.

### Task 4: Plan, Tools, And Settings Tabs

**Files:**
- Create: `web/src/components/plan/*`
- Create: `web/src/components/tools/*`
- Create: `web/src/components/settings/*`
- Test: `npm --prefix web run build`

- [ ] Render plan snapshot and form actions through daemon commands.
- [ ] Render approvals, running commands, and tool log from session snapshot plus live events.
- [ ] Render settings form/raw state from bootstrap and settings commands.

### Task 5: Verification And Documentation

**Files:**
- Modify: `docs/clean-room-daemon-web-ui.md`
- Modify: `README.md`
- Test: `go test ./internal/runtime/daemon ./cmd/agent -count=1`
- Test: `go test ./internal/... ./cmd/agent -count=1`
- Test: `go build ./cmd/agent`

- [ ] Document web build/run workflow and daemon-first usage.
- [ ] Re-run frontend build and Go verification.
- [ ] Commit and push the completed slice.
