package projections_test

import (
	"testing"

	"teamd/internal/runtime/projections"
)

func TestRegistryBuildsDefaultProjectionSet(t *testing.T) {
	t.Parallel()

	registry := projections.NewRegistry()
	registry.Register("session", func() projections.Projection { return projections.NewSessionProjection() })
	registry.Register("run", func() projections.Projection { return projections.NewRunProjection() })

	got, err := registry.Build("session", "run")
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}

	if len(got) != 2 {
		t.Fatalf("Build len = %d, want 2", len(got))
	}
}
