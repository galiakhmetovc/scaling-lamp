# Clean-Room Current Runtime Flow

This document describes how the clean-room runtime works today in `rewrite/clean-room-root`.

It is an implementation snapshot.
It does not describe the target architecture beyond what already exists in code.

## Current End-To-End Flow

1. `cmd/agent` starts with one explicit `--config` path.
2. `runtime.BuildAgent` loads the root config.
3. `config.LoadModuleGraph` walks the config graph using built-in registry metadata.
4. `runtime.ResolveContracts` decodes typed runtime contracts from contract and policy modules.
   Policy strategies are validated through the built-in policy registry before contracts are returned.
5. `BuildAgent` assembles the current runtime instance from `spec.runtime`:
   - configured event log
   - configured projections
   - configured transport executor
   - configured request-shape executor
6. request-shape execution can build provider JSON body bytes from resolved request-shape contract.
   Prompt assets can now be prepended into the message list through the dedicated prompt asset domain.
7. the combined provider client composes request-shape and transport execution into one provider call.

## Process Entry

### `cmd/agent/main.go`

Current role:
- accept `--config`
- call `runtime.BuildAgent`
- fail fast on invalid config or build errors

Current boundary:
- no runtime assembly logic in `main`
- all composition is delegated to the builder

## Config Loading

### Root Config

Root config currently defines explicit contract module paths.

Current shape:
- one agent config
- one map of contract references
- one explicit runtime composition block
- no implicit imports
- no inheritance

### `internal/config/loader.go`

Current role:
- read root YAML
- resolve contract paths relative to the root config file
- load module graph headers and referenced paths

Important implementation detail:
- graph loading is registry-driven
- the loader does not hardcode contract-family-specific traversal anymore
- the loader still stores only headers in the graph, not fully decoded module bodies

### `internal/config/registry.go`

Current role:
- register supported module kinds
- classify them as `contract` or `policy`
- declare which reference fields each module kind exposes

This registry is what allows config graph walking to find referenced policy modules without hardcoding field names in the graph walker.

## Contract Resolution

### `internal/runtime/contract_resolver.go`

Current role:
- load contract files from the root config
- load referenced policy files relative to the referencing contract file
- decode them into typed runtime contracts
- validate policy strategy names before contracts reach executors

### `internal/policies/registry.go`

Current role:
- define the first built-in policy families
- register allowed strategy names per policy kind
- validate strategy names during contract resolution

Current resolved areas:
- `TransportContract`
- `RequestShapeContract`
- `MemoryContract`
- `PromptAssetsContract`

### `internal/contracts/contracts.go`

Current role:
- define the typed runtime contract layer below `runtime` and `provider`

This package exists so:
- resolver can return typed contracts
- provider code can consume those contracts
- runtime can hold the resolved result
- import cycles are avoided

## Builder

### `internal/runtime/agent_builder.go`

Current role:
- load root config
- build module graph
- validate loaded kinds through the built-in module registry
- resolve typed contracts
- build runtime components from `spec.runtime`

Current components built:
- `Contracts`
- `Transport`
- `RequestShape`
- `ProviderClient`
- `EventLog`
- `Projections`
- optional projection snapshot store

Current limitation:
- builder composition is explicit and config-driven now
- component selection still comes only from the built-in component registry

### `internal/runtime/component_registry.go`

Current role:
- register built-in runtime component factories
- map config ids to event log, executor, and projection construction

## Provider Client

### `internal/provider/client.go`

Current role:
- combine request-shape execution and transport execution
- return one normalized result object for a provider call

## Request-Shape Execution

### `internal/provider/request_shape_executor.go`

Current role:
- build exact provider JSON request body from resolved `RequestShapeContract`

Current supported fields:
- `model`
- `messages`
- `tools`
- `response_format`
- `stream`
- sampling fields:
  - `temperature`
  - `top_p`
  - `max_output_tokens`

Current input boundary:
- prompt assets can come in as semantic prompt messages
- raw messages come in as `contracts.Message`
- tools come in as raw inline tool definitions

Current output boundary:
- JSON body bytes only

It does not yet:
- assemble prompt-policy layers
- add provider-specific reasoning fields
- return a richer provider request object

It now does:
- prepend prompt asset messages before raw conversation messages when they are supplied in input

## Transport Execution

### `internal/provider/transport_executor.go`

Current role:
- take resolved `TransportContract`
- build one outbound HTTP request
- send it through an injected HTTP doer

Current applied behaviors:
- static endpoint URL assembly
- bearer auth
- retry handling
- per-request timeout
- endpoint extra headers

Current output boundary:
- simple transport response:
  - status code
  - headers
  - raw body bytes

It does not yet:
- parse provider-specific responses
- apply TLS policy family
- apply rate-limit policy family

## Event Log

### `internal/runtime/event_log.go`

Current role:
- append events
- list events by aggregate

Current implementations:
- `in_memory`
- `file_jsonl`

Current implication:
- file-backed event logs can survive reopen/restart
- event envelopes now carry:
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

## Projections

### `internal/runtime/projections/`

Current built-in projections:
- `session`
- `run`

### `internal/runtime/projections/registry.go`

Current role:
- register projection factories
- build configured projection sets
- serve as the construction source for persistent snapshot restore

Current limitation:
- projection persistence is snapshot-based only
- runtime still needs a higher-level save lifecycle after event application

## What Works Today

Today the clean-room branch can already do this:

- load one explicit agent config
- walk the contract/policy graph
- resolve typed transport/request-shape/memory contracts
- build one runtime instance
- build provider JSON body from request-shape contract
- send HTTP requests from transport contract
- keep a minimal event log and projections surface

## What Is Still Missing

These are current known gaps, not hidden assumptions:

- combined prompt asset execution path in builder/runtime assembly
- richer event-log indexing/compaction
- automatic projection snapshot flushing

## Related Documents

- [clean-room-runtime-skeleton.md](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/docs/clean-room-runtime-skeleton.md)
- [clean-room-contract-resolver.md](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/docs/clean-room-contract-resolver.md)
- [clean-room-builder-composition.md](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/docs/clean-room-builder-composition.md)
- [clean-room-policy-strategy-registries.md](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/docs/clean-room-policy-strategy-registries.md)
- [clean-room-prompt-assets.md](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/docs/clean-room-prompt-assets.md)
- [clean-room-persistent-event-log.md](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/docs/clean-room-persistent-event-log.md)
- [clean-room-persistent-projections.md](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/docs/clean-room-persistent-projections.md)
- [clean-room-provider-client.md](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/docs/clean-room-provider-client.md)
- [clean-room-transport-executor.md](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/docs/clean-room-transport-executor.md)
- [clean-room-request-shape-executor.md](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/docs/clean-room-request-shape-executor.md)
- [clean-room-implemented-strategies.md](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/docs/clean-room-implemented-strategies.md)
