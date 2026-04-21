# Remote A2A Delegation Design

## Goal

Add a remote A2A executor behind the existing delegation routing seam so one daemon can delegate work to another daemon without introducing a second runtime path or polling the model.

## Scope

This slice covers:

- statically configured A2A peers in daemon config
- a public base URL for callback targets
- durable job state for "waiting on remote executor"
- daemon HTTP endpoints to accept remote delegation and receive completion callbacks
- a remote executor path for delegate jobs routed to `a2a:<peer-id>`

This slice does not cover:

- dynamic peer discovery
- judge policies
- multi-hop mesh routing
- MCP transport

## Constraints

- Preserve one canonical runtime path.
- Reuse the current delegate job shape and durable result package.
- Do not rehydrate full child transcripts into parent prompts.
- Keep daemon, CLI, and TUI thin over the same app/runtime layer.

## Approach

### Recommended: Configured peers plus callback completion

Remote delegation uses the existing `JobKind::Delegate` routing seam:

- `local-child` and unknown owners still resolve to the local child-session executor
- `a2a:<peer-id>` resolves to a remote executor

The remote executor:

1. looks up `<peer-id>` in daemon config
2. requires the local daemon to expose a configured `public_base_url`
3. sends a delegation envelope to the remote daemon over HTTP/JSON
4. marks the local job as `waiting_external`
5. waits for a callback from the remote daemon with the compact result package

This keeps the orchestration substrate canonical:

- parent session and parent job stay local
- remote daemon executes work in its own child session
- only the compact result package crosses back
- local session wake-up still happens through the same inbox path

## Durable Substrate Changes

### Job status

Add `waiting_external` to `JobStatus`.

This status means:

- the job has already been accepted by an external executor
- the local daemon must not rerun it
- completion will arrive asynchronously via callback

### Callback metadata

Add optional callback metadata to jobs, stored durably:

- callback URL
- callback bearer token
- remote parent identifiers
- callback sent timestamp

This lets the remote daemon retry callback delivery across restarts without polling the model.

## HTTP Contract

### Outbound request

`POST /v1/a2a/delegations`

Payload includes:

- parent session/job IDs
- label
- goal
- bounded context
- write scope
- expected output
- callback target

Response:

- accepted
- remote session ID
- remote job ID

### Completion callback

`POST /v1/a2a/delegations/{job-id}/complete`

Payload includes terminal outcome:

- completed result package
- or failure/block reason
- remote session/job identifiers

The parent daemon persists the local delegate job outcome and enqueues the normal inbox event.

## Remote Execution Semantics

The receiving daemon creates:

- a child session with lineage metadata copied from the remote parent identifiers
- a system transcript entry describing the delegated task
- a background chat-turn job in that child session

That child job uses the normal chat execution path. On terminal completion, the remote daemon computes the same compact result package shape already used by local delegation.

## Wake-up Semantics

Only the parent daemon wakes the parent session.

Remote child sessions do not emit local wake-up turns for their own callback-driven jobs. They exist as execution context, not as operator-facing parent sessions.

## Testing

Required coverage:

- config round-trip for A2A peers and public base URL
- remote delegate jobs become `waiting_external` after acceptance
- remote daemon accepts delegation and creates a child session plus background chat job
- completion callback updates the parent job and wakes the parent session through the inbox path
- missing peer or missing public base URL blocks the job honestly

## Follow-on Work

After this slice:

1. add richer remote peer capabilities and routing policy
2. add judge sessions on top of local/remote delegation
3. extend from single remote executor to broader mesh behavior
