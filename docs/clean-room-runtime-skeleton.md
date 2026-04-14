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

Current limitation:
- only one contract path is modeled
- no module kind registry or module file loading yet

### `internal/runtime/eventing/events.go`

Shared event model.

Current responsibility:
- define canonical event envelope
- define aggregate kinds
- define first event kinds

This package exists to prevent import cycles between runtime and projections.

### `internal/runtime/event_log.go`

Event log contract and in-memory implementation.

Current responsibility:
- append events
- list events by aggregate

Current limitation:
- no sequence number
- no causation/correlation metadata
- no persistence

### `internal/runtime/projections/projection.go`

Common projection contract.

Current responsibility:
- define the minimal `Apply(event)` shape for projections

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
- register the first projections
- return one built agent instance

Current limitation:
- projections are hardcoded
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
2. Config loading is not yet module-driven beyond the root contract path.
3. Event envelopes are still too small for a serious event-sourced system.
4. There is no contract registry, policy registry, or strategy registry yet.
5. There is no persistent event store or persistent projections yet.

## Next Required Slices

1. config module registry and module loading
2. richer event envelope and event log contract
3. projection registry
4. contract resolver
5. first `TransportContract` executor
