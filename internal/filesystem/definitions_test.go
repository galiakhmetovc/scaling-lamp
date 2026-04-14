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
				ToolIDs: []string{"fs_list", "fs_patch_text", "fs_trash"},
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
	if len(got) != 3 {
		t.Fatalf("definition count = %d, want 3", len(got))
	}
	if got[0].ID != "fs_list" || got[1].ID != "fs_patch_text" || got[2].ID != "fs_trash" {
		t.Fatalf("definitions = %#v", got)
	}
	if got[0].Description == "" {
		t.Fatalf("fs_list description is empty")
	}
	props, ok := got[1].Parameters["properties"].(map[string]any)
	if !ok {
		t.Fatalf("fs_patch_text properties = %#v", got[1].Parameters["properties"])
	}
	for _, field := range []string{"path", "search", "replace"} {
		if _, ok := props[field]; !ok {
			t.Fatalf("fs_patch_text schema missing %q: %#v", field, props)
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
