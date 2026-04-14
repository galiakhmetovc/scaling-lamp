# Telegram Rich Text And Table Rendering Debugging Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Diagnose and fix the mismatch between locally tested Telegram rich-text/table formatting and what actually renders in the live Telegram chat.

**Architecture:** Treat this as a tracing/debugging slice, not a presentation rewrite. Instrument the Telegram adapter so one run can capture four artifacts for the same reply: raw provider text, post-formatting text, exact Telegram API payload including `parse_mode`, and the effective message returned/rendered by Telegram. Only after the pipeline is observable should presentation code be adjusted.

**Tech Stack:** Go 1.24+, existing Telegram adapter, existing formatter, Telegram Bot API `sendMessage`, existing tests, local debug artifacts/logging.

---

## File Structure

- Modify: `internal/transport/telegram/adapter.go`
  Purpose: add bounded debug logging/artifact capture around final reply formatting and Telegram send path.
- Modify: `internal/transport/telegram/formatting.go`
  Purpose: only if trace evidence shows formatter/output mismatch; no speculative renderer changes before evidence exists.
- Modify: `internal/transport/telegram/adapter_test.go`
  Purpose: add one regression that asserts exact `text` plus `parse_mode` payload for final reply sending.
- Create: `memory/artifacts/telegram-rich-text-debug-<timestamp>.md`
  Purpose: store one captured end-to-end example with raw provider reply, formatted reply, outgoing payload, and observed Telegram result.
- Modify: `README.md`
  Purpose: document the chosen Telegram rendering policy only after the pipeline is verified.

---

### Task 1: Capture The Exact Final Reply Pipeline

**Files:**
- Modify: `internal/transport/telegram/adapter.go`
- Create: `memory/artifacts/telegram-rich-text-debug-<timestamp>.md`

- [ ] **Step 1: Add bounded debug capture hooks**

Capture for one targeted reply:
- raw `provider.PromptResponse.Text`
- `a.formatReply(...)` output
- `parse_mode`
- exact form body sent to `sendMessage`

- [ ] **Step 2: Reproduce the live failure with one stable prompt**

Suggested prompt: `какие инструменты тебе доступны? таблицей напиши`

Expected: live Telegram output still mismatches local expectation.

- [ ] **Step 3: Save the captured evidence**

Write one artifact in:
`memory/artifacts/telegram-rich-text-debug-<timestamp>.md`

Include:
- raw provider text
- formatted text
- payload fields
- screenshot/transcribed Telegram render

- [ ] **Step 4: Commit instrumentation only if still useful**

```bash
git add internal/transport/telegram/adapter.go memory/artifacts/telegram-rich-text-debug-<timestamp>.md
git commit -m "chore: capture telegram rich text rendering trace"
```

### Task 2: Identify The Failing Boundary

**Files:**
- Modify: none initially

- [ ] **Step 1: Classify which boundary is wrong**

Possible outcomes:
- provider already returned literal `*...*`
- formatter failed to convert to expected HTML/plain text
- adapter sent wrong `parse_mode`
- Telegram ignored formatting because payload shape/escaping was wrong
- a different process/build served Telegram than the tested code

- [ ] **Step 2: Record the root cause hypothesis**

Write one concise hypothesis in the artifact:
`I think X is the root cause because Y`.

- [ ] **Step 3: Verify the hypothesis with one minimal check**

Examples:
- compare outgoing payload to expected text
- inspect single-process ownership of polling
- send a known hardcoded HTML probe through the same path

### Task 3: Write A Targeted Regression Test

**Files:**
- Modify: `internal/transport/telegram/adapter_test.go`
- Modify: `internal/transport/telegram/formatting_test.go`

- [ ] **Step 1: Add one failing test for the confirmed root cause**

Examples:
- final reply payload must include `parse_mode=HTML`
- table cell values wrapped in `*...*` must render to `<b>...</b>`
- final table renderer must not preserve raw `*...*` markers in output

- [ ] **Step 2: Run the focused tests to verify they fail**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run '<TargetedTestName>' -v`

Expected: FAIL for the exact confirmed mismatch.

- [ ] **Step 3: Implement the minimal fix**

Only change the failing boundary identified in Task 2.

- [ ] **Step 4: Re-run focused tests**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run '<TargetedTestName>' -v`

Expected: PASS

### Task 4: Verify In Live Telegram

**Files:**
- Modify: none

- [ ] **Step 1: Ensure only one coordinator process is polling Telegram**

Run: `ps -ef | rg 'go run ./cmd/coordinator|cmd/coordinator'`

Expected: exactly one live poller for the bot token.

- [ ] **Step 2: Restart the live bot**

Run from the runtime worktree:
```bash
bash -lc 'set -a; source .env; set +a; export GOTMPDIR=$PWD/.tmp/go; go run ./cmd/coordinator'
```

- [ ] **Step 3: Re-run the same Telegram prompt**

Prompt: `какие инструменты тебе доступны? таблицей напиши`

Expected: Telegram output now matches the traced local expectation.

- [ ] **Step 4: Save the successful before/after example**

Append the final observed Telegram render to the artifact file.

### Task 5: Final Verification And Policy Freeze

**Files:**
- Modify: `README.md` if and only if the rendering policy is now stable

- [ ] **Step 1: Run package tests**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram`

Expected: PASS

- [ ] **Step 2: Run full suite**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./...`

Expected: PASS

- [ ] **Step 3: Document the final rendering policy**

Document only verified rules:
- when table reshaping is applied
- whether `HTML` parse mode is used
- what inline formatting is preserved vs. flattened
- what was intentionally rejected

