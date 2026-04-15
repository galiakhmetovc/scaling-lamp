# Web Plan Tools Settings Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the embedded web `Plan`, `Tools`, and `Settings` panes to parity with the daemon-backed TUI operator workflow.

**Architecture:** Split the remaining web panes out of `App.tsx` into dedicated components plus pure view-model helpers. Keep daemon bootstrap and websocket command handling in `App.tsx`, while plan/tools/settings rendering, markdown presentation, and revision-conflict state mapping move into testable frontend modules over the existing daemon API.

**Tech Stack:** React 18, TypeScript, Vite, Vitest, React Markdown, Go daemon API.

---

### Task 1: Extract plan pane model and markdown-aware renderer

**Files:**
- Create: `web/src/plan/model.ts`
- Create: `web/src/plan/PlanPane.tsx`
- Create: `web/src/plan/model.test.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/styles.css`

- [ ] **Step 1: Write the failing plan tests**

Cover:
- selected task details derived from `plan.tasks`
- computed plan notes preview
- markdown rendering for plan goal, task descriptions, and notes

- [ ] **Step 2: Run tests to verify RED**

Run:
```bash
cd web && npm test -- --run
```

Expected: FAIL because the plan model/component do not exist yet.

- [ ] **Step 3: Implement minimal plan model and pane**

Create a focused plan pane that:
- renders goal/details with markdown
- exposes selected-task details
- keeps existing create/add/set-status/add-note actions

- [ ] **Step 4: Re-run plan tests**

Run:
```bash
cd web && npm test -- --run
```

Expected: PASS.

### Task 2: Extract tools pane model and richer detail rendering

**Files:**
- Create: `web/src/tools/model.ts`
- Create: `web/src/tools/ToolsPane.tsx`
- Create: `web/src/tools/model.test.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/styles.css`

- [ ] **Step 1: Write the failing tools tests**

Cover:
- approval cards
- running command detail summaries
- delegate summaries
- reverse-ordered tool log mapping

- [ ] **Step 2: Run tests to verify RED**

Run:
```bash
cd web && npm test -- --run
```

Expected: FAIL because the tools model/component do not exist yet.

- [ ] **Step 3: Implement minimal tools model and pane**

Create a focused tools pane that:
- keeps approve/deny/kill actions
- renders richer detail blocks without inventing new server state

- [ ] **Step 4: Re-run tools tests**

Run:
```bash
cd web && npm test -- --run
```

Expected: PASS.

### Task 3: Extract settings pane model and revision-conflict UX

**Files:**
- Create: `web/src/settings/model.ts`
- Create: `web/src/settings/SettingsPane.tsx`
- Create: `web/src/settings/model.test.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/lib/types.ts`
- Modify: `web/src/styles.css`

- [ ] **Step 1: Write the failing settings tests**

Cover:
- dirty-state detection for form and raw file edits
- conflict-state mapping from daemon `command_failed` errors containing `revision conflict`
- raw file selection and revision display

- [ ] **Step 2: Run tests to verify RED**

Run:
```bash
cd web && npm test -- --run
```

Expected: FAIL because the settings model/component do not exist yet.

- [ ] **Step 3: Implement minimal settings model and pane**

Create a settings pane that:
- shows dirty/clean state
- surfaces revision conflict errors without losing local draft state
- preserves existing form/raw apply actions

- [ ] **Step 4: Re-run settings tests**

Run:
```bash
cd web && npm test -- --run
```

Expected: PASS.

### Task 4: Refactor App orchestration and verify end-to-end

**Files:**
- Modify: `web/src/App.tsx`
- Modify: `docs/clean-room-daemon-web-ui.md`

- [ ] **Step 1: Refactor App to orchestrate only**

Use the new pane components and helper models, keeping:
- daemon connect/bootstrap
- websocket envelope dispatch
- command invocation

- [ ] **Step 2: Run frontend test suite**

Run:
```bash
cd web && npm test -- --run
```

Expected: PASS.

- [ ] **Step 3: Build embedded web assets**

Run:
```bash
cd web && npm run build
```

Expected: PASS.

- [ ] **Step 4: Run repo verification**

Run:
```bash
TMPDIR=$PWD/.tmp-goexec GOCACHE=$PWD/.tmp-goexec/gocache go test ./internal/... ./cmd/agent -count=1
TMPDIR=$PWD/.tmp-goexec GOCACHE=$PWD/.tmp-goexec/gocache go build ./cmd/agent
```

Expected: PASS.
