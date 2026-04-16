package runtime

import (
	"os"
	"path/filepath"
	"testing"

	"teamd/internal/contracts"
	"teamd/internal/promptassembly"
)

func TestBuildFilesystemHeadInputCollectsRecentFilesystemActivity(t *testing.T) {
	t.Parallel()

	got := buildFilesystemHeadInputForMessages(contracts.SessionHeadParams{
		IncludeFilesystemRecent:  true,
		FilesystemRecentMaxItems: 2,
	}, []contracts.Message{
		{Role: "tool", Name: "fs_read_lines", Content: `{"path":"internal/contracts/contracts.go"}`},
		{Role: "tool", Name: "fs_replace_lines", Content: `{"path":"internal/promptassembly/executor.go"}`},
		{Role: "tool", Name: "fs_find_in_files", Content: `{"matches":[{"path":"web/src/App.tsx"},{"path":"internal/runtime/chat.go"}]}`},
	})

	if len(got.Recent.Read) != 1 || got.Recent.Read[0] != "internal/contracts/contracts.go" {
		t.Fatalf("recent read = %#v", got.Recent.Read)
	}
	if len(got.Recent.Edited) != 1 || got.Recent.Edited[0] != "internal/promptassembly/executor.go" {
		t.Fatalf("recent edited = %#v", got.Recent.Edited)
	}
	if len(got.Recent.Found) != 2 {
		t.Fatalf("recent found = %#v", got.Recent.Found)
	}
}

func TestBuildFilesystemTreeEntriesBuildsDepthOneTreeFromRoot(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	mustWriteRuntimeFile(t, filepath.Join(dir, "go.mod"), "module teamd\n")
	mustMkdirAll(t, filepath.Join(dir, "internal"))
	mustMkdirAll(t, filepath.Join(dir, "web"))

	got, err := buildFilesystemTreeEntries(contracts.SessionHeadParams{
		IncludeFilesystemTree:      true,
		FilesystemTreeMaxEntries:   2,
		FilesystemTreeIncludeFiles: true,
		FilesystemTreeIncludeDirs:  true,
	}, dir)
	if err != nil {
		t.Fatalf("buildFilesystemTreeEntries returned error: %v", err)
	}
	if len(got) != 2 {
		t.Fatalf("tree entries len = %d, want 2", len(got))
	}
	if got[0] != (promptassembly.FilesystemTreeEntry{Name: "go.mod", IsDir: false}) {
		t.Fatalf("first tree entry = %#v, want go.mod", got[0])
	}
}

func mustWriteRuntimeFile(t *testing.T, path, body string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("WriteFile(%q): %v", path, err)
	}
}

func mustMkdirAll(t *testing.T, path string) {
	t.Helper()
	if err := os.MkdirAll(path, 0o755); err != nil {
		t.Fatalf("MkdirAll(%q): %v", path, err)
	}
}
