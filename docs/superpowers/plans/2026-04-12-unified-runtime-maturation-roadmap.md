# Unified Runtime Maturation Roadmap

**Goal:** Consolidate the recent architectural assessments into one disciplined roadmap for taking `teamD` from a strong single-agent runtime/control-plane system to a clearer, more governable, more debuggable production-grade platform.

**Scope:** This roadmap intentionally does **not** start Web UI work and does **not** start mesh work. It focuses on the current runtime platform: transport independence, governance, memory clarity, debugging, onboarding, and operational maturity.

**Current Position:** `teamD` already has a solid base:

- one Go binary
- runtime core
- HTTP API as control plane
- CLI over API
- SSE event stream
- approvals
- jobs/workers/plans/artifacts
- memory + compaction + artifact offload
- operator chat

The next phase is not feature sprawl. It is runtime maturation.

---

## What Is Already Strong

- runtime, API, and CLI are separated well enough to support multiple operator surfaces
- persisted runs, approvals, jobs, workers, plans, events, and artifacts survive restart
- risky tools do not run silently; approval and audit paths exist
- event plane already supports polling and SSE
- long-task hygiene exists:
  - compaction
  - artifact offload
  - persistent plans
  - worker handoff
- the project is already teachable, not just operable

This means the roadmap should improve coherence, not rebuild the system from scratch.

## Confirmed Architectural Gaps

### 1. Governance Is Still Split

We have:

- action policy
- memory policy
- approval logic
- session overrides
- auth boundary

But these do not yet form one explicit runtime-owned governance subsystem.

### 2. Transport Independence Is Good, Not Finished

The runtime is no longer Telegram-driven in the old sense, but some mental model and presentation residue still lives in the Telegram path.

### 3. State Is Correct But Hard To Hold In Your Head

Examples:

- active execution state
- persisted run state
- operator-visible control state
- Telegram UI state

This is architecturally legitimate, but under-documented for debugging.

### 4. Prompt Assembly Is Too Distributed

The prompt path is now better than before, but still spans:

- compaction assembler
- runtime prompt preparation
- runtime prompt-context assembler
- transport-side fragment providers

That is flexible, but harder than necessary to reason about.

### 5. Memory Model Is Conservative But Under-Explained

The current design is intentionally stable, but operator/developer guidance is still missing for choosing the right memory policy profile and understanding tradeoffs.

### 6. Debugging Is Strong In Data, Weak In Replay

We already have:

- traces
- logs
- runtime events
- policy snapshots

What we do not yet have is a replay/inspection layer that can walk a run step-by-step.

### 7. Testing And Scaling Guidance Are Not Yet First-Class

The system is testable and operable, but the documentation does not yet explain:

- how to test approval-heavy flows without Telegram
- how to mock providers cleanly
- how to benchmark or reason about scale limits

---

## Roadmap Principles

- Do not add major new surfaces before clarifying the current ones.
- Prefer runtime-owned contracts over transport-owned behavior.
- Prefer explicit diagrams and reference docs over tribal knowledge.
- Do not start mesh until local workers/jobs/delegation/governance are clean.
- Do not do a giant rewrite. Improve one boundary at a time.

---

## Phase 1: Clarity And Architectural Legibility

**Why first:** The next bugs and onboarding friction will come from state complexity and prompt/memory ambiguity more than from missing features.

### Deliverables

- `docs/agent/state-machines.md`
  - run lifecycle
  - approval lifecycle
  - worker lifecycle
  - job lifecycle
- `docs/agent/prompt-assembly-order.md`
  - exact order of prompt construction
  - compaction position
  - workspace/recall/skills injection order
  - examples of what is and is not included
- `docs/agent/memory-policy-cookbook.md`
  - conservative profile
  - more aggressive retrieval profile
  - local operator-heavy profile
  - what can go wrong with each
- `docs/agent/testing.md`
  - mock provider usage
  - approval flow testing without Telegram
  - artifact offload testing
  - worker/job testing

### Success Criteria

- a new engineer can explain where state lives without reading Telegram code
- a new engineer can explain exactly what goes into the prompt
- memory policy choices become intentional instead of implicit

---

## Phase 2: Unified Governance Layer

**Why second:** Governance is currently spread across multiple policy concepts. This is the biggest runtime maturity gap.

### Deliverables

- runtime-owned governance package or subsystem that unifies:
  - action policy
  - memory policy
  - approval rules
  - side-effect classes
  - session overrides
  - policy snapshots
- explicit side-effect classification:
  - shell
  - filesystem writes
  - network access
  - memory promotion
  - delegation actions
- one obvious place to answer:
  - is this action allowed?
  - does it require approval?
  - why did this run need approval?
  - what policy snapshot was active?

### Success Criteria

- policy decisions no longer feel spread across unrelated files
- approval requirements become explainable from one contract
- jobs/workers/runs all use the same governance vocabulary

---

## Phase 3: Core API Hardening

**Why third:** We already have a control plane. Now it needs a cleaner canonical shape.

### Deliverables

- explicit `AgentCore` or equivalent runtime facade
- transport clients use the same core-facing contract:
  - HTTP API
  - CLI
  - operator chat
  - Telegram
- remaining Telegram-only control semantics moved out of transport
- runtime API docs updated to show:
  - canonical interfaces
  - control actions
  - session actions
  - run/job/worker relationships

### Candidate Interface Shape

```go
type AgentCore interface {
    StartRun(ctx context.Context, req RunRequest) (*Run, error)
    CancelRun(ctx context.Context, runID string) error
    GetRun(ctx context.Context, runID string) (*RunView, error)
    ControlState(ctx context.Context, sessionID string, chatID int64) (*ControlState, error)
    ExecuteControlAction(ctx context.Context, sessionID string, req ControlActionRequest) (*ControlActionResult, error)
}
```

The point is not the exact method set. The point is one explicit center of gravity.

### Success Criteria

- runtime orchestration is clearly owned by core services
- Telegram is demonstrably just another client/renderer path
- future UI work would not require another orchestration rewrite

---

## Phase 4: Replay And Inspection

**Why fourth:** We already capture enough data to make replay valuable.

### Deliverables

- replay/inspection mode over persisted runs
- correlation between:
  - runtime events
  - traces
  - final response
  - approvals
  - artifacts
  - policy snapshots
- step-by-step run inspection doc and CLI/API entrypoints

### Minimum Acceptable Scope

- replay a run as ordered observable steps
- inspect where approval happened
- inspect where artifact offload happened
- inspect what final response was produced

### Success Criteria

- debugging no longer depends on manually reading raw logs in several places
- a specific bad run can be inspected without guessing

---

## Phase 5: Operational Maturity

**Why fifth:** Scaling and capacity work should come after the system’s contracts are clearer.

### Deliverables

- `docs/agent/scaling.md`
  - one-instance operating assumptions
  - likely bottlenecks
  - provider rate-limit handling
  - session concurrency notes
  - benchmark method
- resource controls plan:
  - quotas
  - timeouts
  - concurrency caps
  - runaway worker/job protection
- testing and operational checklists aligned

### Success Criteria

- operator can answer “what are the limits of one instance?”
- scaling decisions stop being guesswork

---

## Phase 6: Worker/Delegation Hardening Before Mesh

**Why sixth:** Managed workers must become fully canonical before mesh becomes part of the hot path.

### Deliverables

- clearer worker supervision model
- stronger worker approval visibility
- worker/job/delegation docs aligned with governance
- local delegation path treated as the canonical pre-mesh orchestration layer

### Success Criteria

- workers feel like a stable local orchestration subsystem
- mesh can be layered on top instead of forcing another core refactor

---

## What Not To Start Yet

- Web UI
- full mesh rollout
- plugin marketplace or extension explosion
- broad new tool families
- deep sandbox redesign

These are all valid later, but they would dilute the current maturation phase.

---

## Immediate Next Slice

The first concrete implementation slice should be:

1. `state-machines.md`
2. `prompt-assembly-order.md`
3. `memory-policy-cookbook.md`
4. `testing.md`
5. parallel spec for `AgentCore` facade

This gives the highest leverage:

- reduces onboarding cost
- reduces ambiguity in prompt/memory behavior
- prepares governance refactor with better boundaries
- makes the current system easier to debug before we add replay

---

## Summary

`teamD` is already a strong single-agent runtime/control-plane system.

The next step is not “more features”.

It is:

1. clearer state and prompt model
2. unified governance
3. explicit core API center
4. replay-grade debugging
5. operational maturity

Only after that should the project move seriously toward mesh.
