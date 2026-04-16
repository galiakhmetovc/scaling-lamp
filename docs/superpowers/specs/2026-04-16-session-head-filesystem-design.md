# Session Head Filesystem Design

**Date:** 2026-04-16

## Goal

Add a compact filesystem representation to the prompt `session head` so the model can quickly recover both:
- the recent filesystem working set for the current session
- a shallow workspace orientation snapshot

The feature must be policy-driven and bounded. It must not turn the session head into a recursive tree dump or a replay of filesystem tool payloads.

## Scope

This design adds two optional `session head` sections:

1. `Recent filesystem activity`
2. `Workspace tree depth=1`

Both sections are configured through `SessionHeadParams` and are rendered by the prompt assembly executor.

## Non-Goals

- No recursive tree rendering
- No file contents in the session head
- No best-effort parsing from freeform assistant/user text
- No second filesystem index/store
- No hardcoded limits outside policy

## Design

### 1. Recent Filesystem Activity

The session head should prefer recent filesystem activity over a structural tree, because it reflects the active working set.

The rendered lines should be grouped by activity class:
- `Edited`
- `Read`
- `Found`
- `Moved`
- `Trashed`

Each line is compact and bounded by policy, for example:

`📝 Edited: internal/promptassembly/executor.go, config/.../session-head.yaml`

The recent activity section is sourced from a bounded runtime snapshot rather than transcript scraping. Prompt assembly already has a dedicated input object, so filesystem state should be passed in explicitly as input.

### 2. Workspace Tree Depth=1

The session head should also optionally include a single shallow orientation line from the workspace root.

Example:

`🗂 Tree: cmd/, config/, docs/, internal/, web/, go.mod`

Constraints:
- depth is fixed to `1` for the shipped default
- entries are bounded by policy
- ordering is deterministic
- directories should be visually distinguishable from files

This tree section should be sourced from the configured filesystem root, not from guessed paths.

### 3. Contract and Policy Surface

Extend `SessionHeadParams` with explicit filesystem knobs:

- `include_filesystem_recent`
- `filesystem_recent_max_items`
- `include_filesystem_tree`
- `filesystem_tree_max_entries`
- `filesystem_tree_include_files`
- `filesystem_tree_include_dirs`

The shipped `zai-smoke` session head policy should enable:
- recent filesystem activity
- tree depth=1

All limits must live in policy, not in executor constants.

### 4. Prompt Assembly Input

Extend `promptassembly.Input` with a bounded filesystem snapshot structure. The executor should render filesystem lines from that snapshot only.

The snapshot should include:
- recent activity groups
- tree entries

This keeps the prompt executor pure and deterministic, while the runtime is responsible for collecting the bounded snapshot.

### 5. Runtime Data Source

Recent filesystem activity should come from recorded tool activity in the current session/run flow, normalized into a filesystem head snapshot.

The implementation should recognize at least:
- `fs_read_lines`
- `fs_search_text`
- `fs_find_in_files`
- `fs_replace_lines`
- `fs_replace_in_line`
- `fs_insert_text`
- `fs_move`
- `fs_trash`

The tree depth=1 snapshot should be built from the configured filesystem root and bounded by policy.

### 6. Rendering Rules

Ordering:
1. title
2. session id
3. last user
4. last assistant
5. plan summary
6. recent filesystem activity
7. workspace tree

If `max_items` truncates the head, filesystem sections should obey the same cap as the rest of the session head.

If there is no recent activity, omit the recent section. If tree rendering is disabled, omit the tree section.

## Testing

Add tests covering:
- recent filesystem activity renders grouped compact lines
- tree depth=1 obeys policy limits and marks directories
- disabled filesystem params omit the sections
- contract resolution loads the new session head params

## Implementation Notes

- This is a prompt-assembly feature, not a UI-only feature
- It should reuse the existing policy/config graph
- It should not introduce another storage/index layer
