# Clean-Room Request-Shape Executor

Current baseline:

- request-shape execution is separated from transport execution
- executor consumes resolved request-shape contracts, not raw YAML
- first supported baseline covers:
  - `ModelPolicy.static_model`
  - `MessagePolicy.raw_messages`
  - `ToolPolicy.tools_inline`
  - `ResponseFormatPolicy`
  - `StreamingPolicy`
  - `SamplingPolicy`

## Current Files

### `internal/provider/request_shape_executor.go`

Does:

- build the exact provider JSON body from resolved request-shape contract
- inline raw messages
- inline tools when enabled
- apply model, response format, streaming, and sampling fields

### `internal/provider/request_shape_executor_test.go`

Covers:

- model field
- raw messages
- tool inlining
- response format
- stream flag
- sampling fields

## Current Limitation

- request-shape executor is not yet paired with prompt-policy assembly
- no provider-specific reasoning fields exist yet
- no usage-handling strategy exists yet
- executor currently returns JSON body bytes, not a higher-level provider client request object
