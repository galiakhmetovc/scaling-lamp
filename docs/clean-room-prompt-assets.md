# Clean-Room Prompt Asset Domain

This document describes the first prompt asset domain in the clean-room runtime.

## Current Goal

Prompt assets are no longer only implicit raw messages passed ad hoc into request-shape execution.

There is now a dedicated domain for prompt assets:
- separate contract
- separate policy kind
- separate strategy validation
- explicit request-shape injection point

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

### `internal/provider/request_shape_executor.go`

Current role:
- accept `PromptAssets` in request-shape input
- prepend prompt asset messages before raw conversation messages

## Current Strategy

### `PromptAssetPolicy.inline_assets`

Current behavior:
- assets are declared inline in the policy module
- each asset has:
  - `role`
  - `content`
- request-shape execution prepends them ahead of raw input messages

## Current Limitation

- prompt assets are still plain inline text assets only
- there is no asset selection, filtering, templating, or merge policy yet
- builder does not yet create a separate prompt-asset executor; current application happens in request-shape execution
