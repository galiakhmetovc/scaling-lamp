package workspace

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestBuildAGENTSContextFindsNearestAncestorAgents(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "AGENTS.md"), []byte("Rule: stay focused."), 0o644); err != nil {
		t.Fatalf("write AGENTS: %v", err)
	}
	child := filepath.Join(root, "nested", "deeper")
	if err := os.MkdirAll(child, 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}

	block := BuildAGENTSContext(child)
	if !strings.Contains(block, "AGENTS.md") || !strings.Contains(block, "stay focused") {
		t.Fatalf("unexpected workspace block: %q", block)
	}
}

func TestBuildSelectedContextIncludesOnlyRequestedFilesInsideRoot(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "AGENTS.md"), []byte("Rule: stay focused."), 0o644); err != nil {
		t.Fatalf("write AGENTS: %v", err)
	}
	if err := os.MkdirAll(filepath.Join(root, "docs"), 0o755); err != nil {
		t.Fatalf("mkdir docs: %v", err)
	}
	if err := os.WriteFile(filepath.Join(root, "docs", "guide.md"), []byte("Guide body"), 0o644); err != nil {
		t.Fatalf("write guide: %v", err)
	}
	outside := filepath.Join(t.TempDir(), "secret.txt")
	if err := os.WriteFile(outside, []byte("do not read"), 0o644); err != nil {
		t.Fatalf("write secret: %v", err)
	}

	block := BuildSelectedContext(root, []string{"AGENTS.md", "docs/guide.md", outside, "../escape.md"})
	if !strings.Contains(block, "AGENTS.md") || !strings.Contains(block, "guide.md") {
		t.Fatalf("expected selected files in workspace block, got %q", block)
	}
	if strings.Contains(block, "secret.txt") || strings.Contains(block, "do not read") {
		t.Fatalf("expected outside file to be excluded, got %q", block)
	}
}
