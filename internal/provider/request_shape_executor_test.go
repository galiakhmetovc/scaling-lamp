package provider_test

import (
	"encoding/json"
	"testing"

	"teamd/internal/contracts"
	"teamd/internal/provider"
)

func TestRequestShapeExecutorBuildsProviderPayload(t *testing.T) {
	t.Parallel()

	temperature := 0.2
	maxOutputTokens := 2048
	executor := provider.NewRequestShapeExecutor()

	body, err := executor.Build(contracts.RequestShapeContract{
		ID: "request-shape-main",
		Model: contracts.ModelPolicy{
			Enabled:  true,
			Strategy: "static_model",
			Params: contracts.ModelParams{
				Model: "glm-4.6",
			},
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
			Params: contracts.ResponseFormatParams{
				Type: "json_object",
			},
		},
		Streaming: contracts.StreamingPolicy{
			Enabled:  true,
			Strategy: "static_stream",
			Params: contracts.StreamingParams{
				Stream: false,
			},
		},
		Sampling: contracts.SamplingPolicy{
			Enabled:  true,
			Strategy: "static_sampling",
			Params: contracts.SamplingParams{
				Temperature:     &temperature,
				MaxOutputTokens: &maxOutputTokens,
			},
		},
	}, provider.RequestShapeInput{
		PrependPromptAssets: []contracts.Message{
			{Role: "system", Content: "You are terse."},
		},
		Messages: []contracts.Message{
			{Role: "user", Content: "Ping"},
		},
		AppendPromptAssets: []contracts.Message{
			{Role: "system", Content: "Answer with final text only."},
		},
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
		t.Fatalf("Build returned error: %v", err)
	}

	var payload map[string]any
	if err := json.Unmarshal(body, &payload); err != nil {
		t.Fatalf("Unmarshal returned error: %v", err)
	}

	if payload["model"] != "glm-4.6" {
		t.Fatalf("model = %#v", payload["model"])
	}
	if payload["stream"] != false {
		t.Fatalf("stream = %#v", payload["stream"])
	}
	if payload["temperature"] != 0.2 {
		t.Fatalf("temperature = %#v", payload["temperature"])
	}
	if payload["max_output_tokens"] != float64(2048) {
		t.Fatalf("max_output_tokens = %#v", payload["max_output_tokens"])
	}
	messages, ok := payload["messages"].([]any)
	if !ok || len(messages) != 3 {
		t.Fatalf("messages = %#v", payload["messages"])
	}
	firstMessage, ok := messages[0].(map[string]any)
	if !ok || firstMessage["role"] != "system" {
		t.Fatalf("first message = %#v, want system prompt asset", messages[0])
	}
	lastMessage, ok := messages[2].(map[string]any)
	if !ok || lastMessage["content"] != "Answer with final text only." {
		t.Fatalf("last message = %#v, want appended prompt asset", messages[2])
	}
	tools, ok := payload["tools"].([]any)
	if !ok || len(tools) != 1 {
		t.Fatalf("tools = %#v", payload["tools"])
	}
}
