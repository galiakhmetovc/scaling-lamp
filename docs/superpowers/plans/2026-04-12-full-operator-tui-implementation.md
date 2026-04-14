# Full Operator TUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use test-driven-development for each behavior change before implementation.

**Goal:** Build a full terminal operator IDE for `teamD` over the existing runtime control plane.

**Architecture:** Keep the runtime API-first. The TUI is a client over `AgentCore` surfaces exposed through HTTP API and SSE. No new runtime execution path is introduced. Existing `chat` remains compatibility fallback until the TUI is complete enough to become the primary interactive terminal surface.

**Tech Stack:** Go, terminal UI library to be selected during implementation, existing `internal/api`, `internal/cli`, SSE event stream, project-scoped YAML policy file, existing replay/jobs/workers/plans/artifacts APIs.

---

## Phase 1: TUI Shell And Multi-Pane Foundation

**Outcome:** `teamd-agent tui` opens a real full-screen terminal UI with session navigation, center chat view, right-side live event panel, and bottom input editor.

### Task 1: Add TUI entrypoint and failing shell tests

**Files:**
- Modify: `cmd/coordinator/cli.go`
- Modify: `cmd/coordinator/cli_test.go`
- Create: `internal/cli/tui/app_test.go`

- [ ] Add failing tests for:
  - `teamd-agent tui`
  - `teamd-agent tui list`
  - `teamd-agent tui new`
  - `teamd-agent tui use <session>`
- [ ] Verify red
- [ ] Add minimal dispatch plumbing
- [ ] Verify green

### Task 2: Build initial TUI shell and pane layout

**Files:**
- Create: `internal/cli/tui/app.go`
- Create: `internal/cli/tui/layout.go`
- Create: `internal/cli/tui/model.go`
- Create: `internal/cli/tui/render.go`
- Create: `internal/cli/tui/app_test.go`

- [ ] Add failing tests for:
  - shell starts with three panes
  - current session is visible
  - input area is focusable
- [ ] Verify red
- [ ] Implement minimal shell/layout/model
- [ ] Verify green

### Task 3: Add session list and current-session UX

**Files:**
- Modify: `internal/cli/tui/app.go`
- Modify: `internal/cli/tui/model.go`
- Modify: `internal/cli/tui/render.go`
- Modify: `internal/cli/tui/app_test.go`

- [ ] Add failing tests for:
  - session list renders
  - `tui new` selects a new session
  - `tui use` restores a session
- [ ] Verify red
- [ ] Implement client-side session UX over existing API
- [ ] Verify green

---

## Phase 2: Chat, Live Timeline, And Operator State

**Outcome:** Center pane behaves like a real operator conversation and right pane reflects live runtime state.

### Task 4: Add chat transcript model and input editor

**Files:**
- Create: `internal/cli/tui/chat.go`
- Modify: `internal/cli/tui/app.go`
- Modify: `internal/cli/tui/app_test.go`

- [ ] Add failing tests for:
  - plain input starts a run
  - sent message appears as `you`
  - assistant-visible result appears in transcript
- [ ] Verify red
- [ ] Implement chat transcript/input integration
- [ ] Verify green

### Task 5: Add live SSE event feed into TUI

**Files:**
- Create: `internal/cli/tui/events.go`
- Modify: `internal/cli/client.go`
- Modify: `internal/cli/tui/app.go`
- Modify: `internal/cli/tui/app_test.go`

- [ ] Add failing tests for:
  - run lifecycle events appear in right pane
  - tool/worker/job/memory/plan events map to readable UI blocks
  - stream disconnect degrades cleanly
- [ ] Verify red
- [ ] Implement SSE-driven timeline model
- [ ] Verify green

### Task 6: Add control-state side panels

**Files:**
- Create: `internal/cli/tui/control_state.go`
- Modify: `internal/cli/tui/app.go`
- Modify: `internal/cli/tui/render.go`
- Modify: `internal/cli/tui/app_test.go`

- [ ] Add failing tests for:
  - pending approvals panel
  - active workers/jobs panel
  - plan summary panel
- [ ] Verify red
- [ ] Implement control-state polling/refresh
- [ ] Verify green

---

## Phase 3: Approval Menus And Policy Actions

**Outcome:** approvals are actionable from menus instead of slash commands, with richer operator decisions.

### Task 7: Add approval action menu

**Files:**
- Create: `internal/cli/tui/approvals.go`
- Modify: `internal/cli/tui/app.go`
- Modify: `internal/cli/tui/app_test.go`

- [ ] Add failing tests for menu actions:
  - deny once
  - deny and reply
  - allow once
  - allow for session
  - allow forever
  - allow all for session
- [ ] Verify red
- [ ] Implement approval action menu and wiring
- [ ] Verify green

### Task 8: Define project policy YAML model

**Files:**
- Create: `internal/runtime/project_policy.go`
- Create: `internal/runtime/project_policy_test.go`
- Create: `docs/agent/project-policy.md`

- [ ] Add failing tests for YAML parse/serialize/effective-rule behavior
- [ ] Verify red
- [ ] Implement project-scoped YAML policy model with comments-preserving behavior where practical
- [ ] Verify green

### Task 9: Apply policy changes live

**Files:**
- Modify: `internal/runtime/policy_resolver.go`
- Modify: `internal/api/server.go`
- Modify: `internal/cli/tui/approvals.go`
- Modify: corresponding tests

- [ ] Add failing tests for live policy apply semantics
- [ ] Verify red
- [ ] Implement live reload/apply path
- [ ] Verify green

---

## Phase 4: Policy Editor, Replay, Artifact And Handoff Views

**Outcome:** TUI becomes a real operator/debug environment, not just a fancy chat.

### Task 10: Add policy editor/view

**Files:**
- Create: `internal/cli/tui/policy_editor.go`
- Modify: `internal/cli/tui/app.go`
- Modify: tests

- [ ] Add failing tests for browse/edit/disable/delete rule flows
- [ ] Verify red
- [ ] Implement TUI policy editor
- [ ] Verify green

### Task 11: Add replay view

**Files:**
- Create: `internal/cli/tui/replay.go`
- Modify: `internal/cli/tui/app.go`
- Modify: tests

- [ ] Add failing tests for replay list/open/render
- [ ] Verify red
- [ ] Implement replay inspector using existing replay API
- [ ] Verify green

### Task 12: Add artifact and handoff inspectors

**Files:**
- Create: `internal/cli/tui/artifacts.go`
- Create: `internal/cli/tui/handoff.go`
- Modify: tests

- [ ] Add failing tests for artifact preview and worker handoff inspection
- [ ] Verify red
- [ ] Implement inspectors
- [ ] Verify green

---

## Phase 5: Compatibility, Docs, And Rollout

**Outcome:** `tui` is documented, shippable, and the old `chat` path is clearly demoted to compatibility mode.

### Task 13: Update docs and operator guidance

**Files:**
- Modify: `docs/agent/cli.md`
- Modify: `docs/agent/operator-chat.md`
- Modify: `docs/agent/http-api.md`
- Modify: `docs/agent/core-architecture-walkthrough.md`

- [ ] Document:
  - `teamd-agent tui`
  - mode model
  - approval menus
  - policy YAML model
  - replay/artifact/handoff flows

### Task 14: Full verification and live smoke

- [ ] Run focused tests for TUI packages
- [ ] Run full suite:
  - `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./... -count=1`
  - `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go build ./cmd/coordinator`
- [ ] Rebuild live binary
- [ ] Live smoke:
  - start `teamd-agent tui`
  - switch session
  - send prompt
  - observe approvals/workers/jobs/events
  - inspect artifact/handoff

### Task 15: Decide old chat status

- [ ] Keep `chat` as:
  - compatibility fallback only
  - clearly documented as secondary
- [ ] Do not remove it in the same implementation cycle

---

## Deliverable Rule

A phase is only complete when:

- tests exist for new behavior
- the phase is useful to an operator on its own
- no duplicate runtime model was introduced
