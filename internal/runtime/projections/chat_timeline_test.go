package projections_test

import (
	"testing"

	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func TestChatTimelineProjectionBuildsSessionScopedTimeline(t *testing.T) {
	t.Parallel()

	projection := projections.NewChatTimelineProjection()
	events := []eventing.Event{
		{
			Kind: eventing.EventMessageRecorded,
			Payload: map[string]any{
				"session_id": "session-1",
				"role":       "user",
				"content":    "Ping",
			},
		},
		{
			Kind: eventing.EventToolCallStarted,
			Payload: map[string]any{
				"session_id": "session-1",
				"tool_name":  "fs_list",
			},
		},
		{
			Kind: eventing.EventTaskAdded,
			Payload: map[string]any{
				"session_id":  "session-1",
				"description": "Audit middleware",
			},
		},
		{
			Kind: eventing.EventMessageRecorded,
			Payload: map[string]any{
				"session_id": "session-2",
				"role":       "user",
				"content":    "Other",
			},
		},
	}
	for _, event := range events {
		if err := projection.Apply(event); err != nil {
			t.Fatalf("Apply returned error: %v", err)
		}
	}

	session1 := projection.SnapshotForSession("session-1")
	session2 := projection.SnapshotForSession("session-2")
	if len(session1) != 3 {
		t.Fatalf("session-1 item count = %d, want 3", len(session1))
	}
	if session1[0].Kind != projections.ChatTimelineItemMessage || session1[0].Role != "user" {
		t.Fatalf("session-1 first item = %#v", session1[0])
	}
	if session1[1].Kind != projections.ChatTimelineItemTool {
		t.Fatalf("session-1 second item = %#v", session1[1])
	}
	if session1[1].Content != "fs_list started" {
		t.Fatalf("session-1 second content = %q, want compact tool line", session1[1].Content)
	}
	if session1[2].Kind != projections.ChatTimelineItemPlan {
		t.Fatalf("session-1 third item = %#v", session1[2])
	}
	if len(session2) != 1 || session2[0].Content != "Other" {
		t.Fatalf("session-2 items = %#v", session2)
	}
}

func TestChatTimelineProjectionBuildsCompactToolCompletionLine(t *testing.T) {
	t.Parallel()

	projection := projections.NewChatTimelineProjection()
	event := eventing.Event{
		Kind: eventing.EventToolCallCompleted,
		Payload: map[string]any{
			"session_id": "session-1",
			"tool_name":  "fs_read_lines",
			"arguments": map[string]any{
				"path":       "/tmp/app.go",
				"start_line": 10,
				"end_line":   40,
			},
			"error": `tool call "fs_read_lines": end_line exceeds file length`,
		},
	}
	if err := projection.Apply(event); err != nil {
		t.Fatalf("Apply returned error: %v", err)
	}

	session1 := projection.SnapshotForSession("session-1")
	if len(session1) != 1 {
		t.Fatalf("session-1 item count = %d, want 1", len(session1))
	}
	if got := session1[0].Content; got != "fs_read_lines tmp/app.go:10-40 error: end_line exceeds file length" {
		t.Fatalf("session-1 content = %q", got)
	}
}
