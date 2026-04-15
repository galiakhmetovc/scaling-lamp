# TUI Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decompose the TUI into pane-focused modules and polish pane UX without rewriting the runtime model.

**Architecture:** Keep the current runtime contracts, projections, UI bus, and session-scoped plan model intact. Move pane behavior into focused files with explicit local state and commands, then improve selection, scrolling, focus, and detail rendering within those boundaries.

**Tech Stack:** Go, Bubble Tea, Bubbles (`viewport`, `textarea`, `textinput`), Lip Gloss, Glamour, existing runtime projections and operator methods.

---

### Task 1: Split Shared TUI State And Commands

**Files:**
- Create: `internal/runtime/tui/state.go`
- Create: `internal/runtime/tui/commands.go`
- Modify: `internal/runtime/tui/app.go`
- Test: `internal/runtime/tui/app_test.go`

- [ ] **Step 1: Write or adapt a failing test that still builds the TUI model after state extraction**
- [ ] **Step 2: Run `go test ./internal/runtime/tui -count=1` and confirm failure**
- [ ] **Step 3: Move shared structs and async commands out of `app.go`**
- [ ] **Step 4: Re-run `go test ./internal/runtime/tui -count=1`**
- [ ] **Step 5: Commit**

### Task 2: Extract Sessions And Chat Panes

**Files:**
- Create: `internal/runtime/tui/sessions_pane.go`
- Create: `internal/runtime/tui/chat_pane.go`
- Create: `internal/runtime/tui/render_markdown.go`
- Modify: `internal/runtime/tui/app.go`
- Test: `internal/runtime/tui/app_test.go`

- [ ] **Step 1: Add/adjust failing tests for session activation and chat timeline rendering**
- [ ] **Step 2: Run focused tests and confirm failure**
- [ ] **Step 3: Move sessions and chat rendering/update logic into pane files**
- [ ] **Step 4: Preserve current behavior: timeline rendering, streaming, chat input**
- [ ] **Step 5: Re-run focused tests**
- [ ] **Step 6: Commit**

### Task 3: Extract Plan Pane And Polish Editor UX

**Files:**
- Create: `internal/runtime/tui/plan_pane.go`
- Modify: `internal/runtime/tui/app.go`
- Test: `internal/runtime/tui/app_test.go`
- Test: `internal/runtime/plan_operator_test.go`

- [ ] **Step 1: Add/adjust failing tests for plan selection and form actions**
- [ ] **Step 2: Run focused tests and confirm failure**
- [ ] **Step 3: Move plan rendering/editor logic into `plan_pane.go`**
- [ ] **Step 4: Improve selection/edit feedback without changing domain writes**
- [ ] **Step 5: Re-run focused tests**
- [ ] **Step 6: Commit**

### Task 4: Extract Tools And Settings Panes

**Files:**
- Create: `internal/runtime/tui/tools_pane.go`
- Create: `internal/runtime/tui/settings_pane.go`
- Modify: `internal/runtime/tui/app.go`
- Test: `internal/runtime/tui/app_test.go`

- [ ] **Step 1: Add/adjust failing tests for tools pane and settings mode behavior**
- [ ] **Step 2: Run focused tests and confirm failure**
- [ ] **Step 3: Move tools/settings update and render logic into pane files**
- [ ] **Step 4: Add tools details view and stable settings pane scrolling**
- [ ] **Step 5: Re-run focused tests**
- [ ] **Step 6: Commit**

### Task 5: Full Verification And Docs

**Files:**
- Modify: `docs/clean-room-tui.md`
- Modify: `docs/clean-room-cli-chat.md`
- Modify: `docs/clean-room-current-system-detailed.md`
- Modify: `README.md`

- [ ] **Step 1: Update docs for the decomposed TUI structure and UX improvements**
- [ ] **Step 2: Run full verification**
- [ ] **Step 3: Build fresh binary**
- [ ] **Step 4: Clean temp artifacts**
- [ ] **Step 5: Commit**
