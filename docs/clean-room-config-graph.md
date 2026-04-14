# Clean-Room Config Graph

Current baseline:

- one root config file
- explicit contract paths
- explicit policy references inside contract files
- path resolution relative to the referencing file
- header-only module graph loading before deeper contract resolution
- contract and policy kinds validated through built-in module registry metadata

## Current Files

### `internal/config/types.go`

Defines:
- root config
- explicit contract reference map
- module header
- module graph

### `internal/config/loader.go`

Does:
- root config loading
- path normalization
- module header loading
- registry-driven module graph walking

Current scope:
- root config provides explicit contract paths
- module registry decides which reference fields each module kind exposes
- graph walker resolves referenced module paths relative to the referencing file
- graph traversal no longer knows contract-family-specific keys

Current limitation:
- root config still loads only the first contract layer and does not yet resolve effective contracts
- module registry is still built-in and not yet a full policy/strategy registry system
- graph currently stores headers only, not decoded module payloads

### `internal/config/registry.go`

Does:
- register supported module kinds
- classify modules as contracts vs policies
- declare allowed reference fields per module kind
- validate loaded module headers before runtime wiring

It is still a baseline registry, not yet the final contract/policy/strategy registry system.
