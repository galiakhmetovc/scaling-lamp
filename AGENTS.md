Project instructions for this repository:

- This repository builds a local autonomous coding-agent runtime in Rust.
- Preserve one canonical runtime path. Do not introduce a second chat path, second prompt path, or separate tool loop for a special UI.
- Keep prompt assembly ordered as:
  1. `SYSTEM.md`
  2. `AGENTS.md`
  3. `SessionHead`
  4. `Plan`
  5. `ContextSummary`
  6. offload refs
  7. uncovered transcript tail
- Prefer the canonical structured tool surface:
  - planning: `init_plan`, `add_task`, `set_task_status`, `add_task_note`, `edit_task`, `plan_snapshot`, `plan_lint`
  - filesystem: `fs_read_text`, `fs_read_lines`, `fs_search_text`, `fs_find_in_files`, `fs_list`, `fs_glob`, `fs_write_text`, `fs_patch_text`, `fs_replace_lines`, `fs_insert_text`, `fs_mkdir`, `fs_move`, `fs_trash`
  - execution: `exec_start`, `exec_wait`, `exec_kill`
  - offload retrieval: `artifact_read`, `artifact_search`
- Do not reintroduce shell-snippet style tools or hidden shell-magic abstractions.
- Large tool outputs should offload into artifacts and be retrieved explicitly, not silently rehydrated into prompts.
- Prefer targeted refactors that preserve behavior over broad churn.
- Keep TUI and CLI thin over the same app/runtime layer.

Verification commands for meaningful changes:

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `cargo build -p agentd`
- `cargo build --release -p agentd`
