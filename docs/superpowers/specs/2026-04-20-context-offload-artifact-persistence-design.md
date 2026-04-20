# Context Offload Artifact Persistence Design

## Goal

Add a canonical persistence layer for offloaded context so large context segments can move out of
the live prompt path without losing structured references or durable payload storage.

## Scope

This slice is intentionally narrow:

- add runtime types for `ContextOffloadSnapshot`, `ContextOffloadRef`, and `ContextOffloadPayload`
- persist offload snapshot metadata in SQLite
- persist offload payload bytes in the existing artifact store
- keep session ownership and cleanup consistent when offload snapshots are replaced

This slice does not yet:

- change prompt assembly
- retrieve offloaded context automatically
- expose offload state in CLI or TUI

## Model

`ContextOffloadSnapshot` is the canonical session-level metadata object.

- `session_id`
- `refs[]`
- `updated_at`

Each `ContextOffloadRef` contains:

- stable offload ref id
- human label
- short summary
- artifact id
- token estimate
- message count
- created_at

The actual large payload is stored separately as `ContextOffloadPayload` bytes under the existing
artifact store.

## Persistence

Add a new `context_offloads` table:

- `session_id` primary key
- `refs_json`
- `updated_at`

Payload bytes are written through the existing artifact store with:

- `kind = "context_offload"`
- canonical artifact paths under `artifacts/`
- metadata derived from the matching offload ref

## Replacement Behavior

Writing a new offload snapshot for a session replaces the old snapshot.

- referenced artifacts in the new snapshot are upserted
- artifacts that belonged to the previous snapshot but are no longer referenced are deleted

This keeps offload persistence bounded and avoids orphaned blobs during normal updates.

## Validation

`put_context_offload` requires an exact match between:

- artifact ids referenced by snapshot refs
- artifact ids provided in payloads

If they differ, persistence fails before storing inconsistent state.

