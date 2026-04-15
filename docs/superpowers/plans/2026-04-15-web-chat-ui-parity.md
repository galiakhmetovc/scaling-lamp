# Web Chat UI Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the embedded web `Chat` experience to parity with the daemon-backed TUI for the main operator workflow.

**Architecture:** Extend the daemon `SessionSnapshot` with explicit main-run metadata so the web client can render a stable status bar without inventing local state. Refactor the web chat UI into focused view-model helpers and a dedicated chat pane component that renders timeline blocks, queued drafts, and `/btw` side-runs over the existing daemon bootstrap/websocket/command protocol.

**Tech Stack:** Go daemon/runtime, React 18, TypeScript, Vite, React Markdown, Vitest.

---

### Task 1: Add daemon snapshot fields for web chat status

**Files:**
- Modify: `internal/runtime/daemon/session_snapshot.go`
- Modify: `internal/runtime/daemon/commands.go`
- Modify: `internal/runtime/daemon/queue_runtime.go`
- Modify: `internal/runtime/daemon/server_test.go`
- Modify: `web/src/lib/types.ts`

- [ ] **Step 1: Write the failing daemon test**

Add a test in `internal/runtime/daemon/server_test.go` that:
- creates a session
- starts a main chat run against the test provider
- observes `session.get` or `chat.send` payload
- expects explicit main-run metadata fields for:
  - `provider`
  - `model`
  - `started_at`
  - `input_tokens`
  - `output_tokens`
  - `total_tokens`

- [ ] **Step 2: Run the daemon test and verify it fails**

Run:
```bash
TMPDIR=$PWD/.tmp-goexec GOCACHE=$PWD/.tmp-goexec/gocache go test ./internal/runtime/daemon -run TestSessionSnapshotIncludesMainRunMetadata -count=1
```

Expected: FAIL because `SessionSnapshot` does not yet expose those fields.

- [ ] **Step 3: Implement minimal daemon metadata support**

Add a dedicated main-run view to `SessionSnapshot`, sourcing:
- running state from `sessionRuntime`
- provider/model/token usage from the most recent completed main run
- `started_at` when a main run is active

Keep the source of truth server-side; do not force the web client to infer this from websocket timing.

- [ ] **Step 4: Re-run daemon test**

Run:
```bash
TMPDIR=$PWD/.tmp-goexec GOCACHE=$PWD/.tmp-goexec/gocache go test ./internal/runtime/daemon -run TestSessionSnapshotIncludesMainRunMetadata -count=1
```

Expected: PASS.

### Task 2: Extract web chat view-model helpers under test

**Files:**
- Create: `web/src/chat/model.ts`
- Create: `web/src/chat/model.test.ts`
- Modify: `web/package.json`

- [ ] **Step 1: Write failing web helper tests**

Add `web/src/chat/model.test.ts` covering:
- status bar mapping from snapshot + ui state
- run timer text that stays active while `main_run.active` is true
- queued draft summary ordering
- `/btw` branch mapping
- token approximation including transcript + input + queued drafts

- [ ] **Step 2: Run the web tests and verify they fail**

Run:
```bash
cd web && npm test -- --run
```

Expected: FAIL because the test runner and helper module do not exist yet.

- [ ] **Step 3: Add minimal test runner and helper module**

Add `vitest` to `web/package.json` and implement a focused `web/src/chat/model.ts` with pure functions for:
- token approximation
- status bar state
- queued draft formatting
- `/btw` view mapping

- [ ] **Step 4: Re-run the web helper tests**

Run:
```bash
cd web && npm test -- --run
```

Expected: PASS.

### Task 3: Split web chat UI into a dedicated component

**Files:**
- Create: `web/src/chat/ChatPane.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/styles.css`

- [ ] **Step 1: Write a failing component test**

Add a component-level test in `web/src/chat/model.test.ts` or a dedicated `ChatPane.test.tsx` that renders:
- timeline markdown blocks for tool/plan items
- status bar values
- queued drafts with recall affordance
- `/btw` branch blocks

- [ ] **Step 2: Run the component test and verify it fails**

Run:
```bash
cd web && npm test -- --run
```

Expected: FAIL because `ChatPane` and the richer layout are not implemented yet.

- [ ] **Step 3: Implement the dedicated chat pane**

Refactor `web/src/App.tsx` to:
- move chat rendering into `web/src/chat/ChatPane.tsx`
- keep daemon connectivity in `App.tsx`
- use the pure helper module for display state
- add a lower status bar inside the chat composer section, mirroring TUI semantics

Update `web/src/styles.css` so the status bar and branch blocks are visually distinct and compact.

- [ ] **Step 4: Re-run component tests**

Run:
```bash
cd web && npm test -- --run
```

Expected: PASS.

### Task 4: Wire daemon state updates cleanly into web session UI state

**Files:**
- Modify: `web/src/App.tsx`
- Modify: `web/src/lib/client.ts`
- Modify: `web/src/lib/types.ts`

- [ ] **Step 1: Write a failing regression test for websocket/session state mapping**

Add a test around the reducer/helper path that verifies:
- `ui_event` streaming appends text
- `run.completed` clears streaming but keeps last result usage
- daemon payload refresh updates session snapshot without dropping local `/btw` branch state

- [ ] **Step 2: Run the regression test and verify it fails**

Run:
```bash
cd web && npm test -- --run
```

Expected: FAIL because the current state handling is still coupled inside `App.tsx`.

- [ ] **Step 3: Implement minimal state reducer cleanup**

Extract the envelope/session-ui update logic into focused helpers so:
- `App.tsx` stays orchestration-only
- chat-specific state transitions remain testable
- `/btw` side runs remain isolated from transcript refreshes

- [ ] **Step 4: Re-run the regression test**

Run:
```bash
cd web && npm test -- --run
```

Expected: PASS.

### Task 5: Build, verify, and document the slice

**Files:**
- Modify: `docs/clean-room-daemon-web-ui.md`
- Modify: `README.md` (only if launch or verification steps changed)

- [ ] **Step 1: Update docs**

Document:
- web chat status bar fields
- queue recall behavior
- `/btw` branch rendering semantics

- [ ] **Step 2: Run frontend build**

Run:
```bash
cd web && npm run build
```

Expected: PASS and refresh embedded assets.

- [ ] **Step 3: Run focused daemon and web tests**

Run:
```bash
TMPDIR=$PWD/.tmp-goexec GOCACHE=$PWD/.tmp-goexec/gocache go test ./internal/runtime/daemon -count=1
cd web && npm test -- --run
```

Expected: PASS.

- [ ] **Step 4: Run repo verification**

Run:
```bash
TMPDIR=$PWD/.tmp-goexec GOCACHE=$PWD/.tmp-goexec/gocache go test ./internal/... ./cmd/agent -count=1
TMPDIR=$PWD/.tmp-goexec GOCACHE=$PWD/.tmp-goexec/gocache go build ./cmd/agent
```

Expected: PASS.
