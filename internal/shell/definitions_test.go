package shell_test

import (
	"testing"

	"teamd/internal/contracts"
	"teamd/internal/shell"
)

func TestDefinitionExecutorBuildsStaticAllowlistShellTools(t *testing.T) {
	t.Parallel()

	executor := shell.NewDefinitionExecutor()
	got, err := executor.Build(contracts.ShellToolContract{
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
			Params: contracts.ShellDescriptionParams{
				IncludeExamples:      true,
				IncludeRuntimeLimits: true,
			},
		},
	})
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}
	if len(got) != 1 {
		t.Fatalf("definition count = %d, want 1", len(got))
	}
	if got[0].ID != "shell_exec" {
		t.Fatalf("definition id = %q, want shell_exec", got[0].ID)
	}
	props, ok := got[0].Parameters["properties"].(map[string]any)
	if !ok {
		t.Fatalf("shell_exec properties = %#v", got[0].Parameters["properties"])
	}
	for _, field := range []string{"command", "args", "cwd"} {
		if _, ok := props[field]; !ok {
			t.Fatalf("shell_exec schema missing %q: %#v", field, props)
		}
	}
}

func TestDefinitionExecutorRejectsUnknownShellToolID(t *testing.T) {
	t.Parallel()

	executor := shell.NewDefinitionExecutor()
	_, err := executor.Build(contracts.ShellToolContract{
		Catalog: contracts.ShellCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ShellCatalogParams{
				ToolIDs: []string{"shell_missing"},
			},
		},
		Description: contracts.ShellDescriptionPolicy{
			Enabled:  true,
			Strategy: "static_builtin_descriptions",
		},
	})
	if err == nil {
		t.Fatal("Build returned nil error, want unknown tool failure")
	}
}
