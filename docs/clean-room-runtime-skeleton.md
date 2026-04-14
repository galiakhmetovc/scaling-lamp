# Clean-Room Runtime Skeleton

This document explains the current clean-room runtime skeleton in `rewrite/clean-room-root`.

## Current Files

### `go.mod`

Minimal module definition for the orphan rewrite branch.

### `cmd/agent/main.go`

Process entrypoint.

Current responsibility:
- accept `--config`
- call `runtime.BuildAgent`
- fail fast on invalid config or build errors

It is intentionally thin. Runtime assembly belongs in the builder, not in `main`.

### `internal/config/types.go`

Root config types for the first skeleton.

Current responsibility:
- represent one root agent config
- hold explicit contract references

### `internal/config/loader.go`

Minimal root config loader.

Current responsibility:
- read YAML
- decode root config
- resolve explicit module paths relative to the root config location
- load module headers for validation

Current limitation:
- only one contract path is modeled
- no full module graph loading yet

### `internal/config/registry.go`

Minimal module kind registry.

Current responsibility:
- register supported module kinds
- validate loaded module headers before builder wiring

### `internal/runtime/eventing/events.go`

Shared event model.

Current responsibility:
- define canonical event envelope
- define aggregate kinds
- define first event kinds

This package exists to prevent import cycles between runtime and projections.

Current baseline now includes:
- `Sequence`
- `CorrelationID`
- `CausationID`
- `Source`

### `internal/runtime/event_log.go`

Event log contract and in-memory implementation.

Current responsibility:
- append events
- list events by aggregate

Current limitation:
- no persistence

### `internal/runtime/projections/projection.go`

Common projection contract.

Current responsibility:
- define the minimal `Apply(event)` shape for projections

### `internal/runtime/projections/registry.go`

Minimal projection registry.

Current responsibility:
- register projection factories
- build projection sets by name

### `internal/runtime/projections/session.go`

Minimal `SessionProjection`.

Current responsibility:
- project `session.created` into a small session snapshot

### `internal/runtime/projections/run.go`

Minimal `RunProjection`.

Current responsibility:
- project `run.started` into a small run snapshot

### `internal/runtime/agent_builder.go`

First runtime builder shell.

Current responsibility:
- load root config
- create in-memory event log
- validate transport contract module kind
- assemble projections through a registry
- return one built agent instance

Current limitation:
- no module registry
- no contract resolution
- no executor wiring

## What Is Good About This Skeleton

- Event model is already separated from projection code.
- `main` is thin.
- Config loading is explicit and path-based.
- The first projection boundary exists.
- The system can be extended without importing legacy runtime code.

## What Is Still A Shortcut

These are known temporary shortcuts and should be removed in the next slices.

1. `AgentBuilder` hardcodes projection registration.
2. Config loading is not yet module-driven beyond the root contract path and header validation.
3. Event envelopes are still too small for a serious event-sourced system, even after adding sequence and trace linkage metadata.
4. There is no contract registry, policy registry, or strategy registry yet.
5. There is no persistent event store or persistent projections yet.

## Next Required Slices

1. full contract and policy module graph loading
2. richer event envelope and persistent event log contract
3. contract resolver
4. first `TransportContract` executor
5. persistent projections
