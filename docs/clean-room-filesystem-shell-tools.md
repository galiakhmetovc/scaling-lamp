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

Current shipped shell runtime policy:

- strategy: `workspace_write`
- `cwd: .`
- `timeout: 30s`
- `max_output_bytes: 65536`
- `allow_network: false`

Important limit:

- `allow_network: false` is currently a policy-level contract only
- it is not enforced by OS sandboxing in the current backend

## Current Runtime Path

Current provider path:

1. filesystem and shell definition executors build tool definitions
2. `ToolContract` filters the visible tool surface
3. provider can emit `tool_calls`
4. `ToolExecutionContract` decides allow or deny by tool id
5. runtime dispatches:
   - filesystem calls to `internal/filesystem.Executor`
   - shell calls to `internal/shell.Executor`
6. tool result payloads are returned into the provider loop as `tool` messages

## Current Limits

What is implemented now:

- all first-slice filesystem backends
- one bounded `shell_exec` backend

What is still intentionally limited:

- no interactive shell
- no hard OS-level network sandbox
- no unrestricted shell command execution
- no binary file tooling
