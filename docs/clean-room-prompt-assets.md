# Clean-Room Prompt Asset Domain

This document describes the first prompt asset domain in the clean-room runtime.

## Current Goal

Prompt assets are no longer only implicit raw messages passed ad hoc into request-shape execution.

There is now a dedicated domain for prompt assets:
- separate contract
- separate policy kind
- separate strategy validation
- explicit prompt-asset execution point before request-shape build

## Current Files

### `internal/contracts/contracts.go`

Current role:
- define `PromptAssetsContract`
- define `PromptAssetPolicy`
- define `PromptAssetParams`
- define `PromptAsset`

### `internal/config/registry.go`

Current role:
- register `PromptAssetsContractConfig`
- register `PromptAssetPolicyConfig`

### `internal/policies/registry.go`

Current role:
- register prompt asset family
- validate `inline_assets`

### `internal/runtime/contract_resolver.go`

Current role:
- resolve `prompt_assets` contract from root config
- decode prompt asset policy module
- validate prompt asset strategy through the policy registry
- expose resolved prompt asset assets in runtime contracts

### `internal/provider/prompt_asset_executor.go`

Current role:
- resolve prompt assets from the prompt-asset contract
- select assets by `id`
- split them into prepend/append buckets by `placement`

### `internal/provider/client.go`

Current role:
- call the prompt-asset executor before request-shape execution
- pass resolved prepend/append prompt messages into request-shape execution

## Current Strategy

### `PromptAssetPolicy.inline_assets`

Current behavior:
- assets are declared inline in the policy module
- each asset has:
  - `id`
  - `role`
  - `content`
  - `placement`
- provider client can select assets by `id`
- placement currently supports:
  - `prepend`
  - `append`

## Current Limitation

- prompt assets are still plain inline text assets only
- there is still no templating or richer rule-based selection yet
- builder does not yet construct a dedicated prompt-asset executor as a first-class runtime component
