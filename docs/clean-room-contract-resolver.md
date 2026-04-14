# Clean-Room Contract Resolver

Current baseline:

- one built agent instance resolves typed contracts during build
- resolver reads explicit contract module files
- resolver dispatches contracts by module `kind`, not by root config map key names
- resolver resolves policy module paths relative to the referencing contract file
- resolver returns typed runtime contracts, not raw YAML payloads
- resolver validates policy strategies through the built-in policy registry before contracts are returned
- resolver uses one generic typed policy loading path for referenced policy modules

## Current Files

### `internal/contracts/contracts.go`

Defines the first typed runtime contracts:

- `ResolvedContracts`
- `ProviderRequestContract`
- `TransportContract`
- `RequestShapeContract`
- `MemoryContract`
- `EndpointPolicy`
- `AuthPolicy`
- `RetryPolicy`
- `TimeoutPolicy`
- `OffloadPolicy`

This is the first runtime boundary between config modules and executors.

### `internal/runtime/contract_resolver.go`

Does:

- iterate configured contract module paths
- load each contract header
- dispatch to a typed contract decoder by contract `kind`
- decode referenced transport baseline policies:
  - endpoint
  - auth
  - retry
  - timeout
- decode referenced request-shape baseline policies:
  - model
  - messages
  - tools
  - response format
  - streaming
  - sampling
- decode referenced memory offload policy
- decode referenced prompt asset policy
- validate loaded policy strategies through `internal/policies`
- normalize relative policy paths against the contract file location
- return one typed `ResolvedContracts` object

## Current Limitation

- only transport, request-shape, memory, and prompt-assets are resolved
- there is no effective policy merge layer yet
- resolver is wired into the runtime builder, but higher-level provider composition is still missing
- policy families are still a narrow subset of the final design
- dispatch is kind-driven now, but resolver registration is still built-in rather than fully externalized
