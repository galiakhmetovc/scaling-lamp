# Clean-Room Implemented Strategies

This document describes only the strategies that are implemented in the clean-room rewrite branch right now.

It does not describe planned strategies.
It does not describe target architecture beyond what already runs in code.

## Scope

Current implemented strategy surface lives in these areas:

- transport execution
- request-shape execution
- contract resolution

Primary code files:

- [contracts.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/contracts/contracts.go)
- [contract_resolver.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/runtime/contract_resolver.go)
- [registry.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/policies/registry.go)
- [transport_executor.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/provider/transport_executor.go)
- [request_shape_executor.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/provider/request_shape_executor.go)

Current note:
- implemented strategy names are now centrally validated during contract resolution through the built-in policy registry

## Transport Strategies

### `EndpointPolicy.static`

Status:
- implemented

Current behavior:
- uses `base_url + path`
- uses explicit HTTP `method`
- applies `extra_headers`
- rejects unknown endpoint strategies

Current params:
- `base_url`
- `path`
- `method`
- `extra_headers`

Current runtime boundary:
- resolved in `contract_resolver`
- applied in `transport_executor`

### `AuthPolicy.bearer_token`

Status:
- implemented

Current behavior:
- reads token from `value_env_var`
- writes header using `header`
- prepends `prefix` when provided
- fails if env var is missing

Current params:
- `header`
- `prefix`
- `value_env_var`

Current runtime boundary:
- resolved in `contract_resolver`
- applied in `transport_executor`

### `AuthPolicy.none`

Status:
- implemented

Current behavior:
- does not add auth header

Current runtime boundary:
- handled directly in `transport_executor`

### `RetryPolicy.none`

Status:
- implemented

Current behavior:
- single attempt only
- no retry delay

### `RetryPolicy.fixed`

Status:
- implemented

Current behavior:
- fixed retry delay from `base_delay`
- capped by `max_delay`
- retries only on configured statuses/errors

Current params:
- `max_attempts`
- `base_delay`
- `max_delay`
- `retry_on_statuses`
- `retry_on_errors`

### `RetryPolicy.exponential`

Status:
- implemented

Current behavior:
- exponential backoff starting from `base_delay`
- capped by `max_delay`
- retries only on configured statuses/errors

Current params:
- `max_attempts`
- `base_delay`
- `max_delay`
- `retry_on_statuses`
- `retry_on_errors`

### `RetryPolicy.exponential_jitter`

Status:
- implemented

Current behavior:
- exponential backoff starting from `base_delay`
- jitter added through executor hook
- capped by `max_delay`
- retries on configured statuses/errors

Current params:
- `max_attempts`
- `base_delay`
- `max_delay`
- `retry_on_statuses`
- `retry_on_errors`

Current runtime boundary:
- resolved in `contract_resolver`
- applied in `transport_executor`

### `TimeoutPolicy.per_request`

Status:
- implemented

Current behavior:
- parses `total`
- creates request context deadline for the whole request

Current params:
- `total`

Current runtime boundary:
- resolved in `contract_resolver`
- applied in `transport_executor`

## Request-Shape Strategies

### `ModelPolicy.static_model`

Status:
- implemented

Current behavior:
- emits fixed `model` field into provider payload
- rejects unknown model strategies

Current params:
- `model`

Current runtime boundary:
- resolved in `contract_resolver`
- applied in `request_shape_executor`

### `MessagePolicy.raw_messages`

Status:
- implemented

Current behavior:
- inlines raw message list into `messages`
- does not summarize, compact, or transform messages

Current runtime boundary:
- resolved in `contract_resolver`
- applied in `request_shape_executor`

### `ToolPolicy.tools_inline`

Status:
- implemented

Current behavior:
- inlines tool definitions into `tools`
- does not transform tool schema

Current runtime boundary:
- resolved in `contract_resolver`
- applied in `request_shape_executor`

### `ResponseFormatPolicy`

Current implemented behavior:
- if enabled and `type` is present, emits:
  - `response_format: { type: ... }`

Current params:
- `type`

Current note:
- there is no richer strategy branching here yet
- current code accepts the configured `strategy` field but does not switch behavior on it

### `StreamingPolicy`

Current implemented behavior:
- if enabled, emits `stream`

Current params:
- `stream`

Current note:
- there is no richer strategy branching here yet
- current code accepts the configured `strategy` field but does not switch behavior on it

### `SamplingPolicy`

Current implemented behavior:
- if enabled, emits any present fields:
  - `temperature`
  - `top_p`
  - `max_output_tokens`

Current params:
- `temperature`
- `top_p`
- `max_output_tokens`

Current note:
- there is no richer strategy branching here yet
- current code accepts the configured `strategy` field but does not switch behavior on it

## Important Boundaries

What is implemented now:
- transport and request-shape are separate executors
- contract resolution is explicit and typed
- builder wires the current executors directly

What is not implemented yet:
- combined provider client pipeline
- policy merge layer
- config-driven builder composition
- persistent event log
- persistent projections
- full policy and strategy registries

## Current Verification Surface

Covered by tests:

- [transport_executor_test.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/provider/transport_executor_test.go)
- [request_shape_executor_test.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/provider/request_shape_executor_test.go)
- [contract_resolver_test.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/runtime/contract_resolver_test.go)
- [agent_builder_test.go](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/internal/runtime/agent_builder_test.go)

This document should be updated every time the implemented strategy surface changes.
