package artifacts_test

import (
	"testing"

	"teamd/internal/artifacts"
	"teamd/internal/contracts"
)

func TestDefinitionExecutorBuildsArtifactToolsWhenExposed(t *testing.T) {
	t.Parallel()

	defs, err := artifacts.NewDefinitionExecutor().Build(contracts.MemoryContract{
		Offload: contracts.OffloadPolicy{
			Enabled:  true,
			Strategy: "artifact_store",
			Params: contracts.OffloadParams{
				ExposeRetrievalTools: true,
			},
		},
	})
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}
	if len(defs) != 2 {
		t.Fatalf("definitions len = %d, want 2", len(defs))
	}
	if defs[0].ID != "artifact_read" || defs[1].ID != "artifact_search" {
		t.Fatalf("definitions = %#v", defs)
	}
}

func TestDefinitionExecutorSkipsArtifactToolsWhenNotExposed(t *testing.T) {
	t.Parallel()

	defs, err := artifacts.NewDefinitionExecutor().Build(contracts.MemoryContract{
		Offload: contracts.OffloadPolicy{
			Enabled:  true,
			Strategy: "old_only",
			Params: contracts.OffloadParams{
				ExposeRetrievalTools: true,
			},
		},
	})
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}
	if len(defs) != 0 {
		t.Fatalf("definitions len = %d, want 0", len(defs))
	}
}
