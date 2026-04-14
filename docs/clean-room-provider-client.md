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
3. return:
   - request body bytes
   - normalized transport response

### `internal/provider/client_test.go`

Current coverage:
- request-shape and transport composition
- auth and content-type propagation
- normalized result shape

## Current Limitation

- result still exposes raw transport response, not provider-specific parsed output
- usage parsing is still absent
- provider errors are still transport/body level, not higher-level normalized provider semantics
