# Canonical Filesystem, Exec, and Planning Tool Surface Design

## Goal

Expand the canonical model-facing tool surface so the agent can work effectively with files, commands, and plans without introducing a second runtime path or shell-magic escape hatch.

The new surface should:

- preserve the existing structured `exec_*` lifecycle;
- replace the current coarse filesystem surface with a clearer, more model-friendly shape inspired by the successful clean-room design;
- replace the current coarse planning mutation path with granular planning tools;
- keep offloading and retrieval as explicit memory mechanisms instead of silently rehydrating large payloads into prompts.

## Constraints

- Keep one canonical runtime path.
- Do not reintroduce `shell_snippet`.
- Do not add hard-delete filesystem tools; use trash semantics instead.
- Do not duplicate prompt state in the TUI.
- Keep artifact/offload retrieval explicit through tools.

## Target Tool Surface

### Filesystem

- `fs_list`
  - list entries in a specific directory
- `fs_glob`
  - match workspace paths by glob
- `fs_read_text`
  - read a whole UTF-8 text file when bounded enough
- `fs_read_lines`
  - read a specific inclusive line range with `total_lines`, `eof`, and `next_start_line`
- `fs_search_text`
  - grep-like search within one file
- `fs_find_in_files`
  - grep-like search across files, optionally filtered by glob
- `fs_write_text`
  - full-file write with explicit `mode=create|overwrite|upsert`
- `fs_patch_text`
  - exact text-fragment replacement for known content
- `fs_replace_lines`
  - replace an explicit inclusive line range
- `fs_insert_text`
  - insert before/after a line or prepend/append
- `fs_mkdir`
  - create directories
- `fs_move`
  - move or rename files/directories
- `fs_trash`
  - move files/directories into trash instead of deleting permanently

### Exec

- `exec_start`
- `exec_wait`
- `exec_kill`

These remain structured exec tools with literal executable + args semantics. They must be visible to the model in the canonical tool loop.

### Planning

- `init_plan`
- `add_task`
- `set_task_status`
- `add_task_note`
- `edit_task`
- `plan_snapshot`
- `plan_lint`

The current `plan_read` / `plan_write` pair may remain temporarily as compatibility tools, but the canonical model-facing planning surface should move to the granular tool set above.

### Offload / Retrieval

- `artifact_read`
- `artifact_search`

Large tool results may be offloaded into artifacts. Prompt assembly should expose compact refs only; the model retrieves payloads explicitly through retrieval tools.

## Semantics

### `fs_patch_text` vs `fs_replace_lines`

- `fs_patch_text`
  - anchored by exact text content
  - best when the model knows the fragment to replace but is not relying on line numbers
- `fs_replace_lines`
  - anchored by previously observed line numbers
  - best after `fs_read_lines`

Both remain useful because they optimize for different editing anchors.

### No `fs_replace_in_line`

Do not add `fs_replace_in_line`.

It overlaps too heavily with `fs_replace_lines(start=end)` and previously caused extra tool-selection ambiguity for the model.

### `fs_glob` vs `fs_list`

- `fs_list`
  - local directory inspection
- `fs_glob`
  - workspace discovery by pattern

The agent needs both.

### Large file reading

`fs_read_lines` should return enough metadata for the model to know whether it has finished reading a file:

- `start_line`
- `end_line`
- `total_lines`
- `eof`
- `next_start_line`

For large files, `fs_read_text` should not silently dump unbounded content. It should either:

- return bounded content plus metadata and optional offload ref; or
- reject with guidance to use `fs_read_lines`.

### Large result offloading

Large outputs from tools such as `exec_wait`, `web_fetch`, `fs_find_in_files`, and large file reads may be offloaded to artifacts.

Prompt assembly should include a compact offload refs block, not full payloads. Session head should summarize filesystem activity and high-level state, but not duplicate full offloaded payloads.

## Runtime Integration

### Tool exposure

The current model loop only exposes a narrow automatic tool set. It must be expanded so the model can actually call the new filesystem, planning, and exec tools.

### Permissions

- destructive filesystem tools remain approval-gated in default mode;
- `exec_start` and `exec_kill` remain approval-gated in default mode;
- read-only filesystem and retrieval tools remain auto-allowed;
- `plan_*` tools remain allowed without approval.

### Prompt assembly

Prompt assembly remains:

1. `SessionHead`
2. `Plan`
3. `ContextSummary`
4. `Offload refs`
5. transcript tail

The new tools should enrich this path, not bypass it.

## Migration Strategy

1. Add new filesystem tool definitions and runtime implementations.
2. Expose `exec_*` in the automatic model tool set.
3. Add granular plan tool definitions and runtime support.
4. Preserve old compatibility tools temporarily where needed.
5. Update tests, prompt/session-head integration, and TUI/tool summaries.

## Testing Strategy

- Tool catalog and schema tests
- Tool runtime unit tests for new filesystem behaviors
- Permission tests for new destructive tools
- Provider-loop tests proving the model can call `exec_*`
- End-to-end chat/TUI tests covering file creation, edit, move/trash, and planning flow
