# Chat Runtime V2 Corrective Refactor Design

## Goal

Replace the current chat execution path with a simpler and more reliable runtime model.

The new design is meant to stop the current failure pattern where shell execution, approvals, polling, UI projections, and TUI-local state all drift apart and force repeated symptom fixes.

The target result is:

- one canonical run state for chat execution
- explicit shell tool contracts with no command-vs-snippet ambiguity
- approvals that are part of the run state, not inferred from side channels
- `Chat` and `Tools` rendering from the same snapshot
- websocket events used for streaming and wakeups only, not as truth

## Why This Refactor Is Necessary

The current path is already beyond routine hardening.

Recent failures show a systemic problem:

- the same shell invocation can request approval multiple times or resume incorrectly
- approval continuation can fail after grant without the UI seeing a coherent state transition
- `shell_poll` can busy-spin on silent commands
- `shell_exec` and `shell_start` ambiguously mix structured command execution and shell snippet execution
- `Chat` and `Tools` still rely on mixed truth sources such as `ToolLog`, `PendingApprovals`, daemon events, and local TUI state
- run status can show `idle`, `waiting_shell`, or `AGENT END TURN` at the wrong time because the lifecycle is fragmented

This is not a single bug. It is an architectural defect.

## Non-Goals

This refactor does not try to preserve internal compatibility with the current execution internals.

Out of scope for the first refactor slice:

- rewriting `Workspace`
- redesigning the provider abstraction
- changing session storage format beyond what the new chat runtime needs
- preserving current shell tool payload shapes for model compatibility
- incremental cleanup of the old V1 path

The intent is replacement, not prolonged coexistence of two equally important runtime paths.

## Design Principles

### One Source Of Truth

The canonical truth for an active chat run must live in one run snapshot owned by the runtime.

The following must not be independent sources of truth anymore:

- `ToolLog`
- top-level `PendingApprovals` detached from run state
- daemon status events that mutate TUI state directly
- TUI-local inferred run state

### Explicit Contracts

Shell execution must stop relying on best-effort interpretation.

The runtime must know whether a call is:

- a structured executable invocation
- a shell snippet that requires shell parsing semantics

Those are different contracts and must be represented differently.

### Serialized Continuations

Approval continuation, process waiting, and post-tool resume must be serialized per run.

No two concurrent continuations should mutate the same run state at the same time.

### Events Are Not Truth

UI events remain useful for:

- live output streaming
- wakeup notifications
- lightweight UX feedback

They must not decide final run phase, approval existence, or completion state.

## Scope

The first V2 scope replaces the operator chat execution path:

- chat run engine
- approval handling
- shell execution tools used by chat
- process wait and poll behavior
- chat/tools snapshot model
- TUI rendering for `Chat` and `Tools`

The first V2 scope does not replace:

- `Workspace`
- PTY runtime
- file tree
- editor
- artifact viewer

`Workspace` can remain on the current path until the new chat runtime is stable.

## Runtime Model V2

### Core Entities

Introduce a new chat execution core under a separate namespace, for example:

- `internal/runtimev2/runstate`
- `internal/runtimev2/engine`
- `internal/runtimev2/approval`
- `internal/runtimev2/process`

The center of the system is `RunSnapshotV2`.

Suggested fields:

- `run_id`
- `session_id`
- `phase`
- `started_at`
- `updated_at`
- `finished_at`
- `error`
- `pending_approvals[]`
- `active_processes[]`
- `recent_steps[]`
- `queued_user_messages[]`
- `provider_stream`
- `result`

### Run Phases

The run phase model must be explicit.

Minimum set:

- `running`
- `waiting_approval`
- `waiting_process`
- `resuming`
- `completed`
- `failed`
- `cancelled`

Rules:

- `waiting_approval` means the run is blocked on operator action
- `waiting_process` means the run is blocked on one or more active background processes
- `resuming` means the runtime is actively continuing after an approval or process update
- `completed`, `failed`, and `cancelled` are terminal phases

No boolean `Active` flag is sufficient by itself.

## Tool Contract V2

### Remove Shell Ambiguity

The current mixed shell behavior must be replaced.

V2 introduces two explicit tool families:

- structured process execution
- shell snippet execution

### Structured Process Tools

Suggested tool set:

- `exec_start`
- `exec_wait`
- `exec_kill`

`exec_start` accepts:

- `executable`
- `args[]`
- `cwd`
- `env`

It does not accept:

- `cd ... &&`
- pipes
- redirects
- shell quoting tricks as an execution model

### Shell Snippet Tools

Suggested tool set:

- `shell_snippet_start`
- `shell_snippet_wait`
- `shell_snippet_kill`

`shell_snippet_start` accepts:

- `script`
- `cwd`
- `env`

This path is for:

- `cd dir && cmd`
- pipelines
- redirects
- shell builtins
- shell quoting semantics

The runtime always knows this is shell-interpreted execution.

### Why Two Families

This is the simplest path that satisfies both correctness and safety.

One smart tool that tries to infer intent has already proven unreliable. Two explicit contracts remove a whole class of parsing and approval bugs.

## Approval Model V2

Approval must become part of run state, not a side-channel projection.

Each pending approval should include at least:

- `approval_id`
- `run_id`
- `step_id`
- `tool_kind`
- `display_text`
- `policy_scope`
- `created_at`

Approval actions:

- `approve_once`
- `approve_always`
- `deny_once`
- `deny_always`
- `cancel_and_send`

Rules:

- approval actions update the run state atomically
- repeated submits are idempotent no-op completions, not error paths
- continuation after approval is serialized per run
- the TUI menu must read directly from `RunSnapshotV2.pending_approvals`

`cancel_and_send` must:

- resolve the pending approval
- cancel the blocked step
- enqueue the operator message as a normal user message on the same run/session path

## Process Waiting Model V2

The process model must stop exposing raw polling semantics as a fragile loop pattern.

Each active process should include:

- `process_id`
- `run_id`
- `step_id`
- `kind`
- `display_name`
- `status`
- `started_at`
- `updated_at`
- `exit_code`
- `next_offset`

`exec_wait` and `shell_snippet_wait` should provide wait semantics, not busy-loop semantics.

Expected behavior:

- if new chunks are available, return them
- if the process completed, return terminal status
- if still running with no new output, wait briefly and return after timeout or update

The model should not be able to burn 100 tool rounds on immediate empty responses for a healthy silent process.

## Snapshot And Event Model V2

### Canonical Snapshot

`Chat` and `Tools` must render from `RunSnapshotV2`.

That snapshot is responsible for:

- phase
- current approvals
- active processes
- recent steps
- terminal result

### Events

Events remain, but their role is reduced.

They are allowed to:

- deliver stream chunks
- tell the UI that something changed
- support operator tracing and diagnostics

They are not allowed to:

- set run phase directly in TUI state
- create or remove approvals as truth
- mark a run complete independently of snapshot state

Suggested event family:

- `run.updated`
- `run.phase.changed`
- `run.stream.chunk`
- `run.step.updated`
- `run.completed`
- `run.failed`

## TUI Design V2

### Chat

`Chat` should render from the canonical run snapshot.

This includes:

- status bar run phase
- approval menu visibility
- live process rail
- end-turn marker
- queued user messages

Rules:

- no direct truth mutations from websocket status events
- no approval inference from `ToolLog`
- no local synthetic idle/completed state when snapshot disagrees

### Tools

`Tools` should also read from the same run snapshot.

The pane can still show historical activity, but current operator state comes from:

- `pending_approvals`
- `active_processes`
- `recent_steps`

This removes the current split where `Tools` can show approval while `Chat` does not.

### UX Latitude

This refactor does not promise to preserve the current TUI UX exactly.

If a simpler operator model is better, it should replace the current behavior.

The priority is:

1. correctness
2. operator clarity
3. safety
4. polish

## Daemon Boundary V2

The daemon must expose V2-specific commands or a V2-specific chat-run path.

Recommended rule:

- a run is either V1 or V2, never mixed

The daemon should route new chat runs to V2 after the cutover flag is enabled.

Approval continuation, process wait, and user interjection handling must all execute against the same run engine instance and the same persistent run state.

## Migration Strategy

### Parallel V2 Path

Do not continue deepening the current V1 path.

Build V2 in parallel and cut over deliberately.

Recommended rollout:

1. implement `RunSnapshotV2` and runtime core
2. implement V2 approval state and serialized continuation
3. implement V2 structured process tools
4. implement V2 shell snippet tools
5. switch daemon chat-run path to V2
6. switch `Chat` and `Tools` to `RunSnapshotV2`
7. run smoke tests and event-log tracing on real operator scenarios
8. remove V1 chat approval/poll glue

### Compatibility

Backward compatibility with the current internal shell tool contract is not required.

What matters is:

- the system works reliably
- the operator understands what the runtime is doing
- approvals, process waiting, and completion are coherent

## Testing Strategy

This rewrite needs strong behavioral tests before TUI polish.

Minimum coverage:

- structured exec does not interpret shell snippets
- shell snippet path correctly handles `cd`, pipes, redirects, and builtins
- one approval produces one pending approval state
- repeated approval submit is idempotent
- two approvals on the same run serialize correctly
- a silent long-running process does not burn tool loop rounds
- `waiting_process` stays active until the process really finishes or is killed
- `Chat` and `Tools` show the same approval/process state from the same snapshot
- websocket events cannot clobber canonical run phase
- `cancel_and_send` cancels the blocked step and enqueues a normal user message

Real-world scenario tests should cover at least:

- `ansible-playbook` structured invocation
- shell snippet with `cd ... && ansible-playbook ...`
- approval then immediate second approval
- process kill from `Tools`
- resume old session with active waiting process

## Operational Diagnostics

Tracing added during current debugging should stay during the transition.

V2 should preserve explicit trace points for:

- approval menu shown/selection/submission
- approval continuation started/completed/failed
- process wait started/completed/timed out
- run phase transitions

This makes postmortems possible without guessing from UI symptoms.

## Risks

Main risks:

- V2 and V1 coexistence causing accidental mixed runs
- TUI migration attempting to preserve too much V1 glue
- provider/tool prompting still assuming old shell contract
- leaving `Workspace` coupled to V1 assumptions in shared code

Mitigations:

- strict per-run versioning
- isolate V2 under new packages
- keep `Workspace` out of first-slice cutover
- update prompts/tool descriptions to the new explicit contracts

## Recommendation

Proceed with a parallel V2 corrective refactor focused on `Chat`, `Tools`, shell execution, approvals, and run lifecycle.

Do not spend more time patching the current V1 lifecycle except for minimal safety fixes needed to keep the branch usable during the rewrite.

The right outcome is not “fewer approval bugs.” The right outcome is a runtime model where this class of bug is structurally harder to create.
