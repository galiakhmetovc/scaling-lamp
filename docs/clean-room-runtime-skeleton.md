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
- hold explicit contract references as a map

### `internal/config/loader.go`

Minimal root config loader.

Current responsibility:
- read YAML
- decode root config
- resolve explicit module paths relative to the root config location
- load module graph through registry metadata

Current limitation:
- graph stores headers only
- loader does not decode effective contracts itself; contract decoding now happens in the runtime resolver

### `internal/config/registry.go`

Minimal module kind registry.

Current responsibility:
- register supported module kinds
- classify module kinds
- declare allowed reference fields
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
- expose built-in projection composition
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
- validate loaded contract and policy module kinds
- resolve typed runtime contracts from loaded modules
- build the first transport executor from resolved transport contracts
- build the first request-shape executor from resolved request-shape contracts
- assemble projections through built-in registries
- return one built agent instance

Current limitation:
- builder still chooses default projection composition instead of config-driven composition
- transport and request-shape executors are wired, but no combined provider client exists yet

### `internal/contracts/contracts.go`

Resolved runtime contract types.

Current responsibility:
- define the first typed runtime contracts for one built agent instance
- expose `ProviderRequestContract` and `MemoryContract` as stable runtime surfaces

### `internal/runtime/contract_resolver.go`

First contract resolver.

Current responsibility:
- decode transport, request-shape, and memory contract modules
- resolve policy module paths relative to their contract files
- produce typed resolved contracts for one agent instance

Current limitation:
- only transport, request-shape, and memory are resolved
- there is still no policy merge layer (`global < session < run`)
- execution-time application currently covers transport and request-shape only

### `internal/provider/transport_executor.go`

First provider-facing transport executor.

Current responsibility:
- apply resolved transport contract to one outbound HTTP request
- handle static endpoint, bearer auth, retry, and per-request timeout baseline
- expose a testable execution surface through injected HTTP doer and timing hooks

### `internal/provider/request_shape_executor.go`

First provider request-body executor.

Current responsibility:
- apply resolved request-shape contract to produce exact provider JSON bytes
- handle model, raw messages, inline tools, response format, streaming, and sampling baseline

## What Is Good About This Skeleton

- Event model is already separated from projection code.
- `main` is thin.
- Config loading is explicit and path-based.
- The first projection boundary exists.
- The system can be extended without importing legacy runtime code.

## What Is Still A Shortcut

These are known temporary shortcuts and should be removed in the next slices.

1. `AgentBuilder` still chooses the default projection set instead of using config-driven composition.
2. Config graph loading still stops at module headers and does not decode effective contracts itself.
3. Event envelopes are still too small for a serious event-sourced system, even after adding sequence and trace linkage metadata.
4. Contract resolution is still narrow and only covers the first transport/request-shape/memory path.
5. Provider execution is still split into transport and request-shape pieces with no combined provider client yet.
6. There is no persistent event store or persistent projections yet.
7. There is no full policy and strategy registry system yet.

## Next Required Slices

1. combined provider client over request-shape and transport executors
2. persistent event log
3. persistent projections
4. policy and strategy registries
5. prompt asset policy domain
