package delegation

import (
	"strings"
	"testing"

	"teamd/internal/contracts"
)

func TestDefinitionExecutorBuildsDelegationLifecycleTools(t *testing.T) {
	t.Parallel()

	got, err := NewDefinitionExecutor().Build(contracts.DelegationToolContract{
		Catalog: contracts.DelegationCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.DelegationCatalogParams{
				ToolIDs: []string{"delegate_spawn", "delegate_wait", "delegate_handoff"},
			},
		},
		Description: contracts.DelegationDescriptionPolicy{
			Enabled:  true,
			Strategy: "static_builtin_descriptions",
			Params: contracts.DelegationDescriptionParams{
				IncludeExamples:       true,
				IncludeBackendHints:   true,
				IncludeLifecycleNotes: true,
			},
		},
	})
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}
	if len(got) != 3 {
		t.Fatalf("definition count = %d, want 3", len(got))
	}
	if got[0].ID != "delegate_spawn" || got[1].ID != "delegate_wait" || got[2].ID != "delegate_handoff" {
		t.Fatalf("definitions = %#v", got)
	}
	if !strings.Contains(got[0].Description, "local_worker") || !strings.Contains(got[0].Description, "remote_mesh") {
		t.Fatalf("delegate_spawn description missing backend hints: %q", got[0].Description)
	}
	props, ok := got[1].Parameters["properties"].(map[string]any)
	if !ok {
		t.Fatalf("delegate_wait properties = %#v", got[1].Parameters["properties"])
	}
	for _, field := range []string{"delegate_id", "after_cursor", "after_event_id", "event_limit"} {
		if _, ok := props[field]; !ok {
			t.Fatalf("delegate_wait schema missing %q: %#v", field, props)
		}
	}
}

func TestDefinitionExecutorRejectsUnknownDelegationToolID(t *testing.T) {
	t.Parallel()

	_, err := NewDefinitionExecutor().Build(contracts.DelegationToolContract{
		Catalog: contracts.DelegationCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.DelegationCatalogParams{
				ToolIDs: []string{"delegate_missing"},
			},
		},
		Description: contracts.DelegationDescriptionPolicy{
			Enabled:  true,
			Strategy: "static_builtin_descriptions",
		},
	})
	if err == nil {
		t.Fatal("Build returned nil error, want unknown tool failure")
	}
}
