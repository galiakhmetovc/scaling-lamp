# Clean-Room Transport Executor

Current baseline:

- transport execution is now a dedicated package below runtime builder
- executor consumes resolved transport contracts, not raw YAML
- first supported baseline matches the accepted `z.ai` minimum:
  - `EndpointPolicy.static`
  - `AuthPolicy.bearer_token`
  - `RetryPolicy.exponential_jitter`
  - `TimeoutPolicy.per_request`
  - `TimeoutPolicy.long_running_non_streaming`

## Current Files

### `internal/provider/transport_executor.go`

Does:

- build one outbound HTTP request from `TransportContract`
- apply endpoint URL, method, and extra headers
- apply bearer auth from env
- apply retry policy for retriable statuses and transport errors
- apply timeout strategy via request/operation contexts
- emit per-attempt transport traces through an observer hook

### `internal/provider/transport_executor_test.go`

Covers:

- static endpoint and bearer auth
- retry on retriable status
- per-request timeout propagation
- operation budget for long-running non-streaming requests
- skipping retries for late transport failures
- attempt-level retry/backoff trace emission

## Current Limitation

- transport executor is not yet paired with request-shape policy
- no provider-specific response parsing exists yet
- TLS and rate-limit policy families are not applied yet
- executor is built into the runtime, but not yet part of a fuller provider client
