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
- hold explicit runtime composition under `spec.runtime`

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
- `AggregateVersion`
- `CorrelationID`
- `CausationID`
- `Source`
- `ActorID`
- `ActorType`
- `TraceSummary`
- `TraceRefs`
- `ArtifactRefs`

### `internal/runtime/event_log.go`

Event log contract and in-memory implementation.

Current responsibility:
- append events
- list events by aggregate

Current implementations:
- `InMemoryEventLog`
- `FileEventLog`

Current limitation:
- persistent storage exists, but only as append-only local JSONL

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

### `internal/runtime/projections/store.go`

Current projection snapshot store.

Current responsibility:
- save projection snapshots to JSON
- restore snapshots into a built projection set

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
- validate loaded contract and policy module kinds
- resolve typed runtime contracts from loaded modules
- build the configured event log from `spec.runtime`
- build the configured transport executor from `spec.runtime`
- build the configured request-shape executor from `spec.runtime`
- assemble configured projections from `spec.runtime`
- return one built agent instance

Current limitation:
- component selection is config-driven now, but only through the built-in component registry
- provider client now exists, but runtime assembly still exposes transport and request-shape executors separately too

### `internal/runtime/component_registry.go`

Current builder-component registry.

Current responsibility:
- register built-in runtime components by id
- build event log, executors, and projections from explicit config ids

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
- only transport, request-shape, memory, and prompt-assets are resolved
- there is still no policy merge layer (`global < session < run`)
- execution-time application currently covers transport and request-shape only

### `internal/policies/registry.go`

First policy and strategy registry layer.

Current responsibility:
- define policy families
- register supported policy kinds
- register allowed strategies per policy kind
- validate policy strategies during contract resolution

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
- prepend prompt asset messages when supplied by the prompt asset domain

## What Is Good About This Skeleton

- Event model is already separated from projection code.
- `main` is thin.
- Config loading is explicit and path-based.
- The first projection boundary exists.
- The system can be extended without importing legacy runtime code.

## What Is Still A Shortcut

These are known temporary shortcuts and should be removed in the next slices.

1. Config graph loading still stops at module headers and does not decode effective contracts itself.
2. Event envelopes are still too small for a serious event-sourced system, even after adding sequence and trace linkage metadata.
3. Contract resolution is still narrow and only covers the first transport/request-shape/memory path.
4. Provider client exists, but provider-specific response parsing and higher-level provider semantics are still missing.
5. Persistent event storage and projection snapshot restore both exist, but projection persistence is still snapshot-based only.
6. There is only a first built-in policy/strategy registry layer; config-driven registry composition and strategy-driven decoding still do not exist yet.
7. Prompt assets exist as a separate domain, but only as inline assets prepended during request-shape execution.
8. Builder composition is config-driven, but only against the built-in component registry.
9. Provider client result is still transport-level; provider-specific parsing and usage handling are not there yet.

## Next Required Slices

1. richer prompt asset execution and selection
2. automatic projection snapshot lifecycle
3. provider-specific response and usage handling
