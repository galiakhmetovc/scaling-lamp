# Telegram Run Control UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Telegram request acknowledgement, mutable run status cards, inline `Status` and `Cancel` controls, periodic status refresh, and Telegram-safe formatting for model output.

**Architecture:** Add a small run-state layer inside the Telegram transport so each active request has an ack message, one mutable status message, and structured runtime counters. The adapter will stop streaming raw tool outputs as separate chat messages and instead update one status card from execution events. Formatting will pass through a Telegram-safe renderer that reshapes tables into plain-text blocks before sending final replies.

**Tech Stack:** Go 1.24+, existing Telegram adapter, existing tool-calling loop, existing session/checkpoint store, Telegram Bot API `sendMessage`, `editMessageText`, `editMessageReplyMarkup`, callback queries, Go tests.

---

## File Structure

- Modify: `internal/transport/telegram/adapter.go`
  Purpose: manage ack/status message lifecycle, callback handling, run-state updates, and status refresh.
- Create: `internal/transport/telegram/run_state.go`
  Purpose: hold ephemeral active-run state keyed by chat and run id.
- Create: `internal/transport/telegram/run_state_test.go`
  Purpose: verify state transitions, cancel markers, and stats accumulation.
- Create: `internal/transport/telegram/status_card.go`
  Purpose: render the mutable status card and detailed status view.
- Create: `internal/transport/telegram/status_card_test.go`
  Purpose: verify status-card text and inline controls.
- Create: `internal/transport/telegram/formatting.go`
  Purpose: adapt model markdown into Telegram-safe output and reshape tables.
- Create: `internal/transport/telegram/formatting_test.go`
  Purpose: verify table reshape and markdown-safe transformations.
- Modify: `internal/transport/telegram/adapter_test.go`
  Purpose: cover ack flow, status updates, cancel button, and no-tool-spam behavior.
- Modify: `README.md`
  Purpose: document active-run UX and formatting policy.

---

## Task 1: Add Telegram Run State Model

**Files:**
- Create: `internal/transport/telegram/run_state.go`
- Create: `internal/transport/telegram/run_state_test.go`

- [ ] **Step 1: Write the failing test for active run lifecycle**

```go
func TestRunStateTracksActiveRunAndElapsedTime(t *testing.T) {
    state := NewRunStateStore()
    runID := state.Start(1001, "deploy")

    run, ok := state.Active(1001)
    if !ok || run.ID != runID {
        t.Fatalf("expected active run, got %#v", run)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestRunStateTracksActiveRunAndElapsedTime -v`

Expected: FAIL because run-state storage does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Implement:
- `RunStateStore`
- per-chat active run
- fields for run id, started time, stage, tool count, token counters, status message id, ack message id, cancel requested flag

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestRunStateTracksActiveRunAndElapsedTime -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/run_state.go internal/transport/telegram/run_state_test.go
git commit -m "feat: add telegram run state model"
```

### Task 2: Render Ack And Mutable Status Card

**Files:**
- Create: `internal/transport/telegram/status_card.go`
- Create: `internal/transport/telegram/status_card_test.go`
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing test for ack plus status-card flow**

```go
func TestAdapterReplySendsAckThenStatusCard(t *testing.T) {
    // verify first sendMessage is ack
    // verify second sendMessage is status card
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestAdapterReplySendsAckThenStatusCard -v`

Expected: FAIL because the adapter currently jumps directly to normal reply flow.

- [ ] **Step 3: Write minimal implementation**

Implement:
- short ack message
- initial status card message
- inline buttons `Статус` and `Отменить`
- record returned Telegram message ids in run state

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestAdapterReplySendsAckThenStatusCard -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/status_card.go internal/transport/telegram/status_card_test.go internal/transport/telegram/adapter.go internal/transport/telegram/adapter_test.go
git commit -m "feat: add telegram ack and status card flow"
```

### Task 3: Replace Tool Spam With Status Updates

**Files:**
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing test for no per-tool chat spam**

```go
func TestAdapterReplyDoesNotSendSeparateToolMessages(t *testing.T) {
    // simulate tool-calling loop and verify the adapter edits the status card
    // instead of sending one message per tool result
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestAdapterReplyDoesNotSendSeparateToolMessages -v`

Expected: FAIL because the adapter currently sends tool activity as separate messages.

- [ ] **Step 3: Write minimal implementation**

Change the tool loop so:
- tool executions update run state
- `Что уже сделано` is rebuilt from run state
- status card is edited after meaningful changes
- raw tool output does not go to the chat as standalone messages

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestAdapterReplyDoesNotSendSeparateToolMessages -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/adapter.go internal/transport/telegram/adapter_test.go
git commit -m "feat: route tool progress into telegram status card"
```

### Task 4: Add Periodic Refresh And Cancel Flow

**Files:**
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/run_state.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing test for cancel callback**

```go
func TestAdapterCallbackCancelsActiveRun(t *testing.T) {
    // create active run, trigger callback, verify cancel_requested state
    // and status-card update
}
```

- [ ] **Step 2: Write the failing test for periodic status refresh**

```go
func TestAdapterRefreshesStatusCardDuringLongRun(t *testing.T) {
    // verify editMessageText is used on a timer while the run is active
}
```

- [ ] **Step 3: Run focused tests to verify they fail**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run 'TestAdapterCallbackCancelsActiveRun|TestAdapterRefreshesStatusCardDuringLongRun' -v`

Expected: FAIL because cancel and timed refresh are not implemented yet.

- [ ] **Step 4: Write minimal implementation**

Implement:
- callback `run:cancel`
- cancel flag in run state
- cooperative cancellation hook in the run loop
- periodic `editMessageText` refresh every ~5 seconds while the run is active

- [ ] **Step 5: Run focused tests to verify they pass**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run 'TestAdapterCallbackCancelsActiveRun|TestAdapterRefreshesStatusCardDuringLongRun' -v`

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/transport/telegram/adapter.go internal/transport/telegram/run_state.go internal/transport/telegram/adapter_test.go
git commit -m "feat: add telegram run cancel and periodic refresh"
```

### Task 5: Add On-Demand Status Breakdown

**Files:**
- Modify: `internal/transport/telegram/status_card.go`
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing test for status breakdown callback**

```go
func TestAdapterCallbackShowsDetailedRunStatus(t *testing.T) {
    // verify Status callback renders token/tool/runtime breakdown
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestAdapterCallbackShowsDetailedRunStatus -v`

Expected: FAIL because status details are not separated from the main reply yet.

- [ ] **Step 3: Write minimal implementation**

Expose through `Статус`:
- prompt tokens
- completion tokens
- tool-related counters
- tool output size
- total tool time
- context-window percentage

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestAdapterCallbackShowsDetailedRunStatus -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/status_card.go internal/transport/telegram/adapter.go internal/transport/telegram/adapter_test.go
git commit -m "feat: add telegram run status breakdown"
```

### Task 6: Add Telegram-Safe Formatting And Table Reshape

**Files:**
- Create: `internal/transport/telegram/formatting.go`
- Create: `internal/transport/telegram/formatting_test.go`
- Modify: `internal/transport/telegram/adapter.go`

- [ ] **Step 1: Write the failing test for table reshape**

```go
func TestFormatTelegramReplyReshapesTablesIntoPlainText(t *testing.T) {
    input := "| SERVICE | STATUS | REASON |\n|---|---|---|\n| api | failed | missing DB_URL |"
    out := FormatTelegramReply(input)
    if strings.Contains(out, "|") {
        t.Fatalf("expected table reshape, got %q", out)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestFormatTelegramReplyReshapesTablesIntoPlainText -v`

Expected: FAIL because Telegram-safe formatting helper does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Implement:
- markdown-safe text cleanup
- table detection
- plain-text reshape for tables
- fallback to readable list-oriented output

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestFormatTelegramReplyReshapesTablesIntoPlainText -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/formatting.go internal/transport/telegram/formatting_test.go internal/transport/telegram/adapter.go
git commit -m "feat: add telegram-safe reply formatting"
```

### Task 7: Documentation And Verification

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update runtime documentation**

Document:
- ack + mutable status-card flow
- inline `Status` and `Cancel`
- no persistent keyboard
- no per-tool message spam
- plain-text table reshape policy

- [ ] **Step 2: Run full verification**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./...`

Expected: PASS

- [ ] **Step 3: Manual Telegram verification**

Verify live:
- request gets ack immediately
- status card appears and refreshes
- `Что уже сделано` updates
- `Отменить` works
- `Статус` shows detailed breakdown
- final answer is formatted cleanly
- tables are reshaped into readable plain text

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: document telegram run control ux"
```

---

## Non-Goals

- No persistent reply keyboard
- No image-based output formatting
- No raw tool-output streaming in chat
- No session-management redesign beyond what active-run UX needs

---

## Follow-On Work

- richer cancellation semantics for long-running tools
- more advanced markdown adaptation
- richer status-detail presentation
- linking status UX to approvals and policy actions
