# Session Workspace Operator Surface Design

## Goal

Add a `Workspace` surface to the clean-room TUI so the operator can work inside the active session without dropping out to a separate terminal or editor.

This workspace must support:

- a full interactive terminal
- browsing files and directories
- editing files quickly
- viewing artifacts and tool outputs as raw terminal-style content
- jumping into the workspace from `Chat` and `Tools`

The design target is not "more tabs". The design target is one session-bound operator workspace that other panes can open into.

## Why This Exists

The current TUI already supports:

- chat
- plans
- tool activity
- session metadata
- prompt control

What it does not support well is operator execution work:

- running arbitrary terminal commands directly
- inspecting the repository as a file tree
- making quick file edits without leaving the operator surface
- reading offloaded artifacts and large tool outputs in a terminal-like viewer
- jumping from a tool event or chat line directly into the relevant file, artifact, or command context

Without this, the operator surface is useful for supervision, but weak as a working environment.

## Core Principle

The workspace is session-bound.

Each active chat session gets its own workspace state:

- one PTY terminal
- one file-browser state
- one editor state
- one artifacts viewer state

Switching sessions switches workspace context.

This is intentionally different from a global terminal or a global editor. The workspace should follow the same session model as the rest of the TUI.

## Recommended Layout

Add a new top-level tab:

- `Workspace`

Inside `Workspace`, use a stable two-region layout:

- left: narrow navigator rail
- right: one large main pane

The navigator contains:

- `Terminal`
- `Files`
- `Editor`
- `Artifacts`

The main pane shows only one active mode at a time.

This matches the operator preference for one large working area rather than a permanent split screen.

## Workspace Modes

### Terminal

`Terminal` is the primary operator execution mode.

It uses a full PTY, not `shell_exec` and not `shell_start/poll`.

Requirements:

- one persistent PTY per session
- raw byte input/output
- resize support
- scrollback
- explicit reset/close behavior
- no agent approval flow
- no agent shell restrictions

This is an operator-owned terminal surface. It is not part of the model tool loop.

This lets the operator run real terminal programs such as:

- `micro`
- `vim`
- `htop`
- `git`
- `go test`

The first-time experience should open `Workspace` into `Terminal` by default.

### Files

`Files` is a repo tree browser anchored to the current session workspace root.

It should support:

- expand/collapse directories
- open file in editor
- open directory as current selection
- refresh
- lightweight name filter

The file tree should also be aware of the terminal `cwd`.

If the terminal changes directory, `Files` should softly track the new location:

- highlight the current folder
- avoid aggressive re-expansion or focus stealing

### Editor

`Editor` is a built-in micro-like editor for quick work.

It should not attempt to embed the external `micro` editor.

That is deliberate:

- the operator can already run `micro` inside the PTY
- embedding `micro` as a foreign PTY app would weaken integration with the rest of the workspace

The built-in editor should instead target a micro-like workflow:

- open file
- cursor movement
- scrolling
- insert/delete
- save
- dirty marker
- basic search
- line-aware opening

This gives a fast native path for common edits, while the PTY remains the escape hatch for full editor workflows.

### Artifacts

`Artifacts` is a raw viewer for:

- stored artifacts
- offloaded outputs
- tool result payloads that are too large for inline display

This should render in a terminal-style way:

- monospaced
- scrollable
- searchable
- minimal markdown transformation

The important rule is that artifacts and tool outputs should be readable as operational text, not as decorative UI cards.

## Jump-In Model

This is the most important part of the design.

`Workspace` is not a disconnected tab. Other panes must be able to open directly into it.

### From Chat

If a chat item contains:

- a path
- a path and line range
- an `artifact_ref`
- a `command_id`

the operator should be able to press a single action such as `o` and open the relevant workspace target.

Examples:

- `app.go:10-40` -> `Editor` focused near line 10
- `artifact://...` -> `Artifacts`
- command output associated with a shell command -> `Terminal`

### From Tools

This is the strongest jump-in path.

Examples:

- `fs_read_lines app.go:10-40` -> `Editor`
- `fs_list src/` -> `Files` on `src/`
- `shell_start apt update` -> `Terminal`
- offloaded tool result -> `Artifacts`

The operator should not need to mentally re-locate context. The selected tool row should open the relevant workspace mode directly.

### From Other Panes

These are secondary, but still useful:

- `Plan` can open path-like references from task notes
- `Head` can open known filesystem or artifact paths when present

The first rollout should prioritize `Chat` and `Tools`.

## Runtime Model

This should be implemented as operator/runtime capabilities, not as TUI-local hacks.

Recommended command families:

- `workspace.pty.open`
- `workspace.pty.input`
- `workspace.pty.resize`
- `workspace.pty.snapshot`
- `workspace.pty.reset`
- `workspace.pty.close`

- `workspace.fs.tree`
- `workspace.fs.read`
- `workspace.fs.stat`
- `workspace.fs.search`

- `workspace.editor.open`
- `workspace.editor.save`
- `workspace.editor.search`
- `workspace.editor.close`

- `workspace.artifacts.list`
- `workspace.artifacts.read`
- `workspace.artifacts.search`

The TUI should stay a thin client over this session-bound operator backend.

That is the key to making the same model usable later in the web operator surface.

## TUI Navigation

Top-level tabs become:

- `Sessions`
- `Chat`
- `Workspace`
- `Head`
- `Prompt`
- `Plan`
- `Tools`
- `Settings`

Inside `Workspace`:

- navigator rail selects mode
- main pane shows the active mode

Suggested hotkeys:

- `1` terminal
- `2` files
- `3` editor
- `4` artifacts
- `Tab` and `Shift-Tab` move focus between navigator and main pane
- `o` from `Chat` and `Tools` opens selected item in `Workspace`

The operator should never lose the current session by moving into the workspace.

## Rollout Plan

### Phase 1

- add `Workspace` tab scaffold
- add per-session PTY runtime
- make `Terminal` the default workspace mode

This phase already gives immediate value because the operator can run `micro` or any other terminal program.

### Phase 2

- add `Files`
- add `Artifacts`
- add jump-ins from `Chat` and `Tools`

This makes the workspace coherent and connected to the rest of the TUI.

### Phase 3

- add built-in `Editor` v1
- support open-file, save, search, line-focused navigation

### Phase 4

- refine cross-links
- sync terminal cwd to file tree softly
- add richer artifact navigation and source-path linking

## Constraints

- keep existing session model intact
- do not collapse agent tool execution and operator PTY into one abstraction
- do not reuse `shell_exec` as the operator terminal transport
- do not gate operator PTY through the agent approval flow
- keep the first rollout centered on actual usability, not editor perfection

## Risks

### PTY Complexity

A real PTY brings platform and resize complexity.

This is acceptable because it unlocks the highest-value behavior first.

### Scope Growth

This can easily turn into "build a full terminal IDE".

The phase structure above is there specifically to stop that from happening.

### Cross-Link Fragility

Jump-ins from `Chat` and `Tools` require stable references:

- paths
- line numbers
- artifact refs
- command ids

Those references must stay explicit in the TUI/runtime data model.

## Out Of Scope

Not in the first workspace slice:

- multiple terminal tabs per session
- full `micro` feature parity in the built-in editor
- global workspace independent of sessions
- replacing existing `Chat` or `Tools` tabs
- merging operator PTY and agent shell tool execution
