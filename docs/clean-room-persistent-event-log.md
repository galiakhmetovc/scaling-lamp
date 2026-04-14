# Clean-Room Persistent Event Log

This document describes the first persistent event log implementation in the clean-room runtime.

## Current Implementations

### `InMemoryEventLog`

Current behavior:
- append-only in memory
- sequence assigned on append
- useful for tests and temporary runtime state

### `FileEventLog`

Current behavior:
- append-only JSONL on disk
- creates parent directory when needed
- restores sequence counter by scanning existing events on open
- supports aggregate reads after reopen

Current file format:
- one JSON-encoded `eventing.Event` per line

Current envelope baseline includes:
- global event `Sequence`
- per-aggregate `AggregateVersion`
- `CorrelationID`
- `CausationID`
- `Source`
- `ActorID`
- `ActorType`
- `TraceSummary`
- `TraceRefs`
- `ArtifactRefs`

## Current Config Surface

### `spec.runtime.event_log`

Current supported ids:
- `in_memory`
- `file_jsonl`

### `spec.runtime.event_log_path`

Current role:
- filesystem path for the `file_jsonl` event log

## Current Wiring

### `internal/runtime/component_registry.go`

Current role:
- build the selected event log from runtime config
- pass `event_log_path` into the `file_jsonl` implementation

### `internal/runtime/agent_builder.go`

Current role:
- build configured event log through the component registry

## Current Limitation

- file log is local filesystem only
- reads still scan the whole JSONL file
- there is no compaction, index, or projection checkpointing yet
- envelope is stronger now, but it still lacks a richer replay/debug model beyond the current baseline
