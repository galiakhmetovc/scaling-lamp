# Clean-Room Persistent Projections

This document describes the first persistent projection snapshot layer in the clean-room runtime.

## Current Design

Persistent projections are implemented as snapshot persistence, not full replay caching.

Current flow:
- projections are built from the configured projection registry
- builder optionally opens a projection snapshot store
- stored snapshots are loaded into the built projection set during startup
- `Agent.RecordEvent(...)` now applies events to projections and flushes snapshots automatically when a store is configured

## Current Files

### `internal/runtime/projections/projection.go`

Current role:
- define projection identity
- define event application
- define snapshot export/import boundary

### `internal/runtime/projections/store.go`

Current role:
- persist projection snapshots to one JSON file
- reload snapshots into a fresh projection set

Current format:
- JSON object keyed by projection id
- each value is the serialized snapshot payload for that projection

### `internal/runtime/agent_builder.go`

Current role:
- build projection set from runtime config
- open snapshot store when `projection_store_path` is configured
- restore snapshots during startup
- expose `Agent.RecordEvent(...)` as the runtime path that keeps snapshots current

## Current Config Surface

### `spec.runtime.projection_store_path`

Current role:
- filesystem path to the projection snapshot store

## Current Limitation

- persistence stores snapshots, not replay indexes
- snapshot flushing now happens through `Agent.RecordEvent(...)`, but there is still no richer batching, compaction, or replay-index layer
