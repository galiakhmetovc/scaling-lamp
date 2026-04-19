# Chat-First Terminal UI Design

## Goal

Add a real terminal UI on top of the existing canonical Rust chat runtime so
the operator can work in a full-screen chat experience instead of the current
line-oriented REPL.

This slice must stay chat-first, must not introduce a second runtime path, and
 must not block future work on compaction, offloading, session head, VFS system
prompting, or planning tools.

## Scope

This slice adds:

- a full-screen terminal UI entrypoint in `agentd`
- a dedicated session screen
- a full-screen chat workspace
- lazy scrollback for the current chat transcript
- streaming assistant text in the chat view
- inline tool and approval entries in the same chat timeline
- session commands for switching, creating, renaming, clearing, and approving
- lightweight session metadata in the top bar

This slice does not add:

- autonomous daemon execution
- compactification machinery
- offloading
- session head
- VFS-backed system prompting
- planning tools or planning UI
- a separate approval popup or modal approval workflow
- a web UI

## User Experience

The TUI is screen-based, not split-pane by default.

There are two primary screens:

- `SessionScreen`
- `ChatScreen`

The operator experience should feel like:

1. Open the TUI.
2. Enter or select a session.
3. Work inside a full-screen chat.
4. Use `/session` to jump to the session screen when needed.
5. Return to the same chat state with `Esc`.

The TUI remains command-driven inside the input line instead of relying on a
large permanent menu surface.

## Screen Model

### SessionScreen

The session screen is a dedicated full-screen surface, not an overlay.

It shows:

- a scrollable list of sessions
- session title
- last activity time
- last message preview
- whether the session has a pending approval

It supports:

- selecting an existing session
- creating a new session
- deleting a session

Behavior:

- `Enter` opens the selected session
- `N` opens a create-session input flow
- `D` opens delete confirmation for the selected session
- `Esc` returns to the previous chat screen if one exists

### ChatScreen

The chat screen is the main working surface.

It contains:

- a top status bar
- one lazy scrollable chat timeline
- an input line at the bottom

The timeline is the single main surface. There is no separate transcript viewer
in the first slice.

## Chat Timeline

The timeline renders all activity in one place.

Timeline entry types:

- user messages
- assistant messages
- streamed assistant deltas
- reasoning lines when enabled
- tool status entries
- approval waiting/completed entries
- system-like command feedback for session actions

Every rendered entry must have a timestamp, including:

- user
- assistant
- reasoning
- tool
- approval

Tool activity must stay compact:

- one tool step uses one live-updating timeline entry
- the entry updates through status transitions instead of producing a burst of
  log rows
- the final status remains visible after completion or failure

## Top Bar

The top bar must show lightweight live session metadata:

- current session title
- current model
- reasoning visibility on/off
- think level
- context tokens
- compactifications count
- message count

`context tokens` in the first slice may be approximate or based on the best
known provider/runtime value rather than a universal exact count.

`compactifications count` is display-only in this slice; real compaction logic
is deferred.

## Commands

The first TUI slice supports these chat commands:

- `/session`
- `/new`
- `/rename`
- `/clear`
- `/approve`
- `/approve <approval-id>`
- `/model <name>`
- `/reasoning on`
- `/reasoning off`
- `/think <level>`
- `/compact`
- `/exit`

### Command Semantics

#### `/session`

Opens `SessionScreen`.

The current chat stays alive in memory. Returning from `SessionScreen` must
bring the operator back to the same chat state.

#### `/new`

Creates a new session immediately and switches the chat workspace into it.

#### `/rename`

Renames the current session through a small input dialog.

#### `/clear`

This is destructive and must require confirmation.

On confirmation:

- delete the current session
- create a new empty session immediately
- switch the chat workspace into that new session

`/clear` is therefore not transcript truncation; it is session replacement.

#### `/approve`

Approves the latest pending approval for the current session.

#### `/approve <approval-id>`

Approves a specific approval explicitly.

There is no dedicated approval menu in the first TUI slice. Approval remains a
command-driven action rendered through the timeline.

#### `/model <name>`

Updates the current session's model selection.

#### `/reasoning on|off`

Controls whether reasoning entries are shown in the timeline.

This is a UI/session behavior toggle, not a second provider path.

#### `/think <level>`

Updates the current session's reasoning level.

#### `/compact`

This only invokes a backend hook or placeholder action in the first slice.
Real compaction behavior is deferred to a later phase.

#### `/exit`

Exits the TUI cleanly.

## Dialogs

The first slice uses small shared dialogs rather than building a large modal
system.

### InputDialog

Used for:

- create session
- rename session

### ConfirmDialog

Used for:

- delete session from `SessionScreen`
- `/clear`

Both destructive actions require confirmation.

## State Model

The TUI must not invent a second chat truth.

TUI-local state is limited to UI concerns:

- active screen
- current session id
- previous session id
- selected session row
- input buffer
- scroll offset
- dialog state

Canonical runtime state continues to live in the existing runtime/store path:

- transcript
- run state
- tool state
- approval state
- provider output
- session metadata

## Data Flow

The TUI should reuse the existing `agentd` application boundary rather than
calling providers or stores ad hoc.

Recommended shape:

- TUI sends a command or chat input
- the existing execution layer performs the action
- typed execution events stream back into the TUI
- final persisted state is then read from canonical storage

This preserves one runtime path for:

- `chat send`
- `chat repl`
- future TUI chat

The TUI is only a renderer plus input surface.

## Streaming Behavior

Assistant text must stream as deltas into the timeline.

Reasoning, when enabled, appears as its own timestamped timeline entry type
rather than being merged into assistant text.

Tool state is rendered inline in the timeline, not in a permanent side panel.

Approvals are also rendered inline in the timeline.

## Deferred Work

The TUI design must leave space for later additions, but those additions are
explicitly out of scope for this slice:

- true compaction engine
- offloading
- session head
- VFS-backed system prompt composition
- planning tools and planning UI
- approval menu
- autonomous execution UI

## Testing

Required coverage for this slice:

- session screen lists and opens sessions
- session screen creates a new session
- session screen deletes a session after confirmation
- `/new` creates and switches immediately
- `/rename` updates current session title
- `/clear` confirms, deletes current session, creates a new session, and
  switches into it
- `Esc` from `SessionScreen` returns to the previous chat state
- assistant text streams in the chat timeline
- reasoning visibility toggle works
- tool status updates one timeline entry instead of spamming multiple lines
- `/approve` approves the latest pending approval
- `/approve <approval-id>` overrides the default target
- `/model` and `/think` update current session settings
- all timeline entry types render timestamps

## Summary

This TUI slice should feel like a real chat workspace, not a dashboard.

The main idea is simple:

- dedicated session screen when needed
- full-screen chat by default
- one chat timeline for everything
- command-driven operations
- one canonical runtime under the UI
