package runtime

import (
	"strings"
	"testing"
	"time"

	"teamd/internal/worker"
)

func TestBuildCheckpointDocumentRejectsNoisyContent(t *testing.T) {
	doc, ok := BuildCheckpointDocumentWithPolicy(MemoryPolicy{
		PromoteCheckpoint:    true,
		MaxDocumentBodyChars: 600,
		MaxResolvedFacts:     3,
	}, 1, "1:default", "search weather", worker.Checkpoint{
		WhatHappened:   "results count: 10\nurl: http://localhost",
		WhatMattersNow: "found sources",
	}, time.Now().UTC())
	if ok {
		t.Fatalf("expected noisy checkpoint to be rejected, got %#v", doc)
	}
}

func TestBuildCheckpointDocumentAcceptsDurableSummary(t *testing.T) {
	doc, ok := BuildCheckpointDocumentWithPolicy(MemoryPolicy{
		PromoteCheckpoint:    true,
		MaxDocumentBodyChars: 600,
		MaxResolvedFacts:     3,
	}, 1, "1:default", "fix launcher", worker.Checkpoint{
		WhatHappened:    "Moved launcher under systemd user units",
		WhatMattersNow:  "main and helper now restart reliably",
		ArchiveRefs:     []string{"archive://chat/1/session/default#messages-1-12"},
		SourceArtifacts: []string{"artifact://launcher-log/1"},
	}, time.Now().UTC())
	if !ok {
		t.Fatal("expected durable checkpoint document")
	}
	if doc.Kind != "checkpoint" || doc.Title != "fix launcher" {
		t.Fatalf("unexpected doc: %#v", doc)
	}
	if !strings.Contains(doc.Body, "Archive refs:") || !strings.Contains(doc.Body, "Artifact refs:") {
		t.Fatalf("expected reference sections in body, got %q", doc.Body)
	}
}

func TestBuildContinuityDocumentIncludesFacts(t *testing.T) {
	doc, ok := BuildContinuityDocumentWithPolicy(MemoryPolicy{
		PromoteContinuity:    true,
		MaxDocumentBodyChars: 600,
		MaxResolvedFacts:     3,
	}, Continuity{
		ChatID:        1,
		SessionID:     "1:default",
		UserGoal:      "stabilize bot",
		CurrentState:  "answer_sent",
		ResolvedFacts: []string{"launcher uses systemd", "cancel is async"},
		ArchiveRefs:   []string{"archive://chat/1/session/default#messages-1-8"},
		ArtifactRefs:  []string{"artifact://diagnostic/1"},
		UpdatedAt:     time.Now().UTC(),
	})
	if !ok {
		t.Fatal("expected continuity document")
	}
	if !strings.Contains(doc.Body, "Resolved facts:") {
		t.Fatalf("expected resolved facts in body, got %q", doc.Body)
	}
	if !strings.Contains(doc.Body, "Archive refs:") || !strings.Contains(doc.Body, "Artifact refs:") {
		t.Fatalf("expected reference sections in body, got %q", doc.Body)
	}
}

func TestBuildContinuityDocumentCanBeDisabledByPolicy(t *testing.T) {
	_, ok := BuildContinuityDocumentWithPolicy(MemoryPolicy{
		PromoteContinuity:    false,
		MaxDocumentBodyChars: 600,
		MaxResolvedFacts:     3,
	}, Continuity{
		ChatID:       1,
		SessionID:    "1:default",
		UserGoal:     "stabilize bot",
		CurrentState: "answer_sent",
	})
	if ok {
		t.Fatal("expected continuity promotion disabled")
	}
}

func TestCompactResolvedFactsHonorsPolicyLimit(t *testing.T) {
	facts := CompactResolvedFactsWithPolicy(MemoryPolicy{MaxResolvedFacts: 2}, "- one\n- two\n- three")
	if len(facts) != 2 {
		t.Fatalf("unexpected facts: %#v", facts)
	}
}
