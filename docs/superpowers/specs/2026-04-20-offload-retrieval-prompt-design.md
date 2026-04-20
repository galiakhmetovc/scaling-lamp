# Offload Retrieval Prompt Design

## Goal

Integrate canonical context offloads into the live prompt path without re-inflating the prompt.

## Decision

Follow the old Go runtime model:

- rolling context summary is still injected automatically
- offloaded payload bytes are **not** injected automatically
- the model sees a compact list of offloaded references in the prompt
- full offloaded content is recovered only through explicit retrieval tools

## Prompt Path

Prompt assembly stays single-path and becomes:

1. `SessionHead`
2. `PlanSnapshot`
3. `ContextSummary`
4. `Offloaded Context References`
5. uncovered transcript tail

The new offload block is a compact system message that contains only reference metadata:

- ref id
- label
- summary
- artifact id
- token estimate
- message count

This gives the model a durable memory map without rehydrating the payload text.

## Retrieval Tools

Add two canonical tools:

- `artifact_read`
- `artifact_search`

These are session-scoped retrieval tools over the current session's offloaded context.

`artifact_read` returns the full stored payload for one referenced artifact id.

`artifact_search` searches across the current session's offloaded references and payloads, then
returns matching artifact ids with compact previews.

These tools are exposed to the model only when the current session actually has offloaded context.

## Runtime Boundaries

No second runtime path is introduced.

- prompt assembly still happens in the existing prompt layer
- tool definitions still flow through the canonical tool catalog
- model-driven tool execution still goes through the existing provider loop
- retrieval uses the existing `ContextOffloadRepository`

## Non-Goals

This slice does not:

- auto-hydrate full offloaded payloads into prompt messages
- add a new artifact browser UI
- change compaction policy
- implement background-task offloading
