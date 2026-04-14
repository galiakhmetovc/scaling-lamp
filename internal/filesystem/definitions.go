package filesystem

import (
	"fmt"

	"teamd/internal/contracts"
	"teamd/internal/tools"
)

type DefinitionExecutor struct{}

func NewDefinitionExecutor() *DefinitionExecutor {
	return &DefinitionExecutor{}
}

func (e *DefinitionExecutor) Build(contract contracts.FilesystemToolContract) ([]tools.Definition, error) {
	if e == nil {
		return nil, fmt.Errorf("filesystem definition executor is nil")
	}
	if !contract.Catalog.Enabled {
		return nil, nil
	}
	if contract.Catalog.Strategy != "static_allowlist" {
		return nil, fmt.Errorf("unsupported filesystem catalog strategy %q", contract.Catalog.Strategy)
	}
	if contract.Description.Enabled && contract.Description.Strategy != "static_builtin_descriptions" {
		return nil, fmt.Errorf("unsupported filesystem description strategy %q", contract.Description.Strategy)
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
			return nil, fmt.Errorf("filesystem tool %q is not defined", id)
		}
		out = append(out, definition)
	}
	return out, nil
}

func defaultDefinitions() []tools.Definition {
	return []tools.Definition{
		{
			ID:          "fs_list",
			Name:        "fs_list",
			Description: "List directory entries inside the configured workspace scope.",
			Parameters: objectSchema(
				map[string]any{
					"path": map[string]any{"type": "string"},
				},
				"path",
			),
		},
		{
			ID:          "fs_read_text",
			Name:        "fs_read_text",
			Description: "Read a text file from the configured workspace scope.",
			Parameters: objectSchema(
				map[string]any{
					"path": map[string]any{"type": "string"},
				},
				"path",
			),
		},
		{
			ID:          "fs_write_text",
			Name:        "fs_write_text",
			Description: "Write full text content to a file inside the configured workspace scope.",
			Parameters: objectSchema(
				map[string]any{
					"path":    map[string]any{"type": "string"},
					"content": map[string]any{"type": "string"},
				},
				"path", "content",
			),
		},
		{
			ID:          "fs_patch_text",
			Name:        "fs_patch_text",
			Description: "Replace a text fragment inside a file using explicit search and replacement strings.",
			Parameters: objectSchema(
				map[string]any{
					"path":    map[string]any{"type": "string"},
					"search":  map[string]any{"type": "string"},
					"replace": map[string]any{"type": "string"},
				},
				"path", "search", "replace",
			),
		},
		{
			ID:          "fs_mkdir",
			Name:        "fs_mkdir",
			Description: "Create a directory inside the configured workspace scope.",
			Parameters: objectSchema(
				map[string]any{
					"path": map[string]any{"type": "string"},
				},
				"path",
			),
		},
		{
			ID:          "fs_move",
			Name:        "fs_move",
			Description: "Move or rename a file or directory inside the configured workspace scope.",
			Parameters: objectSchema(
				map[string]any{
					"src":  map[string]any{"type": "string"},
					"dest": map[string]any{"type": "string"},
				},
				"src", "dest",
			),
		},
		{
			ID:          "fs_trash",
			Name:        "fs_trash",
			Description: "Move a file or directory to trash instead of deleting it permanently.",
			Parameters: objectSchema(
				map[string]any{
					"path": map[string]any{"type": "string"},
				},
				"path",
			),
		},
	}
}

func objectSchema(properties map[string]any, required ...string) map[string]any {
	return map[string]any{
		"type":       "object",
		"properties": properties,
		"required":   required,
	}
}
