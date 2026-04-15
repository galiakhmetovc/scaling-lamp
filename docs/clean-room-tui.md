# Clean-Room TUI

`--chat` now launches the terminal workspace UI when stdin is a real TTY.

Top-level tabs:

- `Sessions`
- `Chat`
- `Plan`
- `Tools`
- `Settings`

Current behavior:

- `Sessions`
  - shows persisted sessions from `session_catalog`
  - `n` creates a new session draft
  - `Enter` activates the selected session
- `Chat`
  - uses the active session
  - `Enter` sends the current prompt immediately when the main run is idle
  - `Enter` queues the current prompt when the main run is still active, and the queued draft is sent as early as possible after completion
  - `Tab` stages the current draft into the queue, or recalls the selected queued draft back into the editor when the input is empty
  - `Alt+Up` / `Alt+Down` move the queued-draft cursor
  - `Shift+Enter` keeps multiline editing behavior
  - assistant text streams through the runtime UI bus
  - chat history is rendered as a markdown timeline for the active session
  - tool and plan activity is persisted into that timeline as markdown-rendered blocks
  - final assistant messages render as terminal markdown
  - `/btw <question>` runs a separate no-tools side query against a snapshot of the current session context and renders its answer in a separate branch block inside `Chat`
  - a session-local status bar is shown directly under the input and includes provider, model, wall-clock time, main-run elapsed timer, approximate context tokens, queue length, and active `/btw` count
- `Plan`
  - shows the current active session `plan_head` projection
  - renders the browse/details view through terminal markdown instead of raw plain text
  - supports form-based editing for plan/task operations
  - selected task details include computed state and latest notes
- `Tools`
  - shows tool lifecycle events for the active session
  - shows pending shell approvals before normal tool log entries
  - shows active running shell commands before the normal tool log
  - includes a selected-entry details pane with summarized args/result/error text
  - `a` approves the selected pending shell command
  - `x` denies the selected pending shell command
  - `k` requests kill for the selected running shell command
- `Settings`
  - `Session Overrides`
  - `Config Form`
  - `Raw YAML`

Current settings behavior:

- `Session Overrides`
  - live-only
  - affects the active session runtime behavior
- `Config Form`
  - edits known knobs:
    - `max_tool_rounds`
    - `render_markdown`
    - `markdown_style`
    - `show_tool_calls`
    - `show_tool_results`
    - `show_plan_after_plan_tools`
  - `Ctrl+S` saves to disk
  - `Ctrl+A` saves and rebuilds the agent
  - `r` resets the form back to the loaded config values
- `Raw YAML`
  - enumerates YAML files under the current config root
  - edits raw file content directly
  - `Ctrl+S` saves
  - `Ctrl+A` saves and rebuilds the agent

Current runtime architecture:

- persistent state:
  - event log
  - projections
  - `session_catalog`
  - `transcript`
  - session-scoped `plan_head`
- ephemeral UI state:
  - runtime UI bus
  - streamed text deltas
  - tool lifecycle events
  - turn status changes

Current interaction notes:

- `Chat`, `Plan`, `Tools`, and settings forms use pane-local scrolling
- mouse wheel scrolling is wired for `Chat`, `Plan`, `Tools`, and `Settings`
- queued drafts stay editable while the main run is active because the chat input is no longer blocked during a provider turn
- `F6` toggles mouse capture:
  - `Mouse: on` keeps wheel and click handling inside the TUI and uses alt-screen
  - `Mouse: off` disables capture and exits alt-screen so the terminal can do native text selection
- the TUI code is decomposed into pane-focused modules under `internal/runtime/tui`
- `Plan` keeps the full task tree out of the main chat timeline

Compatibility rule:

- real terminal / TTY:
  - `--chat` runs the TUI
- non-interactive stdin:
  - `--chat` falls back to the legacy line-based chat loop

That fallback exists to keep tests and scripted stdin workflows working while the TUI is the main interactive surface.
