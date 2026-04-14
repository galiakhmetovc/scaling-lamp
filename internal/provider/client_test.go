package provider_test

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"strings"
	"testing"

	"teamd/internal/contracts"
	"teamd/internal/provider"
	"teamd/internal/tools"
)

func TestClientBuildsAndSendsProviderRequest(t *testing.T) {
	t.Setenv("ZAI_API_KEY", "secret-token")

	var captured *http.Request
	client := provider.NewClient(
		provider.NewPromptAssetExecutor(),
		provider.NewRequestShapeExecutor(),
		tools.NewPlanToolExecutor(),
		tools.NewCatalogExecutor(),
		tools.NewExecutionGate(),
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
					Params:   contracts.ModelParams{Model: "glm-4.6"},
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
					Params:   contracts.ResponseFormatParams{Type: "json_object"},
				},
				Streaming: contracts.StreamingPolicy{
					Enabled:  true,
					Strategy: "static_stream",
					Params:   contracts.StreamingParams{Stream: false},
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
		Tools: contracts.ToolContract{
			Catalog: contracts.ToolCatalogPolicy{
				Enabled:  true,
				Strategy: "static_allowlist",
				Params: contracts.ToolCatalogParams{
					ToolIDs: []string{"list_dir", "init_plan"},
				},
			},
			Serialization: contracts.ToolSerializationPolicy{
				Enabled:  true,
				Strategy: "openai_function_tools",
				Params: contracts.ToolSerializationParams{
					IncludeDescriptions: true,
				},
			},
		},
		PlanTools: contracts.PlanToolContract{
			PlanTool: contracts.PlanToolPolicy{
				Enabled:  true,
				Strategy: "default_plan_tools",
				Params: contracts.PlanToolParams{
					ToolIDs: []string{"init_plan"},
				},
			},
		},
	}, provider.ClientInput{
		PromptAssetSelection: []string{"system-core", "tail-guard"},
		Messages:             []contracts.Message{{Role: "user", Content: "Ping"}},
		Tools: []tools.Definition{{
			ID:   "list_dir",
			Name: "list_dir",
			Parameters: map[string]any{
				"type": "object",
			},
		}},
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
	toolsPayload, ok := payload["tools"].([]any)
	if !ok || len(toolsPayload) != 2 {
		t.Fatalf("tools payload = %#v", payload["tools"])
	}
}

func TestClientStreamsTypedTextAndReasoningEvents(t *testing.T) {
	t.Setenv("ZAI_API_KEY", "secret-token")

	client := provider.NewClient(
		provider.NewPromptAssetExecutor(),
		provider.NewRequestShapeExecutor(),
		tools.NewPlanToolExecutor(),
		tools.NewCatalogExecutor(),
		tools.NewExecutionGate(),
		provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"text/event-stream"}},
					Body: io.NopCloser(bytes.NewBufferString(strings.Join([]string{
						`data: {"id":"resp-1","model":"glm-5-turbo","choices":[{"delta":{"role":"assistant","reasoning_content":"Thinking...","content":"Po"},"finish_reason":""}]}`,
						"",
						`data: {"choices":[{"delta":{"content":"ng"},"finish_reason":""}]}`,
						"",
						`data: {"output_text":"!" ,"usage":{"prompt_tokens":12,"completion_tokens":4,"total_tokens":16}}`,
						"",
						"data: [DONE]",
						"",
					}, "\n"))),
				}, nil
			},
		}),
	)

	var got []provider.StreamEvent
	result, err := client.Execute(context.Background(), contracts.ResolvedContracts{
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
				Model:     contracts.ModelPolicy{Enabled: true, Strategy: "static_model", Params: contracts.ModelParams{Model: "glm-5-turbo"}},
				Messages:  contracts.MessagePolicy{Enabled: true, Strategy: "raw_messages"},
				Tools:     contracts.ToolPolicy{Enabled: true, Strategy: "tools_inline"},
				Streaming: contracts.StreamingPolicy{Enabled: true, Strategy: "static_stream", Params: contracts.StreamingParams{Stream: true}},
			},
		},
		PromptAssets: contracts.PromptAssetsContract{
			PromptAsset: contracts.PromptAssetPolicy{Enabled: true, Strategy: "inline_assets"},
		},
	}, provider.ClientInput{
		Messages: []contracts.Message{{Role: "user", Content: "Ping"}},
		StreamObserver: func(event provider.StreamEvent) {
			got = append(got, event)
		},
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}

	if result.Provider.Message.Content != "Pong!" {
		t.Fatalf("provider content = %q, want Pong!", result.Provider.Message.Content)
	}
	if result.Provider.Usage.TotalTokens != 16 {
		t.Fatalf("usage total = %d, want 16", result.Provider.Usage.TotalTokens)
	}
	if len(got) != 4 {
		t.Fatalf("event count = %d, want 4", len(got))
	}
	var (
		sawReasoning bool
		texts        []string
	)
	for _, event := range got {
		switch event.Kind {
		case provider.StreamEventReasoning:
			if event.Text == "Thinking..." {
				sawReasoning = true
			}
		case provider.StreamEventText:
			texts = append(texts, event.Text)
		}
	}
	if !sawReasoning {
		t.Fatalf("reasoning event not found in %#v", got)
	}
	if strings.Join(texts, "") != "Pong!" {
		t.Fatalf("text events = %#v, want Pong!", texts)
	}
}

func TestClientReturnsProviderStatusError(t *testing.T) {
	t.Setenv("ZAI_API_KEY", "secret-token")

	client := provider.NewClient(
		provider.NewPromptAssetExecutor(),
		provider.NewRequestShapeExecutor(),
		tools.NewPlanToolExecutor(),
		tools.NewCatalogExecutor(),
		tools.NewExecutionGate(),
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
					Params:   contracts.ModelParams{Model: "glm-4.6"},
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

func TestClientRejectsProviderToolCallThroughExecutionGate(t *testing.T) {
	t.Setenv("ZAI_API_KEY", "secret-token")

	client := provider.NewClient(
		provider.NewPromptAssetExecutor(),
		provider.NewRequestShapeExecutor(),
		tools.NewPlanToolExecutor(),
		tools.NewCatalogExecutor(),
		tools.NewExecutionGate(),
		provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"application/json"}},
					Body: io.NopCloser(bytes.NewBufferString(`{
  "id":"resp-tools-1",
  "model":"glm-5-turbo",
  "choices":[
    {
      "finish_reason":"tool_calls",
      "message":{
        "role":"assistant",
        "content":"",
        "tool_calls":[
          {
            "id":"call-1",
            "function":{
              "name":"shell.exec",
              "arguments":{"command":"pwd"}
            }
          }
        ]
      }
    }
  ],
  "usage":{"prompt_tokens":8,"completion_tokens":2,"total_tokens":10}
}`)),
				}, nil
			},
		}),
	)

	result, err := client.Execute(context.Background(), contracts.ResolvedContracts{
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
				Model:    contracts.ModelPolicy{Enabled: true, Strategy: "static_model", Params: contracts.ModelParams{Model: "glm-5-turbo"}},
				Messages: contracts.MessagePolicy{Enabled: true, Strategy: "raw_messages"},
			},
		},
		ToolExecution: contracts.ToolExecutionContract{
			Access: contracts.ToolAccessPolicy{
				Enabled:  true,
				Strategy: "deny_all",
			},
		},
	}, provider.ClientInput{
		Messages: []contracts.Message{{Role: "user", Content: "run pwd"}},
	})
	if err == nil {
		t.Fatal("Execute error = nil, want denied tool call error")
	}
	if !strings.Contains(err.Error(), `tool call "shell.exec" denied`) {
		t.Fatalf("Execute error = %q", err)
	}
	if len(result.ToolDecisions) != 1 {
		t.Fatalf("tool decisions len = %d, want 1", len(result.ToolDecisions))
	}
	if result.ToolDecisions[0].Decision.Allowed {
		t.Fatalf("tool decision = %#v, want denied", result.ToolDecisions[0])
	}
}

func TestClientReturnsAllowedProviderToolCallsForRuntimeExecution(t *testing.T) {
	t.Setenv("ZAI_API_KEY", "secret-token")

	client := provider.NewClient(
		provider.NewPromptAssetExecutor(),
		provider.NewRequestShapeExecutor(),
		tools.NewPlanToolExecutor(),
		tools.NewCatalogExecutor(),
		tools.NewExecutionGate(),
		provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"application/json"}},
					Body: io.NopCloser(bytes.NewBufferString(`{
  "id":"resp-tools-1",
  "model":"glm-5-turbo",
  "choices":[
    {
      "finish_reason":"tool_calls",
      "message":{
        "role":"assistant",
        "content":"",
        "tool_calls":[
          {
            "id":"call-1",
            "function":{
              "name":"init_plan",
              "arguments":{"goal":"Refactor auth"}
            }
          }
        ]
      }
    }
  ],
  "usage":{"prompt_tokens":8,"completion_tokens":2,"total_tokens":10}
}`)),
				}, nil
			},
		}),
	)

	result, err := client.Execute(context.Background(), contracts.ResolvedContracts{
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
				Model:    contracts.ModelPolicy{Enabled: true, Strategy: "static_model", Params: contracts.ModelParams{Model: "glm-5-turbo"}},
				Messages: contracts.MessagePolicy{Enabled: true, Strategy: "raw_messages"},
			},
		},
		ToolExecution: contracts.ToolExecutionContract{
			Access: contracts.ToolAccessPolicy{
				Enabled:  true,
				Strategy: "static_allowlist",
				Params: contracts.ToolAccessParams{
					ToolIDs: []string{"init_plan"},
				},
			},
			Approval: contracts.ToolApprovalPolicy{
				Enabled:  true,
				Strategy: "always_allow",
			},
			Sandbox: contracts.ToolSandboxPolicy{
				Enabled:  true,
				Strategy: "default_runtime",
			},
		},
	}, provider.ClientInput{
		Messages: []contracts.Message{{Role: "user", Content: "plan this"}},
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if len(result.Provider.ToolCalls) != 1 {
		t.Fatalf("provider tool calls len = %d, want 1", len(result.Provider.ToolCalls))
	}
	if result.Provider.ToolCalls[0].Name != "init_plan" {
		t.Fatalf("tool call name = %q, want init_plan", result.Provider.ToolCalls[0].Name)
	}
	if len(result.ToolDecisions) != 1 || !result.ToolDecisions[0].Decision.Allowed {
		t.Fatalf("tool decisions = %#v, want one allowed decision", result.ToolDecisions)
	}
}

func TestClientStreamsOpenAICompatibleResponse(t *testing.T) {
	t.Setenv("ZAI_API_KEY", "secret-token")

	var deltas []string
	client := provider.NewClient(
		provider.NewPromptAssetExecutor(),
		provider.NewRequestShapeExecutor(),
		tools.NewPlanToolExecutor(),
		tools.NewCatalogExecutor(),
		tools.NewExecutionGate(),
		provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"text/event-stream"}},
					Body: io.NopCloser(bytes.NewBufferString(strings.Join([]string{
						"data: {\"id\":\"resp-1\",\"model\":\"glm-5-turbo\",\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"Po\"},\"finish_reason\":\"\"}]}",
						"",
						"data: {\"id\":\"resp-1\",\"model\":\"glm-5-turbo\",\"choices\":[{\"delta\":{\"content\":\"ng\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3,\"total_tokens\":15}}",
						"",
						"data: [DONE]",
						"",
					}, "\n"))),
				}, nil
			},
		}),
	)

	result, err := client.Execute(context.Background(), contracts.ResolvedContracts{
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
					Params:   contracts.ModelParams{Model: "glm-5-turbo"},
				},
				Messages:  contracts.MessagePolicy{Enabled: true, Strategy: "raw_messages"},
				Tools:     contracts.ToolPolicy{Enabled: true, Strategy: "tools_inline"},
				Streaming: contracts.StreamingPolicy{Enabled: true, Strategy: "static_stream", Params: contracts.StreamingParams{Stream: true}},
			},
		},
		PromptAssets: contracts.PromptAssetsContract{
			PromptAsset: contracts.PromptAssetPolicy{
				Enabled: true, Strategy: "inline_assets",
			},
		},
	}, provider.ClientInput{
		Messages: []contracts.Message{{Role: "user", Content: "Ping"}},
		StreamObserver: func(event provider.StreamEvent) {
			if event.Kind == provider.StreamEventText {
				deltas = append(deltas, event.Text)
			}
		},
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if result.Provider.Message.Content != "Pong" {
		t.Fatalf("provider message = %q, want Pong", result.Provider.Message.Content)
	}
	if len(deltas) != 2 || deltas[0] != "Po" || deltas[1] != "ng" {
		t.Fatalf("deltas = %#v, want [Po ng]", deltas)
	}
	if result.Provider.Usage.TotalTokens != 15 {
		t.Fatalf("usage total = %d, want 15", result.Provider.Usage.TotalTokens)
	}
}

func TestClientStreamsToolCallsAndReturnsAllowedDecisions(t *testing.T) {
	t.Setenv("ZAI_API_KEY", "secret-token")

	client := provider.NewClient(
		provider.NewPromptAssetExecutor(),
		provider.NewRequestShapeExecutor(),
		tools.NewPlanToolExecutor(),
		tools.NewCatalogExecutor(),
		tools.NewExecutionGate(),
		provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"text/event-stream"}},
					Body: io.NopCloser(bytes.NewBufferString(strings.Join([]string{
						`data: {"id":"resp-1","model":"glm-5-turbo","choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"id":"call-1","index":0,"type":"function","function":{"name":"init_plan","arguments":"{\"goal\":\"Refactor auth\"}"}}]}}]}`,
						"",
						`data: {"id":"resp-1","model":"glm-5-turbo","choices":[{"index":0,"finish_reason":"tool_calls","delta":{"content":""}}],"usage":{"prompt_tokens":12,"completion_tokens":3,"total_tokens":15}}`,
						"",
						`data: [DONE]`,
						"",
					}, "\n"))),
				}, nil
			},
		}),
	)

	result, err := client.Execute(context.Background(), contracts.ResolvedContracts{
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
				Model:     contracts.ModelPolicy{Enabled: true, Strategy: "static_model", Params: contracts.ModelParams{Model: "glm-5-turbo"}},
				Messages:  contracts.MessagePolicy{Enabled: true, Strategy: "raw_messages"},
				Tools:     contracts.ToolPolicy{Enabled: true, Strategy: "tools_inline"},
				Streaming: contracts.StreamingPolicy{Enabled: true, Strategy: "static_stream", Params: contracts.StreamingParams{Stream: true}},
			},
		},
		ToolExecution: contracts.ToolExecutionContract{
			Access: contracts.ToolAccessPolicy{
				Enabled:  true,
				Strategy: "static_allowlist",
				Params: contracts.ToolAccessParams{
					ToolIDs: []string{"init_plan"},
				},
			},
			Approval: contracts.ToolApprovalPolicy{
				Enabled:  true,
				Strategy: "always_allow",
			},
			Sandbox: contracts.ToolSandboxPolicy{
				Enabled:  true,
				Strategy: "default_runtime",
			},
		},
	}, provider.ClientInput{
		Messages: []contracts.Message{{Role: "user", Content: "plan this"}},
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if result.Provider.FinishReason != "tool_calls" {
		t.Fatalf("finish reason = %q, want tool_calls", result.Provider.FinishReason)
	}
	if len(result.Provider.ToolCalls) != 1 {
		t.Fatalf("provider tool calls len = %d, want 1", len(result.Provider.ToolCalls))
	}
	if result.Provider.ToolCalls[0].Name != "init_plan" {
		t.Fatalf("tool call name = %q, want init_plan", result.Provider.ToolCalls[0].Name)
	}
	if goal, _ := result.Provider.ToolCalls[0].Arguments["goal"].(string); goal != "Refactor auth" {
		t.Fatalf("tool call arguments = %#v", result.Provider.ToolCalls[0].Arguments)
	}
	if len(result.ToolDecisions) != 1 || !result.ToolDecisions[0].Decision.Allowed {
		t.Fatalf("tool decisions = %#v", result.ToolDecisions)
	}
}
