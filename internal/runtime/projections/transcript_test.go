package projections_test

import (
	"path/filepath"
	"testing"

	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func TestTranscriptProjectionAppliesAndRestoresMessages(t *testing.T) {
	t.Parallel()

	projection := projections.NewTranscriptProjection()
	for _, event := range []eventing.Event{
		{
			Kind: eventing.EventMessageRecorded,
			Payload: map[string]any{
				"session_id": "session-1",
				"role":       "user",
				"content":    "Ping",
			},
		},
		{
			Kind: eventing.EventMessageRecorded,
			Payload: map[string]any{
				"session_id": "session-1",
				"role":       "assistant",
				"content":    "Pong",
			},
		},
	} {
		if err := projection.Apply(event); err != nil {
			t.Fatalf("Apply returned error: %v", err)
		}
	}

	store, err := projections.NewJSONFileStore(filepath.Join(t.TempDir(), "projections.json"))
	if err != nil {
		t.Fatalf("NewJSONFileStore returned error: %v", err)
	}
	if err := store.Save([]projections.Projection{projection}); err != nil {
		t.Fatalf("Save returned error: %v", err)
	}

	reloaded := projections.NewTranscriptProjection()
	if err := store.Load([]projections.Projection{reloaded}); err != nil {
		t.Fatalf("Load returned error: %v", err)
	}

	got := reloaded.Snapshot().Sessions["session-1"]
	if len(got) != 2 {
		t.Fatalf("message count = %d, want 2", len(got))
	}
	if got[0].Role != "user" || got[0].Content != "Ping" {
		t.Fatalf("first message = %#v", got[0])
	}
	if got[1].Role != "assistant" || got[1].Content != "Pong" {
		t.Fatalf("second message = %#v", got[1])
	}
}
