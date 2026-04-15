# Clean-Room Delegation Contract

`teamD` now has a canonical delegation domain and a working local reference backend for future subagents.

This slice is now **contract + local backend**:

- shared runtime models for delegate lifecycle live in [delegation.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/runtime/delegation.go)
- model-visible tool definitions live in [definitions.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/delegation/definitions.go)
- config/module/policy registries now resolve:
  - `DelegationToolContract`
  - `DelegationExecutionContract`

## Canonical Tool Surface

The delegation tool domain defines:

- `delegate_spawn`
- `delegate_message`
- `delegate_wait`
- `delegate_close`
- `delegate_handoff`

These tools describe the canonical lifecycle for both:

- `local_worker`
- `remote_mesh`

## Contract Split

`DelegationToolContract`

- catalog
- builtin descriptions

`DelegationExecutionContract`

- backend allowlist and default backend
- bounded wait-result semantics for messages/events/artifacts/policy snapshot

## Local Reference Backend

`local_worker` now runs inside the clean-room runtime as a delegate backend:

- each delegate gets its own chat session
- `delegate_spawn` starts bounded delegated work
- `delegate_message` starts a follow-up run on the same delegate
- `delegate_wait` returns incremental transcript messages, lifecycle events, and optional handoff
- `delegate_close` and `delegate_handoff` use the same canonical aggregate

Delegates now also carry a typed policy snapshot:

- delegate sub-runs execute with per-delegate contract overrides instead of blindly reusing the parent global contracts
- tool visibility, tool execution access, and shell approval semantics propagate through the delegate snapshot
- the local backend reconstructs its runtime contract boundary from the persisted `policy_snapshot`

Persisted delegate lifecycle events now include:

- `delegate.spawned`
- `delegate.message_received`
- `delegate.run_started`
- `delegate.completed`
- `delegate.failed`
- `delegate.closed`
- `delegate.handoff_created`

`zai-smoke` now includes:

- visible delegation tools in the global tool catalog
- matching tool-execution allowlist entries
- the `delegate` projection in runtime projections

## Policy And Approval Propagation

The local backend now treats the delegation policy snapshot as an execution boundary:

- `delegate_spawn` captures the current tool and execution contracts into the delegate snapshot
- local delegate turns run with those contract overrides
- delegate shell approvals therefore follow the delegated snapshot, not the ambient parent defaults

This is the reference shape future remote delegates must match.

## What This Enables Next

The follow-up implementation can now:

1. align richer local worker policy/approval propagation onto the same contract
2. add operator/TUI views over the `delegate` projection
3. later add remote/mesh delegates without changing the model-visible lifecycle again
