package tools_test

import (
	"testing"

	"teamd/internal/contracts"
	"teamd/internal/tools"
)

func TestCatalogExecutorBuildSelectsStaticAllowlistInConfiguredOrder(t *testing.T) {
	t.Parallel()

	executor := tools.NewCatalogExecutor()
	got, err := executor.Build(contracts.ToolContract{
		Catalog: contracts.ToolCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ToolCatalogParams{
				ToolIDs: []string{"tool-b", "tool-a"},
			},
		},
	}, tools.CatalogInput{
		Available: []tools.Definition{
			{ID: "tool-a", Name: "filesystem.read_file"},
			{ID: "tool-b", Name: "shell.exec"},
		},
	})
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}
	if len(got) != 2 || got[0].ID != "tool-b" || got[1].ID != "tool-a" {
		t.Fatalf("selected tools = %#v", got)
	}
}

func TestCatalogExecutorSerializeBuildsOpenAIFunctionTools(t *testing.T) {
	t.Parallel()

	executor := tools.NewCatalogExecutor()
	got, err := executor.Serialize(contracts.ToolContract{
		Serialization: contracts.ToolSerializationPolicy{
			Enabled:  true,
			Strategy: "openai_function_tools",
			Params: contracts.ToolSerializationParams{
				IncludeDescriptions: true,
			},
		},
	}, []tools.Definition{{
		ID:          "tool-a",
		Name:        "filesystem.read_file",
		Description: "Read a file.",
		Parameters:  map[string]any{"type": "object"},
	}})
	if err != nil {
		t.Fatalf("Serialize returned error: %v", err)
	}
	if len(got) != 1 {
		t.Fatalf("serialized tool count = %d, want 1", len(got))
	}
	function := got[0]["function"].(map[string]any)
	if function["name"] != "filesystem.read_file" {
		t.Fatalf("function name = %#v", function["name"])
	}
	if function["description"] != "Read a file." {
		t.Fatalf("function description = %#v", function["description"])
	}
}
