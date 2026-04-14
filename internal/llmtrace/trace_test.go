package llmtrace

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"teamd/internal/provider"
)

func TestCollectorWritesTraceFile(t *testing.T) {
	collector := NewCollector(RunMeta{
		RunID: "run-1",
		Query: "hello",
	})

	req := provider.PromptRequest{
		WorkerID: "telegram:1",
		Messages: []provider.Message{{Role: "user", Content: "hello"}},
		Config: provider.RequestConfig{
			Model: "glm-5-turbo",
		},
	}
	ctx := WithCollector(context.Background(), collector)
	ctx, call := StartCall(ctx, req)
	if call == nil {
		t.Fatal("expected call recorder")
	}
	call.RecordProviderHTTP(
		"https://api.z.ai/api/coding/paas/v4/chat/completions",
		map[string][]string{"Authorization": {"Bearer test-key"}, "Content-Type": {"application/json"}},
		map[string][]string{"Content-Type": {"application/json"}},
		[]byte(`{"model":"glm-5-turbo"}`),
		[]byte(`{"choices":[{"message":{"content":"hi"}}]}`),
		200,
	)
	call.Finish(provider.PromptResponse{
		Text:  "hi",
		Model: "glm-5-turbo",
	}, nil)

	dir := t.TempDir()
	path, err := collector.WriteFile(dir)
	if err != nil {
		t.Fatalf("write trace: %v", err)
	}
	if filepath.Dir(path) != dir {
		t.Fatalf("unexpected dir: %s", path)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read trace: %v", err)
	}

	var trace FileTrace
	if err := json.Unmarshal(data, &trace); err != nil {
		t.Fatalf("unmarshal trace: %v", err)
	}
	if trace.Query != "hello" {
		t.Fatalf("unexpected query: %#v", trace.Query)
	}
	if len(trace.Calls) != 1 {
		t.Fatalf("unexpected calls: %#v", trace.Calls)
	}
	if trace.Calls[0].ProviderRequestBody != "{\"model\":\"glm-5-turbo\"}" {
		t.Fatalf("unexpected provider request body: %#v", trace.Calls[0].ProviderRequestBody)
	}
	if trace.Calls[0].ProviderRequestHeaders["Authorization"][0] != "Bearer test-key" {
		t.Fatalf("unexpected provider request headers: %#v", trace.Calls[0].ProviderRequestHeaders)
	}
	if trace.Calls[0].ProviderResponseBody != "{\"choices\":[{\"message\":{\"content\":\"hi\"}}]}" {
		t.Fatalf("unexpected provider response body: %#v", trace.Calls[0].ProviderResponseBody)
	}
	if trace.Calls[0].ProviderResponseHeaders["Content-Type"][0] != "application/json" {
		t.Fatalf("unexpected provider response headers: %#v", trace.Calls[0].ProviderResponseHeaders)
	}
}
