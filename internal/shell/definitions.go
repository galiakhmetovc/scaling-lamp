package shell

import (
	"fmt"

	"teamd/internal/contracts"
	"teamd/internal/tools"
)

type DefinitionExecutor struct{}

func NewDefinitionExecutor() *DefinitionExecutor {
	return &DefinitionExecutor{}
}

func (e *DefinitionExecutor) Build(contract contracts.ShellToolContract) ([]tools.Definition, error) {
	if e == nil {
		return nil, fmt.Errorf("shell definition executor is nil")
	}
	if !contract.Catalog.Enabled {
		return nil, nil
	}
	if contract.Catalog.Strategy != "static_allowlist" {
		return nil, fmt.Errorf("unsupported shell catalog strategy %q", contract.Catalog.Strategy)
	}
	if contract.Description.Enabled && contract.Description.Strategy != "static_builtin_descriptions" {
		return nil, fmt.Errorf("unsupported shell description strategy %q", contract.Description.Strategy)
	}

	all := defaultDefinitions()
	byID := make(map[string]tools.Definition, len(all))
	for _, definition := range all {
		byID[definition.ID] = definition
	}

	out := make([]tools.Definition, 0, len(contract.Catalog.Params.ToolIDs))
	for _, id := range contract.Catalog.Params.ToolIDs {
		definition, ok := byID[id]
		if !ok {
			return nil, fmt.Errorf("shell tool %q is not defined", id)
		}
		out = append(out, definition)
	}
	return out, nil
}

func defaultDefinitions() []tools.Definition {
	return []tools.Definition{
		{
			ID:          "shell_exec",
			Name:        "shell_exec",
			Description: "Run one bounded non-interactive shell command inside the configured workspace scope.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"command": map[string]any{"type": "string"},
					"args": map[string]any{
						"type":  "array",
						"items": map[string]any{"type": "string"},
					},
					"cwd": map[string]any{"type": "string"},
				},
				"required": []string{"command"},
			},
		},
	}
}
