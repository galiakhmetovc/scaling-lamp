package filesystem_test

import (
	"testing"

	"teamd/internal/contracts"
	"teamd/internal/filesystem"
)

func TestDefinitionExecutorBuildsStaticAllowlistFilesystemTools(t *testing.T) {
	t.Parallel()

	executor := filesystem.NewDefinitionExecutor()
	got, err := executor.Build(contracts.FilesystemToolContract{
		Catalog: contracts.FilesystemCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.FilesystemCatalogParams{
				ToolIDs: []string{"fs_list", "fs_read_lines", "fs_search_text", "fs_find_in_files", "fs_replace_lines", "fs_replace_in_line", "fs_insert_text", "fs_replace_in_files", "fs_trash"},
			},
		},
		Description: contracts.FilesystemDescriptionPolicy{
			Enabled:  true,
			Strategy: "static_builtin_descriptions",
			Params: contracts.FilesystemDescriptionParams{
				IncludeExamples:  true,
				IncludeScopeHint: true,
			},
		},
	})
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}
	if len(got) != 9 {
		t.Fatalf("definition count = %d, want 9", len(got))
	}
	if got[0].ID != "fs_list" || got[1].ID != "fs_read_lines" || got[2].ID != "fs_search_text" || got[3].ID != "fs_find_in_files" || got[4].ID != "fs_replace_lines" || got[5].ID != "fs_replace_in_line" || got[6].ID != "fs_insert_text" || got[7].ID != "fs_replace_in_files" || got[8].ID != "fs_trash" {
		t.Fatalf("definitions = %#v", got)
	}
	if got[0].Description == "" {
		t.Fatalf("fs_list description is empty")
	}
	props, ok := got[4].Parameters["properties"].(map[string]any)
	if !ok {
		t.Fatalf("fs_replace_lines properties = %#v", got[4].Parameters["properties"])
	}
	for _, field := range []string{"path", "start_line", "end_line", "content"} {
		if _, ok := props[field]; !ok {
			t.Fatalf("fs_replace_lines schema missing %q: %#v", field, props)
		}
	}
}

func TestDefinitionExecutorRejectsUnknownFilesystemToolID(t *testing.T) {
	t.Parallel()

	executor := filesystem.NewDefinitionExecutor()
	_, err := executor.Build(contracts.FilesystemToolContract{
		Catalog: contracts.FilesystemCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.FilesystemCatalogParams{
				ToolIDs: []string{"fs_missing"},
			},
		},
		Description: contracts.FilesystemDescriptionPolicy{
			Enabled:  true,
			Strategy: "static_builtin_descriptions",
		},
	})
	if err == nil {
		t.Fatal("Build returned nil error, want unknown tool failure")
	}
}
