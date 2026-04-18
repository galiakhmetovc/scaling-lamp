package workspace

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestFilesListTreeFromWorkspaceRoot(t *testing.T) {
	t.Parallel()

	mgr := newWorkspaceFilesManager(t)

	snap, err := mgr.Snapshot("session-1")
	if err != nil {
		t.Fatalf("Snapshot: %v", err)
	}
	if snap.RootPath == "" {
		t.Fatal("RootPath is empty")
	}
	if got := len(snap.Items); got != 2 {
		t.Fatalf("items len = %d, want 2", got)
	}
	if snap.Items[0].Name != "dir" || !snap.Items[0].IsDir {
		t.Fatalf("first item = %#v, want dir directory", snap.Items[0])
	}
	if snap.Items[1].Name != "go.mod" || snap.Items[1].IsDir {
		t.Fatalf("second item = %#v, want go.mod file", snap.Items[1])
	}
}

func TestFilesExpandLoadsDirectoryChildren(t *testing.T) {
	t.Parallel()

	mgr := newWorkspaceFilesManager(t)

	snap, err := mgr.Expand("session-1", "dir")
	if err != nil {
		t.Fatalf("Expand: %v", err)
	}
	if len(snap.Items) != 4 {
		t.Fatalf("items len = %d, want 4", len(snap.Items))
	}
	if !snap.Items[0].IsDir || !snap.Items[0].ChildrenLoaded {
		t.Fatalf("expanded dir item = %#v, want loaded dir", snap.Items[0])
	}
	if got := snap.Items[1]; got.Path != "dir/child.txt" || got.Name != "child.txt" || got.IsDir {
		t.Fatalf("child item = %#v, want dir/child.txt child.txt file", got)
	}
	if got := snap.Items[2]; got.Path != "dir/nested.go" || got.Name != "nested.go" || got.IsDir {
		t.Fatalf("second child item = %#v, want dir/nested.go nested.go file", got)
	}
}

func TestFilesStatIncludesMetadata(t *testing.T) {
	t.Parallel()

	mgr := newWorkspaceFilesManager(t)

	node, err := mgr.Stat("dir/child.txt")
	if err != nil {
		t.Fatalf("Stat: %v", err)
	}
	if node.Path != "dir/child.txt" {
		t.Fatalf("path = %q, want dir/child.txt", node.Path)
	}
	if node.Name != "child.txt" {
		t.Fatalf("name = %q, want child.txt", node.Name)
	}
	if node.Size == 0 {
		t.Fatal("size is zero")
	}
	if node.ModTime.IsZero() {
		t.Fatal("mod time is zero")
	}
}

func TestFilesNormalizeKeepsPathsInsideWorkspaceRoot(t *testing.T) {
	t.Parallel()

	mgr := newWorkspaceFilesManager(t)

	norm, err := mgr.Normalize("dir/../go.mod")
	if err != nil {
		t.Fatalf("Normalize: %v", err)
	}
	if norm != "go.mod" {
		t.Fatalf("normalize result = %q, want go.mod", norm)
	}

	if _, err := mgr.Normalize("../escape.txt"); err == nil {
		t.Fatal("Normalize outside root succeeded, want error")
	}
}

func newWorkspaceFilesManager(t *testing.T) *WorkspaceFilesManager {
	t.Helper()

	root := t.TempDir()
	mustWriteWorkspaceFile(t, filepath.Join(root, "go.mod"), "module teamd\n")
	mustMkdirAllWorkspace(t, filepath.Join(root, "dir"))
	mustWriteWorkspaceFile(t, filepath.Join(root, "dir", "child.txt"), "child contents\n")
	mustWriteWorkspaceFile(t, filepath.Join(root, "dir", "nested.go"), "package dir\n")

	mgr, err := NewWorkspaceFilesManager(root)
	if err != nil {
		t.Fatalf("NewWorkspaceFilesManager: %v", err)
	}
	return mgr
}

func mustWriteWorkspaceFile(t *testing.T, path, body string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("WriteFile(%q): %v", path, err)
	}
}

func mustMkdirAllWorkspace(t *testing.T, path string) {
	t.Helper()
	if err := os.MkdirAll(path, 0o755); err != nil {
		t.Fatalf("MkdirAll(%q): %v", path, err)
	}
}

func TestFilesSnapshotIncludesFreshModTimeForChangedFile(t *testing.T) {
	t.Parallel()

	mgr := newWorkspaceFilesManager(t)
	node, err := mgr.Stat("go.mod")
	if err != nil {
		t.Fatalf("Stat: %v", err)
	}
	if time.Since(node.ModTime) > 24*time.Hour {
		t.Fatalf("mod time too old: %v", node.ModTime)
	}
}
