package provider

import (
	"fmt"

	"teamd/internal/contracts"
)

const (
	PromptAssetPlacementPrepend = "prepend"
	PromptAssetPlacementAppend  = "append"
)

type PromptAssetInput struct {
	SelectedIDs []string
}

type PromptAssetOutput struct {
	Prepend []contracts.Message
	Append  []contracts.Message
}

type PromptAssetExecutor struct{}

func NewPromptAssetExecutor() *PromptAssetExecutor {
	return &PromptAssetExecutor{}
}

func (e *PromptAssetExecutor) Build(contract contracts.PromptAssetsContract, input PromptAssetInput) (PromptAssetOutput, error) {
	if e == nil {
		return PromptAssetOutput{}, fmt.Errorf("prompt-asset executor is nil")
	}
	if !contract.PromptAsset.Enabled {
		return PromptAssetOutput{}, nil
	}
	if contract.PromptAsset.Strategy != "inline_assets" {
		return PromptAssetOutput{}, fmt.Errorf("unsupported prompt-asset strategy %q", contract.PromptAsset.Strategy)
	}

	selected := map[string]struct{}{}
	for _, id := range input.SelectedIDs {
		selected[id] = struct{}{}
	}
	requireSelection := len(selected) > 0
	seenSelected := map[string]struct{}{}

	var out PromptAssetOutput
	for _, asset := range contract.PromptAsset.Params.Assets {
		if requireSelection {
			if _, ok := selected[asset.ID]; !ok {
				continue
			}
			seenSelected[asset.ID] = struct{}{}
		}

		message := contracts.Message{
			Role:    asset.Role,
			Content: asset.Content,
		}

		switch asset.Placement {
		case "", PromptAssetPlacementPrepend:
			out.Prepend = append(out.Prepend, message)
		case PromptAssetPlacementAppend:
			out.Append = append(out.Append, message)
		default:
			return PromptAssetOutput{}, fmt.Errorf("prompt asset %q has unsupported placement %q", asset.ID, asset.Placement)
		}
	}

	if requireSelection {
		for id := range selected {
			if _, ok := seenSelected[id]; !ok {
				return PromptAssetOutput{}, fmt.Errorf("selected prompt asset %q was not found", id)
			}
		}
	}

	return out, nil
}
