# Chat Runtime V2 Corrective Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current chat execution path with a new run engine that has one canonical run snapshot, explicit shell contracts, serialized approval/process continuations, and a TUI that renders `Chat` and `Tools` from one truth source.

**Architecture:** Build a parallel `runtimev2` path instead of continuing to patch the current runtime. The new path introduces a dedicated run state machine, explicit `exec_*` and `shell_snippet_*` tool families, daemon routing for V2 chat runs, and a TUI cutover where `Chat` and `Tools` consume `RunSnapshotV2` only. Keep `Workspace` on the existing path for now.

**Tech Stack:** Go, existing clean-room daemon/http+websocket operator surface, Bubble Tea TUI, current provider/tool loop, event log and projections, existing shell executor primitives where reusable.

---

## File Structure

### Existing files to modify

- `internal/runtime/agent.go`
  - add entrypoints or adapters for V2 chat execution
- `internal/runtime/daemon/commands.go`
  - route chat commands and shell approval commands into V2
- `internal/runtime/daemon/server.go`
  - initialize V2 services and inject them into command handlers
- `internal/runtime/daemon/session_versioning.go`
  - assign per-session or per-run execution version and enforce V1/V2 exclusivity
- `internal/runtime/daemon/session_snapshot.go`
  - expose `RunSnapshotV2` fields in session snapshots or a dedicated V2 snapshot payload
- `internal/runtime/daemon/server_test.go`
  - add daemon-level integration tests for V2 run lifecycle
- `internal/runtime/tui/client.go`
  - add TUI client calls for V2 run snapshot and V2 approval/process actions
- `internal/runtime/tui/state.go`
  - replace mixed V1 chat/tool state with V2-backed view state
- `internal/runtime/tui/app.go`
  - stop mutating run truth from websocket status events
- `internal/runtime/tui/daemon_events.go`
  - reduce events to wakeups/stream updates
- `internal/runtime/tui/chat_pane.go`
  - render live rail, approvals, and run status from `RunSnapshotV2`
- `internal/runtime/tui/tools_data.go`
  - derive running/pending/current state from `RunSnapshotV2`
- `internal/runtime/tui/tools_view.go`
  - render current tool state from V2 snapshot, not V1 activity heuristics
- `internal/runtime/tui/client_model_test.go`
  - replace and extend TUI regressions for V2 behavior
- `internal/shell/executor.go`
  - reuse only the low-level process execution and long-polling pieces that remain valid under V2

### New runtime files

- `internal/runtimev2/types.go`
  - shared V2 structs: `RunSnapshotV2`, `PendingApprovalV2`, `ActiveProcessV2`, `RecentStepV2`, queued messages, provider stream, and terminal result
- `internal/runtimev2/engine.go`
  - run engine orchestration, state transitions, serialized continuation loop
- `internal/runtimev2/runstore.go`
  - persistent or durable in-memory store for active V2 runs keyed by session/run id
- `internal/runtimev2/approval.go`
  - approval action handling and idempotency rules
- `internal/runtimev2/process.go`
  - process registration, wait semantics, kill semantics
- `internal/runtimev2/provider_loop.go`
  - provider/tool-call loop for V2 runs
- `internal/runtimev2/tool_contract.go`
  - V2 model-visible tool definitions and validation
- `internal/runtimev2/tool_dispatch.go`
  - dispatch between `exec_*` and `shell_snippet_*`
- `internal/runtimev2/trace.go`
  - V2 trace helpers for run transitions and approval/process steps

### New daemon and snapshot files

- `internal/runtime/daemon/runv2_snapshot.go`
  - dedicated snapshot translation helpers if `session_snapshot.go` becomes crowded
- `internal/runtime/daemon/runv2_commands_test.go`
  - focused daemon tests for V2 commands and transitions

### New TUI files

- `internal/runtime/tui/run_snapshot_v2.go`
  - helpers for rendering V2 snapshot state cleanly in `Chat` and `Tools`
- `internal/runtime/tui/run_snapshot_v2_test.go`
  - focused tests for snapshot-to-view derivation

### New tests

- `internal/runtimev2/engine_test.go`
- `internal/runtimev2/approval_test.go`
- `internal/runtimev2/process_test.go`
- `internal/runtimev2/provider_loop_test.go`
- `internal/runtimev2/tool_dispatch_test.go`
- `internal/runtime/daemon/runv2_commands_test.go`
- `internal/runtime/tui/run_snapshot_v2_test.go`

## Task 1: Add RunSnapshotV2 Types And State Store

**Files:**
- Create: `internal/runtimev2/types.go`
- Create: `internal/runtimev2/runstore.go`
- Test: `internal/runtimev2/engine_test.go`

- [ ] **Step 1: Write the failing state-shape tests**

Add tests that assert a V2 run snapshot can represent:
- `running`
- `waiting_approval`
- `waiting_process`
- `resuming`
- `completed`
- `failed`
- `cancelled`

Also assert that approvals and active processes live inside the run snapshot, not in detached top-level state.

Also assert that the snapshot explicitly carries:
- queued user messages
- provider stream state
- terminal result state
- error state

- [ ] **Step 2: Run the tests to verify they fail**

Run: `go test ./internal/runtimev2 -run 'TestRunSnapshotV2|TestRunStore' -count=1`
Expected: FAIL because `internal/runtimev2` does not exist yet.

- [ ] **Step 3: Implement minimal V2 state types and store**

Implement:
- `RunSnapshotV2`
- `PendingApprovalV2`
- `ActiveProcessV2`
- `RecentStepV2`
- `QueuedUserMessageV2`
- `ProviderStreamV2`
- `RunResultV2`
- `RunStore` with `Create`, `Get`, `Update`, `Delete`, `ListActiveBySession`

Keep the store minimal and focused on correctness, not long-term persistence.

- [ ] **Step 4: Re-run the tests**

Run: `go test ./internal/runtimev2 -run 'TestRunSnapshotV2|TestRunStore' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtimev2/types.go internal/runtimev2/runstore.go internal/runtimev2/engine_test.go
git commit -m "feat: add runtime v2 run snapshot and store"
```

## Task 2: Add V2 Cutover Gate And Version Discriminator

**Files:**
- Create: `internal/runtime/daemon/session_versioning.go`
- Modify: `internal/runtime/daemon/server.go`
- Modify: `internal/runtime/daemon/session_snapshot.go`
- Test: `internal/runtime/daemon/runv2_commands_test.go`

- [ ] **Step 1: Write the failing cutover tests**

Cover:
- a new chat run can be explicitly created as V2
- an existing V1 session remains V1 until explicitly migrated
- one session/run cannot mix V1 and V2 handlers
- snapshot responses include which execution version owns the active run

- [ ] **Step 2: Run the tests to verify they fail**

Run: `go test ./internal/runtime/daemon -run 'TestRunVersionCutover|TestNoMixedRunVersion' -count=1`
Expected: FAIL because there is no execution version gate yet.

- [ ] **Step 3: Implement minimal versioning**

Implement:
- per-session or per-run execution version marker
- daemon enforcement so a run is either V1 or V2, never mixed
- default routing flag for new chat runs
- snapshot exposure of the current execution version

- [ ] **Step 4: Re-run the tests**

Run: `go test ./internal/runtime/daemon -run 'TestRunVersionCutover|TestNoMixedRunVersion' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/daemon/session_versioning.go internal/runtime/daemon/server.go internal/runtime/daemon/session_snapshot.go internal/runtime/daemon/runv2_commands_test.go
git commit -m "feat: add runtime v2 cutover gate"
```

## Task 3: Add Explicit Tool Contract V2

**Files:**
- Create: `internal/runtimev2/tool_contract.go`
- Create: `internal/runtimev2/tool_dispatch.go`
- Test: `internal/runtimev2/tool_dispatch_test.go`

- [ ] **Step 1: Write the failing tool-contract tests**

Cover:
- `exec_start` accepts `executable`, `args`, `cwd`, `env`
- `exec_start` rejects shell operators like `&&`, pipes, redirects, and builtins-as-command
- `shell_snippet_start` accepts shell script text
- `shell_snippet_start` preserves shell semantics for `cd dir && cmd`

- [ ] **Step 2: Run the tests to verify they fail**

Run: `go test ./internal/runtimev2 -run 'TestExecStartContract|TestShellSnippetContract' -count=1`
Expected: FAIL because the V2 tool contract is missing.

- [ ] **Step 3: Implement the minimal V2 contract**

Implement:
- tool descriptors for `exec_start`, `exec_wait`, `exec_kill`
- tool descriptors for `shell_snippet_start`, `shell_snippet_wait`, `shell_snippet_kill`
- validation helpers that reject ambiguous structured exec inputs
- dispatch helpers that tag each step as structured-exec or shell-snippet

- [ ] **Step 4: Re-run the tests**

Run: `go test ./internal/runtimev2 -run 'TestExecStartContract|TestShellSnippetContract' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtimev2/tool_contract.go internal/runtimev2/tool_dispatch.go internal/runtimev2/tool_dispatch_test.go
git commit -m "feat: add runtime v2 shell tool contracts"
```

## Task 4: Add Process Model With Wait Semantics

**Files:**
- Create: `internal/runtimev2/process.go`
- Modify: `internal/shell/executor.go`
- Test: `internal/runtimev2/process_test.go`

- [ ] **Step 1: Write the failing process tests**

Cover:
- registering an active process in a run
- waiting on a silent running process does not return immediate empty snapshots in a tight loop
- new output wakes the waiter
- process completion transitions from `waiting_process` toward terminal/resuming states
- process kill updates run state coherently

- [ ] **Step 2: Run the tests to verify they fail**

Run: `go test ./internal/runtimev2 -run 'TestProcessWait|TestProcessKill' -count=1`
Expected: FAIL because the V2 process model does not exist.

- [ ] **Step 3: Implement minimal process tracking**

Implement:
- process registration inside a run
- process wait using existing executor long-polling primitives where valid
- process kill
- `next_offset` and chunk accumulation in V2 process state

Keep this layer thin: it owns run-facing state, not low-level `exec.Cmd`.

- [ ] **Step 4: Re-run the tests**

Run: `go test ./internal/runtimev2 -run 'TestProcessWait|TestProcessKill' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtimev2/process.go internal/runtimev2/process_test.go internal/shell/executor.go
git commit -m "feat: add runtime v2 process wait model"
```

## Task 5: Add Approval Model V2

**Files:**
- Create: `internal/runtimev2/approval.go`
- Test: `internal/runtimev2/approval_test.go`

- [ ] **Step 1: Write the failing approval tests**

Cover:
- one blocked step creates one pending approval inside the run snapshot
- `approve_once`, `approve_always`, `deny_once`, `deny_always` are idempotent
- repeated submit on the same approval is a no-op, not an error
- two approvals on the same run serialize correctly
- `cancel_and_send` resolves the blocked step and enqueues a normal user message

- [ ] **Step 2: Run the tests to verify they fail**

Run: `go test ./internal/runtimev2 -run 'TestApprovalLifecycle|TestApprovalIdempotency|TestCancelAndSend' -count=1`
Expected: FAIL because V2 approval handling does not exist.

- [ ] **Step 3: Implement minimal approval handler**

Implement:
- per-run approval state
- idempotent approval action handling
- serialized continuation lock per run
- `cancel_and_send` path that appends a user message back into the run queue

- [ ] **Step 4: Re-run the tests**

Run: `go test ./internal/runtimev2 -run 'TestApprovalLifecycle|TestApprovalIdempotency|TestCancelAndSend' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtimev2/approval.go internal/runtimev2/approval_test.go
git commit -m "feat: add runtime v2 approval model"
```

## Task 6: Add Provider Loop V2 In Two Layers

**Files:**
- Create: `internal/runtimev2/provider_loop.go`
- Create: `internal/runtimev2/engine.go`
- Test: `internal/runtimev2/provider_loop_test.go`
- Test: `internal/runtimev2/engine_test.go`

- [ ] **Step 1: Write the failing provider-step tests**

Cover:
- normal assistant response with no tools
- provider output decoded into V2 step intents
- structured exec tool call leading to `waiting_process`
- shell snippet tool call leading to `waiting_approval`
- tool/provider failure transitions to `failed`

- [ ] **Step 2: Run the provider-step tests to verify they fail**

Run: `go test ./internal/runtimev2 -run 'TestProviderStepDecoder|TestProviderStepTransitions' -count=1`
Expected: FAIL because the provider-step layer does not exist.

- [ ] **Step 3: Implement provider-step decoding**

Implement:
- a narrow provider-step layer that translates provider output into V2 step intents
- explicit step results for assistant text, approval-blocked tool calls, process-starting tool calls, and hard failures

Keep this layer stateless apart from provider response handling.

- [ ] **Step 4: Re-run the provider-step tests**

Run: `go test ./internal/runtimev2 -run 'TestProviderStepDecoder|TestProviderStepTransitions' -count=1`
Expected: PASS

- [ ] **Step 5: Write the failing engine orchestration tests**

Cover:
- resume after approval returns to `resuming`
- resume after process update returns to `resuming`
- queued user messages are preserved in run state
- run completion transitions to `completed`

- [ ] **Step 6: Run the engine tests to verify they fail**

Run: `go test ./internal/runtimev2 -run 'TestProviderLoop|TestEngineTransitions' -count=1`
Expected: FAIL because the engine and provider loop do not exist.

- [ ] **Step 7: Implement minimal engine orchestration**

Implement:
- one serialized loop per run
- provider-step consumption from `provider_loop.go`
- tool dispatch through V2 contracts
- explicit phase transitions
- wakeups on approval/process updates
- queued user message re-entry
- terminal result storage in run state

Do not mix V1 tool loop code into V2 beyond narrow adapters where unavoidable.

- [ ] **Step 8: Re-run the engine tests**

Run: `go test ./internal/runtimev2 -run 'TestProviderLoop|TestEngineTransitions' -count=1`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add internal/runtimev2/engine.go internal/runtimev2/provider_loop.go internal/runtimev2/engine_test.go internal/runtimev2/provider_loop_test.go
git commit -m "feat: add runtime v2 chat engine"
```

## Task 7: Add Daemon Routing For V2 Chat Runs

**Files:**
- Modify: `internal/runtime/daemon/server.go`
- Modify: `internal/runtime/daemon/commands.go`
- Create: `internal/runtime/daemon/runv2_snapshot.go`
- Test: `internal/runtime/daemon/runv2_commands_test.go`

- [ ] **Step 1: Write the failing daemon routing tests**

Cover:
- `chat.send` starts a V2 run for a V2-marked session
- snapshot responses include V2 run phase and embedded approvals/processes
- shell approval actions route into the V2 approval handler
- kill/wait/process updates route into the V2 process model
- V1 sessions continue to route through V1 until migrated or recreated

- [ ] **Step 2: Run the tests to verify they fail**

Run: `go test ./internal/runtime/daemon -run 'TestChatRunV2|TestShellApprovalV2|TestRunSnapshotV2' -count=1`
Expected: FAIL because daemon command handling is still V1-only.

- [ ] **Step 3: Wire daemon to V2**

Add:
- V2 engine initialization on server startup
- V2 chat send path
- V2 approval commands
- V2 process wait/kill commands
- snapshot translation helpers

Keep old V1 commands only where needed for non-chat subsystems such as `Workspace`.

- [ ] **Step 4: Re-run the tests**

Run: `go test ./internal/runtime/daemon -run 'TestChatRunV2|TestShellApprovalV2|TestRunSnapshotV2' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/daemon/server.go internal/runtime/daemon/commands.go internal/runtime/daemon/runv2_snapshot.go internal/runtime/daemon/runv2_commands_test.go
git commit -m "feat: route daemon chat runs through runtime v2"
```

## Task 8: Cut TUI Chat To RunSnapshotV2

**Files:**
- Modify: `internal/runtime/tui/client.go`
- Modify: `internal/runtime/tui/state.go`
- Modify: `internal/runtime/tui/app.go`
- Modify: `internal/runtime/tui/daemon_events.go`
- Modify: `internal/runtime/tui/chat_pane.go`
- Create: `internal/runtime/tui/run_snapshot_v2.go`
- Test: `internal/runtime/tui/run_snapshot_v2_test.go`
- Test: `internal/runtime/tui/client_model_test.go`

- [ ] **Step 1: Write the failing TUI chat tests**

Cover:
- approval menu visibility comes only from V2 snapshot approvals
- `waiting_process` shows active process state in the chat rail
- `AGENT END TURN` appears only for terminal phases
- websocket `idle/completed` events cannot clobber V2 snapshot truth
- `cancel_and_send` updates snapshot-backed chat state coherently
- queued user messages render from V2 snapshot state

- [ ] **Step 2: Run the tests to verify they fail**

Run: `go test ./internal/runtime/tui -run 'TestChatRunSnapshotV2|TestApprovalMenuV2|TestWaitingProcessV2' -count=1`
Expected: FAIL because the TUI is still wired to mixed V1 state.

- [ ] **Step 3: Implement V2-backed chat rendering**

Implement:
- snapshot-to-view helpers in `run_snapshot_v2.go`
- status bar rendering from V2 phase
- approval menu rendering from `pending_approvals`
- live process rail rendering from `active_processes`
- event handling that wakes or streams but does not mutate truth

- [ ] **Step 4: Re-run the tests**

Run: `go test ./internal/runtime/tui -run 'TestChatRunSnapshotV2|TestApprovalMenuV2|TestWaitingProcessV2' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/tui/client.go internal/runtime/tui/state.go internal/runtime/tui/app.go internal/runtime/tui/daemon_events.go internal/runtime/tui/chat_pane.go internal/runtime/tui/run_snapshot_v2.go internal/runtime/tui/run_snapshot_v2_test.go internal/runtime/tui/client_model_test.go
git commit -m "feat: render chat from runtime v2 snapshot"
```

## Task 9: Cut TUI Tools To RunSnapshotV2

**Files:**
- Modify: `internal/runtime/tui/tools_data.go`
- Modify: `internal/runtime/tui/tools_view.go`
- Modify: `internal/runtime/tui/client_model_test.go`

- [ ] **Step 1: Write the failing tools-pane tests**

Cover:
- current approvals shown in `Tools` come from V2 snapshot, not `ToolLog`
- current running processes shown in `Tools` come from V2 snapshot, not stale activities
- `Chat` and `Tools` show the same active approval/process state for one run

- [ ] **Step 2: Run the tests to verify they fail**

Run: `go test ./internal/runtime/tui -run 'TestToolsSnapshotV2|TestChatToolsConsistencyV2' -count=1`
Expected: FAIL because the tools pane still depends on V1 heuristics.

- [ ] **Step 3: Implement V2-backed tools rendering**

Implement:
- current-state sections from `pending_approvals` and `active_processes`
- historical list kept only as historical context
- no approval truth inference from `ToolLog`

- [ ] **Step 4: Re-run the tests**

Run: `go test ./internal/runtime/tui -run 'TestToolsSnapshotV2|TestChatToolsConsistencyV2' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/tui/tools_data.go internal/runtime/tui/tools_view.go internal/runtime/tui/client_model_test.go
git commit -m "feat: render tools pane from runtime v2 snapshot"
```

## Task 10: Add Real-World Integration Regressions

**Files:**
- Test: `internal/runtimev2/engine_test.go`
- Test: `internal/runtime/daemon/runv2_commands_test.go`
- Test: `internal/runtime/tui/client_model_test.go`

- [ ] **Step 1: Write the failing integration regressions**

Add executable regressions for:
- structured `ansible-playbook` invocation through `exec_start`
- shell snippet `cd ... && ansible-playbook ...` through `shell_snippet_start`
- approval followed immediately by second approval on the same run
- process kill from `Tools`
- resuming an old session with an active `waiting_process`

- [ ] **Step 2: Run the regressions to verify they fail**

Run: `go test ./internal/runtimev2 ./internal/runtime/daemon ./internal/runtime/tui -run 'TestAnsiblePlaybookV2|TestShellSnippetV2|TestSecondApprovalV2|TestProcessKillV2|TestResumeWaitingProcessV2' -count=1`
Expected: FAIL until the V2 stack is fully wired.

- [ ] **Step 3: Implement the minimal missing wiring**

Fix only what the regressions expose:
- routing gaps
- snapshot propagation gaps
- approval serialization gaps
- waiting-process resume gaps

- [ ] **Step 4: Re-run the regressions**

Run: `go test ./internal/runtimev2 ./internal/runtime/daemon ./internal/runtime/tui -run 'TestAnsiblePlaybookV2|TestShellSnippetV2|TestSecondApprovalV2|TestProcessKillV2|TestResumeWaitingProcessV2' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtimev2/engine_test.go internal/runtime/daemon/runv2_commands_test.go internal/runtime/tui/client_model_test.go
git commit -m "test: add runtime v2 integration regressions"
```

## Task 11: Remove V1 Chat Approval/Poll Glue

**Files:**
- Modify: `internal/runtime/tool_loop.go`
- Modify: `internal/runtime/tool_loop_resume.go`
- Modify: `internal/runtime/shell_operator.go`
- Modify: `internal/runtime/daemon/queue_runtime.go`
- Test: `internal/runtime/daemon/server_test.go`
- Test: `internal/runtime/tui/client_model_test.go`

- [ ] **Step 1: Write the failing regression tests**

Add tests that assert no V1-only path is still used for:
- chat approval lifecycle
- waiting shell/process state
- event-driven idle/completed clobbering

- [ ] **Step 2: Run the tests to verify failure**

Run: `go test ./internal/runtime ./internal/runtime/daemon ./internal/runtime/tui -run 'TestNoV1ChatLifecyclePath' -count=1`
Expected: FAIL because V1 glue is still wired into the chat path.

- [ ] **Step 3: Remove or isolate V1 glue**

Delete or quarantine:
- old chat approval continuation glue
- V1-only idle/run-active boolean assumptions
- V1-only TUI reload hacks that are obsolete under V2

Leave V1 code only where still required for non-chat features such as `Workspace`.

- [ ] **Step 4: Re-run the tests**

Run: `go test ./internal/runtime ./internal/runtime/daemon ./internal/runtime/tui -run 'TestNoV1ChatLifecyclePath' -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/tool_loop.go internal/runtime/tool_loop_resume.go internal/runtime/shell_operator.go internal/runtime/daemon/queue_runtime.go internal/runtime/daemon/server_test.go internal/runtime/tui/client_model_test.go
git commit -m "refactor: remove v1 chat lifecycle glue"
```

## Task 12: Full Verification And Operator Smoke

**Files:**
- Modify: `docs/superpowers/specs/2026-04-18-chat-runtime-v2-corrective-refactor-design.md`
  - only if implementation reveals needed spec deltas

- [ ] **Step 1: Run focused runtime tests**

Run:
```bash
go test ./internal/runtimev2 -count=1
go test ./internal/runtime/daemon -count=1
go test ./internal/runtime/tui -count=1
```

Expected: PASS

- [ ] **Step 2: Run full repository tests**

Run:
```bash
go test ./... -count=1
```

Expected: PASS

- [ ] **Step 3: Build the binary**

Run:
```bash
go build ./cmd/agent
```

Expected: successful build with no errors

- [ ] **Step 4: Manual operator smoke**

Run the daemon and TUI and verify:
- one approval shows once
- repeated `allow forever` is harmless
- second approval after first approval is coherent
- long-running silent process stays in `waiting_process` without tool-loop exhaustion
- `Tools` and `Chat` show the same approval/process state
- `Ctrl+X` or process kill leaves `waiting_process`
- terminal phase shows `AGENT END TURN` exactly once

- [ ] **Step 5: Final commit if the smoke uncovered fixes**

```bash
git add -A
git commit -m "test: verify runtime v2 chat lifecycle"
```

## Notes For The Implementer

- Prefer new V2 packages over adding more branches to V1 files.
- Do not keep compatibility shims unless they protect a non-chat subsystem that is explicitly out of scope.
- If a step reveals a hidden dependency on `Workspace`, stop and isolate the shared piece instead of dragging `Workspace` into this refactor.
- Keep commits small and descriptive. Do not batch multiple tasks into one commit.
- If a TDD step unexpectedly passes, tighten the test instead of skipping it.
