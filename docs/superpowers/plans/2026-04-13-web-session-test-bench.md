# Web Session Test Bench Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a local web test bench for `teamD` that can create sessions, send messages through the canonical runtime path, and expose transcript mutation, prompt provenance, compaction, recall, SessionHead, and artifact behavior in one live interface.

**Architecture:** Keep the runtime API-first. The web test bench is an embedded client served by the existing Go binary and backed by runtime-owned debug endpoints plus SSE. The only phase-1 write path is "submit user message" into a selected session; all other views are read-only and must reflect canonical runtime state rather than web-owned state.

**Tech Stack:** Go, existing `internal/api`, `internal/runtime`, `internal/transport/telegram` event instrumentation patterns, embedded static web assets, existing HTTP API auth boundary, existing SSE event stream, existing session actions and runtime execution service.

---

## File Structure

### Runtime and API

- Create: `internal/runtime/debug_views.go`
  - debugger-facing view models for sessions, runs, transcript timeline, prompt rounds, and context provenance
- Create: `internal/runtime/debug_service.go`
  - runtime-owned aggregation of debug snapshots and timeline rows
- Create: `internal/runtime/debug_service_test.go`
  - contract tests for debug views and provenance ordering
- Modify: `internal/runtime/types.go`
  - add reusable debug structs if they belong in runtime types
- Modify: `internal/runtime/runtime_api.go`
  - expose debug snapshot methods through the runtime API
- Modify: `internal/runtime/agent_core.go`
  - expose debug methods through `AgentCore`
- Modify: `internal/api/types.go`
  - request/response structs for debug endpoints
- Modify: `internal/api/server.go`
  - add `/api/debug/*` routes and message-submit route
- Modify: `internal/api/server_test.go`
  - API coverage for new debug endpoints

### Event and provenance instrumentation

- Create: `internal/runtime/debug_events.go`
  - helpers for transcript and prompt provenance events
- Create: `internal/runtime/debug_events_test.go`
  - event-shape and persistence coverage
- Modify: `internal/runtime/conversation_engine.go`
  - emit transcript mutation and prompt-assembly debug events
- Modify: `internal/runtime/prompt_context_assembler.go`
  - persist layer provenance and assembled prompt snapshots
- Modify: `internal/runtime/prompt_context.go`
  - thread projected budget and provenance into debug snapshots
- Modify: `internal/runtime/execution_service.go`
  - persist SessionHead-related debug events and message-submit path support
- Modify: `internal/runtime/recent_work.go`
  - surface recent-work provenance cleanly

### Web UI

- Create: `internal/web/handler.go`
  - serves embedded test bench shell and static assets
- Create: `internal/web/handler_test.go`
  - shell route and auth coverage
- Create: `internal/web/assets/index.html`
  - top-level test bench shell
- Create: `internal/web/assets/app.js`
  - session picker, message submit, live stream wiring, pane updates
- Create: `internal/web/assets/styles.css`
  - pane layout and readable debug styling
- Create: `internal/web/assets/app.test.js` or Go-side integration coverage if JS stays minimal
  - UI behavior smoke tests if practical
- Modify: `cmd/coordinator/bootstrap.go`
  - wire embedded web handler into the server

### Docs

- Modify: `docs/agent/http-api.md`
  - document debug endpoints and local web test bench route
- Modify: `docs/agent/runtime-api-walkthrough.md`
  - explain debug view model and provenance surfaces
- Modify: `docs/agent/context-budget.md`
  - point to the new web visibility layer
- Modify: `docs/agent/operator-chat.md`
  - clarify web test bench vs Telegram vs CLI vs TUI

---

## Phase 1: Runtime Debug View Model

**Outcome:** runtime exposes one coherent debug model for sessions, runs, transcript timeline, prompt rounds, and provenance.

### Task 1: Define debug view structs and service boundary

**Files:**
- Create: `internal/runtime/debug_views.go`
- Create: `internal/runtime/debug_service.go`
- Create: `internal/runtime/debug_service_test.go`
- Modify: `internal/runtime/agent_core.go`
- Modify: `internal/runtime/runtime_api.go`

- [ ] **Step 1: Write the failing tests for debug view aggregation**

```go
func TestDebugServiceBuildsSessionAndRunViews(t *testing.T) {
    // expect session snapshot, run snapshot, and timeline rows from persisted runtime state
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'DebugServiceBuildsSessionAndRunViews' -count=1`
Expected: FAIL because debug service does not exist yet.

- [ ] **Step 3: Write minimal debug view structs and service**

- [ ] **Step 4: Run targeted tests to verify they pass**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'DebugServiceBuildsSessionAndRunViews' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/debug_views.go internal/runtime/debug_service.go internal/runtime/debug_service_test.go internal/runtime/agent_core.go internal/runtime/runtime_api.go
git commit -m "feat(teamD): add runtime debug view model"
```

### Task 2: Add context provenance view model

**Files:**
- Modify: `internal/runtime/debug_views.go`
- Modify: `internal/runtime/debug_service.go`
- Modify: `internal/runtime/debug_service_test.go`
- Modify: `internal/runtime/recent_work.go`

- [ ] **Step 1: Write the failing test for provenance ordering**

```go
func TestDebugServiceBuildsContextProvenance(t *testing.T) {
    // expect transcript, SessionHead, recent_work, memory recall, checkpoint, continuity, workspace, skills provenance
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'DebugServiceBuildsContextProvenance' -count=1`
Expected: FAIL because provenance view is incomplete.

- [ ] **Step 3: Implement minimal provenance view**

- [ ] **Step 4: Run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'DebugServiceBuildsContextProvenance' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/debug_views.go internal/runtime/debug_service.go internal/runtime/debug_service_test.go internal/runtime/recent_work.go
git commit -m "feat(teamD): add debug context provenance views"
```

---

## Phase 2: Transcript and Prompt Instrumentation

**Outcome:** runtime emits and persists the timeline and prompt provenance events the web test bench needs.

### Task 3: Emit transcript mutation events

**Files:**
- Create: `internal/runtime/debug_events.go`
- Create: `internal/runtime/debug_events_test.go`
- Modify: `internal/runtime/conversation_engine.go`
- Modify: `internal/runtime/execution_service.go`

- [ ] **Step 1: Write the failing test for transcript append events**

```go
func TestExecuteConversationEmitsTranscriptMutationEvents(t *testing.T) {
    // expect transcript.appended for user, assistant, tool, and system-like inserts
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'TranscriptMutationEvents' -count=1`
Expected: FAIL because these events are not persisted yet.

- [ ] **Step 3: Implement event helpers and minimal emission path**

- [ ] **Step 4: Run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'TranscriptMutationEvents' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/debug_events.go internal/runtime/debug_events_test.go internal/runtime/conversation_engine.go internal/runtime/execution_service.go
git commit -m "feat(teamD): emit transcript mutation debug events"
```

### Task 4: Emit provenance and prompt-assembly events

**Files:**
- Modify: `internal/runtime/prompt_context_assembler.go`
- Modify: `internal/runtime/prompt_context.go`
- Modify: `internal/runtime/debug_events.go`
- Modify: `internal/runtime/debug_events_test.go`

- [ ] **Step 1: Write the failing test for prompt provenance events**

```go
func TestPromptAssemblyEmitsLayerProvenance(t *testing.T) {
    // expect session_head, recent_work, memory recall, workspace, skills, checkpoint, continuity layer signals
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'PromptAssemblyEmitsLayerProvenance' -count=1`
Expected: FAIL because provenance snapshots are not persisted yet.

- [ ] **Step 3: Implement minimal prompt assembly snapshot persistence**

- [ ] **Step 4: Run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'PromptAssemblyEmitsLayerProvenance' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/prompt_context_assembler.go internal/runtime/prompt_context.go internal/runtime/debug_events.go internal/runtime/debug_events_test.go
git commit -m "feat(teamD): persist prompt provenance debug snapshots"
```

### Task 5: Emit compaction, pruning, and artifact debug signals

**Files:**
- Modify: `internal/runtime/prompt_context.go`
- Modify: `internal/runtime/conversation_engine.go`
- Modify: `internal/runtime/debug_events.go`
- Modify: `internal/runtime/debug_events_test.go`

- [ ] **Step 1: Write the failing test for compaction and artifact debug events**

```go
func TestRuntimeEmitsCompactionPruningAndArtifactSignals(t *testing.T) {
    // expect transcript.pruned, transcript.compacted, artifact.offloaded correlation
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'CompactionPruningAndArtifactSignals' -count=1`
Expected: FAIL because these signals are incomplete.

- [ ] **Step 3: Implement minimal signal emission**

- [ ] **Step 4: Run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'CompactionPruningAndArtifactSignals' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/prompt_context.go internal/runtime/conversation_engine.go internal/runtime/debug_events.go internal/runtime/debug_events_test.go
git commit -m "feat(teamD): add compaction and artifact debug signals"
```

---

## Phase 3: Debug API Surfaces

**Outcome:** the server exposes stable `/api/debug/*` routes for the web UI.

### Task 6: Add session and run debug endpoints

**Files:**
- Modify: `internal/api/types.go`
- Modify: `internal/api/server.go`
- Modify: `internal/api/server_test.go`
- Modify: `internal/runtime/runtime_api.go`

- [ ] **Step 1: Write failing API tests for session and run debug views**

```go
func TestServerDebugSessionAndRunEndpoints(t *testing.T) {
    // expect /api/debug/sessions, /api/debug/sessions/{id}, /api/debug/runs/{id}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/api -run 'DebugSessionAndRunEndpoints' -count=1`
Expected: FAIL because routes do not exist yet.

- [ ] **Step 3: Implement minimal routes and response structs**

- [ ] **Step 4: Run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/api -run 'DebugSessionAndRunEndpoints' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/api/types.go internal/api/server.go internal/api/server_test.go internal/runtime/runtime_api.go
git commit -m "feat(teamD): add debug session and run api routes"
```

### Task 7: Add transcript timeline, prompt rounds, and provenance endpoints

**Files:**
- Modify: `internal/api/types.go`
- Modify: `internal/api/server.go`
- Modify: `internal/api/server_test.go`
- Modify: `internal/runtime/debug_service.go`

- [ ] **Step 1: Write failing API tests for transcript timeline and provenance**

```go
func TestServerDebugTimelineAndProvenanceEndpoints(t *testing.T) {
    // expect transcript timeline, prompt rounds, and context provenance routes
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/api -run 'DebugTimelineAndProvenanceEndpoints' -count=1`
Expected: FAIL because these routes do not exist yet.

- [ ] **Step 3: Implement minimal routes**

- [ ] **Step 4: Run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/api -run 'DebugTimelineAndProvenanceEndpoints' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/api/types.go internal/api/server.go internal/api/server_test.go internal/runtime/debug_service.go
git commit -m "feat(teamD): add debug timeline and provenance api routes"
```

### Task 8: Add message submit endpoint using canonical execution path

**Files:**
- Modify: `internal/api/types.go`
- Modify: `internal/api/server.go`
- Modify: `internal/api/server_test.go`
- Modify: `internal/runtime/agent_core.go`
- Modify: `internal/runtime/runtime_api.go`

- [ ] **Step 1: Write failing API test for session message submit**

```go
func TestServerDebugSessionMessageSubmitStartsRun(t *testing.T) {
    // expect POST /api/debug/sessions/{id}/messages to append user message and start a normal run
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/api -run 'DebugSessionMessageSubmitStartsRun' -count=1`
Expected: FAIL because message submit route does not exist yet.

- [ ] **Step 3: Implement the single phase-1 write action**

- [ ] **Step 4: Run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/api -run 'DebugSessionMessageSubmitStartsRun' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/api/types.go internal/api/server.go internal/api/server_test.go internal/runtime/agent_core.go internal/runtime/runtime_api.go
git commit -m "feat(teamD): add debug session message submit route"
```

---

## Phase 4: Embedded Web Shell

**Outcome:** the server serves a local web shell for the session test bench.

### Task 9: Add embedded web handler and shell route

**Files:**
- Create: `internal/web/handler.go`
- Create: `internal/web/handler_test.go`
- Create: `internal/web/assets/index.html`
- Modify: `internal/api/server.go`

- [ ] **Step 1: Write failing tests for shell route and auth**

```go
func TestWebHandlerServesSessionTestBenchShell(t *testing.T) {
    // expect authenticated shell route and local asset serving
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web ./internal/api -run 'SessionTestBenchShell' -count=1`
Expected: FAIL because the web handler does not exist yet.

- [ ] **Step 3: Implement minimal embedded shell route**

- [ ] **Step 4: Run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web ./internal/api -run 'SessionTestBenchShell' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/web/handler.go internal/web/handler_test.go internal/web/assets/index.html internal/api/server.go
git commit -m "feat(teamD): serve embedded web test bench shell"
```

### Task 10: Add layout and client-side session picker

**Files:**
- Modify: `internal/web/assets/index.html`
- Create: `internal/web/assets/styles.css`
- Create: `internal/web/assets/app.js`

- [ ] **Step 1: Add failing integration assertion for session list rendering**

```text
Expect shell markup to include panes for sessions, chat, timeline, and inspector.
```

- [ ] **Step 2: Run the shell tests and verify red**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web -count=1`
Expected: FAIL because shell markup is incomplete.

- [ ] **Step 3: Implement minimal layout and session list fetch**

- [ ] **Step 4: Run shell tests**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/web/assets/index.html internal/web/assets/styles.css internal/web/assets/app.js internal/web/handler_test.go
git commit -m "feat(teamD): add web test bench layout and session picker"
```

---

## Phase 5: Session Creation and Message Flow

**Outcome:** the operator can create a session, submit a message, and see live chat plus timeline updates.

### Task 11: Add new-session flow over existing session actions

**Files:**
- Modify: `internal/web/assets/app.js`
- Modify: `internal/web/handler_test.go`
- Modify: `internal/api/server_test.go`

- [ ] **Step 1: Write failing tests for new-session creation flow**

```text
Expect new session action to call the existing generic session path and refresh the selected session.
```

- [ ] **Step 2: Run tests to verify red**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web ./internal/api -count=1`
Expected: FAIL because new-session UI flow is missing.

- [ ] **Step 3: Implement minimal new-session UI**

- [ ] **Step 4: Run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web ./internal/api -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/web/assets/app.js internal/web/handler_test.go internal/api/server_test.go
git commit -m "feat(teamD): add web test bench session creation"
```

### Task 12: Add chat input and canonical message submit flow

**Files:**
- Modify: `internal/web/assets/app.js`
- Modify: `internal/web/assets/index.html`
- Modify: `internal/web/handler_test.go`

- [ ] **Step 1: Write failing tests for message submit flow**

```text
Expect chat input to post to the debug message-submit route and render the submitted user message.
```

- [ ] **Step 2: Run tests to verify red**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web -count=1`
Expected: FAIL because message submit UI is missing.

- [ ] **Step 3: Implement minimal input form and submit behavior**

- [ ] **Step 4: Run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/web/assets/app.js internal/web/assets/index.html internal/web/handler_test.go
git commit -m "feat(teamD): add web test bench message submit flow"
```

### Task 13: Add live SSE updates into chat and timeline panes

**Files:**
- Modify: `internal/web/assets/app.js`
- Modify: `internal/web/handler_test.go`

- [ ] **Step 1: Write failing tests for live stream updates**

```text
Expect transcript, run state, and timeline panes to update from SSE without reload.
```

- [ ] **Step 2: Run tests to verify red**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web -count=1`
Expected: FAIL because live stream wiring is incomplete.

- [ ] **Step 3: Implement minimal SSE subscription and incremental UI updates**

- [ ] **Step 4: Run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/web/assets/app.js internal/web/handler_test.go
git commit -m "feat(teamD): add live stream updates to web test bench"
```

---

## Phase 6: Inspector Panels

**Outcome:** the right-side panels explain SessionHead, recall, prompt assembly, compaction, and artifacts.

### Task 14: Add SessionHead and provenance inspector

**Files:**
- Modify: `internal/web/assets/app.js`
- Modify: `internal/web/assets/index.html`
- Modify: `internal/web/assets/styles.css`

- [ ] **Step 1: Add failing tests or assertions for SessionHead/provenance panel rendering**

- [ ] **Step 2: Run tests to verify red**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web -count=1`
Expected: FAIL because the inspector is incomplete.

- [ ] **Step 3: Implement minimal SessionHead and provenance sections**

- [ ] **Step 4: Run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/web/assets/app.js internal/web/assets/index.html internal/web/assets/styles.css
git commit -m "feat(teamD): add web session head and provenance inspector"
```

### Task 15: Add prompt assembly and budget inspector

**Files:**
- Modify: `internal/web/assets/app.js`
- Modify: `internal/web/assets/index.html`
- Modify: `internal/web/assets/styles.css`
- Modify: `internal/runtime/debug_service_test.go` (if additional view guarantees are needed)

- [ ] **Step 1: Write failing assertions for prompt assembly panel**

- [ ] **Step 2: Run tests to verify red**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web ./internal/runtime -count=1`
Expected: FAIL because the prompt assembly UI is incomplete.

- [ ] **Step 3: Implement prompt assembly and budget rendering**

- [ ] **Step 4: Run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web ./internal/runtime -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/web/assets/app.js internal/web/assets/index.html internal/web/assets/styles.css internal/runtime/debug_service_test.go
git commit -m "feat(teamD): add web prompt assembly inspector"
```

### Task 16: Add compaction, pruning, and artifact inspector

**Files:**
- Modify: `internal/web/assets/app.js`
- Modify: `internal/web/assets/index.html`
- Modify: `internal/web/assets/styles.css`

- [ ] **Step 1: Write failing assertions for compaction and artifact sections**

- [ ] **Step 2: Run tests to verify red**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web -count=1`
Expected: FAIL because these sections are missing.

- [ ] **Step 3: Implement compaction, pruning, and artifact sections**

- [ ] **Step 4: Run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/web -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/web/assets/app.js internal/web/assets/index.html internal/web/assets/styles.css
git commit -m "feat(teamD): add web compaction and artifact inspector"
```

---

## Phase 7: Docs and Full Verification

**Outcome:** the test bench is documented, testable, and runnable locally.

### Task 17: Update docs

**Files:**
- Modify: `docs/agent/http-api.md`
- Modify: `docs/agent/runtime-api-walkthrough.md`
- Modify: `docs/agent/context-budget.md`
- Modify: `docs/agent/operator-chat.md`

- [ ] **Step 1: Document the web test bench route and debug endpoints**
- [ ] **Step 2: Document transcript and provenance surfaces**
- [ ] **Step 3: Document intended use as a local session test bench**
- [ ] **Step 4: Commit**

```bash
git add docs/agent/http-api.md docs/agent/runtime-api-walkthrough.md docs/agent/context-budget.md docs/agent/operator-chat.md
git commit -m "docs(teamD): document web session test bench"
```

### Task 18: Run verification and local smoke

- [ ] **Step 1: Run focused suites**

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime ./internal/api ./internal/web -count=1
```

Expected: PASS

- [ ] **Step 2: Run full suite**

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./... -count=1
```

Expected: PASS

- [ ] **Step 3: Build binaries**

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go build ./cmd/coordinator ./cmd/worker
```

Expected: PASS

- [ ] **Step 4: Local smoke**

```bash
./coordinator
# open local web test bench route
# create session
# send message
# observe transcript timeline, SessionHead, prompt provenance, compaction/artifact panels
```

Expected: route loads, message submit works, live timeline updates, inspector panels populate.

- [ ] **Step 5: Commit any final polish**

```bash
git add -A
git commit -m "test(teamD): verify web session test bench"
```

---

## Deliverable Rule

The implementation is only complete when:

- the web UI can create a session and submit a message
- transcript mutations update live
- SessionHead, recent_work, memory recall, checkpoint, continuity, workspace, and skills provenance are inspectable
- prompt assembly and budget surfaces explain compaction decisions
- artifact offload is visible in the same debugging flow
- the UI still uses the canonical runtime path instead of a web-only execution path
