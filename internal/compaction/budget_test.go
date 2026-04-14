package compaction

import (
	"testing"

	"teamd/internal/provider"
)

func TestEstimateMessagesReturnsStableNonZeroCost(t *testing.T) {
	messages := []provider.Message{
		{Role: "user", Content: "hello"},
		{Role: "assistant", Content: "world"},
	}

	got := EstimateMessages(messages)
	if got <= 0 {
		t.Fatalf("expected positive estimate, got %d", got)
	}
}
