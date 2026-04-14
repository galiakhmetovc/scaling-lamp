package provider_test

import (
	"bytes"
	"context"
	"io"
	"net/http"
	"testing"

	"teamd/internal/contracts"
	"teamd/internal/provider"
)

func TestClientBuildsAndSendsProviderRequest(t *testing.T) {
	t.Setenv("ZAI_API_KEY", "secret-token")

	var captured *http.Request
	client := provider.NewClient(
		provider.NewRequestShapeExecutor(),
		provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				captured = req
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"X-Test": []string{"ok"}},
					Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-1"}`)),
				}, nil
			},
		}),
	)

	temperature := 0.2
	maxOutputTokens := 2048
	result, err := client.Execute(context.Background(), contracts.ProviderRequestContract{
		Transport: contracts.TransportContract{
			ID: "transport-main",
			Endpoint: contracts.EndpointPolicy{
				Enabled:  true,
				Strategy: "static",
				Params: contracts.EndpointParams{
					BaseURL: "https://api.z.ai",
					Path:    "/api/paas/v4/chat/completions",
					Method:  http.MethodPost,
				},
			},
			Auth: contracts.AuthPolicy{
				Enabled:  true,
				Strategy: "bearer_token",
				Params: contracts.AuthParams{
					Header:      "Authorization",
					Prefix:      "Bearer",
					ValueEnvVar: "ZAI_API_KEY",
				},
			},
		},
		RequestShape: contracts.RequestShapeContract{
			ID: "request-shape-main",
			Model: contracts.ModelPolicy{
				Enabled:  true,
				Strategy: "static_model",
				Params: contracts.ModelParams{Model: "glm-4.6"},
			},
			Messages: contracts.MessagePolicy{
				Enabled:  true,
				Strategy: "raw_messages",
			},
			Tools: contracts.ToolPolicy{
				Enabled:  true,
				Strategy: "tools_inline",
			},
			ResponseFormat: contracts.ResponseFormatPolicy{
				Enabled:  true,
				Strategy: "default",
				Params: contracts.ResponseFormatParams{Type: "json_object"},
			},
			Streaming: contracts.StreamingPolicy{
				Enabled:  true,
				Strategy: "static_stream",
				Params: contracts.StreamingParams{Stream: false},
			},
			Sampling: contracts.SamplingPolicy{
				Enabled:  true,
				Strategy: "static_sampling",
				Params: contracts.SamplingParams{
					Temperature:     &temperature,
					MaxOutputTokens: &maxOutputTokens,
				},
			},
		},
	}, provider.ClientInput{
		PromptAssets: []contracts.Message{{Role: "system", Content: "You are terse."}},
		Messages:     []contracts.Message{{Role: "user", Content: "Ping"}},
		Tools: []map[string]any{
			{
				"type": "function",
				"function": map[string]any{
					"name": "list_dir",
				},
			},
		},
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}

	if captured == nil {
		t.Fatal("captured request is nil")
	}
	if got := captured.Header.Get("Content-Type"); got != "application/json" {
		t.Fatalf("content-type = %q, want application/json", got)
	}
	if got := captured.Header.Get("Authorization"); got != "Bearer secret-token" {
		t.Fatalf("authorization = %q", got)
	}
	if got := captured.URL.String(); got != "https://api.z.ai/api/paas/v4/chat/completions" {
		t.Fatalf("url = %q", got)
	}
	if result.Transport.StatusCode != http.StatusOK {
		t.Fatalf("status = %d, want 200", result.Transport.StatusCode)
	}
	if string(result.Transport.Body) != `{"id":"resp-1"}` {
		t.Fatalf("body = %q", string(result.Transport.Body))
	}
	if len(result.RequestBody) == 0 {
		t.Fatal("request body is empty")
	}
}

