# Telegram Error Surfacing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface run failures and Telegram/runtime transport errors inside Telegram itself, with user-facing error states in the active chat and technical alerts in a separate admin chat.

**Architecture:** Extend the Telegram adapter with an explicit error-reporting layer. Request-scoped failures should terminate the mutable run card in an error state and send a short user-readable message with an error id. System-level failures that may prevent replying to the user chat should be mirrored into a dedicated admin alert chat via a small notifier path that does not depend on the failing run lifecycle.

**Tech Stack:** Go 1.24+, existing Telegram adapter, existing run-state/status-card flow, Telegram Bot API `sendMessage` and `editMessageText`, existing provider/tool loop, Go tests.

---

## File Structure

- Modify: `internal/transport/telegram/adapter.go`
  Purpose: classify errors, terminate run state cleanly, surface request errors to the current chat, and emit admin alerts for transport/system failures.
- Modify: `internal/transport/telegram/status_card.go`
  Purpose: render user-facing error terminal state and short Russian error summary in the mutable run card.
- Modify: `internal/transport/telegram/run_state.go`
  Purpose: add stable error metadata to active run state such as error id, severity, and last failed stage.
- Create: `internal/transport/telegram/error_reporting.go`
  Purpose: centralize error classification, stable error id generation, and user/admin message formatting.
- Create: `internal/transport/telegram/error_reporting_test.go`
  Purpose: verify error classification and formatting for run errors vs. system alerts.
- Modify: `internal/transport/telegram/adapter_test.go`
  Purpose: cover user-visible failures, admin alerts, and behavior when Telegram `sendMessage`/`editMessageText` fails.
- Modify: `internal/config/config.go`
  Purpose: add optional `TEAMD_TELEGRAM_ADMIN_CHAT_ID` wiring for admin alerts.
- Modify: `README.md`
  Purpose: document which errors appear in user chat, which go to admin chat, and how to configure alert routing.

---

### Task 1: Add Error Classification And Formatting

**Files:**
- Create: `internal/transport/telegram/error_reporting.go`
- Create: `internal/transport/telegram/error_reporting_test.go`

- [ ] **Step 1: Write the failing tests for error kinds**

```go
func TestClassifyRunErrorReturnsUserVisibleKind(t *testing.T) {}
func TestClassifyTelegramTransportErrorReturnsAdminAlertKind(t *testing.T) {}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run 'TestClassifyRunError|TestClassifyTelegramTransportError' -v`

Expected: FAIL because the error-reporting layer does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Implement:
- error severity enum: `warning`, `run_error`, `system_error`
- stable short `error_id`
- formatter for user message in Russian
- formatter for admin alert with technical payload

- [ ] **Step 4: Run tests to verify they pass**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run 'TestClassifyRunError|TestClassifyTelegramTransportError' -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/error_reporting.go internal/transport/telegram/error_reporting_test.go
git commit -m "feat: add telegram error classification"
```

### Task 2: Surface Request Failures In The Active Chat

**Files:**
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/status_card.go`
- Modify: `internal/transport/telegram/run_state.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing test for user-visible run failure**

```go
func TestAdapterReplyTurnsRunCardIntoErrorStateOnProviderFailure(t *testing.T) {}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestAdapterReplyTurnsRunCardIntoErrorStateOnProviderFailure -v`

Expected: FAIL because the current adapter only returns an error and leaves detail in logs.

- [ ] **Step 3: Write minimal implementation**

Implement:
- run-state fields for `ErrorID`, `ErrorSeverity`, `FailedStage`
- terminal error state in status card
- short user-facing error message with `error_id`
- keep raw stack/transport details out of the user chat

- [ ] **Step 4: Run the test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestAdapterReplyTurnsRunCardIntoErrorStateOnProviderFailure -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/adapter.go internal/transport/telegram/status_card.go internal/transport/telegram/run_state.go internal/transport/telegram/adapter_test.go
git commit -m "feat: surface telegram run failures in chat"
```

### Task 3: Add Admin Alerts For Transport And System Failures

**Files:**
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/config/config.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing tests for admin alert routing**

```go
func TestAdapterSendsAdminAlertWhenTelegramEditFails(t *testing.T) {}
func TestAdapterSendsAdminAlertWhenProviderRequestFailsBeforeReply(t *testing.T) {}
```

- [ ] **Step 2: Run focused tests to verify they fail**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run 'TestAdapterSendsAdminAlertWhenTelegramEditFails|TestAdapterSendsAdminAlertWhenProviderRequestFailsBeforeReply' -v`

Expected: FAIL because no admin alert path exists yet.

- [ ] **Step 3: Write minimal implementation**

Implement:
- optional config `TEAMD_TELEGRAM_ADMIN_CHAT_ID`
- fallback notifier that can send technical alerts outside the active run flow
- admin alert payload with `chat_id`, `run_id`, failing method/stage, provider/Telegram body when available

- [ ] **Step 4: Run focused tests to verify they pass**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run 'TestAdapterSendsAdminAlertWhenTelegramEditFails|TestAdapterSendsAdminAlertWhenProviderRequestFailsBeforeReply' -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/adapter.go internal/config/config.go internal/transport/telegram/adapter_test.go
git commit -m "feat: add telegram admin error alerts"
```

### Task 4: Document Error Policy And Failure Modes

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Write the failing doc checklist**

Checklist:
- user-visible error policy documented
- admin alert routing documented
- env var documented
- examples of `run_error` vs. `system_error`

- [ ] **Step 2: Update the README**

Document:
- which failures appear in the current chat
- which failures are mirrored to admin chat
- how to set `TEAMD_TELEGRAM_ADMIN_CHAT_ID`
- expectation that Telegram-origin failures may only be visible in admin chat if the user chat cannot be updated

- [ ] **Step 3: Sanity-check docs**

Run: `rg -n "TEAMD_TELEGRAM_ADMIN_CHAT_ID|run_error|system_error" README.md`

Expected: matching lines exist and describe the policy accurately.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: describe telegram error surfacing"
```

### Task 5: Final Verification

**Files:**
- Modify: none
- Test: `internal/transport/telegram/...`, `./...`

- [ ] **Step 1: Run focused Telegram tests**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram`

Expected: PASS

- [ ] **Step 2: Run full test suite**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./...`

Expected: PASS

- [ ] **Step 3: Commit final verification if needed**

```bash
git add -A
git commit -m "test: verify telegram error surfacing" || true
```

