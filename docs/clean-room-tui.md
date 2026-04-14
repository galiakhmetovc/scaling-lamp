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
  - `Ctrl+S` sends the current multiline prompt
  - assistant text streams through the runtime UI bus
  - final assistant messages may render as terminal markdown
- `Plan`
  - shows the current active session `plan_head` projection
- `Tools`
  - shows tool lifecycle events for the active session
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

Compatibility rule:

- real terminal / TTY:
  - `--chat` runs the TUI
- non-interactive stdin:
  - `--chat` falls back to the legacy line-based chat loop

That fallback exists to keep tests and scripted stdin workflows working while the TUI is the main interactive surface.
