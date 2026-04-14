# Runtime Worktree Integration Decision

Date: 2026-04-12
Status: decided

## Context

`teamD-runtime-core-mvp-1` was created as a dedicated worktree branch for the early Go runtime MVP.

The original concern behind `teamD-runtime-merge-runtime-branch` was valid: if the worktree still contained the only real implementation, `master` would have been misleadingly documentation-heavy.

That is no longer the case.

## What Was Compared

Branch comparison on 2026-04-12:

- `master`: current canonical runtime/control-plane implementation
- `teamD-runtime-core-mvp-1`: early alternative runtime branch

`git log --left-right --cherry-pick master...teamD-runtime-core-mvp-1` shows:

- `master` contains the landed runtime/control-plane line:
  - transport-agnostic execution
  - HTTP API + CLI
  - approvals persistence and continuation
  - jobs/workers/plans/artifacts/events
  - AgentCore
  - governance baseline
  - replay
  - worker supervision baseline
  - filesystem artifact storage baseline
- `teamD-runtime-core-mvp-1` still contains a different, older runtime shape:
  - early coordinator/worker/bootstrap skeleton
  - old Telegram runtime implementation
  - old mesh-oriented runtime work
  - older provider, compaction, and artifact wiring

`git diff --stat master...teamD-runtime-core-mvp-1` shows a large divergent change set rather than an isolated unmerged feature.

## Decision

Do **not** merge `teamD-runtime-core-mvp-1` into `master`.

Reason:

1. The branch is no longer an integration backlog of clean missing commits.
2. It represents an older parallel implementation path that has already been superseded on `master`.
3. A direct merge would reintroduce obsolete coordinator/Telegram/runtime code and create conflict-heavy noise.
4. Remaining useful ideas from the branch should be handled as explicit backlog items under:
   - `teamD-runtime-spec-alignment`
   - `teamD-runtime-test-alignment`

## Canonical Integration Path

The canonical runtime lives on `master`.

From this point on:

- new runtime work lands directly on `master`
- any still-useful branch-only concepts must be reintroduced intentionally as scoped tasks
- `teamD-runtime-core-mvp-1` is treated as historical reference, not merge target

## Consequence For Backlog

`teamD-runtime-merge-runtime-branch` is satisfied by this decision:

- branch state reviewed
- integration path chosen
- canonical branch identified
- remaining delta redirected into explicit spec/test alignment work
