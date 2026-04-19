# Session Head Prompt Assembly Design

## Goal

Add the next canonical knowledge-layer slice on top of real context compaction:

- derive one canonical `SessionHead` from current session state
- route all prompt shaping through one `PromptAssembly` path
- make chat execution prepend the session head before the compact summary and
  uncovered raw transcript tail

This slice turns the current compacted prompt path into a proper prompt
assembly path without introducing system-prompt files, VFS sections, or
planning sections yet.

## Non-Goals

- no file-backed system prompt in this slice
- no VFS-backed workspace prompt sections yet
- no planning projection inside the session head yet
- no additional persistence table for session head
- no second prompt path for TUI versus chat/runtime

## Runtime Shape

Add a derived `SessionHead`.

`SessionHead` is canonical runtime state, but it is not persisted directly.
It is rebuilt from canonical persisted state:

- `Session`
- `ContextSummary`
- transcript
- pending approvals / run state

The head should stay compact and operational.

## Session Head Fields

First-phase fields:

- `session_id`
- `title`
- `message_count`
- `context_tokens`
- `compactifications`
- `summary_covered_message_count`
- `pending_approval_count`
- `last_user_preview`
- `last_assistant_preview`

The rendered head should be one synthetic `system` message.

## Prompt Assembly Order

The canonical message order becomes:

1. `SessionHead` synthetic system message
2. compact summary synthetic system message, if a `ContextSummary` exists
3. only uncovered raw transcript messages

`prompt_override` remains separate request-level instructions for now.

This means prompt assembly owns:

- session head rendering
- compact summary message injection
- trimming the covered raw transcript prefix

Execution code should stop assembling these pieces ad hoc.

## Rendering Rules

The session head should be concise, line-oriented, and stable enough for tests.

Suggested first-phase lines:

- `Session: <title>`
- `Session ID: <session_id>`
- `Messages: <count>`
- `Context Tokens: <estimate>`
- `Compactifications: <count>`
- `Summary Covers: <count> messages` when summary exists
- `Pending Approvals: <count>` when non-zero
- `Last User: <preview>` when present
- `Last Assistant: <preview>` when present

The head must not inline the full compact summary text. That remains its own
prompt layer.

## App Surface

Expose a canonical `App::session_head(session_id)` so the derived prompt-facing
state can be inspected without re-deriving it in tests or future UI work.

## Testing

Required coverage:

- canonical session head builds from session, summary, transcript, and pending
  approvals
- prompt assembly keeps the order `session head -> compact summary ->
  uncovered transcript`
- chat execution uses the assembly output instead of the previous ad hoc
  compact-summary-only path
