# Context Budget Engine Design

**Date:** 2026-04-16

## Goal

Introduce a policy-driven runtime `context budget engine` that becomes the canonical source of truth for:
- token accounting
- context-size estimation for the next run
- summary/compaction accounting
- budget signals exposed to daemon, TUI, web, and prompt assembly

Phase 1 covers only:
- canonical token accounting
- runtime/session budget snapshot
- summary counter in `session head`

Actual dialog summarization and the optional judge are later phases built on top of the same surface.

## Why This Needs Its Own Engine

The current token display is an estimate scattered across clients. That is not good enough for:
- reliable operator visibility
- policy-driven compaction
- later verification/judge logic

The system needs one runtime-owned accounting surface, not multiple UI-local counters.

## Scope

### Included in Phase 1

- canonical session-scoped token budget snapshot
- provider-confirmed usage normalization
- runtime estimates for current context and next run
- daemon/API exposure for the snapshot
- TUI/web consumption of the same snapshot
- `session head` display of summarization count

### Not Included in Phase 1

- actual dialog summarization pipeline
- summary artifact generation
- automatic compaction actions
- optional judge/verifier model

## Architecture

### 1. New Contract Family

Add a dedicated `ContextBudgetContract` instead of overloading unrelated contracts.

It should contain:
- `AccountingPolicy`
- `EstimationPolicy`
- `CompactionPolicy`
- `SummaryDisplayPolicy`

Rationale:
- accounting semantics are not prompt assembly semantics
- compaction thresholds are runtime policy, not UI policy
- summary display is presentation-only and should stay narrow

### 2. Canonical Runtime Snapshot

Add a session-scoped `ContextBudgetSnapshot` with fields such as:

- `last_input_tokens`
- `last_output_tokens`
- `last_total_tokens`
- `current_context_tokens`
- `estimated_next_input_tokens`
- `draft_tokens`
- `queued_draft_tokens`
- `summary_tokens`
- `summarization_count`
- `compacted_message_count`
- `source`
- `budget_state`

`source` must explicitly distinguish:
- `provider`
- `estimated`
- `mixed`

The runtime must never silently present estimates as exact provider-confirmed values.

### 3. Provider Usage Normalization

Provider responses already return usage information. Normalize those payloads into a runtime-owned internal shape and persist them through events/projections.

Confirmed provider usage is the source of truth for completed runs.

### 4. Runtime Estimation

The engine must also calculate bounded live estimates for:
- current draft
- queued drafts
- current assembled context
- next-run input estimate

These estimates should be policy-driven. The estimation strategy should be replaceable later if a better tokenizer-backed implementation is introduced.

### 5. Budget State

The engine should classify session state using policy thresholds:
- `healthy`
- `approaching_limit`
- `needs_compaction`

Phase 1 only computes and exposes these states. It does not execute compaction yet.

### 6. Session Head Integration

The `session head` should expose a compact summary-count line, for example:

`🧠 Summaries: 2`

This is display-only. It should be controlled through session-head/display policy and should not embed summary bodies or other compaction internals.

## Data Flow

1. provider completes a run and returns usage
2. runtime records normalized usage
3. context budget projection updates session snapshot
4. daemon surfaces snapshot to web/TUI
5. prompt assembly reads summary counter for `session head`

## Policy Surface

### ContextBudgetContract

#### AccountingPolicy

Controls:
- whether accounting is enabled
- which provider usage fields are trusted

#### EstimationPolicy

Controls:
- estimation strategy
- character/token heuristics
- whether draft and queue are included

#### CompactionPolicy

Controls:
- warning thresholds
- compaction thresholds
- state transitions only in Phase 1

#### SummaryDisplayPolicy

Controls:
- whether summary count is surfaced
- label style if needed later

### Session Head Policy

Add narrow display params:
- `include_summary_counter`

The session head should not decide compaction logic. It should only decide whether to display the already-computed summary counter.

## Testing

Phase 1 should add tests for:
- provider usage normalization
- budget snapshot computation
- estimated context accounting for draft/queued drafts
- daemon/session snapshot exposure
- TUI/web consuming the same canonical fields
- session head rendering summary count under policy

## Risks

### 1. Exact vs estimated confusion

This is the biggest risk. The runtime must mark source explicitly and clients must render accordingly.

### 2. Premature compaction coupling

Do not mix compaction execution into Phase 1. Accounting comes first.

### 3. Provider-specific leakage

The runtime should normalize provider usage into an internal shape instead of letting provider-specific field names spread into daemon/UI code.

## Implementation Direction

Phase 1 should ship:
- the new contract family
- the budget snapshot projection/surface
- daemon/web/TUI display wiring
- summary counter in session head

Only after that should Phase 2 add real dialog summarization and compaction actions.
