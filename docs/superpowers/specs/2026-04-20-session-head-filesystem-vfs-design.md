# Session Head Filesystem And VFS Context Design

## Goal

Add the next canonical knowledge-layer slice on top of `SessionHead` and prompt
assembly:

- derive a bounded `Recent Filesystem Activity` section from canonical run/tool
  state
- derive a shallow `Workspace Tree` section from the canonical workspace root
- inject both sections into the existing `SessionHead` synthetic system message

This slice extends the existing prompt path. It must not create a second prompt
assembly flow for chat, TUI, or future autonomous execution.

## Non-Goals

- no file contents in the session head
- no recursive tree rendering
- no separate persisted VFS projection or database table
- no transcript scraping for filesystem context
- no planning sections in this slice

## Canonical Source Of Truth

`SessionHead` remains a derived runtime structure rebuilt from canonical state.

For this slice the relevant sources are:

- `Session`
- `ContextSummary`
- transcript tail metadata already used by the session head
- `RunSnapshot.recent_steps` for recent filesystem activity
- `WorkspaceRef` for the shallow workspace tree

The design intentionally keeps filesystem context derived, not stored.

## Runtime Shape

Extend `SessionHead` with two bounded sections:

1. `recent_filesystem_activity`
2. `workspace_tree`

The recommended runtime types are:

- `SessionHeadFsActivity`
  - `action`
  - `target`
  - `detail`
  - `recorded_at`
- `SessionHeadWorkspaceEntry`
  - `path`
  - `kind`

The head continues to render as one synthetic `system` message.

## Recent Filesystem Activity

Recent filesystem activity should be derived only from run steps that describe
completed filesystem tool executions.

This requires tool-completion run-step detail to preserve both:

- the tool intent, for example `fs_list path=. recursive=false`
- the completion outcome, for example `fs_list entries=5`

The stable form should be one compact detail string:

`<tool call summary> -> <tool output summary>`

Examples:

- `fs_read path=.env -> fs_read path=.env bytes=42`
- `fs_patch path=src/main.rs edits=1 -> fs_patch path=src/main.rs edits=1`
- `fs_list path=. recursive=false -> fs_list entries=12`

`SessionHead` should parse only filesystem tool-completion steps from the
current session's runs and keep a small newest-first bounded list.

## Workspace Tree

The workspace section should be a shallow root listing only:

- one level deep
- sorted by path
- no file contents
- no recursion
- no secondary filtering logic in this slice

This uses the canonical workspace root already owned by the runtime/app layer.

The rendered tree should be capped to a small number of entries, with an
overflow line when truncated.

## Rendering Rules

Append two optional sections to `SessionHead::render()`:

- `Recent Filesystem Activity:`
- `Workspace Tree:`

Suggested lines:

- `- read .env`
- `- list .`
- `- patch src/main.rs`
- `- search src/`
- `- cmd/`
- `- crates/`
- `- README.md`

If either section is empty, omit it entirely.

The rendering must stay stable and compact enough for direct assertions in unit
tests.

## App And Execution Surface

`App::session_head(session_id)` must derive these sections through the same
canonical app-layer builder used for the existing session head.

`ExecutionService::prompt_messages()` must continue to build provider messages
only through:

1. `SessionHead`
2. `ContextSummary`
3. uncovered transcript tail

This slice only enriches the session head content.

## Testing

Required coverage:

- `SessionHead::render()` includes bounded filesystem and workspace sections
- `App::session_head(session_id)` derives filesystem activity from run steps and
  workspace entries from the configured workspace root
- chat prompt assembly includes the rendered filesystem/VFS session head in the
  same canonical first `system` message
