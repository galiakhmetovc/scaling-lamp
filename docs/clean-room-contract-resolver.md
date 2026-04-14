# Clean-Room Contract Resolver

Current baseline:

- one built agent instance resolves typed contracts during build
- resolver reads explicit contract module files
- resolver resolves policy module paths relative to the referencing contract file
- resolver returns typed runtime contracts, not raw YAML payloads

## Current Files

### `internal/runtime/contracts.go`

Defines the first typed runtime contracts:

- `ResolvedContracts`
- `ProviderRequestContract`
- `TransportContract`
- `MemoryContract`
- `EndpointPolicy`
- `OffloadPolicy`

This is the first runtime boundary between config modules and executors.

### `internal/runtime/contract_resolver.go`

Does:

- load transport contract config
- load memory contract config
- decode referenced endpoint and offload policies
- normalize relative policy paths against the contract file location
- return one typed `ResolvedContracts` object

## Current Limitation

- only transport and memory are resolved
- there is no effective policy merge layer yet
- resolver is not yet connected to any executor
- policy families are still a narrow subset of the final design
