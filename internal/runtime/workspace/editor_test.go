package workspace

import (
	"os"
	"path/filepath"
	"testing"
)

func TestEditorOpenFileBuffer(t *testing.T) {
	t.Parallel()

	mgr := newWorkspaceEditorManager(t)

	buf, err := mgr.Open("session-1", "notes.txt")
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	if buf.SessionID != "session-1" {
		t.Fatalf("session_id = %q, want session-1", buf.SessionID)
	}
	if buf.Path != "notes.txt" {
		t.Fatalf("path = %q, want notes.txt", buf.Path)
	}
	if buf.Content != "hello\n" {
		t.Fatalf("content = %q, want hello\\n", buf.Content)
	}
	if buf.Dirty {
		t.Fatal("dirty = true, want false")
	}
}

func TestEditorUpdateMarksDirty(t *testing.T) {
	t.Parallel()

	mgr := newWorkspaceEditorManager(t)

	buf, err := mgr.Update("session-1", "notes.txt", "edited\n")
	if err != nil {
		t.Fatalf("Update: %v", err)
	}
	if !buf.Dirty {
		t.Fatal("dirty = false, want true")
	}
	if buf.Content != "edited\n" {
		t.Fatalf("content = %q, want edited\\n", buf.Content)
	}
	if current, ok := mgr.Current("session-1", "notes.txt"); !ok || !current.Dirty {
		t.Fatalf("Current = %#v, %v, want dirty buffer", current, ok)
	}
}

func TestEditorSaveClearsDirtyAndWritesToDisk(t *testing.T) {
	t.Parallel()

	mgr := newWorkspaceEditorManager(t)

	if _, err := mgr.Update("session-1", "notes.txt", "saved\n"); err != nil {
		t.Fatalf("Update: %v", err)
	}
	buf, err := mgr.Save("session-1", "notes.txt")
	if err != nil {
		t.Fatalf("Save: %v", err)
	}
	if buf.Dirty {
		t.Fatal("dirty = true, want false")
	}
	got, err := os.ReadFile(filepath.Join(mgr.root, "notes.txt"))
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(got) != "saved\n" {
		t.Fatalf("disk content = %q, want saved\\n", string(got))
	}
}

func TestEditorReopenReturnsCurrentBufferState(t *testing.T) {
	t.Parallel()

	mgr := newWorkspaceEditorManager(t)

	if _, err := mgr.Update("session-1", "notes.txt", "unsaved\n"); err != nil {
		t.Fatalf("Update: %v", err)
	}
	buf, err := mgr.Open("session-1", "notes.txt")
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	if buf.Content != "unsaved\n" {
		t.Fatalf("content = %q, want unsaved\\n", buf.Content)
	}
	if !buf.Dirty {
		t.Fatal("dirty = false, want true")
	}
}

func newWorkspaceEditorManager(t *testing.T) *WorkspaceEditorManager {
	t.Helper()

	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "notes.txt"), []byte("hello\n"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	mgr, err := NewWorkspaceEditorManager(root)
	if err != nil {
		t.Fatalf("NewWorkspaceEditorManager: %v", err)
	}
	return mgr
}
