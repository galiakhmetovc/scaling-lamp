# Implementation Plan: Canonical Filesystem, Exec, and Planning Tool Surface

1. Expand the canonical tool model.
   - Add new filesystem tool names, inputs, outputs, schemas, summaries, and JSON serialization.
   - Add new planning tool names and schemas.
   - Keep compatibility with existing `plan_read` / `plan_write` during the transition.

2. Implement filesystem runtime support.
   - Add `fs_read_text` / `fs_read_lines` split semantics.
   - Add `fs_search_text` and `fs_find_in_files`.
   - Add `fs_write_text(mode=...)`, `fs_patch_text`, `fs_replace_lines`, `fs_insert_text`, `fs_mkdir`, `fs_move`, `fs_trash`.
   - Add large-output handling and artifact/offload integration for bounded reads/searches where appropriate.

3. Expose structured exec tools to the model loop.
   - Include `exec_start`, `exec_wait`, and `exec_kill` in the automatic model tool set.
   - Preserve approval handling and structured-exec restrictions.

4. Replace the canonical planning surface.
   - Add `init_plan`, `add_task`, `set_task_status`, `add_task_note`, `edit_task`, `plan_snapshot`, `plan_lint`.
   - Route them through canonical persistence and prompt assembly.

5. Update prompt/session-head and operator-facing summaries.
   - Ensure the new filesystem actions produce good recent-step summaries.
   - Keep offload refs in prompt and artifacts retrievable through explicit tools.

6. Add verification coverage.
   - Unit tests for catalog, permissions, runtime, and planning behavior.
   - Provider-loop tests for tool exposure.
   - Chat/TUI integration tests for common file/command workflows.
