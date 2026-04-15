# TUI Chat Timeline And Plan Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the TUI usable for real operator work by adding a markdown timeline in `Chat`, pane-local scrolling everywhere, and a form-based editor in `Plan`.

**Architecture:** Keep runtime events and projections as the durable source of truth. Build a TUI-local timeline model from transcript plus tool/plan events, preserve live stream handling through the UI bus, and route all plan edits through the existing event-sourced plan domain.

**Tech Stack:** Go, Bubble Tea, Bubbles (`textarea`, `viewport`, `textinput`), Lip Gloss, Glamour, existing runtime projections and plan services.

---

### Task 1: Add Timeline Projection For Chat Reconstruction

**Files:**
- Create: `internal/runtime/projections/chat_timeline.go`
- Create: `internal/runtime/projections/chat_timeline_test.go`
- Modify: `internal/runtime/projections/registry.go`
- Modify: `internal/runtime/component_registry.go`
- Modify: `config/zai-smoke/agent.yaml`

- [ ] **Step 1: Write failing projection tests**

Cover:
- transcript messages produce timeline items
- tool events produce timeline items
- plan events produce timeline items
- session scoping is preserved

- [ ] **Step 2: Run the focused tests and confirm failure**

Run: `go test ./internal/runtime/projections -run 'TestChatTimeline' -count=1`

- [ ] **Step 3: Implement `chat_timeline` projection**

Projection shape:
- session-scoped snapshots
- ordered durable timeline items for each session

- [ ] **Step 4: Register the projection**

Wire it into both projection registries and shipped `zai-smoke`.

- [ ] **Step 5: Re-run focused projection tests**

Run: `go test ./internal/runtime/projections -run 'TestChatTimeline' -count=1`

- [ ] **Step 6: Commit**

`git commit -m "feat(teamD): add chat timeline projection for tui"`

### Task 2: Rework TUI Chat Pane Around Timeline + Streaming

**Files:**
- Modify: `internal/runtime/tui/app.go`
- Modify: `internal/runtime/tui/app_test.go`
- Modify: `internal/runtime/chat.go` (helpers only if needed)
- Test: `internal/runtime/tui/app_test.go`

- [ ] **Step 1: Write failing TUI tests**

Cover:
- chat view renders timeline entries
- tool/plan lines appear in chat
- current stream block is visible during live turn

- [ ] **Step 2: Run focused TUI tests and confirm failure**

Run: `go test ./internal/runtime/tui -run 'Test.*Timeline|Test.*Stream' -count=1`

- [ ] **Step 3: Replace transcript-only chat rendering with timeline rendering**

Implement:
- session timeline viewport
- live assistant stream item
- final markdown render after completion

- [ ] **Step 4: Keep short tool and plan entries in chat history**

Use one-line markdown summaries derived from runtime event/projection state.

- [ ] **Step 5: Re-run focused TUI tests**

Run: `go test ./internal/runtime/tui -run 'Test.*Timeline|Test.*Stream' -count=1`

- [ ] **Step 6: Commit**

`git commit -m "feat(teamD): render markdown chat timeline in tui"`

### Task 3: Add Pane-Local Scrolling Everywhere

**Files:**
- Modify: `internal/runtime/tui/app.go`
- Modify: `internal/runtime/tui/app_test.go`

- [ ] **Step 1: Write failing tests for pane scrolling state**

Cover:
- chat pane scroll
- plan pane scroll
- tools pane scroll
- settings pane scroll

- [ ] **Step 2: Run focused scrolling tests and confirm failure**

Run: `go test ./internal/runtime/tui -run 'Test.*Scroll' -count=1`

- [ ] **Step 3: Implement per-pane viewport/focus handling**

Each pane should own its own viewport or scrolling state.

- [ ] **Step 4: Verify keyboard navigation still works**

Re-run the new tests plus the existing TUI suite.

- [ ] **Step 5: Commit**

`git commit -m "feat(teamD): add pane-local scrolling to tui"`

### Task 4: Build Form-Based Plan Editor

**Files:**
- Modify: `internal/runtime/tui/app.go`
- Modify: `internal/runtime/tui/app_test.go`
- Modify: `internal/runtime/plans/service.go` only if TUI integration needs small helper seams
- Test: `internal/runtime/tui/app_test.go`
- Test: `internal/runtime/plans/service_test.go` if new operator path needs domain coverage

- [ ] **Step 1: Write failing tests for plan editor actions**

Cover:
- create plan
- add task
- edit task
- status change
- add note

- [ ] **Step 2: Run focused tests and confirm failure**

Run: `go test ./internal/runtime/tui ./internal/runtime/plans -run 'Test.*PlanEditor|Test.*OperatorPlan' -count=1`

- [ ] **Step 3: Add form state and selected-node editing to `Plan` tab**

Implement:
- task selection
- form fields
- submit actions

- [ ] **Step 4: Route plan edits through the existing event-sourced domain**

No direct projection mutation.

- [ ] **Step 5: Re-run focused tests**

Run: `go test ./internal/runtime/tui ./internal/runtime/plans -run 'Test.*PlanEditor|Test.*OperatorPlan' -count=1`

- [ ] **Step 6: Commit**

`git commit -m "feat(teamD): add form-based plan editor to tui"`

### Task 5: Update Docs And Run Full Verification

**Files:**
- Modify: `docs/clean-room-tui.md`
- Modify: `docs/clean-room-cli-chat.md`
- Modify: `docs/clean-room-current-system-detailed.md`
- Modify: `README.md`

- [ ] **Step 1: Update docs for new chat timeline behavior**

Explain:
- chat timeline entries
- plan editing in `Plan`
- pane-local scrolling

- [ ] **Step 2: Run full verification**

Run:
- `mkdir -p .tmp-goexec && TMPDIR=$PWD/.tmp-goexec go test ./internal/provider ./internal/filesystem ./internal/shell ./internal/runtime/... ./cmd/agent -count=1`
- `GOCACHE=$PWD/.tmp-goexec/gocache GOTMPDIR=$PWD/.tmp-goexec go build -o .tmp-goexec/teamd-agent ./cmd/agent`

- [ ] **Step 3: Clean temp artifacts**

Run: `rm -rf .tmp-goexec`

- [ ] **Step 4: Commit**

`git commit -m "docs(teamD): document tui timeline and plan editor"`
