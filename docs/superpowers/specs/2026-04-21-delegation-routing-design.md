# Delegation Routing Design

## Goal

Introduce an explicit delegation routing seam so `JobKind::Delegate` can execute through a local child-session executor today and a future remote A2A executor later, without changing the current delegate job shape or creating a second runtime path.

## Scope

This slice covers:

- a canonical delegation routing decision for background delegate jobs
- a local executor path that preserves current child-session behavior
- a reserved remote executor slot that is not implemented yet
- clear blocked/error semantics when a job resolves to the remote slot

This slice does not cover:

- remote A2A transport
- model-facing delegation tools
- judge policies
- changing prompt assembly or child-session semantics

## Constraints

- Preserve one canonical runtime path for delegate jobs.
- Do not introduce separate local and remote orchestration stacks.
- Keep current delegate job payload shape unchanged.
- Reuse the same durable job, result package, inbox event, and wake-up mechanisms for both local and future remote execution.

## Approach

### Recommended: Explicit Routing Layer Inside Delegate Job Execution

`execute_background_delegate_job` should stop embedding all local execution details directly. Instead:

1. build a validated `DelegateRequest`
2. resolve a `DelegationExecutorKind`
3. dispatch to the matching executor backend

For this slice:

- `LocalChildSession` is fully implemented
- `RemoteA2A` is a reserved slot that returns a clear blocked state

This gives us the same external delegate-job contract while introducing the seam needed for future A2A work.

## Routing Semantics

Routing must not require a new job input shape. The routing decision should derive from current delegate request data plus runtime policy.

For now:

- owners like `local-child` resolve to `LocalChildSession`
- owners prefixed with `a2a:` resolve to `RemoteA2A`
- unknown owners default to local

This keeps compatibility while giving future remote delegation an explicit selector that does not require another schema migration.

## Executor Contract

Both executors must conceptually return the same result:

- child/remote session or run identity
- compact delegation result package
- terminal job status or blocked/failure reason

The local executor already satisfies this through child sessions. The remote executor will later satisfy it through an A2A adapter while reusing the same result package and inbox event path.

## Remote Placeholder Behavior

Because remote A2A is not implemented in this slice:

- routing to `RemoteA2A` must not silently fall back to local
- the delegate job should become `blocked`
- the job error/last progress should clearly say that remote delegation is not configured
- no fake child session should be created

This keeps behavior honest and testable.

## Testing

Required coverage:

- routing resolves local owners to `LocalChildSession`
- routing resolves `a2a:` owners to `RemoteA2A`
- local route preserves the existing child-session delegation behavior
- remote route blocks the job with a clear error and no child session

## Follow-On Work

After this slice:

1. implement remote A2A adapter behind the existing `RemoteA2A` slot
2. add model-facing delegation launch tools
3. add judge sessions as a specialization of local or remote delegation
