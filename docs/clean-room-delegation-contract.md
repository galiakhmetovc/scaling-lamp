# Clean-Room Delegation Contract

`teamD` now has a canonical delegation domain for future subagents.

This slice is intentionally **contract-first**:

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

## Important Boundary

This slice does **not** ship a working local delegate runtime yet.

That is deliberate.

The contract is now stable, but `zai-smoke` does not add delegation tools to the visible global tool allowlist or tool-execution allowlist. That prevents the model from seeing non-executable tools before the local worker backend is aligned.

## What This Enables Next

The follow-up implementation can now:

1. map existing local worker lifecycle onto the canonical delegate models
2. propagate policy snapshot and approvals through the same contract
3. later add remote/mesh delegates without changing the model-visible lifecycle again
