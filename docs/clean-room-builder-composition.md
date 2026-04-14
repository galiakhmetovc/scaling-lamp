# Clean-Room Builder Composition

This document describes how builder composition works in the clean-room runtime today.

## Current Rule

`BuildAgent` no longer chooses projections and runtime executors through hardcoded defaults.

It now reads explicit runtime composition from the root config:

- `event_log`
- `transport_executor`
- `request_shape_executor`
- `projections`

## Current Files

### `internal/config/types.go`

Current role:
- define `spec.runtime` for the root config
- hold explicit runtime component ids

### `internal/runtime/component_registry.go`

Current role:
- register built-in event log factories
- register built-in transport executor factories
- register built-in request-shape executor factories
- register built-in projection factories

Current built-in ids:
- `in_memory`
- `transport_default`
- `request_shape_default`
- `session`
- `run`

### `internal/runtime/agent_builder.go`

Current role:
- read `spec.runtime`
- build the configured event log
- build the configured projection set
- build the configured transport executor
- build the configured request-shape executor

## Current Limitation

- component registry is still built-in, not config-composed
- provider client exists below runtime, but builder still does not construct it
- root config must now carry explicit runtime composition for builder assembly
