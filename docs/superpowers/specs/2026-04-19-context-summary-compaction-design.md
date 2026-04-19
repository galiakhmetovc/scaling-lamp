# Context Summary Compaction Design

## Goal

Replace the current `/compact` placeholder with a real runtime-owned compaction
path that:

- materializes a bounded summary for a session
- persists that summary canonically
- records how much raw transcript history the summary covers
- makes future chat turns use the summary plus only the uncovered raw messages

This is the first slice of the broader knowledge layer. It is intentionally
limited to context summary compaction. It does not yet add the full session head
or VFS-backed prompt context.

## Non-Goals

- no event-sourced compaction history
- no UI-local summary state
- no transcript mutation or destructive transcript rewriting
- no planner or filesystem sections in the prompt yet
- no offloading to external storage in this slice

## Runtime Shape

Add one canonical `ContextSummary` per session.

The summary stores:

- `session_id`
- `summary_text`
- `covered_message_count`
- `summary_token_estimate`
- `updated_at`

The summary is not part of transcript history. Transcript remains the append-only
raw chronology. The summary is operational state used to shape future provider
requests.

## Compaction Semantics

`compact_session(session_id)`:

1. reads the raw transcript in chronological order
2. chooses a bounded prefix to summarize
3. keeps the most recent raw messages verbatim
4. asks the configured provider for a rolling summary of the covered prefix
5. persists the resulting summary state
6. increments the session compactification counter

The raw transcript is left intact.

## First-Phase Policy

This slice uses shipped runtime defaults instead of a user-facing policy graph.

Defaults:

- require at least 8 raw transcript messages before compaction
- keep the last 6 transcript messages verbatim
- cap summary output to 1024 output tokens
- trim persisted summary text to a bounded character limit

If the session does not meet the threshold, compaction is a no-op.

## Prompt Use

Future chat turns for a compacted session should use:

1. one synthetic system message carrying the compact summary
2. only the uncovered trailing raw messages

This must happen in the canonical execution path, not only in TUI/CLI commands.

If no summary exists, execution continues to use the raw transcript as-is.

## Summary Prompt

The provider prompt for compaction should preserve:

- user intent and current goals
- key architectural decisions
- important files, paths, and artifacts
- open approvals / blockers if visible in transcript
- unresolved next steps

The summary must stay concise and operational. It is not a transcript rewrite.

## Persistence

Store the summary in canonical persistence as a dedicated record keyed by
`session_id`.

This slice keeps the summary inline in SQLite rather than file-backed payloads,
because the summary is expected to be small and is operational metadata.

## Top Bar / UI Effects

The existing compactification counter becomes real.

`context tokens` may continue to be approximate, but should incorporate:

- persisted summary token estimate
- uncovered raw transcript estimate

`/compact` in TUI must call the canonical compaction path instead of a metadata
placeholder.

## Testing

Required coverage:

- compacting a session persists a summary record and increments the counter
- compacting with too little history is a no-op
- future chat turns prepend the summary system message and exclude covered raw
  transcript messages
- TUI `/compact` uses the canonical path and not just a counter increment
