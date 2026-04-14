package tools

import (
	"fmt"

	"teamd/internal/contracts"
)

type Definition struct {
	ID          string
	Name        string
	Description string
	Parameters  map[string]any
}

type CatalogInput struct {
	Available []Definition
}

type CatalogExecutor struct{}

func NewCatalogExecutor() *CatalogExecutor {
	return &CatalogExecutor{}
}

func (e *CatalogExecutor) Build(contract contracts.ToolContract, input CatalogInput) ([]Definition, error) {
	if e == nil {
		return nil, fmt.Errorf("tool catalog executor is nil")
	}
	if !contract.Catalog.Enabled {
		return nil, nil
	}
	if contract.Catalog.Strategy != "static_allowlist" {
		return nil, fmt.Errorf("unsupported tool catalog strategy %q", contract.Catalog.Strategy)
	}

	availableByID := make(map[string]Definition, len(input.Available))
	for _, definition := range input.Available {
		availableByID[definition.ID] = definition
	}

	out := make([]Definition, 0, len(contract.Catalog.Params.ToolIDs))
	seen := map[string]struct{}{}
	for _, id := range contract.Catalog.Params.ToolIDs {
		definition, ok := availableByID[id]
		if !ok {
			return nil, fmt.Errorf("tool %q is not registered in available catalog", id)
		}
		if contract.Catalog.Params.Dedupe {
			if _, ok := seen[id]; ok {
				continue
			}
			seen[id] = struct{}{}
		}
		out = append(out, definition)
	}
	if len(out) == 0 && !contract.Catalog.Params.AllowEmpty {
		return nil, fmt.Errorf("tool catalog resolved empty selection")
	}
	return out, nil
}

func (e *CatalogExecutor) Serialize(contract contracts.ToolContract, definitions []Definition) ([]map[string]any, error) {
	if e == nil {
		return nil, fmt.Errorf("tool catalog executor is nil")
	}
	if !contract.Serialization.Enabled {
		return nil, nil
	}
	if contract.Serialization.Strategy != "openai_function_tools" {
		return nil, fmt.Errorf("unsupported tool serialization strategy %q", contract.Serialization.Strategy)
	}
	out := make([]map[string]any, 0, len(definitions))
	for _, definition := range definitions {
		function := map[string]any{
			"name":       definition.Name,
			"parameters": definition.Parameters,
		}
		if contract.Serialization.Params.IncludeDescriptions && definition.Description != "" {
			function["description"] = definition.Description
		}
		out = append(out, map[string]any{
			"type":     "function",
			"function": function,
		})
	}
	return out, nil
}
