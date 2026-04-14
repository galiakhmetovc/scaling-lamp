# Context Budget

## Why context feels "full" early

`teamD` does not treat the full provider context window as fully available for raw transcript.

There are three separate numbers:

- `ContextWindowTokens`
- `PromptBudgetTokens`
- `CompactionTriggerTokens`

The practical consequence is:

- compaction can start before the full model window is exhausted
- transcript can lose residency before the operator sees "100%"

This is intentional. The runtime keeps headroom for:

- checkpoint summary
- SessionHead
- memory recall
- skills metadata
- active-turn preservation

## What the runtime now tracks

The runtime now records a prompt budget snapshot per run:

- raw transcript tokens
- checkpoint tokens
- workspace tokens
- SessionHead tokens
- memory recall tokens
- skills catalog tokens
- active skills tokens
- system overhead tokens
- final prompt token estimate
- prompt-budget percent
- full-window percent

This snapshot is runtime-owned and available through generic control surfaces, not only Telegram UI.

The local web session test bench exposes the same budget signals alongside:

- transcript mutation timeline
- `SessionHead`
- recent-work provenance
- recall provenance
- compaction/pruning signals
- artifact offload references

## Prompt budget percent vs context window percent

These are different numbers.

- `Prompt budget percent`
  - how close the final assembled prompt is to the runtime prompt budget
- `Context window percent`
  - how close the final assembled prompt is to the full provider context window

If the second number looks "low" while context already feels tight, the first one is usually the explanation.

## Why compaction now starts earlier and more honestly

Older `teamD` behavior checked only the raw transcript estimate.

Current behavior checks the projected final prompt:

1. raw session messages
2. checkpoint
3. runtime-owned prompt layers
4. final projected prompt size

That means compaction can trigger even when raw transcript alone is still below threshold, if the final prompt would exceed the configured trigger after injected layers are added.

## Context classes

The runtime now classifies injected prompt layers explicitly:

- `always_loaded`
  - workspace
  - SessionHead
- `trigger_loaded`
  - recent-work follow-up guidance
  - memory recall
  - active skills
- `on_demand`
  - large artifacts, large skill docs, long memory bodies

This makes it easier to reason about what deserves constant prompt residency and what should stay out of the hot path.

## Pruning vs compaction

These are different mechanisms.

`Compaction`:

- rewrites older session history into a checkpoint
- durable effect

`Pruning`:

- reduces old prompt baggage for the current provider request
- does not rewrite durable transcript state

Pruning exists so `teamD` does not have to choose only between:

- keeping everything inline
- summarizing everything away

## Operator-facing implications

If context feels bad, inspect these in order:

1. `Prompt budget percent`
2. `System overhead`
3. `Recent context` from SessionHead
4. whether long tool outputs were offloaded or are still competing for residency
5. whether recall and active skills are larger than expected

The fastest way to inspect this live is now the embedded web test bench:

- open `/debug/test-bench`
- pick the target session
- watch `transcript.appended`, `prompt.assembled`, `session_head.updated`
- inspect run-level context provenance via the right-hand pane

## Relevant code

- [prompt_budget.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/prompt_budget.go)
- [prompt_context.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/prompt_context.go)
- [prompt_context_assembler.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/prompt_context_assembler.go)
- [context_layers.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/context_layers.go)
- [pruning.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/pruning.go)
- [assembler.go](/home/admin/AI-AGENT/data/projects/teamD/internal/compaction/assembler.go)
