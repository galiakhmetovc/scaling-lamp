package memory

import (
	"strings"
	"testing"
)

func TestFormatRecallBlock(t *testing.T) {
	block := FormatRecallBlock([]RecallItem{{
		Kind:  "continuity",
		Title: "stabilize bot",
		Body:  "launcher moved to systemd and cancel no longer blocks poll loop",
	}})
	if !strings.Contains(block, "Relevant memory recall.") {
		t.Fatalf("unexpected block: %q", block)
	}
	if !strings.Contains(block, "[continuity] stabilize bot") {
		t.Fatalf("unexpected block: %q", block)
	}
}
