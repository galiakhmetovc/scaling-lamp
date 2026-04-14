package artifacts

import (
	"testing"
)

func TestFilesystemStoreSaveGetAndSearch(t *testing.T) {
	root := t.TempDir()
	store, err := NewFilesystemStore(root)
	if err != nil {
		t.Fatalf("new filesystem store: %v", err)
	}

	ref, err := store.Save("run", "run-1", "report.txt", []byte("alpha\nbeta"))
	if err != nil {
		t.Fatalf("save: %v", err)
	}
	if ref == "" {
		t.Fatal("expected artifact ref")
	}

	item, ok, err := store.Get(ref)
	if err != nil {
		t.Fatalf("get: %v", err)
	}
	if !ok {
		t.Fatal("expected stored artifact")
	}
	if item.OwnerType != "run" || item.OwnerID != "run-1" || string(item.Payload) != "alpha\nbeta" {
		t.Fatalf("unexpected artifact: %+v", item)
	}

	results, err := store.Search(SearchQuery{OwnerType: "run", OwnerID: "run-1", Query: "beta", Limit: 5})
	if err != nil {
		t.Fatalf("search: %v", err)
	}
	if len(results) != 1 || results[0].Ref != ref {
		t.Fatalf("unexpected search results: %+v", results)
	}
}
