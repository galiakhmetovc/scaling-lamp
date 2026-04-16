package artifacts_test

import (
	"context"
	"path/filepath"
	"strings"
	"testing"

	"teamd/internal/artifacts"
)

func TestStoreWriteReadAndSearch(t *testing.T) {
	t.Parallel()

	store, err := artifacts.NewStore(filepath.Join(t.TempDir(), "artifacts"))
	if err != nil {
		t.Fatalf("NewStore returned error: %v", err)
	}
	record, err := store.Write(context.Background(), "fs_read_text", "alpha\nbeta\ngamma\n", 10)
	if err != nil {
		t.Fatalf("Write returned error: %v", err)
	}
	if record.Ref == "" || !strings.HasPrefix(record.Ref, "artifact://") {
		t.Fatalf("record ref = %q, want artifact:// prefix", record.Ref)
	}
	_, content, err := store.Read(context.Background(), record.Ref)
	if err != nil {
		t.Fatalf("Read returned error: %v", err)
	}
	if content != "alpha\nbeta\ngamma\n" {
		t.Fatalf("content = %q, want full artifact body", content)
	}
	results, err := store.Search(context.Background(), "beta", 5)
	if err != nil {
		t.Fatalf("Search returned error: %v", err)
	}
	if len(results) != 1 || results[0].Ref != record.Ref {
		t.Fatalf("search results = %#v, want single matching record", results)
	}
}
