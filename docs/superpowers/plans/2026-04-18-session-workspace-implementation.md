# Session Workspace Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a session-bound `Workspace` tab to the clean-room TUI with a persistent PTY terminal, file tree, artifact viewer, jump-ins from `Chat` and `Tools`, and a first-pass built-in editor.

**Architecture:** Extend the existing operator/runtime surface with a session-scoped workspace backend instead of TUI-local hacks. Keep `shell_exec` and PTY separated: agent tools remain agent tools, while the operator terminal becomes a dedicated PTY service exposed through daemon commands and consumed by the TUI. Build the feature in phases so `Terminal` lands first, then `Files` and `Artifacts`, then jump-ins, then `Editor` v1.

**Tech Stack:** Go, Bubble Tea, existing clean-room runtime projections/event log, daemon websocket/http operator surface, PTY support in Go, existing TUI pane architecture in `internal/runtime/tui`.

---

## File Structure

### Existing files to modify

- `internal/runtime/tui/state.go`
  - add `tabWorkspace`
  - add session-bound workspace state structs
- `internal/runtime/tui/app.go`
  - wire `Workspace` tab into top-level routing and sizing
- `internal/runtime/tui/chat_pane.go`
  - add open-in-workspace actions from selected chat items
- `internal/runtime/tui/tools_view.go`
  - add open-in-workspace actions from tool log / approvals / running commands
- `internal/runtime/tui/tools_data.go`
  - parse jump targets such as path, line range, artifact ref, command id
- `internal/runtime/tui/client.go`
  - add operator client methods for workspace PTY/files/artifacts/editor
- `internal/runtime/daemon/commands.go`
  - add daemon commands for workspace operations
- `internal/runtime/daemon/session_snapshot.go`
  - optionally expose lightweight workspace session metadata if needed
- `internal/runtime/daemon/server.go`
  - ensure workspace commands are available over websocket/http operator transport

### New backend files

- `internal/runtime/workspace/pty.go`
  - PTY session manager
- `internal/runtime/workspace/files.go`
  - file tree / read / stat helpers
- `internal/runtime/workspace/artifacts.go`
  - artifact listing and raw viewer helpers
- `internal/runtime/workspace/editor.go`
  - in-memory editor buffers, dirty tracking, save logic
- `internal/runtime/workspace/types.go`
  - shared workspace request/response structs

### New TUI files

- `internal/runtime/tui/workspace_pane.go`
  - `Workspace` tab layout and routing
- `internal/runtime/tui/workspace_terminal.go`
  - PTY viewer/input handling
- `internal/runtime/tui/workspace_files.go`
  - file tree navigator
- `internal/runtime/tui/workspace_artifacts.go`
  - artifact list + raw viewer
- `internal/runtime/tui/workspace_editor.go`
  - editor mode and rendering
- `internal/runtime/tui/workspace_jump.go`
  - jump-in helpers from `Chat` and `Tools`

### Tests

- `internal/runtime/workspace/pty_test.go`
- `internal/runtime/workspace/files_test.go`
- `internal/runtime/workspace/artifacts_test.go`
- `internal/runtime/workspace/editor_test.go`
- `internal/runtime/daemon/server_test.go`
- `internal/runtime/tui/workspace_pane_test.go`
- `internal/runtime/tui/app_test.go`

## Task 1: Add Workspace Tab Scaffold

**Files:**
- Modify: `internal/runtime/tui/state.go`
- Modify: `internal/runtime/tui/app.go`
- Create: `internal/runtime/tui/workspace_pane.go`
- Test: `internal/runtime/tui/workspace_pane_test.go`

- [ ] **Step 1: Write the failing tab-routing test**

Add a test that creates a model, triggers the top-tab rendering, and asserts:
- `Workspace` appears in the top tabs
- `f3/f4/...` ordering is updated consistently
- switching to `tabWorkspace` renders a placeholder view instead of falling through to another pane

- [ ] **Step 2: Run the test to verify it fails**

Run: `go test ./internal/runtime/tui -run TestWorkspaceTabScaffold -count=1`
Expected: FAIL because `tabWorkspace` and its view do not exist yet.

- [ ] **Step 3: Add the minimal scaffold**

Implement:
- new `tabWorkspace` enum in `internal/runtime/tui/state.go`
- top-tab label in `internal/runtime/tui/app.go`
- `viewWorkspace()` and `updateWorkspace(...)` stubs in `internal/runtime/tui/workspace_pane.go`

- [ ] **Step 4: Re-run the test**

Run: `go test ./internal/runtime/tui -run TestWorkspaceTabScaffold -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/tui/state.go internal/runtime/tui/app.go internal/runtime/tui/workspace_pane.go internal/runtime/tui/workspace_pane_test.go
git commit -m "feat: add workspace tab scaffold"
```

## Task 2: Add Session-Bound PTY Backend

**Files:**
- Create: `internal/runtime/workspace/types.go`
- Create: `internal/runtime/workspace/pty.go`
- Test: `internal/runtime/workspace/pty_test.go`

- [ ] **Step 1: Write the failing PTY manager tests**

Add tests for:
- creating one PTY per session
- reopening returns the same PTY id for the same session
- resize updates cols/rows
- input writes to the PTY
- close/reset tears down the PTY cleanly

- [ ] **Step 2: Run the tests to verify they fail**

Run: `go test ./internal/runtime/workspace -run TestPTY -count=1`
Expected: FAIL because the workspace PTY manager does not exist.

- [ ] **Step 3: Implement minimal PTY manager**

Implement:
- `WorkspacePTYManager`
- `Open(sessionID, cols, rows)`
- `Input(ptyID, data)`
- `Resize(ptyID, cols, rows)`
- `Snapshot(sessionID)`
- `Reset(sessionID)`
- `Close(sessionID)`

Use one PTY per session and hold scrollback in memory for snapshotting.

- [ ] **Step 4: Re-run the PTY tests**

Run: `go test ./internal/runtime/workspace -run TestPTY -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/workspace/types.go internal/runtime/workspace/pty.go internal/runtime/workspace/pty_test.go
git commit -m "feat: add session workspace pty backend"
```

## Task 3: Expose PTY Through Daemon Commands

**Files:**
- Modify: `internal/runtime/daemon/commands.go`
- Modify: `internal/runtime/daemon/server.go`
- Test: `internal/runtime/daemon/server_test.go`

- [ ] **Step 1: Write failing daemon command tests**

Add websocket/http daemon tests for:
- `workspace.pty.open`
- `workspace.pty.input`
- `workspace.pty.resize`
- `workspace.pty.snapshot`
- `workspace.pty.reset`

Assert that command payloads are session-scoped and return stable PTY metadata.

- [ ] **Step 2: Run the daemon tests to verify failure**

Run: `go test ./internal/runtime/daemon -run TestWorkspacePTY -count=1`
Expected: FAIL because the daemon does not recognize workspace commands.

- [ ] **Step 3: Wire PTY manager into daemon**

Add command routing in `internal/runtime/daemon/commands.go`.
Keep operator PTY separate from shell tool execution; do not reuse `shell_exec`.

- [ ] **Step 4: Re-run daemon tests**

Run: `go test ./internal/runtime/daemon -run TestWorkspacePTY -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/daemon/commands.go internal/runtime/daemon/server.go internal/runtime/daemon/server_test.go
git commit -m "feat: expose workspace pty commands"
```

## Task 4: Add Terminal Mode To Workspace Pane

**Files:**
- Modify: `internal/runtime/tui/client.go`
- Modify: `internal/runtime/tui/state.go`
- Modify: `internal/runtime/tui/workspace_pane.go`
- Create: `internal/runtime/tui/workspace_terminal.go`
- Test: `internal/runtime/tui/workspace_pane_test.go`

- [ ] **Step 1: Write failing terminal-mode TUI tests**

Add tests for:
- entering `Workspace` defaults to `Terminal`
- first entry opens a PTY for the active session
- terminal input goes to PTY client methods
- switching sessions switches PTY context

- [ ] **Step 2: Run the tests to confirm failure**

Run: `go test ./internal/runtime/tui -run TestWorkspaceTerminal -count=1`
Expected: FAIL because TUI has no workspace terminal mode.

- [ ] **Step 3: Implement terminal mode**

Add:
- workspace navigator state
- terminal mode rendering
- terminal keystroke forwarding
- PTY snapshot refresh path

Keep it minimal: one large terminal pane and a narrow navigator.

- [ ] **Step 4: Re-run the TUI tests**

Run: `go test ./internal/runtime/tui -run TestWorkspaceTerminal -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/tui/client.go internal/runtime/tui/state.go internal/runtime/tui/workspace_pane.go internal/runtime/tui/workspace_terminal.go internal/runtime/tui/workspace_pane_test.go
git commit -m "feat: add workspace terminal mode"
```

## Task 5: Add Files Mode

**Files:**
- Create: `internal/runtime/workspace/files.go`
- Create: `internal/runtime/tui/workspace_files.go`
- Modify: `internal/runtime/tui/client.go`
- Test: `internal/runtime/workspace/files_test.go`
- Test: `internal/runtime/tui/workspace_pane_test.go`

- [ ] **Step 1: Write failing file-tree backend tests**

Cover:
- listing tree nodes from workspace root
- directory expansion
- stat/path metadata
- safe path normalization inside workspace root

- [ ] **Step 2: Run the backend tests**

Run: `go test ./internal/runtime/workspace -run TestFiles -count=1`
Expected: FAIL

- [ ] **Step 3: Implement backend file-tree helpers**

Return structured items:
- `path`
- `name`
- `is_dir`
- `children_loaded`
- `size`
- `mod_time`

- [ ] **Step 4: Write failing TUI tests for Files mode**

Cover:
- switching from terminal to files
- expanding/collapsing directories
- opening a file target into `Editor`

- [ ] **Step 5: Implement Files mode in TUI**

Add navigation keys:
- up/down
- left collapse
- right expand
- enter open
- `/` filter

- [ ] **Step 6: Run both backend and TUI tests**

Run:
- `go test ./internal/runtime/workspace -run TestFiles -count=1`
- `go test ./internal/runtime/tui -run TestWorkspaceFiles -count=1`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add internal/runtime/workspace/files.go internal/runtime/workspace/files_test.go internal/runtime/tui/workspace_files.go internal/runtime/tui/client.go internal/runtime/tui/workspace_pane_test.go
git commit -m "feat: add workspace files mode"
```

## Task 6: Add Artifacts Mode

**Files:**
- Create: `internal/runtime/workspace/artifacts.go`
- Create: `internal/runtime/tui/workspace_artifacts.go`
- Modify: `internal/runtime/tui/client.go`
- Test: `internal/runtime/workspace/artifacts_test.go`
- Test: `internal/runtime/tui/workspace_pane_test.go`

- [ ] **Step 1: Write failing artifact backend tests**

Cover:
- listing artifacts for the current session
- reading raw artifact contents
- searching artifact contents

- [ ] **Step 2: Run backend tests to confirm failure**

Run: `go test ./internal/runtime/workspace -run TestArtifacts -count=1`
Expected: FAIL

- [ ] **Step 3: Implement artifact backend**

Use existing artifact store facilities. Return terminal-style raw text plus metadata.

- [ ] **Step 4: Write failing TUI tests for Artifacts mode**

Cover:
- listing artifacts
- opening the selected artifact in a raw viewer
- clamping the viewer to the pane height

- [ ] **Step 5: Implement Artifacts mode**

No markdown decoration. Keep monospaced raw rendering.

- [ ] **Step 6: Run backend and TUI tests**

Run:
- `go test ./internal/runtime/workspace -run TestArtifacts -count=1`
- `go test ./internal/runtime/tui -run TestWorkspaceArtifacts -count=1`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add internal/runtime/workspace/artifacts.go internal/runtime/workspace/artifacts_test.go internal/runtime/tui/workspace_artifacts.go internal/runtime/tui/client.go internal/runtime/tui/workspace_pane_test.go
git commit -m "feat: add workspace artifacts mode"
```

## Task 7: Add Jump-Ins From Chat And Tools

**Files:**
- Create: `internal/runtime/tui/workspace_jump.go`
- Modify: `internal/runtime/tui/chat_pane.go`
- Modify: `internal/runtime/tui/tools_view.go`
- Modify: `internal/runtime/tui/tools_data.go`
- Test: `internal/runtime/tui/app_test.go`

- [ ] **Step 1: Write failing jump-in tests**

Cover:
- path in chat item opens `Editor`
- path + line range opens `Editor` at the right line
- artifact ref opens `Artifacts`
- `shell_start` / `command_id` opens `Terminal`
- file listing target opens `Files`

- [ ] **Step 2: Run tests to verify failure**

Run: `go test ./internal/runtime/tui -run TestWorkspaceJumpIn -count=1`
Expected: FAIL

- [ ] **Step 3: Implement jump target parsing**

Parse from chat/tool entries:
- path
- line range
- artifact ref
- command id
- cwd

- [ ] **Step 4: Implement `o` / `Enter` jump actions**

Add open-in-workspace behavior from `Chat` and `Tools`.

- [ ] **Step 5: Re-run tests**

Run: `go test ./internal/runtime/tui -run TestWorkspaceJumpIn -count=1`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/runtime/tui/workspace_jump.go internal/runtime/tui/chat_pane.go internal/runtime/tui/tools_view.go internal/runtime/tui/tools_data.go internal/runtime/tui/app_test.go
git commit -m "feat: add workspace jump-ins from chat and tools"
```

## Task 8: Add Editor v1

**Files:**
- Create: `internal/runtime/workspace/editor.go`
- Create: `internal/runtime/tui/workspace_editor.go`
- Modify: `internal/runtime/tui/client.go`
- Test: `internal/runtime/workspace/editor_test.go`
- Test: `internal/runtime/tui/workspace_pane_test.go`

- [ ] **Step 1: Write failing editor backend tests**

Cover:
- open file buffer
- dirty tracking
- save
- reopen same file returns current buffer state
- line-focused open

- [ ] **Step 2: Run backend tests**

Run: `go test ./internal/runtime/workspace -run TestEditor -count=1`
Expected: FAIL

- [ ] **Step 3: Implement editor backend**

Keep it small:
- one buffer per opened file per session
- text content
- dirty bool
- cursor position
- save-to-disk

- [ ] **Step 4: Write failing TUI tests for Editor mode**

Cover:
- file opened from `Files` or jump-in lands in `Editor`
- typing changes buffer
- save clears dirty marker
- status bar shows file path and dirty state

- [ ] **Step 5: Implement Editor mode**

Use a micro-like interaction model, but do not attempt full `micro` parity.

- [ ] **Step 6: Run backend and TUI tests**

Run:
- `go test ./internal/runtime/workspace -run TestEditor -count=1`
- `go test ./internal/runtime/tui -run TestWorkspaceEditor -count=1`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add internal/runtime/workspace/editor.go internal/runtime/workspace/editor_test.go internal/runtime/tui/workspace_editor.go internal/runtime/tui/client.go internal/runtime/tui/workspace_pane_test.go
git commit -m "feat: add workspace editor v1"
```

## Task 9: Add Soft Terminal/Files Cross-Linking

**Files:**
- Modify: `internal/runtime/workspace/pty.go`
- Modify: `internal/runtime/tui/workspace_terminal.go`
- Modify: `internal/runtime/tui/workspace_files.go`
- Test: `internal/runtime/workspace/pty_test.go`
- Test: `internal/runtime/tui/workspace_pane_test.go`

- [ ] **Step 1: Write failing cross-link tests**

Cover:
- terminal cwd changes update `Files` current location softly
- `open shell here` from files updates terminal cwd
- file tree does not steal focus when cwd changes

- [ ] **Step 2: Run tests to verify failure**

Run: `go test ./internal/runtime/tui -run TestWorkspaceTerminalFilesSync -count=1`
Expected: FAIL

- [ ] **Step 3: Implement soft sync**

Add:
- current-cwd tracking on PTY snapshots
- file-tree current path highlighting
- explicit "shell here" action from files

- [ ] **Step 4: Re-run tests**

Run: `go test ./internal/runtime/tui -run TestWorkspaceTerminalFilesSync -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/workspace/pty.go internal/runtime/tui/workspace_terminal.go internal/runtime/tui/workspace_files.go internal/runtime/workspace/pty_test.go internal/runtime/tui/workspace_pane_test.go
git commit -m "feat: sync workspace terminal and files"
```

## Task 10: Final Verification And Cleanup

**Files:**
- Modify only as needed if verification reveals issues

- [ ] **Step 1: Run focused workspace-related tests**

Run:
- `go test ./internal/runtime/workspace/... -count=1`
- `go test ./internal/runtime/tui -count=1`
- `go test ./internal/runtime/daemon -count=1`

Expected: PASS

- [ ] **Step 2: Run full test suite**

Run: `go test ./... -count=1`
Expected: PASS

- [ ] **Step 3: Build the agent binary**

Run: `go build ./cmd/agent`
Expected: success with no compile errors

- [ ] **Step 4: Manual operator smoke test**

Run the TUI and verify:
- `Workspace` opens
- PTY terminal starts and accepts input
- `micro` can be launched from the terminal
- file tree opens a file
- artifact viewer opens large output
- `Chat` and `Tools` jump into workspace targets

- [ ] **Step 5: Final integration commit**

```bash
git add -A
git commit -m "feat: add session-bound workspace operator surface"
```
