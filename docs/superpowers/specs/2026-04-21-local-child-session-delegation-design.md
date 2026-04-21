# Local Child-Session Delegation Design

## Goal

Add local child-session delegation on top of the existing durable background job and session inbox substrate, so a parent session can launch a bounded delegated task that runs in a child session and later wakes the parent with a compact result package.

## Scope

This slice covers:

- a canonical local delegation job executor for `JobKind::Delegate`
- child-session creation owned by the daemon/runtime
- delegation result packages with compact summaries and artifact refs
- a `delegation_result_ready` inbox event delivered back to the parent session
- operator-visible transcript entries in the parent session

This slice does not cover:

- remote A2A delegation transport
- delegation routing across multiple executors
- model-facing delegation tools
- judge policy or verdict schemas

## Constraints

- Preserve one canonical runtime path for chat, approvals, background jobs, wake-ups, and delegated child work.
- Do not create a second prompt assembly path for child sessions.
- Keep prompt assembly ordered as:
  1. `SYSTEM.md`
  2. `AGENTS.md`
  3. `SessionHead`
  4. `Plan`
  5. `ContextSummary`
  6. offload refs
  7. uncovered transcript tail
- Child work must reuse the same chat execution path used by foreground and background chat turns.
- Delegation results must stay compact in parent-session context. Full child transcripts are not silently rehydrated into prompts.

## Approach

### Recommended: Delegate Job Creates a Child Session and Returns a Result Package

`JobKind::Delegate` remains a normal durable background job. Its executor:

1. creates a child session with stable parent linkage
2. writes operator-visible system entries into parent and child transcripts
3. runs the delegated goal inside the child session through the canonical chat turn path
4. compacts the outcome into a delegation result package
5. emits `delegation_result_ready` into the parent session inbox

This keeps delegation on the same substrate as every other long-running job and preserves one daemon-owned wake-up path.

## Data Model

### Session Linkage

Sessions gain optional delegation metadata:

- `parent_session_id: Option<String>`
- `parent_job_id: Option<String>`
- `delegation_label: Option<String>`

These fields are for lineage and operator visibility. They do not change prompt assembly order.

### Delegate Job Input

`JobExecutionInput::Delegate` expands from a minimal placeholder into a structured request:

- `label`
- `goal`
- `expected_output`
- `bounded_context`
- `write_scope`
- `owner`

The shape mirrors the existing `DelegateRequest` validation rules instead of inventing a second delegation schema.

### Delegate Job Result

`JobResult` gains a `Delegation` variant containing:

- `child_session_id`
- `summary`
- `changed_paths`
- `artifact_refs`
- `residual_risks`

This is the durable record of what the child session produced. The parent session only sees a compact wake-up event plus operator-visible transcript entry.

## Child Session Lifecycle

When a delegate job starts:

1. the daemon creates a child session under the parent
2. the child session title is derived from the label or goal
3. the child session receives a system transcript entry describing the delegated objective and boundaries
4. the parent session receives a system transcript entry that the delegation started

The child session then runs one canonical chat turn with the delegation goal as user input. Later slices can evolve this into richer child-session loops without changing the basic substrate.

## Parent Wake-Up Semantics

On delegate completion, failure, or cancellation:

- the job reaches a terminal state in durable job storage
- the daemon creates exactly one `delegation_result_ready` inbox event for successful completion
- failures continue to emit the normal failure wake-up path

When the parent session is idle, the daemon wake-up loop consumes the inbox event and starts the normal wake-up turn. The parent transcript shows:

- delegation started
- delegation result ready

The model then decides what to do next from the compact result package already present in session context.

## Operator Visibility

The minimum visible information should be:

- parent session background job list shows the delegate job as `delegate`
- child session is a normal session and can be opened directly
- parent transcript includes the child session id and compact summary

This makes delegation inspectable without requiring a special subagent UI in this slice.

## Failure Handling

- If child session creation fails, the delegate job fails normally.
- If child chat execution fails, the delegate job fails and the parent gets a standard failure wake-up.
- If result packaging fails validation, the delegate job fails rather than returning malformed changed paths or artifact refs.

## Testing

Required coverage:

- delegate jobs create child sessions with correct lineage metadata
- child sessions execute delegated work through the canonical chat turn path
- successful delegation writes a durable `JobResult::Delegation`
- successful delegation emits exactly one `delegation_result_ready` inbox event
- parent wake-up transcript shows a compact delegation summary instead of full child transcript rehydration
- failed child execution fails the delegate job and uses the existing failure wake-up path

## Follow-On Work

After this slice:

1. delegation routing with a local executor and future remote executor slot
2. model-facing background/delegation launch surface
3. remote A2A adapter
4. judge sessions as a specialization of local or remote delegation
