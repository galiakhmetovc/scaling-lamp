# Clean-Room Config Graph

Current baseline:

- one root config file
- explicit contract paths
- explicit policy references inside contract files
- path resolution relative to the referencing file
- header-only module graph loading before deeper contract resolution

## Current Files

### `internal/config/types.go`

Defines:
- root config
- contract references
- module header
- module graph

### `internal/config/loader.go`

Does:
- root config loading
- path normalization
- module header loading
- minimal contract-to-policy graph loading

Current scope:
- transport contract
- memory contract
- endpoint policy ref
- offload policy ref

This is still a narrow slice, not the final general config graph system.

### `internal/config/registry.go`

Does:
- register supported module kinds
- validate kinds before builder wiring

It is currently a minimal runtime guardrail, not yet a full contract/policy/strategy registry system.
