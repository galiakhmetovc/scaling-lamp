# MCP Runtime Surface Design

## Goal

Expose MCP-backed tools, resources, and prompts through the existing canonical runtime path so models can use live MCP connectors without introducing a second tool loop, second prompt path, or sidecar chat runtime.

## Scope

This slice covers `teamD-mcp.2` only:

- surface live MCP tools as normal provider-callable tools
- surface MCP resources and prompts through bounded canonical tools
- keep MCP execution inside the existing provider loop and execution service
- preserve daemon-managed connector lifecycle from `teamD-mcp.1`

This slice does **not** cover:

- TUI/CLI operator controls for MCP connectors
- HTTP/SSE MCP transports
- automatic prompt injection from MCP prompts/resources

## External Reference

The design intentionally combines two proven patterns:

- `OpenClaw`: deterministic MCP tool naming and ordering so tool blocks stay cache-stable
- `Hermes`: capability-aware exposure of resources and prompts as explicit runtime utilities rather than hidden prompt mutation

## Architecture

### One Runtime Path

MCP-backed capability must stay inside the same path already used by built-in tools:

1. provider tool list is assembled in the existing provider-loop path
2. provider responses return tool calls in the same structure as today
3. execution stays inside `execute_model_tool_call(...)` and related provider-loop helpers
4. tool results are returned through the same tool-output continuation path

No MCP-specific chat path, prompt assembler, or tool loop is added.

### Two MCP Surfaces

#### Dynamic MCP Tools

Callable MCP tools are exposed dynamically as provider tools when a connector is:

- persisted
- enabled
- running
- successfully discovered

These tools are not added as static built-in tool definitions. Instead, the provider-loop merges discovered MCP tool definitions into the normal built-in tool list immediately before making a provider request.

Dynamic names must be provider-safe and deterministic. The exposed name format is:

- `mcp__<safe-connector>__<safe-tool>`

Ordering is deterministic by exposed safe name so repeated discovery order changes do not churn the provider tool block.

#### Canonical MCP Resource/Prompt Utilities

Resources and prompts are **not** expanded into per-item dynamic tools. Instead, they are surfaced as four stable built-in tools:

- `mcp_search_resources`
- `mcp_read_resource`
- `mcp_search_prompts`
- `mcp_get_prompt`

These are bounded, explicit retrieval tools like the existing memory/offload surfaces.

## Discovery Model

`teamD-mcp.1` already gives the daemon a live stdio MCP client per enabled connector. `teamD-mcp.2` extends that runtime registry to keep a discovery snapshot per running connector:

- discovered tools
- discovered resources
- discovered prompts
- server capability bits

The worker performs discovery after connection succeeds and stores the snapshot in the shared MCP registry. When a connector stops or fails, the snapshot is removed.

This gives the provider-loop a synchronous, deterministic view of currently available MCP capability without re-querying every connector during prompt assembly.

## Capability Rules

### Tools

MCP tools appear only when:

- the connector is running
- discovery succeeded
- the server actually exposed tools

### Resources

`mcp_search_resources` and `mcp_read_resource` work only for connectors whose server capabilities expose resources. Search only lists discovered metadata; read performs the live MCP `resources/read` call.

### Prompts

`mcp_search_prompts` and `mcp_get_prompt` work only for connectors whose server capabilities expose prompts. Search only lists discovered metadata; get performs the live MCP `prompts/get` call.

This keeps the runtime surface honest: the model should not see utilities that a connector session cannot actually support.

## Naming and Safety

### Provider-safe dynamic names

Dynamic MCP tool names are sanitized for provider compatibility:

- keep ASCII letters, numbers, `_`, and `-`
- replace other characters with `-`
- build the final name from the connector prefix plus tool name
- sort final names deterministically

If two discovered tools sanitize to the same exposed name, the registry must disambiguate deterministically with numeric suffixes while preserving a reverse mapping to the actual connector/tool pair.

### Agent Allowlist

Dynamic MCP tools must still obey agent tool allowlists. The allowlist model is extended so an agent can allow MCP dynamic tools through a generic MCP capability token rather than enumerating every discovered tool name.

### Permission Policy

Dynamic MCP tools participate in the existing permission model. Their effective policy is derived from MCP tool annotations:

- `read_only_hint=true` maps to read-only
- non-read-only MCP tools require approval under default permission mode

The generic resource/prompt utility tools are read-only built-ins.

## Tool Surface

### Dynamic callable MCP tools

Dynamic MCP tools are represented internally as a generic MCP tool-call shape that preserves:

- exposed provider-safe tool name
- resolved connector id
- resolved remote MCP tool name
- raw JSON arguments

The model only sees the exposed dynamic tool name and the original MCP input schema.

### Stable built-in MCP utilities

The canonical built-in MCP utility tools have these responsibilities:

- `mcp_search_resources`: bounded metadata search/listing across discovered resources
- `mcp_read_resource`: read one resource from one connector by URI
- `mcp_search_prompts`: bounded metadata search/listing across discovered prompts
- `mcp_get_prompt`: fetch one prompt from one connector, optionally with prompt arguments

## Output Semantics

Dynamic MCP tool execution returns through the same provider tool-output channel as built-in tools.

Resource and prompt reads return structured JSON plus flattened text where useful:

- resources include per-content entries plus a best-effort flattened text view
- prompts include returned prompt messages plus a best-effort flattened text view

Large MCP outputs may stay inline in this slice unless they naturally fit existing offload rules; no second offload mechanism is introduced.

## Invariants

- one canonical provider loop remains the only execution path
- MCP prompts/resources are never silently injected into the main prompt
- dynamic MCP tool ordering is deterministic
- resources/prompts are capability-aware
- daemon lifecycle remains the single owner of MCP connector state
