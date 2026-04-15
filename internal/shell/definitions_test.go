package shell

import (
	"strings"
	"testing"

	"teamd/internal/contracts"
)

func TestDefaultDefinitionsDescribeCrossPlatformShellExecUsage(t *testing.T) {
	t.Parallel()

	definitions, err := NewDefinitionExecutor().Build(contracts.ShellToolContract{
		Catalog: contracts.ShellCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ShellCatalogParams{
				ToolIDs: []string{"shell_exec"},
			},
		},
		Description: contracts.ShellDescriptionPolicy{
			Enabled:  true,
			Strategy: "static_builtin_descriptions",
		},
	})
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}
	if len(definitions) != 1 {
		t.Fatalf("len(definitions) = %d, want 1", len(definitions))
	}
	description := definitions[0].Description
	for _, want := range []string{
		"Windows builtin commands like echo, dir, and type",
		"POSIX example",
		"Windows example",
	} {
		if !strings.Contains(description, want) {
			t.Fatalf("description missing %q: %q", want, description)
		}
	}
}
