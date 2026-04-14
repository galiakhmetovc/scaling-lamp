# Web Session Test Bench Design

Date: 2026-04-13
Status: draft

## Goal

Build a local web interface for `teamD` that lets an operator:

- create a session
- send a message into that session
- watch the raw session transcript change over time
- inspect how compaction, pruning, SessionHead, artifacts, replay, and prompt assembly affect that session

The web UI is a testing bench for session behavior first, and a debugger second.

It must let the operator inspect:

- the raw session transcript
- every transcript mutation over time
- how session-local state and recall layers were assembled
- run state transitions
- prompt-assembly context layers
- compaction and pruning effects
- approvals, timeouts, and artifacts in the same timeline

The product is not a polished operator console. It is an interactive local test bench with full runtime visibility.

## Primary User Need

The operator wants to test the runtime by sending a message and then seeing, literally, what happened to the session over time:

- what messages existed at each point
- what new messages were appended
- when tool calls and tool results entered the transcript
- when compaction or pruning changed what was carried forward
- when `SessionHead` changed
- what context was actually assembled for a given round
- when artifacts were offloaded and referenced
- how the session-level state changed after each run
- which layer supplied a fact: transcript, SessionHead, recent_work, memory recall, checkpoint, continuity, workspace, or skills

This is primarily a runtime-debugging and trust problem, not a presentation problem.

## Non-Goals

- no approval decisions in phase 1
- no cancel in phase 1
- no policy edits in phase 1
- no second runtime path beside the existing API/control plane
- no separate frontend deployment in phase 1
- no side-by-side multi-run compare in phase 1
- no hidden chain-of-thought rendering
- no mesh-specific UX in phase 1

## Recommendation

Build an embedded local interactive test bench inside the existing Go binary.

The UI should be served by the current server process and consume the same runtime-facing surfaces that already power CLI and Telegram:

- HTTP API
- `AgentCore`
- `runtime_events`
- SSE event stream
- replay endpoints
- session and control snapshots

This avoids a second deploy pipeline and keeps one auth boundary.

## Alternatives Considered

### 1. Embedded Read-Only Debug Console

Serve HTML, CSS, and JavaScript from the current Go binary and use the existing API and SSE stream.

Pros:

- one process and one auth boundary
- direct fit for current API-first architecture
- fastest path to operator value
- no new backend contract split

Cons:

- frontend is less independently deployable
- future rich UI work may eventually want a separate app

### 2. Separate Frontend App

Create a dedicated frontend that talks to the existing API.

Pros:

- cleaner long-term frontend separation
- easier future evolution into a full operator web console

Cons:

- unnecessary moving parts for phase 1
- extra build and deploy complexity now
- higher risk of drift from runtime/server version

### 3. Replay-Only Static Inspector

Build only post-hoc replay pages without live updates.

Pros:

- lowest realtime complexity
- good for forensic debugging

Cons:

- does not satisfy "I want to see how it changes"
- weak for live prompt and transcript debugging

### Chosen Path

Approach 1.

## Product Shape

The web UI is a local interactive test bench with one primary write action:

- submit a user message into a selected session

Everything else remains read-only in phase 1.

The main job is:

- expose runtime and transcript truth with enough fidelity that an operator can understand why the agent behaved as it did

The UI is not a polished general-purpose chat client.

It is specifically a place to inspect all runtime layers that influence a session:

- transcript
- SessionHead
- recent_work
- memory recall
- checkpoint and continuity
- workspace context
- skills context
- prompt budgeting
- compaction and pruning
- artifact offload

## Canonical Runtime Boundary

The web debugger is a client over runtime-owned data. It must not invent its own state model.

The source of truth remains:

- `AgentCore`
- runtime store
- runtime events
- replay
- session snapshots
- control snapshots

The web layer may maintain client-side view state such as:

- selected session
- selected run
- expanded timeline rows
- active filters

It must not own runtime semantics.

## Canonical Debugging Questions

The UI must make these questions answerable without reading server logs:

- what did the transcript contain before this round?
- what did `SessionHead` contain before this round?
- what came from `recent_work` rather than transcript?
- what came from `memory recall` rather than recent session state?
- what came from checkpoint or continuity?
- what came from workspace bootstrap files?
- what came from skills?
- what was pruned before assembly?
- what was compacted into checkpoint or continuity?
- what was moved into artifacts instead of staying inline?
- why did the model appear to remember or forget a fact?

## Core UX

### Top Bar

Shows:

- current session
- selected run
- live/idle indicator
- auth state
- current filters

### Left Pane: Session and Run Picker

Purpose:

- choose the session to inspect
- create a new session
- choose a run inside that session
- narrow scope without leaving the page

Contents:

- recent sessions
- `new session`
- active session marker
- runs for selected session
- active/completed/failed filters
- latest run metadata summary

### Center Pane: Chat and Transcript

Purpose:

- let the operator send a message into the selected session
- show the live chat transcript
- anchor the testing loop around one selected session

Required:

- transcript view that stays close to raw stored messages
- input bar
- submit button or keyboard send
- clear run boundary markers
- visible operator notes, tool results, checkpoint inserts, and recent-work inserts

This is the place where the operator performs the test action and sees the human-readable session flow.

### Right/Secondary Timeline Pane: Transcript Timeline

Purpose:

- show the raw session transcript as a timeline of mutations

This is the primary debugging surface.

Every timeline row must show:

- timestamp
- run id if present
- event type
- short summary
- expandable raw payload

Timeline row types in phase 1:

- `transcript.appended`
- `transcript.pruned`
- `transcript.compacted`
- `run.started`
- `run.completed`
- `run.failed`
- `run.waiting_operator`
- `tool.called`
- `tool.completed`
- `artifact.offloaded`
- `approval.requested`
- `approval.decided`
- `session_head.updated`
- `memory.recalled`
- `checkpoint.saved`
- `continuity.saved`
- `recent_work.prepared`
- `workspace.injected`
- `skills.injected`
- `timeout.decision_requested`
- `timeout.auto_continue`

Transcript append rows must distinguish message kinds:

- `user`
- `assistant`
- `tool`
- `system`
- `operator_note`
- `checkpoint_insert`
- `recent_work_insert`
- `memory_recall_insert`
- `workspace_insert`
- `skills_insert`

The operator must be able to see both:

- the raw appended content
- the transcript shape after that append

This implies per-row snapshot support or reconstructable snapshots.

### Right Pane: State and Assembly Inspector

Purpose:

- show why the transcript currently looks the way it does
- expose prompt and runtime state that the center timeline alone cannot explain

Sections:

- `Run State`
  - stage
  - waiting reason
  - status
  - current tool
  - approvals
  - timeout state
- `SessionHead`
  - last completed run
  - current goal
  - last result summary
  - recent artifacts
  - current project
  - open loops if present
- `Recall and Session State Provenance`
  - current SessionHead snapshot
  - recent_work contribution
  - memory recall contribution
  - checkpoint contribution
  - continuity contribution
  - workspace contribution
  - skills contribution
  - per-layer source refs where available
- `Prompt Assembly`
  - workspace block
  - SessionHead block
  - recent_work block
  - memory recall block
  - checkpoint block
  - continuity block
  - skills block
  - transcript tail
  - older prefix
  - pruned content summary
- `Budget`
  - raw transcript estimate
  - system overhead
  - final prompt estimate
  - percentage of prompt budget
  - percentage of model context window
  - compaction trigger state
  - projected compaction decision inputs

This panel exists specifically so the operator can answer:

- why did compaction happen here?
- why did this fact survive or disappear?
- why was a tool result moved into an artifact?
- what recent session truth was carried into the next round?
- did this fact come from transcript, SessionHead, memory recall, checkpoint, continuity, workspace, or skills?

## Debug Model

The web UI must make three layers separable:

### 1. Transcript Truth

What messages were stored for the session.

### 2. Prompt Assembly Truth

What subset and what injected context were assembled for a given model round.

This includes provenance across session-state and recall layers:

- SessionHead
- recent_work
- memory recall
- checkpoint
- continuity
- workspace
- skills

### 3. Runtime State Truth

What the run believed it was doing:

- state
- tool
- approval wait
- timeout state
- worker or job activity

The interface must make it hard to confuse these layers.

## Required API and Event Surfaces

The existing API is not yet enough for a good transcript debugger without client-side over-joining. Phase 1 should add a dedicated debug view model.

### Snapshot Endpoints

Add read-only endpoints:

- `GET /api/debug/sessions?chat_id=<id>&limit=<n>`
  - optimized session list for debugger
- `GET /api/debug/sessions/{session_id}`
  - session snapshot including:
    - latest run
    - SessionHead
    - latest recall/session-state provenance snapshot
    - budget snapshot
    - recent artifacts
- `POST /api/debug/sessions/{session_id}/messages`
  - phase-1 single write action
  - submit a user message into the selected session
  - starts a normal runtime run using the canonical execution path
- `GET /api/debug/runs/{run_id}`
  - run snapshot including:
    - replay summary
    - control state
    - artifact refs
    - latest prompt budget
- `GET /api/debug/runs/{run_id}/timeline`
  - ordered timeline rows for one run
- `GET /api/debug/sessions/{session_id}/transcript`
  - current raw transcript
- `GET /api/debug/sessions/{session_id}/transcript/timeline`
  - transcript mutation timeline
- `GET /api/debug/runs/{run_id}/prompt-rounds`
  - prompt assembly snapshots per round
- `GET /api/debug/runs/{run_id}/context-provenance`
  - structured view of where assembled context came from:
    - transcript
    - SessionHead
    - recent_work
    - memory recall
    - checkpoint
    - continuity
    - workspace
    - skills

### Live Stream

Add:

- `GET /api/debug/stream?session_id=<id>&run_id=<id>&after_id=<n>`

This can wrap the existing SSE stream but should emit a debugger-facing event envelope that is already normalized for the web UI.

The stream should be sufficient to update:

- chat transcript
- transcript timeline
- run state
- SessionHead view
- prompt and budget panels

## Required Runtime Instrumentation

To make the debugger accurate, we need first-class events for transcript and prompt changes.

### New Event Types

Add persisted runtime events for:

- `transcript.appended`
- `transcript.pruned`
- `transcript.compacted`
- `session_head.updated`
- `recent_work.prepared`
- `memory.recalled`
- `workspace.injected`
- `skills.injected`
- `prompt.assembled`
- `prompt.layer_budgeted`
- `prompt.layer_pruned`
- `prompt.compaction_triggered`

### Transcript Mutation Envelope

Each transcript mutation event must carry enough data to reconstruct the change:

- session id
- run id if present
- message role
- message subtype
- content preview
- full content ref or inline raw content
- transcript length before
- transcript length after
- optional snapshot cursor

When the mutation originates from an injected non-transcript layer, the event should also carry:

- `source_layer`
- `source_ref`
- `source_summary`

### Prompt Assembly Snapshot

For each model round, persist a structured debug snapshot:

- round number
- run id
- final prompt estimate
- layer breakdown
- layer provenance
- what was pruned
- whether compaction was triggered
- resulting transcript window boundaries

This does not require storing the full prompt text forever in phase 1, but it must store enough structure to explain assembly.

## UI Behavior

### Session Selection

Default to the most recent active session. The operator can switch session from the left pane.

### Session Creation

The operator can create a new named session from the left pane without leaving the page.

This should call the existing generic session action path, not invent a web-only session flow.

### Run Selection

If no run is selected:

- show session-wide transcript timeline

If a run is selected:

- filter and highlight rows related to that run

### Message Submission

Submitting a message must:

- use the canonical runtime execution path
- append a normal `user` message
- create a normal run
- allow the operator to watch the resulting timeline live

This is intentionally the only phase-1 write action.

After submission, the interface should let the operator inspect the full causality chain for the resulting run:

- transcript appends
- SessionHead changes
- recall injections
- checkpoint and continuity saves
- prompt assembly decisions
- compaction or pruning
- artifact offload

### Timeline Navigation

Required:

- auto-scroll toggle
- collapse and expand payloads
- filter by event type
- filter by run
- jump to compaction point
- jump to approval
- jump to tool call

### Snapshot Inspection

Clicking a timeline row should update the right pane to that point-in-time view when possible.

The operator should be able to answer:

- what had happened by this point?
- what was in transcript by this point?
- what would the next prompt have seen?

## Security and Access Model

Phase 1 uses the existing local operator auth boundary.

Requirements:

- web routes are local or trusted-network only under the same server process
- same bearer token model as current HTTP API
- no anonymous access
- one bounded write action: submit user message
- no secrets-only convenience rendering

The UI must avoid promoting credentials into dedicated summary cards. If secrets exist in transcript or files, they appear only because the underlying runtime data already contains them; the web UI must not amplify them.

## Frontend Technology

For phase 1, keep the frontend minimal and embedded.

Recommended:

- server-served static assets from the Go binary
- lightweight JavaScript application
- no separate node-based mandatory build step if avoidable

The point is debugger utility, not frontend framework sophistication.

If a library is used, it should support:

- incremental rendering of long timelines
- efficient diff updates from SSE
- collapsible panels

## Data Model

### Debug Session View

Fields:

- `session_id`
- `chat_id`
- `latest_run_id`
- `active_run`
- `session_head`
- `budget_snapshot`
- `recent_artifacts`

### Debug Timeline Row

Fields:

- `event_id`
- `timestamp`
- `session_id`
- `run_id`
- `kind`
- `subtype`
- `summary`
- `payload`
- `raw_content_ref`
- `snapshot_ref`

### Prompt Round View

Fields:

- `run_id`
- `round`
- `assembled_at`
- `final_prompt_estimate`
- `layer_breakdown`
- `pruned_layers`
- `compaction_triggered`
- `transcript_window`

## Incremental Rollout

### Phase 1: Debug Data Surfaces and Message Entry

- add debug endpoints
- add transcript mutation events
- add prompt assembly snapshots
- add session message submit endpoint
- no advanced control actions yet

### Phase 2: Embedded Interactive UI Shell

- session list
- new session
- run list
- chat transcript and input
- transcript timeline pane
- state and prompt inspector right pane
- SSE live updates

### Phase 3: Timeline Fidelity and Polish

- point-in-time snapshot reconstruction
- filters
- collapse and expand payloads
- jump links
- compaction/pruning visualization

### Phase 4: Post-Phase-1 Decisions

Decide later whether the web test bench should remain narrowly interactive or grow into a full operator surface.

This decision must be separate from phase 1.

## Testing Strategy

### Runtime Tests

- transcript append emits `transcript.appended`
- pruning emits `transcript.pruned`
- compaction emits `transcript.compacted`
- SessionHead save emits `session_head.updated`
- memory recall emits `memory.recalled`
- recent_work emits `recent_work.prepared`
- workspace and skills injections emit debug events where applicable
- prompt assembly writes prompt round snapshots

### API Tests

- debug endpoints return stable shapes
- timeline ordering is deterministic
- SSE debug stream resumes from cursor

### UI Tests

- session selection loads timeline
- live events append without full reload
- timeline filters work
- point-in-time inspector updates correctly

## Risks

### 1. Event Volume

Transcript and prompt-layer events may become large.

Mitigation:

- keep payload previews inline and raw payloads behind refs
- paginate historical timeline
- bound retained prompt assembly detail if needed

### 2. Over-Joining in the Browser

If the UI has to correlate transcript, replay, run state, and budget from many endpoints, it will become fragile.

Mitigation:

- add dedicated debug view endpoints early

### 3. Secret Exposure

A raw transcript debugger can expose sensitive material already present in stored state.

Mitigation:

- no special secret surfacing
- preserve current auth boundary
- later add redaction hooks if needed

### 4. Confusion Between Replay and Live Timeline

Operators may conflate replay data and transcript mutation truth.

Mitigation:

- visually separate:
  - transcript timeline
  - prompt assembly
  - replay summary
  - runtime state

## Success Criteria

Phase 1 is successful when an operator can:

- create a new session
- submit a message from the web UI
- pick a session
- pick a run
- watch transcript mutations live
- see when compaction or pruning changed context
- inspect SessionHead updates
- inspect memory recall and recent_work contribution
- inspect checkpoint and continuity contribution
- inspect workspace and skills contribution
- inspect prompt-layer budget composition
- inspect artifact offload behavior
- explain, from the UI alone, why a later run appeared to "forget" or carry forward a fact

## Relationship To Existing Roadmaps

This web debugger is complementary to:

- Telegram operator UX
- terminal TUI
- replay and inspection depth

It should reuse the same runtime surfaces and instrumentation work, not fork them.

The correct long-term shape is:

- Telegram: operational remote control
- TUI: terminal operator IDE
- Web test bench: richest local chat-plus-timeline inspector for session behavior
