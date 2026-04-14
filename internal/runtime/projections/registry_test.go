package projections_test

import (
	"testing"

	"teamd/internal/runtime/projections"
)

func TestRegistryBuildsDefaultProjectionSet(t *testing.T) {
	t.Parallel()

	registry := projections.NewBuiltInRegistry()

	got, err := registry.BuildDefaults()
	if err != nil {
		t.Fatalf("BuildDefaults returned error: %v", err)
	}

	if len(got) != 2 {
		t.Fatalf("BuildDefaults len = %d, want 2", len(got))
	}
}
