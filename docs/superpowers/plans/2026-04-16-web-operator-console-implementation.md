# Web Operator Console Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rework the daemon-backed web UI into a clearer operator console with stronger layout hierarchy, a better `Sessions/Chat` experience, and consistent styling for `Plan`, `Tools`, and `Settings`.

**Architecture:** Keep the current daemon bootstrap/websocket protocol untouched unless a real gap is discovered. Implement the redesign through modular React view components and a revised shared stylesheet/token system, with layout decisions validated by frontend tests and a final live daemon smoke.

**Tech Stack:** React, TypeScript, Vite, Vitest, embedded Go-served static assets.

---

## File Structure

- Modify: `web/src/App.tsx` — reduce shell responsibility to header, tabs, and pane routing.
- Modify: `web/src/styles.css` — replace flat panel styling with tiered surface system and responsive shell rules.
- Modify: `web/src/layout.ts` — centralize tab/layout helpers and shell-level display rules.
- Modify: `web/src/chat/ChatPane.tsx` — recompose chat workspace under new layout.
- Modify: `web/src/chat/model.ts` — keep chat status view aligned with new shell metadata.
- Modify: `web/src/chat/model.test.tsx` — layout/status regression coverage.
- Modify: `web/src/plan/PlanPane.tsx` — align plan presentation to new surface hierarchy.
- Modify: `web/src/tools/ToolsPane.tsx` — align tools/operator presentation to new surface hierarchy.
- Modify: `web/src/settings/SettingsPane.tsx` — align settings editing surfaces and conflict presentation.
- Optionally modify: `web/src/lib/types.ts` only if view-layer typing needs clearer shell metadata.
- Modify: `docs/clean-room-daemon-web-ui.md` — document the new operator-console structure.

### Task 1: Lock the shell hierarchy with failing tests

**Files:**
- Modify: `web/src/chat/model.test.tsx`
- Modify: `web/src/layout.ts`
- Modify: `web/src/App.tsx`

- [ ] **Step 1: Write the failing tests**

Add tests that assert:
- `Chat` does not render the session rail.
- the shell exposes a distinct control-header structure.
- layout helpers express the intended tab behavior.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web && npm test -- --run`
Expected: FAIL on the new shell/layout expectations.

- [ ] **Step 3: Write minimal implementation**

Implement the shell structure in `App.tsx` and helpers in `layout.ts` so the tests pass without yet finishing the full visual polish.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web && npm test -- --run`
Expected: PASS for updated shell/layout tests.

### Task 2: Rebuild the shared visual system

**Files:**
- Modify: `web/src/styles.css`

- [ ] **Step 1: Add failing coverage via existing render tests**

Extend existing render assertions where helpful to distinguish new shell classes and status regions instead of generic panels.

- [ ] **Step 2: Run tests to verify red**

Run: `cd web && npm test -- --run`
Expected: FAIL where class/structure assumptions changed.

- [ ] **Step 3: Write minimal implementation**

Refactor `styles.css` to introduce:
- control header styling
- tiered surface rules (`primary`, `secondary`, `utility`)
- chat-first workspace rules
- responsive collapse behavior

- [ ] **Step 4: Run tests and build**

Run:
- `cd web && npm test -- --run`
- `cd web && npm run build`

Expected: tests PASS and Vite build succeeds.

### Task 3: Bring Sessions and Chat to the new operator-console layout

**Files:**
- Modify: `web/src/App.tsx`
- Modify: `web/src/chat/ChatPane.tsx`
- Modify: `web/src/chat/model.ts`
- Modify: `web/src/chat/model.test.tsx`

- [ ] **Step 1: Write the failing tests**

Add/extend tests for:
- chat sidebar composition
- clearer status region grouping
- queue and `/btw` region placement
- absence of redundant session UI inside chat

- [ ] **Step 2: Run tests to verify it fails**

Run: `cd web && npm test -- --run`
Expected: FAIL on chat layout expectations.

- [ ] **Step 3: Write minimal implementation**

Update `ChatPane.tsx` and related model wiring to:
- make timeline the dominant surface
- move operational metadata into a secondary sidebar
- keep composer as primary action surface

- [ ] **Step 4: Run tests and build**

Run:
- `cd web && npm test -- --run`
- `cd web && npm run build`

Expected: PASS.

### Task 4: Align Plan, Tools, and Settings to the same visual system

**Files:**
- Modify: `web/src/plan/PlanPane.tsx`
- Modify: `web/src/tools/ToolsPane.tsx`
- Modify: `web/src/settings/SettingsPane.tsx`

- [ ] **Step 1: Write or extend failing tests**

Where useful, extend pane tests to assert the new hierarchy markers or explicit section structure.

- [ ] **Step 2: Run tests to verify it fails**

Run: `cd web && npm test -- --run`
Expected: FAIL on updated pane expectations.

- [ ] **Step 3: Write minimal implementation**

Refactor each pane so it uses the new primary/secondary/utility surface model instead of identical panel treatment.

- [ ] **Step 4: Run tests and build**

Run:
- `cd web && npm test -- --run`
- `cd web && npm run build`

Expected: PASS.

### Task 5: Document and live-verify the redesign

**Files:**
- Modify: `docs/clean-room-daemon-web-ui.md`

- [ ] **Step 1: Update docs**

Document the operator-console shell, tab roles, and responsive behavior.

- [ ] **Step 2: Run repository verification**

Run:
- `cd web && npm test -- --run`
- `cd web && npm run build`
- `TMPDIR=$PWD/.tmp-goexec GOCACHE=$PWD/.tmp-goexec/gocache go test ./internal/... ./cmd/agent -count=1`
- `TMPDIR=$PWD/.tmp-goexec GOCACHE=$PWD/.tmp-goexec/gocache go build ./cmd/agent`

Expected: all PASS.

- [ ] **Step 3: Live daemon smoke**

Run a websocket/bootstrap smoke against the daemon and verify:
- web assets still load
- `session.create` works
- `chat.send` still streams

- [ ] **Step 4: Commit**

```bash
git add web/src/App.tsx web/src/chat/ChatPane.tsx web/src/chat/model.ts web/src/chat/model.test.tsx web/src/layout.ts web/src/styles.css web/src/plan/PlanPane.tsx web/src/tools/ToolsPane.tsx web/src/settings/SettingsPane.tsx docs/clean-room-daemon-web-ui.md docs/superpowers/specs/2026-04-16-web-operator-console-design.md docs/superpowers/plans/2026-04-16-web-operator-console-implementation.md
git commit -m "feat(teamD): redesign web ui as operator console"
```
