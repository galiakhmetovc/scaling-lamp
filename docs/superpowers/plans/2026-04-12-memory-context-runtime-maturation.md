# Memory And Context Runtime Maturation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `teamD`'s context model reliable and legible by maturing `SessionHead`, adding prompt budget transparency, switching compaction decisions to projected prompt accounting, separating context classes, and introducing pruning before deeper retrieval compression work.

**Architecture:** Keep the existing single-agent runtime and control plane, but strengthen the internal context stack. `SessionHead` becomes the canonical recent-context layer, prompt budgeting becomes a runtime-visible subsystem instead of an opaque estimate, and prompt assembly is split into explicit layers with pruning before future selection/extraction work. No transport-specific logic should become the source of truth.

**Tech Stack:** Go, existing `runtime` / `compaction` / `memory` / `telegram` packages, beads, existing HTTP API and CLI control plane, existing replay and artifact surfaces.

---

## File Map

### Runtime State And Prompt Assembly

- Modify: `internal/runtime/store.go`
  - Extend state/storage contracts if new prompt-budget or pruning artifacts need persistence.
- Modify: `internal/runtime/types.go`
  - Add any new view structs for budget breakdown or context-layer reporting.
- Modify: `internal/runtime/runtime_api.go`
  - Surface SessionHead and future budget breakdown through generic API views.
- Modify: `internal/runtime/agent_core.go`
  - Keep any new runtime context surfaces accessible through `AgentCore`.
- Modify: `internal/runtime/prompt_context.go`
  - Move compaction trigger logic from raw-history-only to projected final prompt accounting.
- Modify: `internal/runtime/prompt_context_assembler.go`
  - Add explicit context-layer accounting and future context class handling.
- Create: `internal/runtime/prompt_budget.go`
  - Central place for projected prompt accounting and layer-by-layer token estimates.
- Create: `internal/runtime/context_layers.go`
  - Define always-loaded / trigger-loaded / on-demand layer categorization.
- Create: `internal/runtime/pruning.go`
  - Prune old prompt baggage without mutating durable transcript state.

### Compaction

- Modify: `internal/compaction/budget.go`
  - Extend estimation helpers or add clearer split between rough estimate and layer accounting.
- Modify: `internal/compaction/assembler.go`
  - Consume pruning outputs or explicit layer budgets rather than only recency-based fitting.
- Modify: `internal/compaction/service.go`
  - Only if compaction metadata or lineage needs to carry new context accounting details.

### Transport / Operator Surfaces

- Modify: `internal/transport/telegram/conversation.go`
  - Publish richer context metrics and use projected budget information in run state.
- Modify: `internal/transport/telegram/run_state.go`
  - Hold context-layer metrics for status rendering.
- Modify: `internal/transport/telegram/status_card.go`
  - Show prompt-budget percent, full-window percent, and overhead breakdown clearly.
- Modify: `internal/cli/chat_console.go`
  - If needed, render richer context metrics in operator chat or status.
- Modify: `internal/runtime/control_actions.go`
  - Add budget breakdown to generic `/status` reporting.

### Tests

- Modify: `internal/runtime/runtime_api_test.go`
- Modify: `internal/runtime/prompt_context_assembler_test.go`
- Modify: `internal/runtime/execution_service_test.go`
- Modify: `internal/compaction/assembler_test.go`
- Create: `internal/runtime/prompt_budget_test.go`
- Create: `internal/runtime/pruning_test.go`
- Modify: `internal/transport/telegram/adapter_test.go`
- Modify: `tests/integration/coordinator_flow_test.go`

### Docs

- Modify: `docs/agent/prompt-assembly-order.md`
- Modify: `docs/agent/05-memory-and-recall.md`
- Modify: `docs/agent/06-compaction.md`
- Create: `docs/agent/context-budget.md`
- Modify: `docs/agent/operator-chat.md`
- Modify: `docs/agent/core-architecture-walkthrough.md`

### Tracking / Decisions

- Modify: `memory/2026-04-12.md`
- Optionally add decisions in `memory/decisions/` when one of the major architectural calls below is finalized.

---

## Task 1: Finish SessionHead As Canonical Recent-Context Layer

**Files:**
- Modify: `internal/runtime/types.go`
- Modify: `internal/runtime/runtime_api.go`
- Modify: `internal/runtime/agent_core.go`
- Modify: `internal/runtime/control_actions.go`
- Test: `internal/runtime/runtime_api_test.go`

- [ ] **Step 1: Write failing tests for generic recent-context visibility**

Add tests that prove:

- `SessionState(...)` returns `SessionHead`
- `ListSessions(...)` returns `SessionHead`
- `ControlState(...)` / `FormatControlReport(...)` expose `SessionHead`

Expected failures:

- missing `Head`
- missing fields
- missing recent-context section in control report

- [ ] **Step 2: Run targeted tests to verify they fail for the intended reason**

Run:

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'SessionHead|ControlState|SessionState' -count=1
```

Expected:

- FAIL due to missing generic surfaces or missing fields in rendered control state

- [ ] **Step 3: Implement the minimal runtime/API changes**

Implement only enough to make tests pass:

- propagate `SessionHead` through runtime views
- keep it transport-agnostic
- do not add phrase-matching or Telegram-only semantics

- [ ] **Step 4: Re-run targeted tests**

Run:

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'SessionHead|ControlState|SessionState' -count=1
```

Expected:

- PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/types.go internal/runtime/runtime_api.go internal/runtime/agent_core.go internal/runtime/control_actions.go internal/runtime/runtime_api_test.go
git commit -m "feat(teamD): expose session head through runtime surfaces"
```

### Exit Criteria

- `SessionHead` is visible through generic runtime surfaces
- no transport-specific phrase logic exists
- operator surfaces can build on runtime truth instead of transcript guessing

---

## Task 2: Add Prompt Budget Transparency

**Files:**
- Create: `internal/runtime/prompt_budget.go`
- Modify: `internal/runtime/types.go`
- Modify: `internal/runtime/control_actions.go`
- Modify: `internal/transport/telegram/run_state.go`
- Modify: `internal/transport/telegram/status_card.go`
- Test: `internal/runtime/prompt_budget_test.go`
- Test: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write failing tests for budget breakdown**

Add tests that assert a projected budget report can show, at minimum:

- full context window
- prompt budget
- raw transcript estimate
- checkpoint estimate
- SessionHead estimate
- memory recall estimate
- skills estimate
- final projected total

Also add a Telegram/UI-oriented test asserting the status surface can show:

- prompt-budget percent
- full-window percent

- [ ] **Step 2: Run the new tests to confirm failure**

Run:

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime ./internal/transport/telegram -run 'PromptBudget|ContextPercent|Status' -count=1
```

Expected:

- FAIL because no structured budget breakdown exists yet

- [ ] **Step 3: Implement `prompt_budget.go`**

Define a focused runtime helper for:

- per-layer token estimates
- projected final prompt size
- prompt-budget percent
- full-window percent

Keep this runtime-owned. Do not compute the logic independently in Telegram.

- [ ] **Step 4: Thread the metrics into control and status views**

Wire the new budget breakdown into:

- runtime control report
- Telegram run state/status card

- [ ] **Step 5: Re-run targeted tests**

Run:

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime ./internal/transport/telegram -run 'PromptBudget|ContextPercent|Status' -count=1
```

Expected:

- PASS

- [ ] **Step 6: Commit**

```bash
git add internal/runtime/prompt_budget.go internal/runtime/types.go internal/runtime/control_actions.go internal/transport/telegram/run_state.go internal/transport/telegram/status_card.go internal/runtime/prompt_budget_test.go internal/transport/telegram/adapter_test.go
git commit -m "feat(teamD): add prompt budget transparency"
```

### Exit Criteria

- operators can see where prompt budget is going
- `75%` no longer means "something vague"
- prompt-budget and full-window percentages are separate and explicit

---

## Task 3: Switch Compaction Trigger To Projected Final Prompt Accounting

**Files:**
- Modify: `internal/runtime/prompt_context.go`
- Modify: `internal/runtime/prompt_context_assembler.go`
- Modify: `internal/runtime/prompt_budget.go`
- Test: `internal/runtime/execution_service_test.go`
- Test: `internal/runtime/prompt_context_assembler_test.go`

- [ ] **Step 1: Write failing tests for projected trigger behavior**

Add tests covering:

- raw transcript below trigger, but final projected prompt above trigger due to injected layers
- compaction should run in that case
- raw transcript above trigger but final budget after pruning/projection is still legitimate and remains deterministic

- [ ] **Step 2: Run targeted tests to verify failure**

Run:

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'Compaction|PromptContext|Projected' -count=1
```

Expected:

- FAIL because current trigger checks raw history only

- [ ] **Step 3: Refactor `prepareConversationRound(...)` and trigger logic**

Change the logic so that compaction decisions use a projected final prompt estimate that includes:

- checkpoint
- SessionHead
- memory recall
- skills blocks
- transcript tail

Do not recurse infinitely. Use a deterministic pre-assembly estimate path.

- [ ] **Step 4: Re-run targeted tests**

Run:

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'Compaction|PromptContext|Projected' -count=1
```

Expected:

- PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/prompt_context.go internal/runtime/prompt_context_assembler.go internal/runtime/prompt_budget.go internal/runtime/execution_service_test.go internal/runtime/prompt_context_assembler_test.go
git commit -m "feat(teamD): trigger compaction from projected prompt budget"
```

### Exit Criteria

- compaction no longer waits until raw transcript alone crosses the line
- injected system overhead participates in trigger decisions
- operator-visible metrics and runtime behavior align better

---

## Task 4: Separate Context Classes

**Files:**
- Create: `internal/runtime/context_layers.go`
- Modify: `internal/runtime/prompt_context_assembler.go`
- Modify: `internal/transport/telegram/conversation.go`
- Test: `internal/runtime/prompt_context_assembler_test.go`

- [ ] **Step 1: Write failing tests for context-layer classification**

Add tests asserting that context pieces are explicitly classified:

- always-loaded
- trigger-loaded
- on-demand

Minimum cases:

- workspace bootstrap is always-loaded
- SessionHead is always-loaded
- continuity recall is trigger-loaded
- large skills docs and artifacts remain on-demand

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'ContextLayers|PromptContextAssembler' -count=1
```

Expected:

- FAIL because no explicit context class model exists

- [ ] **Step 3: Implement `context_layers.go` and update assembler**

Introduce a small, explicit model for context classes.

Requirements:

- no behavior explosion
- no plugin system
- no transport-specific source of truth
- deterministic ordering

- [ ] **Step 4: Re-run tests**

Run:

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'ContextLayers|PromptContextAssembler' -count=1
```

Expected:

- PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/context_layers.go internal/runtime/prompt_context_assembler.go internal/transport/telegram/conversation.go internal/runtime/prompt_context_assembler_test.go
git commit -m "refactor(teamD): classify prompt context layers"
```

### Exit Criteria

- context is no longer "one fuzzy bucket"
- runtime can reason about what deserves constant residency
- future pruning logic has explicit layer inputs

---

## Task 5: Add Pruning As A Separate Layer

**Files:**
- Create: `internal/runtime/pruning.go`
- Create: `internal/runtime/pruning_test.go`
- Modify: `internal/compaction/assembler.go`
- Modify: `internal/runtime/prompt_context.go`
- Modify: `docs/agent/06-compaction.md`

- [ ] **Step 1: Write failing pruning tests**

Cover these cases:

- old noisy tool output remains in durable transcript but is not fully retained in prompt assembly
- recent active-turn messages are not pruned away
- pruning does not rewrite stored transcript
- pruning runs before last-chance fitting of older prefix

- [ ] **Step 2: Run pruning tests to verify failure**

Run:

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime ./internal/compaction -run 'Pruning|AssemblePrompt' -count=1
```

Expected:

- FAIL because pruning is not yet a distinct phase

- [ ] **Step 3: Implement pruning**

Create a narrow pruning layer that:

- operates on prompt residency, not durable storage
- removes or compresses low-value old prompt baggage
- preserves the active tail and critical recent context

- [ ] **Step 4: Re-run pruning tests**

Run:

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime ./internal/compaction -run 'Pruning|AssemblePrompt' -count=1
```

Expected:

- PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/pruning.go internal/runtime/pruning_test.go internal/compaction/assembler.go internal/runtime/prompt_context.go docs/agent/06-compaction.md
git commit -m "feat(teamD): add prompt pruning before compaction fitting"
```

### Exit Criteria

- `teamD` gains a middle layer between "keep everything" and "rewrite everything"
- compaction and pruning become separate concepts
- old tool noise no longer competes as aggressively for prompt residency

---

## Task 6: Prepare Selection/Extraction-Based Old-Prefix Handling

**Files:**
- Modify: `internal/runtime/pruning.go`
- Modify: `internal/runtime/prompt_budget.go`
- Create: `internal/runtime/prefix_selection.go`
- Create: `internal/runtime/prefix_selection_test.go`
- Modify: `docs/agent/05-memory-and-recall.md`
- Modify: `docs/agent/prompt-assembly-order.md`

- [ ] **Step 1: Write failing tests for old-prefix selection**

Add tests that prove:

- older prefix retention is not purely reverse-recency
- higher-value older blocks can be selected over less useful but newer noise
- active tail remains protected

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'PrefixSelection|Pruning' -count=1
```

Expected:

- FAIL because selection does not yet exist

- [ ] **Step 3: Implement minimal selection-first behavior**

Do not jump straight to full extraction chains.

Implement only:

- block-level selection for older prefix candidates
- deterministic ranking using runtime heuristics already available

Leave sentence-level extraction for a later follow-up if needed.

- [ ] **Step 4: Re-run tests**

Run:

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./internal/runtime -run 'PrefixSelection|Pruning' -count=1
```

Expected:

- PASS

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/pruning.go internal/runtime/prompt_budget.go internal/runtime/prefix_selection.go internal/runtime/prefix_selection_test.go docs/agent/05-memory-and-recall.md docs/agent/prompt-assembly-order.md
git commit -m "feat(teamD): add selection-first retention for old context"
```

### Exit Criteria

- old prefix retention is no longer naive recency-only filling
- groundwork exists for later extraction-based compression

---

## Task 7: Update Operator And Engineering Docs

**Files:**
- Create: `docs/agent/context-budget.md`
- Modify: `docs/agent/prompt-assembly-order.md`
- Modify: `docs/agent/05-memory-and-recall.md`
- Modify: `docs/agent/06-compaction.md`
- Modify: `docs/agent/core-architecture-walkthrough.md`
- Modify: `docs/agent/operator-chat.md`

- [ ] **Step 1: Write failing doc checklist**

Create a checklist in the plan or issue comments covering:

- SessionHead role
- prompt budget breakdown
- projected trigger behavior
- context classes
- pruning vs compaction

- [ ] **Step 2: Update docs**

Document:

- exact prompt-layer order
- why compaction can trigger before full-window saturation
- what pruning does and does not do
- how operators should read context metrics

- [ ] **Step 3: Verify doc paths and references**

Run:

```bash
rg -n "SessionHead|prompt budget|pruning|context class|projected" docs/agent
```

Expected:

- the new concepts appear in the right docs

- [ ] **Step 4: Commit**

```bash
git add docs/agent/context-budget.md docs/agent/prompt-assembly-order.md docs/agent/05-memory-and-recall.md docs/agent/06-compaction.md docs/agent/core-architecture-walkthrough.md docs/agent/operator-chat.md
git commit -m "docs(teamD): document layered context and budget governance"
```

### Exit Criteria

- operators understand why context appears to shrink "early"
- developers understand the difference between SessionHead, compaction, pruning, and memory recall

---

## Final Verification

- [ ] **Step 1: Run the full test suite**

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go test ./... -count=1
```

Expected:

- all green

- [ ] **Step 2: Build the shipped binaries**

```bash
GOTMPDIR=$PWD/.tmp/go GOCACHE=$PWD/.tmp/gocache go build ./cmd/coordinator ./cmd/worker
```

Expected:

- successful build

- [ ] **Step 3: Smoke-check operator surfaces**

At minimum:

```bash
./teamd-agent control 1001:default run.status 1001
./teamd-agent sessions show 1001:default
```

Expected:

- SessionHead and budget/context data are visible through generic surfaces

- [ ] **Step 4: Close or update beads**

Close completed tasks and create any follow-up issues for:

- extraction-based compression
- cache-aware prompt stability
- model-switch SessionHead hygiene if not fully addressed here

---

## Suggested Task Ordering For Execution

1. Task 1: Finish SessionHead surfaces
2. Task 2: Prompt budget transparency
3. Task 3: Projected compaction trigger
4. Task 4: Context classes
5. Task 5: Pruning
6. Task 6: Old-prefix selection
7. Task 7: Docs

This ordering matters because:

- SessionHead must be canonical before richer behaviors rely on it
- transparency must arrive before deeper budget logic, otherwise changes remain opaque
- projected trigger decisions should be built on visible layer accounting
- context classes should exist before pruning
- pruning should exist before smarter selection

## Not In Scope For This Plan

These are intentionally excluded from this execution plan:

- mesh integration changes
- TUI implementation
- Telegram-specific phrase semantics for "оформи как проект"
- extraction-based sentence compressor using extra LLM calls
- model/provider prompt caching optimization as a primary goal

Those are separate follow-ups once the runtime context stack is clear and observable.
