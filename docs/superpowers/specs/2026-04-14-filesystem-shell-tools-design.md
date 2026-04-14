# Filesystem and Shell Tools Design

This document defines the next clean-room tool domains after internal planning tools.

It adds project-local filesystem tools and restricted shell tools to the existing tool pipeline.
It does not bypass `ToolContract` or `ToolExecutionContract`.

## Goal

Add two new agent-facing tool domains:

- filesystem tools for safe workspace file operations
- shell tools for controlled command execution

The result should let the agent inspect and edit repo state and run bounded commands without collapsing prompt assembly, tool exposure, and execution safety into ad hoc runtime code.

## Scope

First-slice scope:

- model-visible filesystem tool definitions
- model-visible shell tool definitions
- execution safety policies for each domain
- runtime execution backends for allowed calls
- shipped config path for local workspace usage

Out of scope:

- interactive PTY shell
- unrestricted command execution
- unrestricted filesystem access outside workspace
- public-network tool execution by default
- replacing existing plan tools

## Domain Boundaries

### Filesystem Tool Domain

`FilesystemToolContract` is responsible for:

- which filesystem tools exist
- how they are described to the model
- how they are serialized into the general tool surface

`FilesystemExecutionContract` is responsible for:

- path scope
- read/write/delete policy
- size limits
- mutation safety

### Shell Tool Domain

`ShellToolContract` is responsible for:

- which shell tools exist
- how they are described to the model
- how they are serialized into the general tool surface

`ShellExecutionContract` is responsible for:

- which commands may run
- runtime restrictions
- approvals
- time/output/network limits

### Existing Boundaries That Stay

- `ToolContract` remains the model-visible selection layer
- `ToolExecutionContract` remains the shared gate for general tool-call allow/deny
- domain-specific execution contracts add restrictions after the shared gate, not instead of it

This means:

1. domain tool definitions are built first
2. `ToolContract` chooses what is visible
3. provider emits tool calls
4. `ToolExecutionContract` allows or denies by tool id
5. domain execution contracts apply filesystem/shell-specific rules
6. runtime executes the call or returns a tool error result

## Filesystem Tools

First-slice tool set:

- `fs_list(path)`
- `fs_read_text(path)`
- `fs_write_text(path, content)`
- `fs_patch_text(path, search, replace)`
- `fs_mkdir(path)`
- `fs_move(src, dest)`
- `fs_trash(path)`

Deliberately not included in first slice:

- binary file editing
- direct permanent delete
- chmod/chown tools
- recursive copy helpers

### Filesystem Tool Responsibility

- `fs_list`
  - list directory entries for model navigation
- `fs_read_text`
  - read bounded text files
- `fs_write_text`
  - write full file content when allowed
- `fs_patch_text`
  - perform targeted text replacement with explicit search/replace input
- `fs_mkdir`
  - create missing directories in allowed scope
- `fs_move`
  - rename or move within allowed scope
- `fs_trash`
  - move a file or directory to trash instead of permanent deletion

## Shell Tools

First-slice tool set:

- `shell_exec(command, args[], cwd?)`

Deliberately not included in first slice:

- interactive shell sessions
- long-lived process management
- streaming PTY output
- arbitrary pipelines assembled by the model

### Shell Tool Responsibility

- `shell_exec`
  - run one bounded non-interactive command
  - return exit code, stdout, stderr, and timing metadata

## Contracts, Policies, Strategies, Params

## FilesystemToolContract

`FilesystemToolContract` controls which filesystem tools exist and how they are described.

### Policy: `FilesystemCatalogPolicy`

Answers:

- which filesystem tool ids are available for this runtime

#### Strategy: `static_allowlist`

Params:

- `tool_ids[]`
- `allow_empty`
- `dedupe`

### Policy: `FilesystemDescriptionPolicy`

Answers:

- how filesystem tools are described to the model

#### Strategy: `static_builtin_descriptions`

Params:

- `include_examples`
- `include_scope_hint`

## FilesystemExecutionContract

`FilesystemExecutionContract` controls path scope and mutation safety.

### Policy: `FilesystemScopePolicy`

Answers:

- where filesystem tools may operate

#### Strategy: `workspace_only`

Params:

- `root_path`
- `read_subpaths[]`
- `write_subpaths[]`

#### Strategy: `allowlist_paths`

Params:

- `allowed_paths[]`
- `read_only_paths[]`
- `write_paths[]`

### Policy: `FilesystemMutationPolicy`

Answers:

- which mutation kinds are allowed

#### Strategy: `allow_writes`

Params:

- `allow_write`
- `allow_move`
- `allow_mkdir`

#### Strategy: `require_approval_for_writes`

Params:

- `approval_message_template`

#### Strategy: `trash_only_delete`

Params:

- `trash_dir` optional

### Policy: `FilesystemIOPolicy`

Answers:

- how much data may be read or written

#### Strategy: `bounded_text_io`

Params:

- `max_read_bytes`
- `max_write_bytes`
- `encoding`

## ShellToolContract

`ShellToolContract` controls which shell tools exist and how they are described.

### Policy: `ShellCatalogPolicy`

Answers:

- which shell tool ids are exposed

#### Strategy: `static_allowlist`

Params:

- `tool_ids[]`
- `allow_empty`

### Policy: `ShellDescriptionPolicy`

Answers:

- how shell tools are described to the model

#### Strategy: `static_builtin_descriptions`

Params:

- `include_examples`
- `include_runtime_limits`

## ShellExecutionContract

`ShellExecutionContract` controls command safety and runtime limits.

### Policy: `ShellCommandPolicy`

Answers:

- which commands may run at all

#### Strategy: `static_allowlist`

Params:

- `allowed_commands[]`
- `allowed_prefixes[]`
- `deny_patterns[]`

#### Strategy: `deny_all`

Params:

- none

### Policy: `ShellApprovalPolicy`

Answers:

- whether a matching command needs approval

#### Strategy: `always_allow`

Params:

- none

#### Strategy: `always_require`

Params:

- `approval_message_template`

#### Strategy: `require_for_patterns`

Params:

- `patterns[]`
- `approval_message_template`

### Policy: `ShellRuntimePolicy`

Answers:

- execution environment restrictions

#### Strategy: `workspace_write`

Params:

- `cwd`
- `timeout`
- `max_output_bytes`
- `allow_network`

#### Strategy: `read_only`

Params:

- `cwd`
- `timeout`
- `max_output_bytes`
- `allow_network`

#### Strategy: `deny_exec`

Params:

- none

## Recommended First Shipped Configuration

### Filesystem

- `FilesystemCatalogPolicy.static_allowlist`
  - `fs_list`
  - `fs_read_text`
  - `fs_write_text`
  - `fs_patch_text`
  - `fs_mkdir`
  - `fs_move`
  - `fs_trash`
- `FilesystemDescriptionPolicy.static_builtin_descriptions`
- `FilesystemScopePolicy.workspace_only`
- `FilesystemMutationPolicy.allow_writes`
- `FilesystemMutationPolicy.trash_only_delete`
- `FilesystemIOPolicy.bounded_text_io`

### Shell

- `ShellCatalogPolicy.static_allowlist`
  - `shell_exec`
- `ShellDescriptionPolicy.static_builtin_descriptions`
- `ShellCommandPolicy.static_allowlist`
- `ShellApprovalPolicy.always_allow`
- `ShellRuntimePolicy.workspace_write`
  - `allow_network=false`
  - bounded `timeout`
  - bounded `max_output_bytes`

## Tool Result Shape

Filesystem tools should return compact structured JSON:

- `status`
- `path`
- `entries[]` for list
- `content` for reads
- `bytes`
- `changed`
- `error` optional

Shell tools should return compact structured JSON:

- `status`
- `command`
- `args[]`
- `cwd`
- `exit_code`
- `stdout`
- `stderr`
- `duration_ms`
- `timed_out`

## Runtime Integration

The clean-room runtime flow becomes:

1. build built-in plan tool definitions from `PlanToolContract`
2. build built-in filesystem tool definitions from `FilesystemToolContract`
3. build built-in shell tool definitions from `ShellToolContract`
4. merge them into the general tool surface
5. `ToolContract` selects visible tool ids
6. request-shape serializes visible tools
7. provider emits tool calls
8. `ToolExecutionContract` applies shared allow/deny gate
9. domain execution contracts apply filesystem/shell-specific rules
10. runtime executes tool call and appends tool-result message

## Safety Rules

These rules are mandatory in first slice:

- filesystem access must stay inside configured scope
- no permanent delete tool
- shell commands must be allowlisted
- shell execution must be non-interactive
- shell network must default to disabled
- command output must be bounded
- shell cwd must stay in workspace scope

## Smell Checks

Do not accept these shortcuts:

- exposing filesystem/shell execution without domain-specific contracts
- reusing `ToolExecutionContract` as the only safety layer
- letting model pass arbitrary raw shell command strings without command policy validation
- implementing delete as permanent `rm`
- reading or writing arbitrary absolute paths by default
- hardcoding tool descriptions in `chat.go` or `provider/client.go`
- bypassing `ToolContract` for filesystem/shell tools

## Follow-Up

Expected later follow-up, not in first slice:

- streaming shell output
- human approval UX
- richer patch format than simple search/replace
- binary artifact support
- domain-specific tool traces and artifact capture
