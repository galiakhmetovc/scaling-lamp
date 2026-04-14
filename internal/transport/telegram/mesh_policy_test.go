package telegram

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestAdapterMeshCommandShowsCurrentPolicy(t *testing.T) {
	var texts []string

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/bottest-token/sendMessage":
			if err := r.ParseForm(); err != nil {
				t.Fatalf("parse form: %v", err)
			}
			texts = append(texts, r.PostForm.Get("text"))
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{"ok": true, "result": map[string]any{"message_id": 1}})
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/mesh"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if len(texts) != 1 {
		t.Fatalf("expected 1 reply, got %d", len(texts))
	}
	if !strings.Contains(texts[0], "Mesh policy") || !strings.Contains(texts[0], "profile: direct") {
		t.Fatalf("unexpected policy text: %q", texts[0])
	}
}

func TestAdapterMeshModeCommandStoresSessionPolicy(t *testing.T) {
	var texts []string

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/bottest-token/sendMessage":
			if err := r.ParseForm(); err != nil {
				t.Fatalf("parse form: %v", err)
			}
			texts = append(texts, r.PostForm.Get("text"))
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{"ok": true, "result": map[string]any{"message_id": 1}})
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/mesh mode deep"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if len(texts) != 1 || !strings.Contains(texts[0], "profile: deep") {
		t.Fatalf("unexpected mode reply: %#v", texts)
	}

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/mesh"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if len(texts) != 2 || !strings.Contains(texts[1], "profile: deep") {
		t.Fatalf("expected persisted deep policy, got %#v", texts)
	}
}

func TestAdapterMeshSetCommandOverridesSessionPolicy(t *testing.T) {
	var texts []string

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/bottest-token/sendMessage":
			if err := r.ParseForm(); err != nil {
				t.Fatalf("parse form: %v", err)
			}
			texts = append(texts, r.PostForm.Get("text"))
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{"ok": true, "result": map[string]any{"message_id": 1}})
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/mesh set sample_k=4"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if len(texts) != 1 || !strings.Contains(texts[0], "sample_k: 1 -> 4") {
		t.Fatalf("unexpected override reply: %#v", texts)
	}

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/mesh"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if len(texts) != 2 || !strings.Contains(texts[1], "sample_k: 4") {
		t.Fatalf("expected overridden policy, got %#v", texts)
	}
}
