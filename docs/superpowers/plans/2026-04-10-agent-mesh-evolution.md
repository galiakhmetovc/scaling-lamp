# Agent Mesh Evolution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Evolve the current cold-start mesh from simple candidate comparison into a two-mode multi-agent runtime that supports proposal synthesis, single-winner tool execution, and composite task decomposition.

**Architecture:** Keep `owner` as the only user-facing agent and final responder. Extend mesh execution into two layers: `proposal round` for all sampled agents without side effects, and `execution round` where exactly one selected executor may run tools. Add a second orchestration mode for composite tasks, where owner/planner decomposes work into substeps, routes each substep to the best available executor, and integrates the results into one final answer.

**Tech Stack:** Go 1.24+, existing `internal/mesh`, existing Telegram owner ingress, Postgres registry/score storage, z.ai for classifier/judge/planner, existing MCP/tool runtime, Go tests.

---

## Scope

This plan covers only the next-stage mesh behavior discussed in session:
- user-controlled orchestration policy
- persistent agent identity with leased runtime lifecycle
- clarification round before proposal round
- proposal-only cold-start collaboration
- execution brief synthesis from multiple proposals
- single-winner real tool execution
- composite task planning and split execution
- owner-side integration of partial outputs

This plan does **not** include:
- public networking or cross-host discovery
- capability declarations at registration time
- security/policy hardening beyond existing runtime guards
- UI redesign in Telegram

---

## Target Runtime Modes

### Mode 0: Clarification

Use before proposal round when the task is ambiguous, underspecified, or high-value.

Flow:
1. owner receives raw user task
2. owner runs clarification mode:
   - `single`
   - `sampled`
   - `all`
3. one or more agents produce `clarification candidates`
4. owner synthesizes a `clarified_task`
5. proposal round runs against `clarified_task`, not raw user text

Rules:
- clarification round must not execute tools
- clarification may return follow-up questions instead of proceeding
- clarification policy must be user-controllable from Telegram

### Runtime Lifecycle

Agents should not be modeled as disposable one-shot processes.

Principles:
- `AgentIdentity` is persistent
- agent memory namespaces are persistent
- agent LLM sessions are persistent
- runtime processes are leased, not immortal and not one-shot

Runtime states:
- `starting`
- `warm`
- `idle`
- `draining`
- `stopped`

Lifecycle rules:
- ingress agent is always-on
- spawned peers are created on demand
- recently used peers remain warm for an `IdleTTL`
- long-lived or pinned work may extend the lease
- shutdown happens when lease expires or the runtime is explicitly drained
- a `draining` runtime must not accept new work, but must finish accepted work

### Mode 1: Single-Winner Execution

Use when the task is effectively one unit of work.

Flow:
1. owner receives user task
2. owner runs task classification
3. owner samples peers
4. all candidates return **proposal-only** outputs
5. owner evaluates proposals
6. owner synthesizes an `execution_brief`
7. exactly one `winner-executor` runs real tools
8. owner sends final answer to the user

Rules:
- proposal round must not create side effects
- only one agent may run tools in execution round
- owner always remains final answer authority
- proposal round uses `PartialQuorum` and must not block forever on the slowest peer
- proposal round is governed by explicit `ProposalTimeout` and `MinQuorumSize`

### Mode 2: Composite Task Planning

Use when the task has multiple deliverables or separable phases.

Examples:
- write a script and document it
- inspect a system, fix an issue, then summarize the change
- research a topic, produce code, then explain deployment steps

Flow:
1. owner detects `task_shape=composite`
2. planner creates a structured task plan
3. task plan is split into substeps
4. owner routes each substep to the best executor
5. subresults return to owner
6. owner integrates outputs into one final answer

Rules:
- different substeps may be executed by different agents
- owner remains the only user-facing integrator
- integration output must cite which parts were produced by which substep internally, even if user sees only final synthesis
- MVP composite planning stays linear; no dependency graph yet

---

## Required Data Model Extensions

- Extend `Envelope`
  - add `Mode` (`clarify|proposal|execute|plan|subtask|integrate`)
  - add `ParentStepID`
  - add `ExecutionBrief`
  - add `TaskShape`
  - add optional `Dependencies []StepID` as forward-compatible field, unused in MVP scheduling
- Add `ClarifiedTask`
  - `Goal`
  - `Deliverables`
  - `Constraints`
  - `Assumptions`
  - `MissingInfo`
  - `TaskClass`
  - `TaskShape`
- Extend `CandidateReply`
  - add `Proposal`
  - add `ExecutionNotes`
  - add `Artifacts`
  - add `RejectionReason`
- Add `Proposal`
  - `Understanding`
  - `PlannedChecks`
  - `SuggestedTools`
  - `Risks`
  - `DraftConclusion`
- Add `ProposalMetadata`
  - `EstimatedTokens`
  - `SuggestedTools`
  - `Confidence`
  - `RiskFlags`
- Add `ExecutionBrief`
  - `Goal`
  - `RequiredSteps`
  - `Constraints`
  - `AdoptedIdeas`
  - `ConflictsToResolve`
  - `RequiredChecks`
- Add `ProposalPolicy`
  - `ProposalTimeout`
  - `MinQuorumSize`
  - `RetryCount`
- Add `OrchestrationPolicy`
  - `Profile`
  - `ClarificationMode`
  - `MaxClarificationRounds`
  - `ProposalMode`
  - `SampleK`
  - `MinQuorumSize`
  - `ProposalTimeout`
  - `ExecutionMode`
  - `AllowToolExecution`
  - `CompositePlanning`
  - `JudgeMode`
- Add `AgentIdentity`
  - `AgentID`
  - `MemoryNamespace`
  - `SessionNamespace`
  - `PreferredModelProfile`
- Add `IdentityRegistry`
  - persistent store for identities, preferred model profiles, and namespace bindings
- Add `RuntimeLease`
  - `RuntimeID`
  - `AgentID`
  - `StartedAt`
  - `LastUsedAt`
  - `IdleTTL`
  - `MaxLifetime`
  - `Pinned`
  - `State`
- Add `ResourceBudget`
  - `MaxSpawnedPeersPerTask`
  - `MaxWarmPeers`
  - `MaxConcurrentAgents`
  - `MaxLeaseLifetime`
- Add `RejectionReason` enum-like values
  - `tool_unavailable`
  - `low_confidence`
  - `ambiguous_task`
  - `timeout`
- Add `TaskPlan`
  - `TaskShape`
  - `Steps []PlannedStep`
- Add `PlannedStep`
  - `StepID`
  - `Title`
  - `TaskClass`
  - `Description`
  - `RequiresTools`

---

## File Structure

- Modify: `internal/mesh/types.go`
  Purpose: add proposal, execution brief, task plan, and task shape contracts.
- Create: `internal/mesh/planner.go`
  Purpose: owner-side planner for `single` vs `composite` routing and composite step generation.
- Create: `internal/mesh/clarifier.go`
  Purpose: build clarification requests, parse clarification candidates, and synthesize a `ClarifiedTask`.
- Create: `internal/mesh/clarifier_test.go`
  Purpose: verify clarification parsing, synthesis, and missing-info handling.
- Create: `internal/mesh/policy.go`
  Purpose: define orchestration profiles and per-request/session policy overrides.
- Create: `internal/mesh/policy_test.go`
  Purpose: verify policy parsing, defaults, and override precedence.
- Create: `internal/mesh/lifecycle.go`
  Purpose: define leased runtime lifecycle, warm/idle transitions, and shutdown policy.
- Create: `internal/mesh/lifecycle_test.go`
  Purpose: verify lease renewal, idle expiry, and pinned runtime behavior.
- Create: `internal/mesh/spawner.go`
  Purpose: spawn/reuse/drain peer runtime instances while preserving persistent identities and sessions.
- Create: `internal/mesh/spawner_test.go`
  Purpose: verify ingress-only startup, peer reuse, and no immediate shutdown after one task.
- Create: `internal/mesh/planner_test.go`
  Purpose: verify task shape parsing and task plan extraction.
- Create: `internal/mesh/proposal.go`
  Purpose: build proposal-only prompts and normalize proposal replies.
- Create: `internal/mesh/proposal_test.go`
  Purpose: verify proposal parsing and synthesis inputs.
- Modify: `internal/mesh/service.go`
  Purpose: split current owner flow into proposal round, execution brief synthesis, and single-winner execution.
- Create: `internal/mesh/identity_registry.go`
  Purpose: persist agent identities, preferred model profiles, and namespace bindings independently of runtime instances.
- Create: `internal/mesh/identity_registry_test.go`
  Purpose: verify identity persistence and runtime/session rebinding across restarts.
- Modify: `internal/mesh/service_test.go`
  Purpose: verify proposal round, winner-only tool execution, and composite routing behavior.
- Create: `internal/mesh/briefing.go`
  Purpose: synthesize multiple proposals into one execution brief for the winner.
- Create: `internal/mesh/briefing_test.go`
  Purpose: verify brief generation and conflict handling.
- Create: `internal/mesh/tool_executor.go`
  Purpose: run real tool-backed execution from an execution brief.
- Create: `internal/mesh/tool_executor_test.go`
  Purpose: verify only one executor runs tools and proposal-only rounds stay side-effect free.
- Modify: `internal/mesh/evaluator.go`
  Purpose: judge proposals rather than final user replies, and separately judge substep outputs where needed.
- Create: `internal/mesh/execution_policy.go`
  Purpose: enforce proposal-only side-effect freedom and execution-brief validation before real tool execution.
- Modify: `internal/transport/telegram/adapter.go`
  Purpose: show owner-side progress for clarification/proposal/execution/composite plan execution and expose mesh slash-commands.
- Modify: `internal/transport/telegram/adapter_test.go`
  Purpose: verify `/mesh` slash-commands and per-session orchestration policy control.
- Modify: `README.md`
  Purpose: document mesh runtime modes and execution semantics.

---

## Phase 1: Proposal-Only Single-Winner Execution

### Task 0: Add Orchestration Policy And Telegram Controls

**Files:**
- Create: `internal/mesh/policy.go`
- Create: `internal/mesh/policy_test.go`
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing tests for policy defaults and `/mesh` commands**
- [ ] **Step 2: Run focused tests to verify they fail**
- [ ] **Step 3: Implement `OrchestrationPolicy` with profiles:**
  - `fast`
  - `balanced`
  - `deep`
  - `composite`
- [ ] **Step 3.1: Add Telegram commands:**
  - `/mesh help`
  - `/mesh`
  - `/mesh mode <profile>`
  - `/mesh set clarification_mode=<off|single|sampled|all>`
  - `/mesh set proposal_mode=<off|sampled|all>`
  - `/mesh set sample_k=<n>`
  - `/mesh set execution_mode=<owner|winner>`
  - `/mesh set composite_planning=<off|auto|force>`
- [ ] **Step 3.2: Log policy changes with:**
  - `user_id`
  - `session_id`
  - `old_value`
  - `new_value`
- [ ] **Step 4: Run focused tests to verify they pass**
- [ ] **Step 5: Commit**

### Task 0.25: Add Persistent Identity And Leased Runtime Contracts

**Files:**
- Create: `internal/mesh/lifecycle.go`
- Create: `internal/mesh/lifecycle_test.go`
- Create: `internal/mesh/spawner.go`
- Create: `internal/mesh/spawner_test.go`

- [ ] **Step 1: Write the failing tests for leased runtime lifecycle**
- [ ] **Step 2: Run focused tests to verify they fail**
- [ ] **Step 3: Add `AgentIdentity` and `RuntimeLease` contracts**
- [ ] **Step 3.1: Implement warm/idle/draining transitions with `IdleTTL`, `MaxLifetime`, and `Pinned`**
- [ ] **Step 3.2: Implement spawn/reuse semantics so peers are reused before spawning a fresh process**
- [ ] **Step 3.3: Add `ResourceBudget` limits and owner-only spawn authority**
- [ ] **Step 3.4: Serialize lease state transitions to avoid renew/drain/assign races**
- [ ] **Step 4: Run focused tests to verify they pass**
- [ ] **Step 5: Commit**

### Task 0.3: Add Identity Registry

**Files:**
- Create: `internal/mesh/identity_registry.go`
- Create: `internal/mesh/identity_registry_test.go`

- [ ] **Step 1: Write the failing tests for persistent identity lookup and rebinding**
- [ ] **Step 2: Run focused tests to verify they fail**
- [ ] **Step 3: Implement `IdentityRegistry` interface plus in-memory MVP implementation for:**
  - `AgentID`
  - model profile preferences
  - memory/session namespaces
  - session continuation binding
- [ ] **Step 3.1: Keep storage behind an interface so Postgres-backed identity registry can be added later without changing lifecycle/service code**
- [ ] **Step 4: Run focused tests to verify they pass**
- [ ] **Step 5: Commit**

### Task 0.5: Add Clarification Round

**Files:**
- Create: `internal/mesh/clarifier.go`
- Create: `internal/mesh/clarifier_test.go`
- Modify: `internal/mesh/service.go`
- Modify: `internal/mesh/service_test.go`

- [ ] **Step 1: Write the failing tests for clarification candidate synthesis**
- [ ] **Step 2: Run focused tests to verify they fail**
- [ ] **Step 3: Implement `ClarifiedTask` flow with modes:**
  - `off`
  - `single`
  - `sampled`
  - `all`
- [ ] **Step 3.1: If `MissingInfo` is critical, return a follow-up question to Telegram instead of entering proposal round**
- [ ] **Step 3.2: Make this a hard-stop rule, not a soft hint**
- [ ] **Step 3.3: Respect `MaxClarificationRounds`; after the limit, continue with explicit low-confidence assumptions**
- [ ] **Step 4: Run focused tests to verify they pass**
- [ ] **Step 5: Commit**

### Task 1: Add Proposal And Brief Contracts

**Files:**
- Modify: `internal/mesh/types.go`
- Test: `internal/mesh/proposal_test.go`

- [ ] **Step 1: Write the failing test for proposal parsing**
- [ ] **Step 2: Run focused test to verify it fails**
- [ ] **Step 3: Add `Proposal`, `ExecutionBrief`, `TaskShape` contracts**
- [ ] **Step 3.1: Include `ProposalMetadata` and `RejectionReason` in the same contract pass**
- [ ] **Step 4: Run focused test to verify it passes**
- [ ] **Step 5: Commit**

### Task 2: Add Proposal Builder

**Files:**
- Create: `internal/mesh/proposal.go`
- Create: `internal/mesh/proposal_test.go`

- [ ] **Step 1: Write the failing test for proposal-only request generation**
- [ ] **Step 2: Run focused test to verify it fails**
- [ ] **Step 3: Implement proposal prompt builder and proposal parser**
- [ ] **Step 3.1: Support complete/partial proposal quality and `PartialQuorum`-ready metadata**
- [ ] **Step 3.2: Add explicit `ProposalTimeout`, `MinQuorumSize`, and retry hooks in the proposal collection contract**
- [ ] **Step 4: Run focused test to verify it passes**
- [ ] **Step 5: Commit**

### Task 3: Add Execution Brief Synthesis

**Files:**
- Create: `internal/mesh/briefing.go`
- Create: `internal/mesh/briefing_test.go`

- [ ] **Step 1: Write the failing test for multi-proposal synthesis**
- [ ] **Step 2: Run focused test to verify it fails**
- [ ] **Step 3: Implement owner-side brief synthesis from proposal set**
  - start with a structured brief, not free-form prose
  - `RequiredSteps`, `Constraints`, and `ConflictsToResolve` are mandatory fields
  - first MVP may use selected proposal + adopted comments instead of full open-ended synthesis
- [ ] **Step 3.1: Add deterministic validation**
  - reject empty `RequiredSteps`
  - reject unresolved `ConflictsToResolve` before execution
- [ ] **Step 4: Run focused test to verify it passes**
- [ ] **Step 5: Commit**

### Task 4: Split Mesh Service Into Proposal Round And Execution Round

**Files:**
- Modify: `internal/mesh/service.go`
- Modify: `internal/mesh/service_test.go`

- [ ] **Step 1: Write the failing test proving only the winner enters execution mode**
- [ ] **Step 2: Run focused test to verify it fails**
- [ ] **Step 3: Refactor service to run:
  - clarification round
  - proposal round for all candidates
  - `PartialQuorum` collection
  - timeout/retry/fallback policy when quorum is not met
  - evaluation over proposals
  - brief synthesis
  - execution round for winner only**
- [ ] **Step 3.1: Add a deterministic-first proposal scoring contract before any optional LLM judging**
- [ ] **Step 4: Run focused test to verify it passes**
- [ ] **Step 5: Commit**

### Task 5: Add Tool-Backed Winner Executor

**Files:**
- Create: `internal/mesh/tool_executor.go`
- Modify: `internal/mesh/tool_executor_test.go`

- [ ] **Step 1: Write the failing test for winner-only tool execution**
- [ ] **Step 1.1: Write the failing test proving proposal round cannot call tools**
- [ ] **Step 2: Run focused test to verify it fails**
- [ ] **Step 3: Implement tool-backed execution from `ExecutionBrief`**
- [ ] **Step 3.1: Enforce execution policy:
  - proposal-only executors cannot access tools
  - execution round validates `ExecutionBrief` before tool calls**
- [ ] **Step 3.2: Record execution provenance so owner can map result/artifact -> agent -> step**
- [ ] **Step 4: Run focused test to verify it passes**
- [ ] **Step 5: Commit**

---

## Phase 2: Composite Task Planning

### Task 6: Add Planner For Task Shape Detection

**Files:**
- Create: `internal/mesh/planner.go`
- Create: `internal/mesh/planner_test.go`

- [ ] **Step 1: Write the failing test for `single` vs `composite` detection**
- [ ] **Step 2: Run focused test to verify it fails**
- [ ] **Step 3: Implement planner call that returns `TaskShape` and initial plan**
- [ ] **Step 3.1: Keep the initial plan linear (`[]PlannedStep` in order), no DAG/dependency graph yet**
- [ ] **Step 4: Run focused test to verify it passes**
- [ ] **Step 5: Commit**

### Task 7: Add Composite Step Routing

**Files:**
- Modify: `internal/mesh/service.go`
- Modify: `internal/mesh/service_test.go`

- [ ] **Step 1: Write the failing test for multi-step routing**
- [ ] **Step 2: Run focused test to verify it fails**
- [ ] **Step 3: Implement:
  - per-step executor selection
  - per-step execution
  - owner-side collection of step outputs
  - `TraceID + StepID` logging for every substep**
- [ ] **Step 3.1: Keep one runtime bound to one identity in MVP; no multiplexing of multiple identities into one runtime**
- [ ] **Step 4: Run focused test to verify it passes**
- [ ] **Step 5: Commit**

### Task 8: Add Owner Integration Layer

**Files:**
- Modify: `internal/mesh/service.go`
- Modify: `internal/mesh/evaluator.go`
- Test: `internal/mesh/service_test.go`

- [ ] **Step 1: Write the failing test for integrated final answer from multiple substeps**
- [ ] **Step 2: Run focused test to verify it fails**
- [ ] **Step 3: Implement owner-side integration of:
  - script output
  - verification output
  - documentation output**
- [ ] **Step 4: Run focused test to verify it passes**
- [ ] **Step 5: Commit**

---

## Phase 3: Telegram And Runtime UX Alignment

### Task 9: Reflect Proposal / Execution / Composite States In Telegram

**Files:**
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing test for run state transitions**
- [ ] **Step 2: Run focused test to verify it fails**
- [ ] **Step 3: Update status card stages to show:
  - clarification round
  - proposal round
  - selecting winner
  - execution round
  - integrating composite outputs**
- [ ] **Step 4: Run focused test to verify it passes**
- [ ] **Step 5: Commit**

### Task 10: Document Mesh Runtime Modes

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Document:
  - proposal-only round
  - single-winner tool execution
  - composite task planning
  - owner-only final response semantics**
- [ ] **Step 2: Review docs against actual runtime behavior**
- [ ] **Step 3: Commit**

---

## Acceptance Criteria

- Simple tasks can run through:
  - proposal round
  - winner selection
  - winner-only tool execution
- Composite tasks can run through:
  - plan generation
  - per-step routing
  - owner integration
- Owner remains the only user-facing responder
- Proposal rounds are side-effect free
- Proposal rounds can complete under `PartialQuorum`
- Proposal collection has explicit timeout/quorum behavior
- Users can inspect and change orchestration policy from Telegram slash-commands
- `/mesh help` exposes the current command surface
- Agent identity, memory namespaces, and LLM sessions persist across runtime restarts
- Peer runtimes use lease/TTL lifecycle instead of immediate teardown after one task
- Only owner may spawn or drain additional peers in MVP
- Clarification with critical missing info blocks execution and returns to the user
- Tool execution is performed by exactly one selected executor per simple task
- Proposal executors are prevented from calling tools by enforced boundary, not convention
- Composite outputs preserve provenance (`trace -> step -> agent -> result/artifact`)
- Composite planning uses a linear ordered step list in MVP
- `ExecutionBrief` is validated before execution
- Full `go test ./...` remains green

---

## Recommended Execution Order

Priority order for implementation:
1. `Task 0: Orchestration Policy And Telegram Controls`
2. `Task 0.25: Persistent Identity And Leased Runtime Contracts`
3. `Task 0.3: Identity Registry` using in-memory MVP behind an interface
4. `Task 1-3: Proposal Contracts, Proposal Builder, Execution Brief Synthesis`
5. `Task 4-5: Service Split And Tool-Backed Winner Executor`
6. `Task 0.5: Clarification Round`
7. `Task 6-8: Composite Task Planning`
8. `Task 9-10: Telegram UX Alignment And Documentation`

---

## Notes

- Do not reintroduce static capabilities as a prerequisite.
- Use current outcome-based scoring as routing input, but not as a hard lock.
- Keep the existing cold-start mesh slice working while adding these stages incrementally.
- Prefer adding new types and glue over destabilizing the current HTTP transport or Telegram ingress path in one large rewrite.
- Log every mode transition with `TraceID`; log composite substeps with `StepID`.
- Proposal evaluation should be deterministic-first; if LLM scoring is added, document the exact scoring factors and fallback order.
- Do not hardcode agent responsibility to a model. Different models are allowed per agent, but specialization must still emerge through scoring/outcomes.
- Do not model spawned peers as throwaway one-shot workers; runtime instances are leased and reusable while identities/sessions remain persistent.
- In MVP, only owner has spawn authority; peers cannot recursively spawn more peers.
