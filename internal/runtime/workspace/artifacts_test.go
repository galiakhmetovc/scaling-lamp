package workspace

import (
	"context"
	"path/filepath"
	"testing"

	"teamd/internal/artifacts"
)

func TestArtifactsSnapshotListsNewestFirst(t *testing.T) {
	t.Parallel()

	mgr := newWorkspaceArtifactsManager(t)

	snap, err := mgr.Snapshot("session-1")
	if err != nil {
		t.Fatalf("Snapshot: %v", err)
	}
	if snap.SessionID != "session-1" {
		t.Fatalf("session_id = %q, want session-1", snap.SessionID)
	}
	if snap.RootPath == "" {
		t.Fatal("root path is empty")
	}
	if got := len(snap.Items); got != 2 {
		t.Fatalf("items len = %d, want 2", got)
	}
	if snap.Items[0].Ref == snap.Items[1].Ref {
		t.Fatalf("artifact refs are duplicated: %#v", snap.Items)
	}
	if snap.Items[0].CreatedAt.Before(snap.Items[1].CreatedAt) {
		t.Fatalf("artifacts not sorted newest-first: %#v", snap.Items)
	}
}

func TestArtifactsSnapshotOpensFirstArtifactContent(t *testing.T) {
	t.Parallel()

	mgr := newWorkspaceArtifactsManager(t)

	snap, err := mgr.Snapshot("session-1")
	if err != nil {
		t.Fatalf("Snapshot: %v", err)
	}
	if snap.SelectedRef == "" {
		t.Fatal("selected_ref is empty")
	}
	if snap.Content == "" {
		t.Fatal("content is empty")
	}
}

func TestArtifactsOpenReturnsRawContent(t *testing.T) {
	t.Parallel()

	mgr := newWorkspaceArtifactsManager(t)
	snap, err := mgr.Snapshot("session-1")
	if err != nil {
		t.Fatalf("Snapshot: %v", err)
	}

	opened, err := mgr.Open("session-1", snap.Items[1].Ref)
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	if opened.SelectedRef != snap.Items[1].Ref {
		t.Fatalf("selected_ref = %q, want %q", opened.SelectedRef, snap.Items[1].Ref)
	}
	if opened.Content == "" {
		t.Fatal("opened content is empty")
	}
}

func newWorkspaceArtifactsManager(t *testing.T) *WorkspaceArtifactsManager {
	t.Helper()

	root := filepath.Join(t.TempDir(), "artifacts")
	store, err := artifacts.NewStore(root)
	if err != nil {
		t.Fatalf("NewStore: %v", err)
	}
	if _, err := store.Write(context.Background(), "fs_read_lines", "alpha\nbeta\ngamma\n", 32); err != nil {
		t.Fatalf("Write first artifact: %v", err)
	}
	if _, err := store.Write(context.Background(), "shell_exec", "one\ntwo\nthree\n", 32); err != nil {
		t.Fatalf("Write second artifact: %v", err)
	}

	mgr, err := NewWorkspaceArtifactsManager(root)
	if err != nil {
		t.Fatalf("NewWorkspaceArtifactsManager: %v", err)
	}
	return mgr
}
