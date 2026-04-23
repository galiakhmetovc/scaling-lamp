# Memory Foundation Design

## Goal

Build a native memory foundation for `teamD` that supports:

- durable historical session retrieval
- searchable project knowledge
- real session archival tiers
- bounded memory retrieval through canonical tools

This design must preserve one canonical runtime path and avoid introducing a second memory service.

## Scope

This slice covers:

- retention and archival policy for historical sessions
- native exact search and read APIs for sessions
- native exact search and read APIs for project knowledge
- archive bundle format for cold session storage
- indexing model for session memory and knowledge memory

This slice does not cover:

- semantic/vector retrieval as a required dependency
- graph memory
- full-workspace semantic indexing by default
- channel adapters
- workflow packaging

## Constraints

- Keep CLI, TUI, and HTTP thin over the same app/runtime layer.
- Keep large retrieval bounded and cursor-based.
- Reuse existing persistence and archive infrastructure where possible.
- Do not silently rehydrate archived memory into prompts.
- Do not treat the entire code repository as memory by default.

## Problem

`teamD` already stores:

- sessions
- transcripts
- context summaries
- context offloads
- artifacts

But it lacks:

- first-class historical retrieval tools
- retention tiers
- physical archive bundles for cold sessions
- searchable project knowledge outside the session timeline

This creates a gap versus systems like OpenClaw and Hermes, which already expose session-history search and stronger memory retrieval ergonomics.

## Recommended Architecture

Use a dual-domain native memory foundation.

### Domain 1: Session Memory

Session memory contains everything created by the runtime itself:

- sessions
- transcripts
- context summaries
- context offloads and artifact references
- related run/job metadata when needed for recall

This domain gets a real lifecycle:

- `active`
- `warm`
- `cold`

### Domain 2: Knowledge Memory

Knowledge memory contains project text sources outside the session timeline:

- `README.md`
- `SYSTEM.md`
- `AGENTS.md`
- `docs/**`
- `projects/**`
- `notes/**`
- later, optional configured extra roots

This domain does not use the same retention lifecycle as session memory. Its source of truth remains the workspace files.

## Why Two Domains

Session history and project knowledge are different retrieval problems.

Session history answers:

- what did the agent/operator already discuss or do
- what happened in previous runs
- what artifacts and summaries were produced

Project knowledge answers:

- what the repository already documents
- what prior project notes and decisions say

If these are merged into one undifferentiated memory blob, retrieval quality will degrade and ranking will become noisy.

## Retention Model

Retention applies only to Session Memory.

### Active

- session is live or recently active
- full payload stays in primary storage
- reads behave like current storage

### Warm

- session is inactive but still fully searchable and readable
- full payload stays in primary storage
- prompt assembly still uses explicit bounded retrieval, not eager replay

### Cold

- full session payload moves into an archive bundle
- primary storage keeps:
  - metadata
  - search documents
  - archive pointer
  - small bounded snippets
- full reads go through archive hydration on demand

## Archive Bundle Format

Cold sessions are stored under:

- `data_dir/archives/sessions/<session_id>/`

Bundle contents:

- `manifest.json`
- `summary.json`
- `transcript.ndjson`
- `artifacts/<artifact_id>.bin`
- optional sidecar metadata files if needed later

Manifest must include:

- `session_id`
- `archive_version`
- `created_at`
- `archived_at`
- transcript byte length and checksum
- list of included artifacts with checksums
- summary availability

## Storage Design

Build on top of `agent-persistence`.

### New retention metadata

Add a `session_retention` table:

- `session_id`
- `tier`
- `last_accessed_at`
- `archived_at`
- `archive_manifest_path`
- `archive_version`
- `updated_at`

### New session search docs

Add a `session_search_docs` table:

- `doc_id`
- `session_id`
- `source_kind`
- `source_ref`
- `body`
- `updated_at`

Add an FTS table:

- `session_search_fts`

Indexed session sources:

- session title
- context summary
- transcript text
- artifact labels or summaries

### New knowledge source metadata

Add a `knowledge_sources` table:

- `source_id`
- `path`
- `kind`
- `sha256`
- `byte_len`
- `mtime`
- `indexed_at`

### New knowledge search docs

Add a `knowledge_search_docs` table:

- `doc_id`
- `source_id`
- `path`
- `kind`
- `body`
- `updated_at`

Add an FTS table:

- `knowledge_search_fts`

Indexed knowledge file kinds in the first slice:

- Markdown
- plain text
- JSON
- YAML
- TOML

## Canonical Tool Surface

First slice should expose four tools:

- `session_search`
- `session_read`
- `knowledge_search`
- `knowledge_read`

### session_search

Input:

- `query`
- optional `tiers`
- optional `agent_profile_id`
- optional time range
- `limit`
- optional `cursor`

Output:

- bounded result rows
- `session_id`
- `title`
- `agent_name`
- `tier`
- `updated_at`
- `match_source`
- `snippet`
- `next_cursor`
- `truncated`

### session_read

Input:

- `session_id`
- `mode = summary | timeline | transcript | artifacts`
- optional `cursor`
- optional `max_bytes`
- optional `max_items`

Output:

- session metadata
- bounded text/items
- `tier`
- `from_archive`
- `next_cursor`
- `truncated`

### knowledge_search

Input:

- `query`
- optional `kinds`
- optional `roots`
- `limit`
- optional `cursor`

Output:

- `path`
- `kind`
- `snippet`
- `sha256`
- `mtime`
- `next_cursor`
- `truncated`

### knowledge_read

Input:

- `path`
- `mode = excerpt | full`
- optional `cursor`
- optional `max_bytes`
- optional `max_lines`

Output:

- `path`
- `kind`
- bounded text
- `next_cursor`
- `truncated`

## Retrieval Philosophy

Use progressive disclosure.

Every retrieval flow should follow:

1. search index
2. snippet or summary
3. full source only on explicit read

Do not inject entire archives, transcripts, or project docs into prompts by default.

## First Implementation Slice

The first slice should be intentionally narrow:

1. retention metadata and retention tier model
2. archive bundle writer and reader for cold sessions
3. session search indexing
4. `session_search`
5. `session_read`

Knowledge search should begin in the same project but can follow immediately after session memory is stable.

## Follow-On Slices

After exact retrieval is stable:

1. `knowledge_search`
2. `knowledge_read`
3. background index refresh and archival workers
4. optional semantic/provider-backed retrieval layer
5. transcript reflection and project/user memory

## Error Handling

Expected failure modes:

- archive bundle missing
- archive manifest integrity mismatch
- stale knowledge index entry
- read request too large
- unknown session id or path

Expected behavior:

- return bounded structured tool errors
- never crash the run because a memory read failed
- allow reindex or archive-repair flows later

## Testing Strategy

The implementation should be test-driven and phased.

Tests should cover:

- retention metadata round-trip
- archive bundle round-trip
- cold session read hydration
- bounded `session_search`
- bounded `session_read`
- knowledge indexing and read for canonical roots
- stale/missing archive failure behavior

## Recommended Execution Order

1. persistence schema for retention metadata
2. runtime types for tiers and archive manifests
3. archive bundle writer/reader
4. session search index
5. `session_search`
6. `session_read`
7. knowledge index
8. `knowledge_search`
9. `knowledge_read`

## Bottom Line

The right first memory system for `teamD` is not a semantic graph and not a second memory daemon.

It is:

- native
- dual-domain
- retention-aware
- archive-backed
- exact-search first
- bounded by default

That closes the biggest real gap versus OpenClaw and Hermes while preserving the current canonical runtime path.
