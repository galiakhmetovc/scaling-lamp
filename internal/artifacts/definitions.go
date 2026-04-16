package artifacts

import (
	"fmt"

	"teamd/internal/contracts"
	"teamd/internal/tools"
)

type DefinitionExecutor struct{}

func NewDefinitionExecutor() *DefinitionExecutor {
	return &DefinitionExecutor{}
}

func (e *DefinitionExecutor) Build(contract contracts.MemoryContract) ([]tools.Definition, error) {
	if e == nil {
		return nil, fmt.Errorf("artifact definition executor is nil")
	}
	if !contract.Offload.Enabled || contract.Offload.Strategy != "artifact_store" || !contract.Offload.Params.ExposeRetrievalTools {
		return nil, nil
	}
	return []tools.Definition{
		{
			ID:          "artifact_read",
			Name:        "artifact_read",
			Description: "Read the full content of an offloaded artifact by artifact_ref when a previous tool result was replaced with a compact placeholder.",
			Parameters: objectSchema(
				map[string]any{
					"artifact_ref": map[string]any{"type": "string"},
				},
				"artifact_ref",
			),
		},
		{
			ID:          "artifact_search",
			Name:        "artifact_search",
			Description: "Search across offloaded artifacts by query and return matching artifact references with previews.",
			Parameters: objectSchema(
				map[string]any{
					"query": map[string]any{"type": "string"},
					"limit": map[string]any{"type": "integer"},
				},
				"query",
			),
		},
	}, nil
}

func objectSchema(properties map[string]any, required ...string) map[string]any {
	return map[string]any{
		"type":       "object",
		"properties": properties,
		"required":   required,
	}
}
