# ContextPolicy Design

## Goal

Replace the current legacy context/runtime behavior model with one policy-driven context system where every non-essential layer is optional, strategy-based, and observable.

The design must let `teamD` decide per global/session/run scope:

- what data layers exist
- what layers are projected into prompt
- what gets offloaded
- what gets summarized
- what tools are exposed and how they execute
- what the web bench renders

## Why

`teamD` already has several context-shaped objects and flows:

- `SessionHead`
- raw/session transcript
- plans
- artifact offload
- VFS
- tool allowlists
- display-only web rendering limits

They work, but they are still mostly encoded as behavior in code paths instead of one explicit resolved policy.

The immediate problem is not “missing memory”.  
The immediate problem is that the current system is legacy:

- context composition is scattered
- offload is partially hardcoded
- summarization is not a first-class contract
- tool exposure and execution still mix policy with transport logic
- display has knobs that are not rooted in one resolved policy surface

This work is a full replacement of that legacy model, not a patch series on top of it.

## Accepted Architecture Baseline

### Rewrite Boundary

- The new agent is a clean-room rewrite.
- The current implementation is legacy reference only.
- No compatibility hacks, temporary simplifications, or staged "start simpler first" shortcuts are allowed.
- New behavior may land only through data objects, policies, strategies, resolved contracts, and executors.

### Runtime Foundation

- Source of truth is an event log.
- Reads happen through projections.
- Canonical projections include:
  - `SessionProjection`
  - `SessionHeadProjection`
  - `PlanProjection`
  - `WorkspacePointerProjection`
  - `RunProjection`
- Raw provider and tool I/O should not bloat domain events by default.
- Domain events should carry trace summaries plus stable trace or artifact references.

### Process And Builder Model

- One binary.
- One root config path.
- One config produces one agent instance.
- More agents mean more process launches with different config files.
- The binary receives only the root config path and builds the runtime autonomously.
- The initial rewrite stays inside one compiled binary with built-in registries.
- External plugin binaries and Go dynamic plugins are explicitly deferred.

### Configuration Model

- Configuration is modular and explicit.
- Root config composes the system from module files.
- Contracts and policies are stored separately.
- Contracts reference policy modules by path.
- Every module carries its own stable `id`.
- Path loads the module; `id` validates and identifies it.
- No `extends`.
- No hidden imports.
- No implicit multi-agent config graph.

### Contract Model

- Policies are merged into resolved contracts.
- Resolved contracts are applied by executors.
- Canonical runtime contracts are:
  - `ProviderRequestContract`
  - `MemoryContract`
  - `ExecutionContract`
  - `DisplayContract`
  - `ObservabilityContract`

### Prompt And Asset Separation

- Config graph defines how the system is assembled.
- Policies define how the system behaves.
- Prompt assets and semantic prompt resources are their own policy-driven domain and are not free-form config blobs.

## Core Principles

1. Everything non-essential is optional.
2. Data objects and behavior policies are separate concerns.
3. Presence in storage does not imply presence in prompt.
4. Large data bodies stay outside prompt and are referenced by stable pointers.
5. Policy is resolved explicitly with one effective view:
   - `run override`
   - over `session override`
   - over `global default`
6. Web must show both:
   - raw configured policy
   - effective resolved policy
7. Contracts are declarative; executors perform I/O.
8. Everything optional stays explicitly optional, including `SessionHead`, workspace-derived prompt layers, plan projection, tracing, and display layers.

## Scope

This slice defines the replacement model and integration shape for a configurable context system.

It adds:

1. canonical data objects for context-bearing state
2. first-class `ContextPolicy`
3. policy families with strategies and params
4. effective policy resolution rules
5. prompt/runtime/display boundaries

It does not add:

- a final UI for editing every policy
- full implementation of every strategy
- provider-specific context optimizers
- automatic boundary testing

## Canonical Data Objects

### 1. SessionHead

Short operational head intended for prompt projection.

Candidate fields:

- `CurrentGoal`
- `LastResultSummary`
- `CurrentProject`
- `CurrentPlanID`
- `CurrentPlanTitle`
- `CurrentPlanItems`
- `RecentArtifactRefs`
- `OpenLoops`
- `LastCompletedRunID`

`SessionHead` is prompt-oriented and must stay small.

### 2. WorkspacePointer

Persistent workspace map intended for navigation and lazy retrieval, not for direct full prompt injection.

Candidate fields:

- `SessionID`
- `WorkspaceRoot`
- `OpenFiles`
- `Artifacts`
- `TreeHint`
- `SessionState`

`OpenFiles` candidate fields:

- `Path`
- `Reason`
- `UpdatedAt`
- `Transient`
- `Checksum` later

`Artifacts` candidate fields:

- `Path`
- `Summary`
- `Relevance`
- `Tags`
- `CreatedAt`

`SessionState` candidate fields:

- `CurrentGoal`
- `BlockedOn`
- `NextActions`

### 3. ArtifactRegistry

Stable metadata layer over offloaded bodies stored in VFS or `.agent/memory/...`.

Candidate fields:

- `ArtifactID`
- `Path`
- `Kind`
- `Summary`
- `SizeChars`
- `Tags`
- `SourceTool`
- `CreatedAt`

### 4. Transcript

Conversation history remains canonical conversation data, but prompt inclusion is policy-driven and no legacy inclusion path should survive the rewrite.

### 5. Plan

Plan remains its own source of truth.  
Projection into `SessionHead` is separate from plan storage.

## ContextPolicy

`ContextPolicy` is the first-class runtime object that controls how context-bearing data is projected, trimmed, summarized, offloaded, executed, and displayed.

Candidate top-level shape:

```yaml
context_policy:
  transport: {}
  request_shape: {}
  prompt: {}
  offload: {}
  summarization: {}
  workspace: {}
  tools: {}
  display: {}
  observability: {}
```

Each policy family follows one consistent contract:

```yaml
enabled: true
strategy: compact
params: {}
```

Or for multi-layer families:

```yaml
session_head:
  enabled: true
  strategy: compact
  params: {}
```

## Policy Families

### 0. TransportPolicy

Controls where and how HTTP delivery happens before request shape and prompt content are considered.

Transport is composed from:

- `EndpointPolicy`
- `AuthPolicy`
- `RetryPolicy`
- `TimeoutPolicy`
- `TLSPolicy`
- `RateLimitPolicy`

#### EndpointPolicy

Purpose: where requests go.

Example strategies:

- `static`
- `env_resolved`
- `failover`

Example params:

- `base_url`
- `path`
- `method`
- `query`
- `extra_headers`

#### AuthPolicy

Purpose: how requests authenticate.

Example strategies:

- `bearer_token`
- `api_key_header`
- `none`

Example params:

- `header`
- `prefix`
- `value_env_var`

#### RetryPolicy

Purpose: when and how to retry.

Example strategies:

- `none`
- `fixed`
- `exponential`
- `exponential_jitter`

Example params:

- `max_attempts`
- `base_delay`
- `max_delay`
- `retry_on_statuses`
- `retry_on_errors`

#### TimeoutPolicy

Purpose: how long to wait.

Example strategies:

- `per_request`
- `split`
- `streaming`

Example params:

- `total`
- `connect`
- `idle`

#### TLSPolicy

Purpose: how server identity is validated.

Example strategies:

- `system`
- `custom_ca`
- `insecure`

Example params:

- `ca_file`
- `cert_file`
- `key_file`
- `skip_verify`

#### RateLimitPolicy

Purpose: client-side throttling.

Example strategies:

- `none`
- `token_bucket`

Example params:

- `requests_per_second`
- `burst`

#### Minimal z.ai Baseline

The first real provider slice for `z.ai` should start with:

- `EndpointPolicy.static`
- `AuthPolicy.bearer_token`
- `RetryPolicy.exponential_jitter`
- `TimeoutPolicy.per_request`

`TLSPolicy` and `RateLimitPolicy` stay in the contract from the start even if first executor support is deferred.

### 0A. RequestShapePolicy

Controls provider body shape independently from transport and prompt-layer policy.

Candidate concerns:

- model selection
- sampling fields
- reasoning fields
- tools serialization
- response format
- streaming flags
- usage handling

### 1. PromptPolicy

Controls what enters the provider payload beyond base `messages`, config, and explicit tools.

Prompt layers:

- `session_head`
- `workspace_focus`
- `plan`
- `recent_artifacts`
- `tree_hint`
- `history_summary`

Example strategies:

- `off`
- `full`
- `compact`
- `top_k`
- `budgeted`
- `explicit_only`

Example params:

- `max_chars`
- `max_items`
- `budget_tokens`
- `artifact_count`
- `file_count`

Key rule:

- `SessionHead` may be enabled or disabled
- `WorkspacePointer` itself is never injected wholesale
- only a short derived `workspace_focus` projection may enter prompt

### 2. OffloadPolicy

Controls what leaves prompt-visible history and becomes artifact-backed.

Objects:

- `tool_outputs`
- `assistant_messages`
- `user_messages`
- `workspace_reads`
- `diffs`
- `logs`

Example strategies:

- `off`
- `old_only`
- `size_based`
- `tool_aware`
- `aggressive`

Example params:

- `small_keep_chars`
- `offload_chars`
- `force_offload_chars`
- `preview_mode`
- `offload_last_result`

Key rule:

- offload never destroys addressability
- large bodies move to artifacts
- prompt keeps a stable preview plus artifact reference

### 3. SummarizationPolicy

Controls when older history is compressed and how summary enters prompt.

Objects:

- `older_messages`
- `tool_history`
- `artifact_history`
- `workspace_history`

Example strategies:

- `off`
- `manual`
- `on_threshold`
- `always_refresh`
- `model_summary`
- `rolling_summary`

Example params:

- `keep_last_n`
- `refresh_mode`
- `summary_budget_chars`
- `trigger_chars`
- `trigger_tokens`

Key rule:

- summary is a separate layer
- raw last `N` messages remain untouched
- older history may be replaced in prompt by summary while canonical transcript still exists

### 4. WorkspacePolicy

Controls how `WorkspacePointer` is maintained and projected.

Objects:

- `open_files`
- `artifacts`
- `tree_hint`
- `checksums`
- `session_state`

Example strategies:

- `off`
- `minimal`
- `active_only`
- `recent_only`
- `relevance_scored`
- `full_metadata`

Example params:

- `max_open_files`
- `max_artifacts`
- `tree_depth`
- `tree_paths`
- `include_checksums`

### 5. ToolPolicy

Controls exposure and execution semantics for tools.

Objects:

- `allowlist`
- `approval_mode`
- `execution_mode`
- `auto_approve`

Example strategies:

- `deny_by_default`
- `allow_selected`
- `allow_group`
- `manual`
- `auto_approve`

Example params:

- `allowed_tools`
- `allowed_groups`
- `timeout_ms`
- `output_limits`

### 6. DisplayPolicy

Controls web presentation only.  
It must never silently mutate runtime behavior.

Objects:

- `turn_limit`
- `message_limit`
- `char_limit`
- `view_mode`
- `pane_visibility`

Example strategies:

- `compact`
- `full`
- `mobile`
- `debug`
- `forensic`

### 7. ObservabilityPolicy

Controls trace capture, trace storage mode, inline summaries, and trace-display behavior.

Candidate concerns:

- provider request trace capture
- provider response trace capture
- tool I/O trace capture
- inline summary vs artifact reference
- redaction strategy
- forensic display mode

## Effective Policy Resolution

`ContextPolicy` is resolved in this order:

1. `global default`
2. `session override`
3. `run override`

Result:

- one explicit `EffectiveContextPolicy`
- visible in runtime API and web
- used consistently by prompt assembly, offload, summarization, tool exposure, and display

Candidate runtime shape:

```go
type EffectiveContextPolicy struct {
    Transport     TransportPolicy
    RequestShape  RequestShapePolicy
    Prompt        PromptPolicy
    Offload       OffloadPolicy
    Summarization SummarizationPolicy
    Workspace     WorkspacePolicy
    Tools         ToolPolicy
    Display       DisplayPolicy
    Observability ObservabilityPolicy
}
```

## Runtime Boundaries

### Prompt Boundary

Prompt assembly consumes:

- base `messages`
- resolved transport-independent request-shape policy
- resolved prompt policy
- optional compact projections from `SessionHead`
- optional compact projections from `WorkspacePointer`
- optional history summary

Prompt assembly does not consume:

- full `WorkspacePointer`
- raw artifact bodies
- unbounded tree dumps

### Storage Boundary

Store holds:

- `SessionHead`
- `WorkspacePointer`
- `ArtifactRegistry`
- `Plan`
- transcript metadata
- effective session-level policy

Files/VFS hold:

- offloaded bodies
- raw logs
- large artifacts

### Display Boundary

Web renders:

- configured policy
- effective policy
- what prompt preview includes
- what was offloaded
- what was summarized

Display trimming is not runtime trimming.

## Initial Strategy Set

The first implementation should stay narrow.

Recommended v1 strategies:

- `TransportPolicy`: `static`, `bearer_token`, `exponential_jitter`, `per_request`
- `RequestShapePolicy`: `static_model`, `raw_messages`, `tools_inline`
- `PromptPolicy`: `off`, `compact`, `top_k`
- `OffloadPolicy`: `off`, `old_only`, `tool_aware`
- `SummarizationPolicy`: `off`, `manual`, `model_summary`
- `WorkspacePolicy`: `off`, `active_only`, `recent_only`
- `ToolPolicy`: `deny_by_default`, `allow_selected`, `manual`, `auto_approve`
- `DisplayPolicy`: `compact`, `debug`, `mobile`
- `ObservabilityPolicy`: `refs_only`, `full_forensic`

## Validation Rules

Even with maximum configurability, runtime must reject invalid combinations.

Examples:

- `workspace_focus.enabled=true` while `workspace.enabled=false`
  - allowed only if a derived focus source exists
- `offload_last_result=true` with `preview_mode=none`
  - invalid
- `history_summary.enabled=true` with `keep_last_n < 0`
  - invalid
- `allowed_tools=[]` with `execution_mode=auto_approve`
  - valid but useless, should warn

## Success Criteria

- every non-essential context layer is explicitly optional
- policies are modeled separately from data objects
- effective policy can be resolved and displayed
- `SessionHead` stays short
- `WorkspacePointer` remains stored state, not prompt dump
- offload and summarization replace legacy hardcoded behavior with policy-driven execution
- no new behavior lands in the legacy style

## Legacy Position

Everything in the current system that shapes transport, prompt, memory, tool execution, and display outside resolved contracts should be treated as legacy behavior pending replacement.

## Non-goals

- no attempt to auto-tune every threshold in this slice
- no provider-specific token optimizer in this slice
- no full policy editing UI in this slice
- no replacement of existing transcript/log persistence
