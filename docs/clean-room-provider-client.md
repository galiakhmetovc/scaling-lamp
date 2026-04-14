# Clean-Room Provider Client

This document describes the first combined provider client in the clean-room runtime.

## Current Goal

The clean-room runtime no longer has to treat request-shape execution and transport execution as two unrelated steps.

There is now one provider-facing client that:
- builds the provider request body
- sends the request through transport execution
- returns one normalized result object

## Current Files

### `internal/provider/client.go`

Current role:
- define `ClientInput`
- define `ClientResult`
- combine request-shape and transport execution into one call

Current flow:
1. build provider request body through `RequestShapeExecutor`
2. execute HTTP request through `TransportExecutor`
3. parse provider-specific response body into a normalized provider result
4. return:
   - request body bytes
   - raw transport response
   - normalized provider response

This provider client is now the execution core behind the runtime smoke path in `internal/runtime/smoke.go`.

### `internal/provider/client_test.go`

Current coverage:
- request-shape and transport composition
- auth and content-type propagation
- parsed assistant response
- usage extraction
- provider error normalization

## Current Limitation

- parser currently assumes an OpenAI-compatible response shape
- provider result is normalized, but provider-specific reasoning/tool-call parsing is still absent
