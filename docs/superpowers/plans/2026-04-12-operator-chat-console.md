# Operator Chat Console Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `teamd-agent chat <chat_id> <session_id>` as an interactive operator console over the existing runtime control plane.

**Architecture:** Keep the implementation API-first. The CLI starts runs through existing HTTP endpoints, watches runtime events over SSE, and renders a readable chat timeline with local slash commands for approvals, plans, artifacts, and worker handoffs. No new runtime execution path is introduced.

**Tech Stack:** Go, existing `teamd/internal/api`, `teamd/internal/cli`, SSE over `text/event-stream`, existing runtime plans/jobs/workers/approvals APIs.

---

### Task 1: Add failing tests for chat console skeleton

**Files:**
- Create: `internal/cli/chat_console_test.go`
- Modify: `cmd/coordinator/cli_test.go`
- Modify: `cmd/coordinator/cli.go`

- [ ] **Step 1: Write failing tests for basic chat loop**

Cover:
- starting `teamd-agent chat 1001 1001:default`
- typing plain text starts a run
- `/quit` exits cleanly
- bad usage returns help text

- [ ] **Step 2: Run targeted tests to verify failure**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/cli ./cmd/coordinator -run 'Chat|CLI' -count=1`

Expected: fail because chat console types and command handling do not exist yet.

- [ ] **Step 3: Add minimal chat command plumbing**

Implement:
- `case "chat"` in `cmd/coordinator/cli.go`
- initial console entrypoint in `internal/cli`

- [ ] **Step 4: Re-run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/cli ./cmd/coordinator -run 'Chat|CLI' -count=1`

Expected: new skeleton tests pass.

- [ ] **Step 5: Commit**

```bash
git add cmd/coordinator/cli.go cmd/coordinator/cli_test.go internal/cli/chat_console_test.go
git commit -m "feat(teamD): add operator chat console skeleton"
```

### Task 2: Add SSE-backed live event rendering

**Files:**
- Create: `internal/cli/chat_console.go`
- Modify: `internal/cli/client.go`
- Modify: `internal/cli/client_test.go`
- Test: `internal/cli/chat_console_test.go`

- [ ] **Step 1: Write failing tests for streaming render**

Cover:
- `StreamEvents` drives readable lines into the console
- stream disconnect falls back cleanly
- run lifecycle events appear as `system:` lines

- [ ] **Step 2: Run targeted tests to verify failure**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/cli -run 'ChatConsole|StreamEvents' -count=1`

- [ ] **Step 3: Implement chat renderer and event subscription**

Add:
- event-to-line renderer
- per-run watch loop
- polling fallback to `RunStatus`

- [ ] **Step 4: Re-run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/cli -run 'ChatConsole|StreamEvents' -count=1`

- [ ] **Step 5: Commit**

```bash
git add internal/cli/chat_console.go internal/cli/client.go internal/cli/client_test.go internal/cli/chat_console_test.go
git commit -m "feat(teamD): add live event rendering to operator chat"
```

### Task 3: Add slash commands for approvals, plans, handoffs, and artifacts

**Files:**
- Modify: `internal/cli/chat_console.go`
- Modify: `internal/cli/chat_console_test.go`
- Modify: `internal/cli/client.go`

- [ ] **Step 1: Write failing tests for slash commands**

Cover:
- `/approve <id>`
- `/reject <id>`
- `/plan`
- `/plans`
- `/handoff <worker_id>`
- `/artifact <ref>`
- `/status`
- `/cancel`

- [ ] **Step 2: Run targeted tests to verify failure**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/cli -run 'ChatConsoleCommands' -count=1`

- [ ] **Step 3: Implement local command dispatch**

Use existing client methods only. Keep chat command handling local to the console package.

- [ ] **Step 4: Re-run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/cli -run 'ChatConsoleCommands' -count=1`

- [ ] **Step 5: Commit**

```bash
git add internal/cli/chat_console.go internal/cli/chat_console_test.go internal/cli/client.go
git commit -m "feat(teamD): add operator actions to chat console"
```

### Task 4: Add minimal tab completion

**Files:**
- Modify: `internal/cli/chat_console.go`
- Modify: `internal/cli/chat_console_test.go`

- [ ] **Step 1: Write failing tests for completion**

Cover:
- slash command completion
- approval id completion
- worker id completion
- artifact ref completion

- [ ] **Step 2: Run targeted tests to verify failure**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/cli -run 'Completion' -count=1`

- [ ] **Step 3: Implement minimal completion**

Use the smallest viable terminal input helper. Avoid bringing in a full TUI stack.

- [ ] **Step 4: Re-run targeted tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/cli -run 'Completion' -count=1`

- [ ] **Step 5: Commit**

```bash
git add internal/cli/chat_console.go internal/cli/chat_console_test.go
git commit -m "feat(teamD): add minimal completion to chat console"
```

### Task 5: Docs, end-to-end verification, and live smoke

**Files:**
- Modify: `docs/agent/cli.md`
- Modify: `docs/agent/http-api.md`
- Modify: `docs/agent/core-architecture-walkthrough.md`

- [ ] **Step 1: Update docs**

Document:
- `teamd-agent chat`
- operator actions
- visible plans/approvals/workers/jobs
- SSE dependency on `/api/events/stream`

- [ ] **Step 2: Run focused tests**

Run:
- `GOTMPDIR=$PWD/.tmp/go go test ./internal/cli ./cmd/coordinator -count=1`

- [ ] **Step 3: Run full suite**

Run:
- `GOTMPDIR=$PWD/.tmp/go go test ./... -count=1`
- `GOTMPDIR=$PWD/.tmp/go go build ./cmd/coordinator`

- [ ] **Step 4: Live smoke**

Run:
- rebuild live binaries in `/home/administrator/teamD` and `/home/administrator/teamD-helper`
- restart `teamd-main` and `teamd-helper`
- smoke:
  - `teamd-agent chat 1001 1001:default`
  - send one prompt
  - verify readable events and clean exit

- [ ] **Step 5: Commit**

```bash
git add docs/agent/cli.md docs/agent/http-api.md docs/agent/core-architecture-walkthrough.md
git commit -m "docs(teamD): document operator chat console"
```
