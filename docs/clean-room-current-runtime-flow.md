# Clean-Room Current Runtime Flow

This document describes how the clean-room runtime works today in `rewrite/clean-room-root`.

It is an implementation snapshot.
It does not describe the target architecture beyond what already exists in code.

## Current End-To-End Flow

1. `cmd/agent` starts with one explicit `--config` path.
   It can run a smoke request through `--smoke` or an interactive chat loop through `--chat`.
2. `runtime.BuildAgent` loads the root config.
3. `config.LoadModuleGraph` walks the config graph using built-in registry metadata.
4. `runtime.ResolveContracts` decodes typed runtime contracts from contract and policy modules.
   Policy strategies are validated through the built-in policy registry before contracts are returned.
5. `BuildAgent` assembles the current runtime instance from `spec.runtime`:
   - configured event log
   - configured projections
   - configured prompt-assembly executor
   - configured prompt-asset executor
   - configured tool-catalog executor
   - configured tool-execution gate
   - configured transport executor
   - configured request-shape executor
   - configured provider client
6. prompt-assembly execution can build top-of-prompt messages from:
   - file-backed system prompt
   - projection-backed session head
7. prompt-asset execution can resolve selected prompt assets from the dedicated prompt-asset contract.
8. tool-catalog execution can choose visible tools and serialize them for provider request bodies.
9. request-shape execution can build provider JSON body bytes from resolved request-shape contract.
10. the combined provider client composes prompt-assembly, prompt-asset execution, tool-catalog execution, request-shape execution, and transport execution into one provider call.
11. `internal/runtime/cli` owns terminal chat UX for `--chat` and `--resume`.

## Process Entry

### `cmd/agent/main.go`

Current role:
- accept `--config`
- optionally accept `--smoke`
- optionally accept `--chat` and `--resume`
- call `runtime.BuildAgent`
- optionally execute one smoke request through the built runtime agent
- optionally delegate terminal chat to `internal/runtime/cli`
- fail fast on invalid config, build errors, or smoke execution errors

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
- iterate configured contract module paths independently of root config map key names
- dispatch contract decoding by loaded `kind`
- load referenced policy files relative to the referencing contract file
- decode them into typed runtime contracts
- validate policy strategy names before contracts reach executors

Current resolver shape:
- generic contract iteration
- built-in kind-based dispatch
- generic typed policy loading for referenced policy modules

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
- `PromptAssemblyContract`
- `ToolContract`
- `PlanToolContract`
- `ToolExecutionContract`
- `ProviderTraceContract`
- `ChatContract`

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
- `PromptAssembly`
- `PromptAssets`
- `PlanTools`
- `ToolCatalog`
- `ToolExecution`
- `Transport`
- `RequestShape`
- `ProviderClient`
- `EventLog`
- `Projections`
- optional projection snapshot store

Current built-in projections:
- `session`
- `run`
- `transcript`
- `active_plan`
- `plan_archive`
- `plan_head`

### `internal/runtime/smoke.go`

Current role:
- define one runtime smoke seam above `ProviderClient`
- create session and run events for a smoke call
- record run start and run completion/failure through the event log and projections
- send one user prompt through the configured provider client

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
- combine prompt assembly, prompt-asset execution, tool selection, request-shape execution, and transport execution
- return one normalized result object for a provider call

Current provider pipeline now:
- build prompt-assembly messages from file-backed system prompt and projection-backed session head
- resolve selected prompt assets into prepend/append message buckets
- build built-in plan-tool definitions from `PlanToolContract`
- build visible tool surface from `ToolContract`
- serialize tools for provider payloads
- build request-shape JSON body
- optionally capture the exact outbound provider request through `ProviderTraceContract`
- execute transport
- parse provider-specific response body
- parse provider-emitted tool calls when present in non-streaming responses
- run parsed tool calls through `ToolExecutionContract`
- execute allowed built-in plan tool calls through the runtime plan-domain service
- append resulting tool-result messages and repeat provider execution until final assistant output
- extract normalized usage fields
- feed the combined result into the runtime smoke path when `cmd/agent --smoke` is used

Current limitation:
- parser still assumes OpenAI-compatible top-level wire shapes first
- stream semantics now emit typed `text` and `reasoning` events
- only the internal plan-tools domain has a real execution backend today
- non-plan tool backends are still not implemented

## Provider Trace Capture

Current provider trace support is policy-driven.

Current contract:
- `ProviderTraceContract`

Current policy family:
- `ProviderTracePolicy`

Current strategies:
- `none`
- `inline_request`

Current runtime event:
- `provider.request.captured`

Current behavior:
- if enabled, runtime records the exact assembled outbound provider request into the run event stream
- capture happens in both `--smoke` and `--chat`
- shipped `zai-smoke` config enables inline request capture

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
- top-of-prompt messages now arrive from the dedicated prompt-assembly executor
- prompt assets arrive as prepend/append message buckets from the dedicated prompt-asset executor
- raw messages come in as `contracts.Message`
- tools arrive as serialized provider tool entries from `ToolContract`

Current output boundary:
- JSON body bytes only

It does not yet:
- add provider-specific reasoning fields
- return a richer provider request object

## Prompt Assembly

### `internal/promptassembly/executor.go`

Current role:
- load system prompt text from file
- build a projection-backed session head
- place session head at outbound `messages[0]`
- keep system prompt as a separate message layer

Current behavior:
- `SystemPromptPolicy.file_static`
  - reads text from file configured in policy params
- `SessionHeadPolicy.projection_summary`
  - builds a compact summary from transcript/session state
  - with shipped config, emits it at `placement: message0`

This means the clean-room runtime now has an explicit prompt-assembly layer ahead of request-shape execution.

## Tool Surface And Safety

### `internal/tools/catalog.go`

Current role:
- choose visible tool definitions from runtime input
- serialize them into provider-compatible `tools` payload entries

Current behavior:
- `ToolCatalogPolicy.static_allowlist`
  - selects listed tool ids in configured order
- `ToolSerializationPolicy.openai_function_tools`
  - emits OpenAI-compatible function tools

### `internal/tools/execution_gate.go`

Current role:
- evaluate parsed provider-emitted tool calls through access, approval, and sandbox policies

Current behavior:
- denied calls fail immediately
- approval-required calls fail immediately
- allowed calls carry resolved sandbox descriptor

Current limitation:
- there is still no actual tool execution runtime after the gate
- tool execution contract currently protects the boundary, but does not yet run tools

It now does:
- prepend selected prompt asset messages before raw conversation messages
- append selected prompt asset messages after raw conversation messages

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

## Chat Resume Read Model

`TranscriptProjection` now stores ordered `message.recorded` messages by `session_id`.

Current runtime behavior:
- `ResumeChatSession(...)` reads from transcript snapshot first
- raw event replay is used only as a fallback recovery path

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
- higher-level batching, compaction, and replay-indexing are still missing

### `internal/runtime/agent_builder.go` and `Agent.RecordEvent(...)`

Current role:
- restore projection snapshots at startup when a store is configured
- provide the runtime entrypoint that:
  - appends events to the event log
  - applies them to projections
  - auto-saves projection snapshots

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

- richer event-log indexing/compaction
- provider-specific response and usage handling

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
