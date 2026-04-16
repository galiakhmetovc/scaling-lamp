package projections

import (
	"testing"

	"teamd/internal/runtime/eventing"
)

func TestFilesystemHeadProjectionTracksRecentFilesystemActivityBySession(t *testing.T) {
	t.Parallel()

	projection := NewFilesystemHeadProjection()
	err := projection.Apply(eventing.Event{
		Kind: eventing.EventToolCallCompleted,
		Payload: map[string]any{
			"session_id": "session-1",
			"tool_name":  "fs_replace_lines",
			"arguments":  map[string]any{"path": "internal/promptassembly/executor.go"},
		},
	})
	if err != nil {
		t.Fatalf("Apply returned error: %v", err)
	}

	snapshot := projection.SnapshotForSession("session-1")
	if len(snapshot.Edited) != 1 || snapshot.Edited[0] != "internal/promptassembly/executor.go" {
		t.Fatalf("edited = %#v", snapshot.Edited)
	}
}
