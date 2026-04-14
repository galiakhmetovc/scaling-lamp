package runtime

import (
	"strings"
	"testing"

	"teamd/internal/artifacts"
)

func TestArtifactOffloadKeepsSmallOutputInline(t *testing.T) {
	store := artifacts.NewInMemoryStore()
	out, err := MaybeOffloadToolResult(store, ArtifactOffloadPolicy{
		MaxInlineChars: 100,
		MaxInlineLines: 10,
		PreviewLines:   3,
	}, ArtifactOwnerRef{OwnerType: "run", OwnerID: "run-1"}, "shell.exec", "short output")
	if err != nil {
		t.Fatalf("MaybeOffloadToolResult: %v", err)
	}
	if out.Offloaded {
		t.Fatalf("expected inline result, got offloaded %+v", out)
	}
	if out.Content != "short output" {
		t.Fatalf("unexpected inline content: %q", out.Content)
	}
}

func TestArtifactOffloadPersistsLargeOutputAndReturnsPreview(t *testing.T) {
	store := artifacts.NewInMemoryStore()
	large := "line1\nline2\nline3\nline4\nline5"
	out, err := MaybeOffloadToolResult(store, ArtifactOffloadPolicy{
		MaxInlineChars: 12,
		MaxInlineLines: 3,
		PreviewLines:   2,
	}, ArtifactOwnerRef{OwnerType: "run", OwnerID: "run-1"}, "shell.exec", large)
	if err != nil {
		t.Fatalf("MaybeOffloadToolResult: %v", err)
	}
	if !out.Offloaded || out.ArtifactRef == "" {
		t.Fatalf("expected offloaded result, got %+v", out)
	}
	if !strings.Contains(out.Content, "artifact_ref: "+out.ArtifactRef) {
		t.Fatalf("expected artifact ref in preview, got %q", out.Content)
	}
	if !strings.Contains(out.Content, "line1") || !strings.Contains(out.Content, "line2") {
		t.Fatalf("expected preview lines in content, got %q", out.Content)
	}
	item, ok, err := store.Get(out.ArtifactRef)
	if err != nil || !ok {
		t.Fatalf("artifact lookup failed: ok=%v err=%v", ok, err)
	}
	if string(item.Payload) != large {
		t.Fatalf("unexpected persisted payload: %q", string(item.Payload))
	}
}

