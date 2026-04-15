# Clean-Room Filesystem And Shell Tools

This document describes the current filesystem and shell tool domains in `rewrite/clean-room-root`.

It is an implementation snapshot of what exists today in code and in shipped `zai-smoke` config.

## Current Tool Surface

Current built-in filesystem tools:

- `fs_list`
- `fs_read_text`
- `fs_write_text`
- `fs_patch_text`
- `fs_mkdir`
- `fs_move`

Current built-in shell tools:

- `shell_exec`
- `shell_start`
- `shell_poll`
- `shell_kill`

These tools are:

1. defined by their domain contracts
2. selected into the visible model surface through `ToolContract`
3. allowed through the shared `ToolExecutionContract`
4. restricted again by domain-specific execution contracts

## Current Shipped `zai-smoke` Config

Current shipped `zai-smoke` enables:

- `FilesystemToolContract`
- `FilesystemExecutionContract`
- `ShellToolContract`
- `ShellExecutionContract`

Current tool exposure allowlist includes:

- plan tools
- shipped filesystem tools:
  - `fs_list`
  - `fs_read_text`
  - `fs_write_text`
  - `fs_patch_text`
  - `fs_mkdir`
  - `fs_move`
- `shell_exec`
- `shell_start`
- `shell_poll`
- `shell_kill`

## Current Filesystem Execution Safety

Current shipped filesystem scope:

- strategy: `workspace_only`
- root path: `.`

Current shipped filesystem mutation policy:

- strategy: `allow_writes`
- writes allowed
- move allowed
- mkdir allowed

Current shipped filesystem IO policy:

- strategy: `bounded_text_io`
- `max_read_bytes: 131072`
- `max_write_bytes: 131072`
- `encoding: utf-8`

Important limit:

- `fs_trash` exists in the backend but is not currently shipped in `zai-smoke`
- current shipped mutation policy is `allow_writes`, so shipped visible tools are limited to the operations that are actually allowed by that policy

## Current Shell Execution Safety

Current shipped shell command policy:

- strategy: `static_allowlist`
- allowed commands:
  - `pwd`
  - `ls`
  - `cat`
  - `rg`
  - `go`
  - `git`
  - `echo`
  - `printf`
  - `head`
  - `sed`
  - `wc`
  - `find`

Current shipped richer command argument rules:

- `go`
  - allowed argument prefixes:
    - `test`
    - `build`
    - `env`
    - `version`
    - `list`
  - denied argument patterns:
    - `env -w`
- `git`
  - allowed argument prefixes:
    - `status`
    - `diff`
    - `log`
    - `rev-parse`
    - `branch`
  - denied argument patterns:
    - `push`
    - `reset --hard`

Current shipped shell runtime policy:

- strategy: `workspace_write`
- `cwd: .`
- `timeout: 30s`
- `max_output_bytes: 65536`
- `allow_network: true`

Hard-isolation behavior:

- when `allow_network: false`, the shell backend now requires a real Linux `unshare --net` launcher path
- if that launcher is unavailable or blocked by host permissions, shell execution fails closed with an explicit isolation error

Current shipped limit:

- `zai-smoke` keeps `allow_network: true` so the default shell tool remains usable on hosts where namespace isolation is not available

## Current Runtime Path

Current provider path:

1. filesystem and shell definition executors build tool definitions
2. `ToolContract` filters the visible tool surface
3. provider can emit `tool_calls`
4. `ToolExecutionContract` decides allow or deny by tool id
5. runtime dispatches:
   - filesystem calls to `internal/filesystem.Executor`
   - shell calls to a persistent `internal/shell.Executor`
6. tool result payloads are returned into the provider loop as `tool` messages

Current async shell lifecycle:

1. `shell_start` validates command policy and starts a bounded background process
2. runtime returns a `command_id`
3. `shell_poll` returns current status plus output chunks after an optional `after_offset`
4. `shell_kill` requests termination for a running command
5. active command state is held inside the runtime shell executor for the life of the agent process

Current shell approval lifecycle:

1. `shell_exec` and `shell_start` also evaluate `ShellApprovalPolicy`
2. when approval is required, runtime returns `status: approval_pending`
3. the payload includes:
   - `approval_id`
   - `command_id`
   - `message`
4. operator approval/denial is handled through the TUI `Tools` pane
5. `Approve(...)` starts the reserved command and `Deny(...)` persists a denial result

Current persisted shell lifecycle events:

- `shell.command.approval_requested`
- `shell.command.approval_granted`
- `shell.command.approval_denied`
- `shell.command.started`
- `shell.command.output.chunk`
- `shell.command.kill_requested`
- `shell.command.completed`

Current persisted shell projection:

- `shell_command`
- tracks approval state, command status, latest offset, latest output chunk, exit code, and kill-pending state

## Current Limits

What is implemented now:

- all first-slice filesystem backends
- one bounded `shell_exec` backend
- async shell lifecycle via `shell_start` / `shell_poll` / `shell_kill`
- persisted shell lifecycle events and `shell_command` projection
- richer per-command shell argument policies through `command_rules`

What is still intentionally limited:

- no interactive shell
- no hard OS-level network sandbox
- no unrestricted shell command execution
- no binary file tooling
