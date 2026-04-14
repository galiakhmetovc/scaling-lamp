# Clean-Room Policy And Strategy Registries

This document describes the first real policy and strategy registry layer in the clean-room runtime.

It covers only what exists in code today.

## Current Files

### `internal/policies/registry.go`

Current responsibility:
- define policy families
- register supported policy module kinds
- register allowed strategy names per policy kind
- validate strategy names before contracts become runtime state

Current built-in families:
- endpoint
- auth
- retry
- timeout
- offload
- model
- message
- tool
- response_format
- streaming
- sampling

## Current Runtime Use

### `internal/runtime/contract_resolver.go`

Current role:
- build one built-in policy registry
- validate each loaded policy module against that registry
- reject unsupported strategy names during contract resolution

This means invalid strategies now fail before executor application.

## Current Extensibility Boundary

The registry layer already supports:
- adding new allowed strategies for an existing policy kind without builder edits
- injecting a custom policy registry through `ResolveContractsWithRegistry(...)`

The registry layer does not yet support:
- full generic decoding of policy payloads
- strategy-specific param decoders
- config-driven registry composition
- execution-time dispatch directly from strategy registry metadata

## Why This Slice Exists

Before this slice, strategy validation lived only as scattered executor checks.

After this slice:
- policy kind knowledge is centralized
- allowed strategy names are centralized
- resolver-side validation happens before runtime contracts are built

This is not the final policy system, but it is the first real registry layer instead of ad hoc validation.
