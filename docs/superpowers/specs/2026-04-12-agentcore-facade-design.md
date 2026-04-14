# AgentCore Facade Design

## Goal

Define a single runtime-owned orchestration facade that becomes the canonical center of gravity for `teamD`.

The facade is not meant to replace every runtime service with one giant object. It is meant to make one thing explicit:

- transports and operator surfaces should call one canonical runtime contract
- orchestration should stop being mentally modeled as "HTTP API plus some Telegram logic plus CLI helpers"

`AgentCore` is the contract that makes runtime ownership obvious.

## Why This Is Needed

`teamD` already has a strong control plane:

- runtime API
- execution service
- control actions
- session actions
- jobs
- workers
- plans
- approvals
- events

The problem is not missing capability.

The problem is shape.

Today the runtime center is spread across:

- `internal/runtime/runtime_api.go`
- `internal/runtime/execution_service.go`
- `internal/runtime/control_actions.go`
- `internal/runtime/session_actions.go`
- store-backed services for jobs, workers, and plans

This works, but it is still harder than necessary for a new engineer to answer:

- what is the canonical runtime entrypoint?
- what does a transport call?
- where is orchestration owned?
- what is the difference between runtime queries and transport rendering?

## Current Problems

### 1. Runtime ownership is split across several top-level services

Each individual service is reasonable, but the absence of one explicit facade means transports still rely on understanding several packages at once.

### 2. Transport independence is real, but not obvious enough

The system is already much less Telegram-centric than before, but the code still does not visibly communicate:

- HTTP API
- CLI
- operator chat
- Telegram

are all peers over the same core orchestration surface.

### 3. Control-plane semantics are broader than `runtime.API`

`runtime.API` already owns many runtime queries, but start/cancel/control/session/delegation concerns are not yet gathered into one obvious facade.

## Design Principles

- keep existing focused services
- do not collapse everything into one god object
- add one explicit facade that wires them together
- separate:
  - orchestration entrypoints
  - query surfaces
  - transport rendering
- make the facade stable enough that future Web UI and mesh-prep layers can depend on it

## Proposed Shape

`AgentCore` is a runtime-owned interface implemented by a concrete facade in `internal/runtime`.

Candidate shape:

```go
type AgentCore interface {
    StartRun(ctx context.Context, req StartRunRequest) (RunView, bool, <-chan error, error)
    StartRunDetached(ctx context.Context, req StartRunRequest) (RunView, bool, error)
    ResumeApprovalContinuation(ctx context.Context, approvalID string) (bool, error)

    Run(runID string) (RunView, bool, error)
    ListRuns(query RunQuery) ([]RunView, error)

    ControlState(sessionID string, chatID int64) (ControlState, error)
    ExecuteControlAction(sessionID string, req ControlActionRequest) (ControlActionResult, error)
    ExecuteSessionAction(req SessionActionRequest) (SessionActionResult, error)

    ListApprovals(sessionID string) ([]ApprovalView, error)
    Approve(approvalID string) (ApprovalView, bool, error)
    Reject(approvalID string) (ApprovalView, bool, error)

    ListEvents(query EventQuery) ([]RuntimeEvent, error)

    ListJobs(query JobQuery) ([]JobRecord, error)
    Job(jobID string) (JobRecord, bool, error)
    CancelJob(jobID string) (JobRecord, error)

    ListWorkers(query WorkerQuery) ([]WorkerRecord, error)
    Worker(workerID string) (WorkerRecord, bool, error)
    WorkerHandoff(workerID string) (WorkerHandoff, bool, error)

    ListPlans(query PlanQuery) ([]PlanRecord, error)
    Plan(planID string) (PlanRecord, bool, error)
}
```

The exact method set can change. The important decision is structural:

- one obvious orchestration facade
- composed out of existing narrow services

## What AgentCore Is Not

### Not a replacement for focused services

`RunManager`, `ExecutionService`, `PlansService`, `WorkersService`, `JobsService`, approval service, and stores should stay separate.

`AgentCore` composes them. It does not erase them.

### Not a transport renderer

`AgentCore` should not know:

- Telegram callback ids
- Telegram status cards
- terminal chat formatting
- web component shape

It should return runtime domain data and control results. Rendering stays outside.

### Not a policy engine by itself

Governance remains its own next slice. `AgentCore` should consume governance/policy inputs, not absorb that subsystem.

## Internal Structure

Recommended file additions:

- `internal/runtime/agent_core.go`
  - interface and concrete facade
- `internal/runtime/agent_core_test.go`
  - contract-level tests

Recommended existing services reused by the facade:

- `runtime.API`
- `ExecutionService`
- `PlansService`
- `JobsService`
- `WorkersService`
- `approvals.Service`
- session action service

## Data Flow

### HTTP API

HTTP handlers should depend on `AgentCore`, not on several partially overlapping runtime services.

### CLI

CLI stays an HTTP client.  
It does not call `AgentCore` directly.  
But the API handlers it uses should now be thin wrappers over `AgentCore`.

### Telegram

Telegram adapter should depend on `AgentCore` for:

- run start/cancel/status
- approvals
- session actions
- control state

It keeps only:

- update normalization
- callback routing
- rendering
- status-card UX

### Operator Chat

Operator chat remains CLI-over-HTTP.  
Its behavior benefits from `AgentCore` because the API below it becomes cleaner and more canonical.

## Migration Plan

### Step 1

Introduce the facade without changing external behavior.

### Step 2

Move HTTP API handlers to depend on `AgentCore`.

### Step 3

Move Telegram control paths to depend on `AgentCore` instead of mixing several runtime entrypoints directly.

### Step 4

Update docs to present `AgentCore` as the canonical runtime surface.

## Testing Strategy

Add contract tests that verify `AgentCore`:

- starts and queries runs correctly
- assembles control state correctly
- delegates control actions correctly
- exposes approvals/jobs/workers/plans without transport assumptions

The most important property to test is not implementation detail.  
It is boundary clarity.

## Success Criteria

- a new engineer can point to one file and say "that is the runtime facade"
- HTTP API handlers get thinner
- Telegram control code gets thinner
- runtime orchestration ownership becomes obvious without reading transport code
- future replay/governance work has one stable place to integrate with

## Non-goals

- no Web UI work
- no mesh work
- no full governance refactor in the same slice
- no rewrite of all runtime types

This is a boundary-clarification refactor, not a restart.
