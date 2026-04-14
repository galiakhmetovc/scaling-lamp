package memory

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestOllamaEmbedderEmbedParsesVector(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/embed" {
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"model":"nomic-embed-text:latest","embeddings":[[0.1,0.2,0.3]]}`))
	}))
	defer server.Close()

	embedder := NewOllamaEmbedder(server.URL, "nomic-embed-text:latest")
	embedder.HTTPClient = server.Client()

	vec, err := embedder.Embed(context.Background(), "hello")
	if err != nil {
		t.Fatalf("embed: %v", err)
	}
	if len(vec) != 3 {
		t.Fatalf("unexpected vector length: %d", len(vec))
	}
	if vec[0] != float32(0.1) || vec[2] != float32(0.3) {
		t.Fatalf("unexpected vector: %#v", vec)
	}
}
