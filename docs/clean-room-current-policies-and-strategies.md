# Clean-Room Current Policies And Strategies

This document is the current reference for clean-room policies and strategies in `rewrite/clean-room-root`.

It describes the system as it exists now in code.
It is not a target design document.

## 1. How To Read This

There are three layers:

1. contract
   - groups policy families into one runtime area
2. policy family
   - answers one specific behavioral question
3. strategy
   - the concrete algorithm or behavior used by that family

Each policy family has:

- `enabled`
- `strategy`
- `params`

Current sources of truth:

- [contracts.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/contracts/contracts.go)
- [registry.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/policies/registry.go)
- [contract_resolver.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/runtime/contract_resolver.go)

## 2. Current Contract Areas

Current resolved contracts are:

- `ProviderRequestContract`
  - `TransportContract`
  - `RequestShapeContract`
- `MemoryContract`
- `PromptAssetsContract`
- `ProviderTraceContract`
- `ChatContract`

What each contract is responsible for:

- `TransportContract`
  - how outbound HTTP is sent
- `RequestShapeContract`
  - what JSON payload is built for the provider
- `MemoryContract`
  - how prompt history may be offloaded/compacted
- `PromptAssetsContract`
  - how static prompt fragments are stored and selected
- `ProviderTraceContract`
  - how outbound provider request traces are captured
- `ChatContract`
  - how terminal chat UX behaves

## 3. Transport Contract

`TransportContract` answers:

- where to send the request
- how to authenticate
- when to retry
- how long the call may run

### 3.1 EndpointPolicy

Responsibility:

- build the request URL
- choose HTTP method
- apply endpoint-level extra headers

Supported params:

- `base_url`
- `path`
- `method`
- `extra_headers`

Current implemented strategies:

- `static`
  - uses `base_url + path`
  - uses explicit `method`
  - applies `extra_headers`

### 3.2 AuthPolicy

Responsibility:

- add outbound authentication material to the request

Supported params:

- `header`
- `prefix`
- `value_env_var`

Current implemented strategies:

- `none`
  - sends no auth header
- `bearer_token`
  - reads token from `value_env_var`
  - writes it into `header`
  - prefixes it with `prefix` when configured

### 3.3 RetryPolicy

Responsibility:

- decide whether another attempt should happen after a failure
- compute delay between attempts

Supported params:

- `max_attempts`
- `base_delay`
- `max_delay`
- `retry_on_statuses`
- `retry_on_errors`
- `early_failure_window`

What the params mean:

- `max_attempts`
  - total allowed attempts including the first one
- `base_delay`
  - initial retry delay
- `max_delay`
  - cap on computed delay
- `retry_on_statuses`
  - HTTP statuses that are retryable
- `retry_on_errors`
  - transport-level error classes that are retryable
- `early_failure_window`
  - threshold used by long-running transport logic to distinguish early failure from late timeout/failure

Current implemented strategies:

- `none`
  - no retry
- `fixed`
  - constant delay between attempts
- `exponential`
  - backoff grows exponentially
- `exponential_jitter`
  - exponential backoff with jitter

### 3.4 TimeoutPolicy

Responsibility:

- bound request lifetime or attempt lifetime

Supported params:

- `total`
- `connect`
- `idle`
- `operation_budget`
- `attempt_timeout`

What the params mean right now:

- `total`
  - whole-request deadline for `per_request`
- `connect`
  - declared in type, not used by current executors
- `idle`
  - declared in type, not used by current executors
- `operation_budget`
  - upper time budget for the entire long-running provider call
- `attempt_timeout`
  - per-attempt deadline inside a long-running provider call

Current implemented strategies:

- `per_request`
  - uses `total` as one deadline for the whole request
- `long_running_non_streaming`
  - designed for long non-streaming calls
  - may use large or unset per-attempt budget
  - uses `operation_budget`
  - optionally uses `attempt_timeout`

## 4. RequestShape Contract

`RequestShapeContract` answers:

- what model to call
- which messages to send
- whether tools are included
- whether the call streams
- which output formatting and sampling fields are emitted

### 4.1 ModelPolicy

Responsibility:

- choose provider model name

Supported params:

- `model`

Current implemented strategies:

- `static_model`
  - emits a fixed model string

### 4.2 MessagePolicy

Responsibility:

- decide how conversation messages enter the provider payload

Supported params:

- none

Current implemented strategies:

- `raw_messages`
  - sends raw messages as-is

### 4.3 ToolPolicy

Responsibility:

- decide how tools enter the provider payload

Supported params:

- none

Current implemented strategies:

- `tools_inline`
  - sends tool definitions inline in `tools`

### 4.4 ResponseFormatPolicy

Responsibility:

- optionally add `response_format` to the provider payload

Supported params:

- `type`

Current implemented strategies:

- `default`
  - when enabled and `type` is present, emits:
    - `response_format: { type: ... }`

### 4.5 StreamingPolicy

Responsibility:

- decide whether the provider call should stream

Supported params:

- `stream`

Current implemented strategies:

- `static_stream`
  - emits fixed boolean `stream`

### 4.6 SamplingPolicy

Responsibility:

- optionally emit provider sampling controls

Supported params:

- `temperature`
- `top_p`
- `max_output_tokens`

Current implemented strategies:

- `static_sampling`
  - emits configured sampling fields when enabled and present

## 5. Memory Contract

`MemoryContract` answers:

- when accumulated text should be offloaded/compacted

### 5.1 OffloadPolicy

Responsibility:

- decide offload threshold for old content

Supported params:

- `max_chars`

Current implemented strategies:

- `old_only`
  - offload/compaction threshold based on older content only

## 6. PromptAssets Contract

`PromptAssetsContract` answers:

- how static prompt fragments are stored
- which prompt fragments can be selected and where they are inserted

### 6.1 PromptAssetPolicy

Responsibility:

- provide reusable prompt assets for prepend/append placement

Supported params:

- `assets`

Each asset supports:

- `id`
- `role`
- `content`
- `placement`

Current implemented strategies:

- `inline_assets`
  - stores all assets inline in the policy module
  - current placements:
    - `prepend`
    - `append`

## 7. Chat Contract

`ChatContract` answers:

- how terminal input is collected
- when the input buffer is sent
- how streamed output is shown
- which status information is printed
- which slash commands exist
- how resume behaves

### 7.1 ChatInputPolicy

Responsibility:

- define prompt rendering for terminal input

Supported params:

- `primary_prompt`
- `continuation_prompt`

Current implemented strategies:

- `multiline_buffer`
  - supports multiline terminal input
  - uses configured prompts for first line vs continuation lines

### 7.2 ChatSubmitPolicy

Responsibility:

- decide when buffered multiline input is submitted

Supported params:

- `empty_line_threshold`

Current implemented strategies:

- `double_enter`
  - submit happens after configured count of consecutive empty lines
  - with current shipped config, `1` empty line after content means “double Enter”

### 7.3 ChatOutputPolicy

Responsibility:

- define terminal output handling after a streamed answer

Supported params:

- `show_final_newline`

Current implemented strategies:

- `streaming_text`
  - prints streamed text chunks as they arrive
  - may add a final newline after the streamed answer

### 7.4 ChatStatusPolicy

Responsibility:

- control operator-facing status/header output in the terminal

Supported params:

- `show_header`
- `show_usage`

Current implemented strategies:

- `inline_terminal`
  - prints header/status lines directly into the terminal stream

### 7.5 ChatCommandPolicy

Responsibility:

- define available slash commands

Supported params:

- `exit_command`
- `help_command`
- `session_command`

Current implemented strategies:

- `slash_commands`
  - enables slash-command handling in terminal chat

### 7.6 ChatResumePolicy

Responsibility:

- define how chat resume is allowed from CLI

Supported params:

- `require_explicit_id`

Current implemented strategies:

- `explicit_resume_only`
  - resume requires explicit `--resume <session-id>`

## 8. ProviderTrace Contract

`ProviderTraceContract` answers:

- should the outbound provider request be captured at all
- should raw assembled request JSON be kept
- should decoded request payload be kept inline in the event

### 8.1 ProviderTracePolicy

Responsibility:

- capture the exact assembled outbound provider request

Supported params:

- `include_raw_body`
- `include_decoded_payload`

Current implemented strategies:

- `none`
  - do not record provider request capture
- `inline_request`
  - write provider request capture into the run event stream

Current runtime event:

- `provider.request.captured`

## 9. Current Built-In Strategy Registry

These are the current built-in policy kinds and allowed strategies validated during contract resolution:

- `EndpointPolicyConfig`
  - `static`
- `AuthPolicyConfig`
  - `none`
  - `bearer_token`
- `RetryPolicyConfig`
  - `none`
  - `fixed`
  - `exponential`
  - `exponential_jitter`
- `TimeoutPolicyConfig`
  - `per_request`
  - `long_running_non_streaming`
- `OffloadPolicyConfig`
  - `old_only`
- `ModelPolicyConfig`
  - `static_model`
- `MessagePolicyConfig`
  - `raw_messages`
- `ToolPolicyConfig`
  - `tools_inline`
- `ResponseFormatPolicyConfig`
  - `default`
- `StreamingPolicyConfig`
  - `static_stream`
- `SamplingPolicyConfig`
  - `static_sampling`
- `PromptAssetPolicyConfig`
  - `inline_assets`
- `ProviderTracePolicyConfig`
  - `none`
  - `inline_request`
- `ChatInputPolicyConfig`
  - `multiline_buffer`
- `ChatSubmitPolicyConfig`
  - `double_enter`
- `ChatOutputPolicyConfig`
  - `streaming_text`
- `ChatStatusPolicyConfig`
  - `inline_terminal`
- `ChatCommandPolicyConfig`
  - `slash_commands`
- `ChatResumePolicyConfig`
  - `explicit_resume_only`

## 10. Current z.ai Smoke Selections

This is what the shipped `config/zai-smoke` configuration currently selects.

### 9.1 Transport

- `EndpointPolicy.static`
  - `base_url = https://api.z.ai/api/coding/paas/v4`
  - `path = /chat/completions`
  - `method = POST`
- `AuthPolicy.bearer_token`
  - `header = Authorization`
  - `prefix = Bearer`
  - `value_env_var = TEAMD_ZAI_API_KEY`
- `RetryPolicy.exponential_jitter`
  - `max_attempts = 3`
  - `base_delay = 100ms`
  - `max_delay = 1s`
  - `early_failure_window = 5s`
  - `retry_on_statuses = [429, 500, 502, 503]`
  - `retry_on_errors = [transport_error]`
- `TimeoutPolicy.long_running_non_streaming`
  - `operation_budget = 1h`

### 9.2 RequestShape

- `ModelPolicy.static_model`
  - `model = glm-5-turbo`
- `MessagePolicy.raw_messages`
- `ToolPolicy.tools_inline`
- `ResponseFormatPolicy.default`
  - `enabled = false`
- `StreamingPolicy.static_stream`
  - `stream = true`
- `SamplingPolicy.static_sampling`
  - `enabled = false`

### 9.3 Memory

- `OffloadPolicy.old_only`
  - `max_chars = 1200`

### 9.4 PromptAssets

- `PromptAssetPolicy.inline_assets`
  - `assets = []`

### 10.5 Chat

- `ChatInputPolicy.multiline_buffer`
  - `primary_prompt = "> "`
  - `continuation_prompt = ". "`
- `ChatSubmitPolicy.double_enter`
  - `empty_line_threshold = 1`
- `ChatOutputPolicy.streaming_text`
  - `show_final_newline = true`
- `ChatStatusPolicy.inline_terminal`
  - `show_header = true`
  - `show_usage = true`
- `ChatCommandPolicy.slash_commands`
  - `exit_command = /exit`
  - `help_command = /help`
  - `session_command = /session`
- `ChatResumePolicy.explicit_resume_only`
  - `require_explicit_id = true`

### 10.6 ProviderTrace

- `ProviderTracePolicy.inline_request`
  - `include_raw_body = true`
  - `include_decoded_payload = true`

## 11. Runtime Boundaries

Current boundary between layers:

- `contract_resolver`
  - loads contract modules
  - loads policy modules
  - validates strategy names
  - decodes typed params
- executors / runtime components
  - apply the resolved behavior

Current main executors/components:

- transport behavior
  - [transport_executor.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/provider/transport_executor.go)
- request-shape behavior
  - [request_shape_executor.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/provider/request_shape_executor.go)
- prompt assets behavior
  - [prompt_asset_executor.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/provider/prompt_asset_executor.go)
- terminal chat behavior
  - [chat.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/runtime/cli/chat.go)

That is the current implementation snapshot.
