package projections_test

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func TestRegistryBuildsDefaultProjectionSet(t *testing.T) {
	t.Parallel()

	registry := projections.NewBuiltInRegistry()

	got, err := registry.BuildDefaults()
	if err != nil {
		t.Fatalf("BuildDefaults returned error: %v", err)
	}

	if len(got) != 3 {
		t.Fatalf("BuildDefaults len = %d, want 3", len(got))
	}
}

func TestJSONFileStoreSavesAndLoadsProjectionSnapshots(t *testing.T) {
	t.Parallel()

	registry := projections.NewBuiltInRegistry()
	set, err := registry.Build("session", "run")
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}

	now := time.Date(2026, 4, 14, 12, 10, 0, 0, time.UTC)
	if err := set[0].Apply(eventing.Event{
		Kind:          eventing.EventSessionCreated,
		OccurredAt:    now,
		AggregateID:   "session-1",
		AggregateType: eventing.AggregateSession,
	}); err != nil {
		t.Fatalf("session Apply returned error: %v", err)
	}
	if err := set[1].Apply(eventing.Event{
		Kind:          eventing.EventRunStarted,
		OccurredAt:    now,
		AggregateID:   "run-1",
		AggregateType: eventing.AggregateRun,
		Payload: map[string]any{
			"session_id": "session-1",
		},
	}); err != nil {
		t.Fatalf("run Apply returned error: %v", err)
	}

	store, err := projections.NewJSONFileStore(filepath.Join(t.TempDir(), "projections.json"))
	if err != nil {
		t.Fatalf("NewJSONFileStore returned error: %v", err)
	}
	if err := store.Save(set); err != nil {
		t.Fatalf("Save returned error: %v", err)
	}

	reloaded, err := registry.Build("session", "run")
	if err != nil {
		t.Fatalf("Build reloaded set returned error: %v", err)
	}
	if err := store.Load(reloaded); err != nil {
		t.Fatalf("Load returned error: %v", err)
	}

	sessionProjection, ok := reloaded[0].(*projections.SessionProjection)
	if !ok {
		t.Fatalf("projection type = %T, want *SessionProjection", reloaded[0])
	}
	if sessionProjection.Snapshot().SessionID != "session-1" {
		t.Fatalf("SessionID = %q, want %q", sessionProjection.Snapshot().SessionID, "session-1")
	}

	runProjection, ok := reloaded[1].(*projections.RunProjection)
	if !ok {
		t.Fatalf("projection type = %T, want *RunProjection", reloaded[1])
	}
	if runProjection.Snapshot().RunID != "run-1" {
		t.Fatalf("RunID = %q, want %q", runProjection.Snapshot().RunID, "run-1")
	}
}

func TestJSONFileStoreQuarantinesCorruptSnapshotsAndLoadsEmptyState(t *testing.T) {
	t.Parallel()

	registry := projections.NewBuiltInRegistry()
	set, err := registry.Build("session", "run")
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}

	dir := t.TempDir()
	storePath := filepath.Join(dir, "projections.json")
	if err := os.WriteFile(storePath, []byte("not-json: ["), 0o644); err != nil {
		t.Fatalf("WriteFile(%q): %v", storePath, err)
	}

	store, err := projections.NewJSONFileStore(storePath)
	if err != nil {
		t.Fatalf("NewJSONFileStore returned error: %v", err)
	}
	if err := store.Load(set); err != nil {
		t.Fatalf("Load returned error: %v", err)
	}

	matches, err := filepath.Glob(storePath + ".corrupt-*")
	if err != nil {
		t.Fatalf("Glob returned error: %v", err)
	}
	if len(matches) != 1 {
		t.Fatalf("corrupt backup matches = %d, want 1", len(matches))
	}
	if _, err := os.Stat(storePath); !os.IsNotExist(err) {
		t.Fatalf("Stat(%q) error = %v, want not-exist", storePath, err)
	}

	sessionProjection, ok := set[0].(*projections.SessionProjection)
	if !ok {
		t.Fatalf("projection type = %T, want *SessionProjection", set[0])
	}
	if sessionProjection.Snapshot().SessionID != "" {
		t.Fatalf("SessionID = %q, want empty after corrupt-store recovery", sessionProjection.Snapshot().SessionID)
	}
}
