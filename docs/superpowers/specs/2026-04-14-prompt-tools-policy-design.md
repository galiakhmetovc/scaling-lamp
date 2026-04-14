# Prompt And Tool Policy Design

## Goal

Add three new clean-room contract domains so prompt composition, tool exposure, and tool execution safety become explicit resolved policy surfaces:

- `PromptAssemblyContract`
- `ToolContract`
- `ToolExecutionContract`

This slice must let the runtime:

- load a system prompt from a file
- compose `messages[0]` from a policy-driven session head
- expose tools to the model through a dedicated tool domain
- gate tool execution through a separate safety domain

## Why

The current rewrite already has clean separation for:

- transport
- request shape
- memory
- prompt assets
- provider trace
- chat UX

But it still lacks three important first-class domains:

1. prompt assembly before request serialization
2. tool visibility to the model
3. tool execution safety after the model emits a tool call

If these are added ad hoc into existing executors, the clean-room boundary will erode quickly:

- system prompt logic will leak into chat/runtime code
- session head logic will leak into message serialization
- tool exposure will remain coupled to request-shape internals
- tool execution safety will remain scattered across runtime conditions

This work keeps those concerns policy-driven and independently extensible.

## Accepted Contract Boundary

Three top-level contracts are introduced.

### 1. PromptAssemblyContract

Responsibility:

- compose the prompt-facing message prefix before request-shape serialization
- own system prompt and session head logic

Families:

- `SystemPromptPolicy`
- `SessionHeadPolicy`

### 2. ToolContract

Responsibility:

- decide which tools are visible to the model
- decide how tools are serialized into the provider request body

Families:

- `ToolCatalogPolicy`
- `ToolSerializationPolicy`

### 3. ToolExecutionContract

Responsibility:

- decide whether a requested tool call may run
- decide whether approval is required
- decide what runtime safety restrictions apply

Families:

- `ToolAccessPolicy`
- `ToolApprovalPolicy`
- `ToolSandboxPolicy`

## Explicit Non-Goals

This slice does not require:

- full tool approval UX
- dynamic tool discovery by arbitrary predicates
- provider-specific tool serialization variants beyond the first baseline
- advanced system prompt templating
- full transcript compression or automatic prompt budgeting

## PromptAssemblyContract

`PromptAssemblyContract` controls what is prepended to the conversational transcript before request serialization.

It sits between:

- session/transcript state
- request-shape execution

It does not perform transport or provider I/O.

Legacy reference confirms two distinct behaviors that the clean-room design must preserve:

- system prompt layers are injected as separate `role=system` messages
- the session summary or checkpoint layer is assembled before ordinary transcript history and occupies the top prompt slot

The clean-room design must keep these as separate concerns rather than collapsing both into one generic prompt asset.

### SystemPromptPolicy

Responsibility:

- source the system prompt body
- decide whether it is required
- normalize the loaded text before insertion

First required strategy:

- `file_static`

Behavior of `file_static`:

- reads prompt text from a file path
- trims trailing whitespace when configured
- emits one `system` message
- fails resolution or runtime startup when `required=true` and the file is missing

Params:

- `path`
- `role`
- `required`
- `trim_trailing_whitespace`

Baseline defaults:

- `role=system`
- `required=true`
- `trim_trailing_whitespace=true`

Accepted invariant:

- system prompt is sourced from a file
- system prompt remains its own distinct prompt layer
- system prompt is not encoded inside session head text

### SessionHeadPolicy

Responsibility:

- build a compact summary block from projections
- place it into the top of the outbound message list

First required strategies:

- `off`
- `projection_summary`

Behavior of `projection_summary`:

- reads from projections, not raw runtime conditionals
- builds a bounded summary block
- inserts the resulting message according to placement rules

Params:

- `placement`
- `title`
- `max_items`
- `include_session_id`
- `include_open_loops`
- `include_last_user_message`
- `include_last_assistant_message`

Accepted placement values:

- `message0`
- `after_system`

For the first implementation slice, the required baseline is:

- `projection_summary`
- `placement=message0`

That is not optional baseline behavior for the shipped chat config. It is part of the accepted architecture for the current rewrite target.

### Prompt Assembly Order

The runtime order must be:

1. session head message at `messages[0]` when enabled with `placement=message0`
2. one or more separate system prompt messages loaded from file-backed prompt assembly policy
3. transcript messages for the current session

This is intentionally not a naive prepend model.

The clean-room implementation must preserve both facts simultaneously:

- `messages[0]` is the session head
- system prompt is still represented as separate prompt content sourced from file

This mirrors the legacy shape where the summary/checkpoint layer and injected system context are distinct layers.

## ToolContract

`ToolContract` controls the model-visible tool surface.

It is the source of truth for:

- which tools the provider sees
- how those tools are encoded into the request body

It does not decide whether a tool call is safe to execute. That belongs to `ToolExecutionContract`.

### ToolCatalogPolicy

Responsibility:

- select the set of visible tools for the current runtime/session

First required strategy:

- `static_allowlist`

Behavior of `static_allowlist`:

- resolves a fixed list of tool ids from configuration
- preserves configured order unless dedupe removes duplicates
- fails when unknown tool ids are requested

Params:

- `tool_ids`
- `allow_empty`
- `dedupe`

### ToolSerializationPolicy

Responsibility:

- turn internal tool definitions into provider request payload fields

First required strategy:

- `openai_function_tools`

Behavior of `openai_function_tools`:

- emits OpenAI-compatible `tools` entries
- encodes name, description, and JSON schema
- is consumed by `RequestShapeExecutor`

Params:

- `strict_json_schema`
- `include_descriptions`

### Contract Boundary With RequestShape

`RequestShapeContract` continues to own the overall provider JSON shape.

But after this slice:

- `RequestShapeContract` no longer owns tool discovery or tool selection
- it consumes already resolved tool surface from `ToolContract`
- `ToolPolicy` in request-shape becomes a request-level serialization behavior, not a source of truth for which tools exist

## ToolExecutionContract

`ToolExecutionContract` controls whether a provider-emitted tool call may run and under which restrictions.

### ToolAccessPolicy

Responsibility:

- decide whether a tool id is executable at all

First required strategies:

- `static_allowlist`
- `deny_all`

Params for `static_allowlist`:

- `tool_ids`

### ToolApprovalPolicy

Responsibility:

- decide whether execution requires external confirmation

First required strategies:

- `always_allow`
- `always_require`
- `require_for_destructive`

Params:

- `destructive_tool_ids`
- `approval_message_template`

In the first slice only `always_allow` is required end-to-end.

Other strategies may be present in the contract model and registry, but UX wiring may remain follow-up work if not immediately implemented.

### ToolSandboxPolicy

Responsibility:

- define runtime restrictions for tool execution

First required strategies:

- `default_runtime`
- `read_only`
- `workspace_write`
- `deny_exec`

Params:

- `allow_network`
- `allow_write_paths`
- `deny_write_paths`
- `timeout`
- `max_output_bytes`

## Runtime Integration

The provider request pipeline becomes:

1. load transcript for current session
2. build prompt prefix through `PromptAssemblyExecutor`
3. resolve visible tools through `ToolCatalogExecutor`
4. serialize tools through `ToolSerializationPolicy`
5. build provider body through `RequestShapeExecutor`
6. send body through `TransportExecutor`

The tool execution pipeline becomes:

1. provider client parses tool call request
2. runtime resolves the target tool definition
3. `ToolAccessPolicy` allows or rejects it
4. `ToolApprovalPolicy` decides whether approval is required
5. `ToolSandboxPolicy` chooses execution restrictions
6. tool executor runs or rejects
7. tool result is recorded into events and transcript

## Config Graph Additions

New top-level contract paths in root config:

- `prompt_assembly`
- `tools`
- `tool_execution`

New config files in shipped baseline:

- `config/zai-smoke/contracts/prompt-assembly.yaml`
- `config/zai-smoke/contracts/tools.yaml`
- `config/zai-smoke/contracts/tool-execution.yaml`

New policy directories:

- `config/zai-smoke/policies/prompt-assembly/`
- `config/zai-smoke/policies/tools/`
- `config/zai-smoke/policies/tool-execution/`

System prompt source file must live outside policy YAML and be referenced by path.

## Required Runtime Objects

New resolved contract fields:

- `ResolvedContracts.PromptAssembly`
- `ResolvedContracts.Tools`
- `ResolvedContracts.ToolExecution`

New executors:

- `PromptAssemblyExecutor`
- `ToolCatalogExecutor`
- `ToolExecutionExecutor`

Existing components that must integrate:

- `RequestShapeExecutor`
- `ProviderClient`
- `ChatSession` / `ChatTurn`
- `TranscriptProjection`

## Event And Projection Expectations

At minimum, this slice should introduce event coverage for:

- prompt assembly inputs used for a run
- selected visible tools for a run
- tool execution allow/deny decision
- tool approval decision
- tool sandbox selection

`TranscriptProjection` remains the conversation read model.

`SessionHeadPolicy.projection_summary` must read from projections, not re-derive state ad hoc from runtime objects.

## First Implementation Slice

The first implementation slice must deliver:

1. `PromptAssemblyContract`
   - `SystemPromptPolicy.file_static`
   - `SessionHeadPolicy.projection_summary`
2. `ToolContract`
   - `ToolCatalogPolicy.static_allowlist`
   - `ToolSerializationPolicy.openai_function_tools`
3. `ToolExecutionContract`
   - `ToolAccessPolicy.static_allowlist`
   - `ToolApprovalPolicy.always_allow`
   - `ToolSandboxPolicy.default_runtime`

This is sufficient to:

- load system prompt from a file
- inject session head into `messages[0]`
- expose explicit tools to the model
- gate tool execution through one clean resolved safety contract

## Follow-Up Work

Likely immediate follow-ups after the first slice:

- richer session head templates and budgeting
- system prompt file templating
- richer tool registries and tags
- provider-specific tool serialization
- approval UX integration
- destructive tool classification
- richer sandbox policies

## Acceptance Criteria

The design is acceptable when implementation can proceed without ambiguity on:

- which domain owns system prompt behavior
- which domain owns `messages[0]` session head behavior
- which domain owns model-visible tool selection
- which domain owns tool execution safety
- where prompt assembly ends and request-shape begins
- where tool exposure ends and execution safety begins
