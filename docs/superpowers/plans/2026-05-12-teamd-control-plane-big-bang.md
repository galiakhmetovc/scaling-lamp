# teamD Control Plane Big-Bang Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Import Hermes Workspace as a full React/Node control-plane app and adapt its first runtime boundary to `agentd`.

**Architecture:** Add a standalone app under `apps/control-plane`. Keep the imported Hermes UI and server structure, then add a teamD adapter module that proxies runtime status to `agentd`. `agentd` remains the only runtime for sessions, prompts, tools, mesh, and persistent data.

**Tech Stack:** React 19, TanStack Start/Router, Vite, Node, TypeScript, pnpm, existing Rust `agentd` HTTP API.

---

### Task 1: Import Hermes Workspace

**Files:**
- Create: `apps/control-plane/**`
- Create: `apps/control-plane/TEAMD_ADAPTATION.md`
- Create: `docs/current/web-control-plane.md`

- [ ] Copy the full `hermes-workspace` tree into `apps/control-plane`.
- [ ] Preserve `LICENSE` and add teamD adaptation notes.
- [ ] Remove nested `.git` if present.
- [ ] Keep package lock and project files intact.

### Task 2: Add teamD Adapter Boundary

**Files:**
- Create: `apps/control-plane/src/server/teamd-agentd-client.ts`
- Create: `apps/control-plane/src/routes/api/teamd-status.ts`
- Modify: `apps/control-plane/src/routes/api/ping.ts`

- [ ] Add `TEAMD_AGENTD_BASE_URL`, defaulting to `http://127.0.0.1:5140`.
- [ ] Implement typed fetch helpers for `/v1/status` and `/v1/web/snapshot`.
- [ ] Add `/api/teamd-status` for the web UI and smoke tests.
- [ ] Keep this as adapter/proxy only; do not read Postgres directly.

### Task 3: Prove Build And Runtime Smoke

**Files:**
- Modify as needed under `apps/control-plane`.

- [ ] Run package install in `apps/control-plane`.
- [ ] Run TypeScript/build checks available in the imported app.
- [ ] Fix import/build errors caused by path assumptions.
- [ ] Smoke `GET /api/ping` and `GET /api/teamd-status` against a running `agentd` when available.

### Task 4: Track Follow-Up Module Adaptation

**Files:**
- Update: `.beads` via `bd`.
- Update: `docs/current/web-control-plane.md`

- [ ] Create follow-up beads for chat/sessions, agents, files/artifacts, terminal, memory/KV/SilverBullet, skills, MCP/tools, schedules/jobs, swarm/mesh, settings/auth.
- [ ] Mark which Hermes endpoints are retained, adapted, or pending.

### Task 5: Verify And Commit

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo build -p agentd`.
- [ ] Run available control-plane build checks.
- [ ] Commit import/adaptation as one explicit big-bang commit.

