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
			ID:          "fs_read_lines",
			Name:        "fs_read_lines",
			Description: "Read an explicit inclusive line range from a text file inside the configured workspace scope.",
			Parameters: objectSchema(
				map[string]any{
					"path":       map[string]any{"type": "string"},
					"start_line": map[string]any{"type": "integer"},
					"end_line":   map[string]any{"type": "integer"},
				},
				"path", "start_line", "end_line",
			),
		},
		{
			ID:          "fs_search_text",
			Name:        "fs_search_text",
			Description: "Search a text file for a literal query and return matching lines with their line numbers.",
			Parameters: objectSchema(
				map[string]any{
					"path":  map[string]any{"type": "string"},
					"query": map[string]any{"type": "string"},
					"limit": map[string]any{"type": "integer"},
				},
				"path", "query",
			),
		},
		{
			ID:          "fs_find_in_files",
			Name:        "fs_find_in_files",
			Description: "Search across multiple files in scope and return matching lines with paths and line numbers.",
			Parameters: objectSchema(
				map[string]any{
					"query": map[string]any{"type": "string"},
					"glob":  map[string]any{"type": "string"},
					"limit": map[string]any{"type": "integer"},
				},
				"query",
			),
		},
		{
			ID:          "fs_write_text",
			Name:        "fs_write_text",
			Description: "Write full text content to a file inside the configured workspace scope with explicit create, overwrite, or upsert semantics.",
			Parameters: objectSchema(
				map[string]any{
					"path":    map[string]any{"type": "string"},
					"content": map[string]any{"type": "string"},
					"mode":    map[string]any{"type": "string", "enum": []string{"create", "overwrite", "upsert"}},
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
			ID:          "fs_replace_lines",
			Name:        "fs_replace_lines",
			Description: "Replace an explicit inclusive line range in a text file with new text content.",
			Parameters: objectSchema(
				map[string]any{
					"path":       map[string]any{"type": "string"},
					"start_line": map[string]any{"type": "integer"},
					"end_line":   map[string]any{"type": "integer"},
					"content":    map[string]any{"type": "string"},
				},
				"path", "start_line", "end_line", "content",
			),
		},
		{
			ID:          "fs_insert_text",
			Name:        "fs_insert_text",
			Description: "Insert text before or after a line number, or prepend or append text at the file edges.",
			Parameters: objectSchema(
				map[string]any{
					"path":     map[string]any{"type": "string"},
					"line":     map[string]any{"type": "integer"},
					"position": map[string]any{"type": "string", "enum": []string{"before", "after", "prepend", "append"}},
					"content":  map[string]any{"type": "string"},
				},
				"path", "position", "content",
			),
		},
		{
			ID:          "fs_replace_in_line",
			Name:        "fs_replace_in_line",
			Description: "Replace text within a specific line of a file, or replace the whole line content explicitly.",
			Parameters: objectSchema(
				map[string]any{
					"path":    map[string]any{"type": "string"},
					"line":    map[string]any{"type": "integer"},
					"search":  map[string]any{"type": "string"},
					"replace": map[string]any{"type": "string"},
					"content": map[string]any{"type": "string"},
				},
				"path", "line",
			),
		},
		{
			ID:          "fs_replace_in_files",
			Name:        "fs_replace_in_files",
			Description: "Replace literal text across multiple files in scope with bounded file and hit limits.",
			Parameters: objectSchema(
				map[string]any{
					"query":   map[string]any{"type": "string"},
					"replace": map[string]any{"type": "string"},
					"glob":    map[string]any{"type": "string"},
					"limit":   map[string]any{"type": "integer"},
				},
				"query", "replace",
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
