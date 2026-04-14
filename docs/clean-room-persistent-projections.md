# Clean-Room Persistent Projections

This document describes the first persistent projection snapshot layer in the clean-room runtime.

## Current Design

Persistent projections are implemented as snapshot persistence, not full replay caching.

Current flow:
- projections are built from the configured projection registry
- builder optionally opens a projection snapshot store
- stored snapshots are loaded into the built projection set during startup

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

## Current Config Surface

### `spec.runtime.projection_store_path`

Current role:
- filesystem path to the projection snapshot store

## Current Limitation

- persistence stores snapshots, not replay indexes
- snapshots are not auto-flushed after every event yet
- builder restores snapshots, but runtime still needs a higher-level projection lifecycle for continuous persistence
