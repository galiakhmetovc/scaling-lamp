# ContextPolicy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the entire legacy context/runtime behavior layer with a policy-driven contract system where all non-essential behavior is optional, strategy-based, merged into resolved contracts, and applied consistently by transport, prompt assembly, memory handling, tool execution, and web display.

**Architecture:** Treat the current system as legacy and replace it bottom-up. Keep canonical data objects (`SessionHead`, `WorkspacePointer`, `ArtifactRegistry`, `Transcript`, `Plan`) separate from behavior policy families. Resolve policy families into four runtime contracts: `ProviderRequestContract`, `MemoryContract`, `ExecutionContract`, and `DisplayContract`. Apply those contracts through dedicated executors instead of scattered conditionals. Start from the bottom of the stack with transport/request delivery policy, then move upward into prompt, memory, tools, and web surfaces until legacy paths can be removed.

**Tech Stack:** Go, SQLite/Postgres runtime store, `internal/runtime`, `internal/provider`, `internal/api`, `internal/transport/telegram`, embedded web shell, stdlib.

---

## Legacy System Inventory To Replace

The rewrite is not scoped to one package. It replaces all currently active legacy context/runtime behavior with policy families, strategies, and resolved contracts.

Current behavior that must be migrated:

- provider transport defaults and overrides
- provider request body shaping
- raw conversation request preview and transport path
- `SessionHead` projection and prompt rendering
- recent work projection
- memory recall inclusion
- checkpoint/continuity inclusion
- workspace inclusion
- skills inclusion
- artifact offload thresholds and preview format
- raw conversation old-tool-output offload
- raw conversation model-driven history summarization
- workspace tracking and VFS-derived focus
- plan projection into `SessionHead`
- tool allowlist and approval semantics
- raw tool execution path
- web test bench display trimming and panel visibility

Anything new added to these areas during the rewrite must enter through policy families and contracts, not new legacy-style conditionals.

## Replacement Mapping

### Data Objects

- `SessionHead`
- `WorkspacePointer`
- `ArtifactRegistry`
- `Transcript`
- `Plan`

### Policy Families

- `TransportPolicy`
- `RequestShapePolicy`
- `PromptPolicy`
- `OffloadPolicy`
- `SummarizationPolicy`
- `WorkspacePolicy`
- `ToolPolicy`
- `DisplayPolicy`

### Resolved Contracts

- `ProviderRequestContract`
- `MemoryContract`
- `ExecutionContract`
- `DisplayContract`

### Executors

- provider transport executor
- provider request-shape executor
- prompt assembler
- workspace tracker
- offloader
- summarizer
- tool gate/executor
- display adapter

## Rewrite Rules

1. No new behavior lands as transport-specific branching if it can be expressed as policy + strategy + contract.
2. Presence in store never implies inclusion in prompt.
3. Prompt preview and actual request application must stay aligned.
4. Display trimming must not silently mutate runtime behavior.
5. Offloaded content must remain addressable by tools.
6. Each rewrite slice must leave a working system behind; no “half-switched” subsystem without adapter compatibility.
7. Existing legacy paths are transitional only and should be deleted once the contract-backed path is proven.

## File Structure And Responsibility

### Runtime Policy Core

- Create: `internal/runtime/context_policy.go`
  Responsibility: policy family data model and validation
- Create: `internal/runtime/context_contracts.go`
  Responsibility: resolved contract shapes
- Create: `internal/runtime/context_policy_resolver.go`
  Responsibility: merge and resolve `global < session < run`
- Create: `internal/runtime/workspace_pointer.go`
  Responsibility: persisted workspace pointer model
- Create: `internal/runtime/workspace_pointer_service.go`
  Responsibility: reactive workspace updates from tools and runtime events
- Create: `internal/runtime/history_summary_service.go`
  Responsibility: model-driven history summarization

### Provider Transport And Request Shape

- Create: `internal/provider/transport_policy.go`
  Responsibility: apply transport contract to HTTP request delivery
- Create: `internal/provider/request_shape_policy.go`
  Responsibility: apply request-shape contract to provider JSON body
- Modify: `internal/provider/provider.go`
  Responsibility: canonical provider request structs and contract-aware request inputs
- Modify: `internal/provider/zai/client.go`
  Responsibility: contract-aware provider execution

### Runtime Application Points

- Modify: `internal/runtime/execution_service.go`
  Responsibility: attach resolved contracts to run context
- Modify: `internal/runtime/prompt_context_assembler.go`
  Responsibility: contract-driven prompt layer application
- Modify: `internal/runtime/recent_work.go`
  Responsibility: recent-work projection under prompt contract
- Modify: `internal/runtime/store.go`
- Modify: `internal/runtime/sqlite_store.go`
- Modify: `internal/runtime/postgres_store.go`
  Responsibility: persist session policy and workspace pointer

### Tool And Transport Surfaces

- Modify: `internal/transport/telegram/provider_tools.go`
  Responsibility: contract-driven tool exposure
- Modify: `internal/api/server.go`
  Responsibility: raw conversation/web routes, raw tool execution, policy/debug endpoints
- Modify: `internal/api/types.go`
  Responsibility: API types for configured/effective policy and contract views

### Docs And Web

- Modify: `docs/agent/http-api.md`
- Modify: `docs/agent/operator-chat.md`
- Modify: `docs/agent/05-memory-and-recall.md`
  Responsibility: document policy-driven runtime behavior

## Replacement Strategy

The system must be rewritten bottom-up and shipped with compatibility layers only as temporary migration scaffolding.

### Phase A: Transport And Request Contracts

Replace:
- hardcoded provider URL/auth/header/timeout choices
- ad hoc request-shape serialization

With:
- `ProviderRequestContract.Transport`
- `ProviderRequestContract.RequestShape`

### Phase B: Prompt Contract

Replace:
- hardcoded prompt-layer assembly toggles

With:
- `ProviderRequestContract.Prompt`

### Phase C: Memory Contract

Replace:
- hardcoded offload thresholds
- hardcoded raw old-tool-output logic
- ad hoc history summary and workspace focus behavior

With:
- `MemoryContract`

### Phase D: Execution Contract

Replace:
- scattered tool allowlists and approval rules

With:
- `ExecutionContract`

### Phase E: Display Contract

Replace:
- hardcoded web display trimming and panel logic

With:
- `DisplayContract`

### Phase F: Cleanup

Remove:
- legacy branching that duplicates resolved contract behavior
- obsolete helper paths that recompute policy locally

### Task 1: Define policy families, contracts, and validation rules

**Files:**
- Create: `internal/runtime/context_policy.go`
- Create: `internal/runtime/context_contracts.go`
- Create: `internal/runtime/context_policy_test.go`
- Modify: `internal/runtime/types.go`

- [ ] **Step 1: Write failing tests for policy family structs, contract structs, and validation of incompatible strategy combinations**
- [ ] **Step 2: Run `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime -run 'ContextPolicy|ContextContracts' -count=1`**
- [ ] **Step 3: Implement canonical policy families: `TransportPolicy`, `RequestShapePolicy`, `PromptPolicy`, `OffloadPolicy`, `SummarizationPolicy`, `WorkspacePolicy`, `ToolPolicy`, `DisplayPolicy`**
- [ ] **Step 4: Implement canonical resolved contracts: `ProviderRequestContract`, `MemoryContract`, `ExecutionContract`, `DisplayContract`**
- [ ] **Step 5: Re-run targeted runtime tests**
- [ ] **Step 6: Commit**

### Task 2: Persist session-level context policy and workspace state

**Files:**
- Create: `internal/runtime/workspace_pointer.go`
- Modify: `internal/runtime/store.go`
- Modify: `internal/runtime/sqlite_store.go`
- Modify: `internal/runtime/postgres_store.go`
- Modify: `internal/runtime/runtime_api.go`
- Test: `internal/runtime/sqlite_store_test.go`

- [ ] **Step 1: Write failing store tests for saving/loading `WorkspacePointer` and session-level `ContextPolicy`**
- [ ] **Step 2: Run targeted store tests**
- [ ] **Step 3: Add DB schema, migrations, runtime API accessors, and save/load methods**
- [ ] **Step 4: Re-run targeted store tests**
- [ ] **Step 5: Commit**

### Task 2A: Inventory legacy behavior and map it to policy/contract replacements

**Files:**
- Modify: `docs/superpowers/plans/2026-04-14-context-policy-implementation.md`
- Verify: `internal/provider`
- Verify: `internal/runtime`
- Verify: `internal/api`
- Verify: `internal/transport/telegram`

- [ ] **Step 1: Enumerate all currently hardcoded legacy behavior that affects transport, request shape, prompt composition, memory, tools, and display**
- [ ] **Step 2: Add a checked replacement mapping row for each discovered behavior**
- [ ] **Step 3: Verify no active legacy subsystem is left outside the replacement inventory**
- [ ] **Step 4: Commit**

### Task 3: Add resolver from policy families to effective contracts

**Files:**
- Create: `internal/runtime/context_policy_resolver.go`
- Create: `internal/runtime/context_policy_resolver_test.go`
- Modify: `internal/runtime/execution_service.go`
- Modify: `internal/runtime/session_overrides.go`

- [ ] **Step 1: Write failing tests for `global < session < run` merge precedence**
- [ ] **Step 2: Run `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime -run 'ContextPolicyResolver' -count=1`**
- [ ] **Step 3: Implement resolver that produces `EffectiveContextPolicy` plus resolved `ProviderRequestContract`, `MemoryContract`, `ExecutionContract`, and `DisplayContract`**
- [ ] **Step 4: Thread resolved contracts into run context**
- [ ] **Step 5: Re-run targeted resolver tests**
- [ ] **Step 6: Commit**

### Task 4: Rewrite provider transport path around `ProviderRequestContract`

**Files:**
- Create: `internal/provider/transport_policy.go`
- Create: `internal/provider/transport_policy_test.go`
- Modify: `internal/provider/provider.go`
- Modify: `internal/provider/zai/client.go`
- Modify: `internal/api/server.go`

- [ ] **Step 1: Write failing tests for transport policy over URL, auth, headers, and timeouts**
- [ ] **Step 2: Run targeted provider tests**
- [ ] **Step 3: Implement `TransportPolicy` application for:
  - base URL/path selection
  - auth strategy
  - header strategy
  - timeout strategy
  - retry strategy placeholder**
- [ ] **Step 4: Ensure raw conversation request path uses `ProviderRequestContract.Transport` instead of legacy request assembly**
- [ ] **Step 5: Re-run targeted provider tests**
- [ ] **Step 6: Commit**

### Task 5: Rewrite request body shaping around `ProviderRequestContract`

**Files:**
- Create: `internal/provider/request_shape_policy.go`
- Create: `internal/provider/request_shape_policy_test.go`
- Modify: `internal/provider/provider.go`
- Modify: `internal/provider/zai/client.go`
- Modify: `internal/api/types.go`

- [ ] **Step 1: Write failing tests for request shape policy over model, reasoning, sampling, messages serialization, tools serialization, and response format**
- [ ] **Step 2: Run targeted provider/body-shape tests**
- [ ] **Step 3: Implement `RequestShapePolicy` application to build the exact provider JSON body**
- [ ] **Step 4: Re-run targeted tests**
- [ ] **Step 5: Commit**

### Task 6: Rewrite prompt assembly around `ProviderRequestContract.Prompt`

**Files:**
- Modify: `internal/runtime/prompt_context_assembler.go`
- Modify: `internal/runtime/recent_work.go`
- Modify: `internal/transport/telegram/session_head_prompt.go`
- Test: `internal/runtime/prompt_context_assembler_test.go`

- [ ] **Step 1: Write failing tests for optional `SessionHead`, optional `workspace_focus`, optional `plan`, optional `recent_artifacts`, optional `tree_hint`, and optional `history_summary`**
- [ ] **Step 2: Run targeted prompt assembly tests**
- [ ] **Step 3: Implement prompt-layer gating and strategy-driven compact projections**
- [ ] **Step 4: Re-run targeted prompt tests**
- [ ] **Step 5: Commit**

### Task 7: Rewrite memory behavior around `MemoryContract`

**Files:**
- Create: `internal/runtime/workspace_pointer_service.go`
- Create: `internal/runtime/history_summary_service.go`
- Modify: `internal/api/server.go`
- Modify: `internal/api/types.go`
- Modify: `internal/vfs/root.go`
- Test: `internal/runtime/workspace_pointer_service_test.go`
- Test: `internal/runtime/history_summary_service_test.go`
- Test: `internal/api/server_test.go`

- [ ] **Step 1: Write failing tests for reactive `WorkspacePointer` updates from VFS/tool events**
- [ ] **Step 2: Write failing tests for model-driven summarization with `keep_last_n` and manual refresh**
- [ ] **Step 3: Write failing tests for `off`, `old_only`, and `tool_aware` offload strategies**
- [ ] **Step 4: Run targeted memory/offload/summary tests**
- [ ] **Step 5: Implement `MemoryContract` application for:
  - workspace tracking
  - artifact registry updates
  - offload policy
  - older-history summarization**
- [ ] **Step 6: Re-run targeted tests**
- [ ] **Step 7: Commit**

### Task 8: Rewrite tool exposure and execution around `ExecutionContract`

**Files:**
- Modify: `internal/transport/telegram/provider_tools.go`
- Modify: `internal/api/server.go`
- Modify: `internal/runtime/runtime_api.go`
- Test: `internal/transport/telegram/provider_tools_test.go`
- Test: `internal/api/server_test.go`

- [ ] **Step 1: Write failing tests for `deny_by_default`, `allow_selected`, manual execution, and auto-approve**
- [ ] **Step 2: Run targeted tool/execution tests**
- [ ] **Step 3: Replace scattered tool allowlist and approval decisions with resolved `ExecutionContract`**
- [ ] **Step 4: Re-run targeted tests**
- [ ] **Step 5: Commit**

### Task 9: Rewrite web/API surfaces around `DisplayContract`

**Files:**
- Modify: `internal/api/server.go`
- Modify: `internal/api/types.go`
- Modify: `docs/agent/http-api.md`
- Modify: `docs/agent/operator-chat.md`
- Modify: `docs/agent/05-memory-and-recall.md`
- Test: `internal/api/server_test.go`

- [ ] **Step 1: Add API shapes for configured policy, effective policy, resolved contracts, `WorkspacePointer`, and prompt preview provenance**
- [ ] **Step 2: Add read-only web rendering for `SessionHead`, `WorkspacePointer`, configured policy, effective policy, and resolved contracts**
- [ ] **Step 3: Add raw-conversation-first policy editing controls after read-only views are stable**
- [ ] **Step 4: Update docs to explain contract resolution and application**
- [ ] **Step 5: Verify references with `rg -n 'ContextPolicy|WorkspacePointer|ProviderRequestContract|MemoryContract|ExecutionContract|DisplayContract' internal docs/agent`**
- [ ] **Step 6: Commit**

### Task 10: Remove legacy behavior after contract adoption

**Files:**
- Modify: `internal/provider`
- Modify: `internal/runtime`
- Modify: `internal/api`
- Modify: `internal/transport/telegram`

- [ ] **Step 1: Identify remaining hardcoded policy logic that duplicates resolved contracts**
- [ ] **Step 2: Delete or collapse obsolete helper paths after contract-backed coverage is in place**
- [ ] **Step 3: Re-run targeted rewrite tests**
- [ ] **Step 4: Commit**

### Task 11: Full verification and controlled rollout

**Files:**
- No new files expected

- [ ] **Step 1: Run `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./... -count=1`**
- [ ] **Step 2: Run `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go build -o coordinator ./cmd/coordinator`**
- [ ] **Step 3: Run `GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go build -o worker ./cmd/worker`**
- [ ] **Step 4: Roll only the intended live service path after explicit verification of affected contract surfaces**
- [ ] **Step 5: Close the bead after verification**
