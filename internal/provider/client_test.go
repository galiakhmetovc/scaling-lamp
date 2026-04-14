package provider_test

import (
	"bytes"
	"context"
	"encoding/json"
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
		provider.NewPromptAssetExecutor(),
		provider.NewRequestShapeExecutor(),
		provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				captured = req
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"X-Test": []string{"ok"}},
					Body: io.NopCloser(bytes.NewBufferString(`{
  "id":"resp-1",
  "model":"glm-4.6",
  "choices":[
    {
      "index":0,
      "finish_reason":"stop",
      "message":{"role":"assistant","content":"Pong"}
    }
  ],
  "usage":{
    "prompt_tokens":12,
    "completion_tokens":3,
    "total_tokens":15
  }
}`)),
				}, nil
			},
		}),
	)

	temperature := 0.2
	maxOutputTokens := 2048
	result, err := client.Execute(context.Background(), contracts.ResolvedContracts{
		ProviderRequest: contracts.ProviderRequestContract{
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
		},
		PromptAssets: contracts.PromptAssetsContract{
			ID: "prompt-assets-main",
			PromptAsset: contracts.PromptAssetPolicy{
				Enabled:  true,
				Strategy: "inline_assets",
				Params: contracts.PromptAssetParams{
					Assets: []contracts.PromptAsset{
						{ID: "system-core", Role: "system", Content: "You are terse.", Placement: "prepend"},
						{ID: "tail-guard", Role: "system", Content: "Answer with final text only.", Placement: "append"},
					},
				},
			},
		},
	}, provider.ClientInput{
		PromptAssetSelection: []string{"system-core", "tail-guard"},
		Messages:             []contracts.Message{{Role: "user", Content: "Ping"}},
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
	if result.Provider.ID != "resp-1" {
		t.Fatalf("provider id = %q, want resp-1", result.Provider.ID)
	}
	if result.Provider.Model != "glm-4.6" {
		t.Fatalf("provider model = %q, want glm-4.6", result.Provider.Model)
	}
	if result.Provider.Message.Role != "assistant" || result.Provider.Message.Content != "Pong" {
		t.Fatalf("provider message = %#v, want assistant Pong", result.Provider.Message)
	}
	if result.Provider.FinishReason != "stop" {
		t.Fatalf("finish reason = %q, want stop", result.Provider.FinishReason)
	}
	if result.Provider.Usage.InputTokens != 12 || result.Provider.Usage.OutputTokens != 3 || result.Provider.Usage.TotalTokens != 15 {
		t.Fatalf("provider usage = %#v, want 12/3/15", result.Provider.Usage)
	}
	if len(result.RequestBody) == 0 {
		t.Fatal("request body is empty")
	}

	var payload map[string]any
	if err := json.Unmarshal(result.RequestBody, &payload); err != nil {
		t.Fatalf("Unmarshal returned error: %v", err)
	}
	messages, ok := payload["messages"].([]any)
	if !ok || len(messages) != 3 {
		t.Fatalf("messages = %#v", payload["messages"])
	}
	firstMessage, ok := messages[0].(map[string]any)
	if !ok || firstMessage["content"] != "You are terse." {
		t.Fatalf("first message = %#v, want prepended prompt asset", messages[0])
	}
	lastMessage, ok := messages[2].(map[string]any)
	if !ok || lastMessage["content"] != "Answer with final text only." {
		t.Fatalf("last message = %#v, want appended prompt asset", messages[2])
	}
}

func TestClientReturnsProviderStatusError(t *testing.T) {
	t.Setenv("ZAI_API_KEY", "secret-token")

	client := provider.NewClient(
		provider.NewPromptAssetExecutor(),
		provider.NewRequestShapeExecutor(),
		provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				return &http.Response{
					StatusCode: http.StatusTooManyRequests,
					Body:       io.NopCloser(bytes.NewBufferString(`{"error":"rate limited"}`)),
					Header:     http.Header{},
				}, nil
			},
		}),
	)

	_, err := client.Execute(context.Background(), contracts.ResolvedContracts{
		ProviderRequest: contracts.ProviderRequestContract{
			Transport: contracts.TransportContract{
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
				Model: contracts.ModelPolicy{
					Enabled:  true,
					Strategy: "static_model",
					Params: contracts.ModelParams{Model: "glm-4.6"},
				},
				Messages: contracts.MessagePolicy{
					Enabled:  true,
					Strategy: "raw_messages",
				},
			},
		},
	}, provider.ClientInput{
		Messages: []contracts.Message{{Role: "user", Content: "Ping"}},
	})
	if err == nil {
		t.Fatal("Execute error = nil, want provider status error")
	}
	if got := err.Error(); got != `provider returned status 429: {"error":"rate limited"}` {
		t.Fatalf("Execute error = %q", got)
	}
}
