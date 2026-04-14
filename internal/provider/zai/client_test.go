package zai

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"teamd/internal/llmtrace"
	"teamd/internal/provider"
)

func TestClientGeneratePostsChatCompletionRequest(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/chat/completions" {
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		if got := r.Header.Get("Authorization"); got != "Bearer test-key" {
			t.Fatalf("unexpected auth header: %s", got)
		}
		if got := r.Header.Get("Content-Type"); !strings.Contains(got, "application/json") {
			t.Fatalf("unexpected content type: %s", got)
		}

		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode request: %v", err)
		}
		if body["model"] != "glm-5-turbo" {
			t.Fatalf("unexpected model: %v", body["model"])
		}
		thinking, ok := body["thinking"].(map[string]any)
		if !ok {
			t.Fatalf("expected thinking payload, got %#v", body["thinking"])
		}
		if thinking["type"] != "enabled" {
			t.Fatalf("unexpected thinking.type: %v", thinking["type"])
		}
		if thinking["clear_thinking"] != true {
			t.Fatalf("unexpected thinking.clear_thinking: %v", thinking["clear_thinking"])
		}

		messages, ok := body["messages"].([]any)
		if !ok || len(messages) != 1 {
			t.Fatalf("unexpected messages: %#v", body["messages"])
		}

		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"hello from zai","reasoning_content":"chain"}}],"usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, "test-key").WithModel("glm-5-turbo").WithThinking("enabled", true)

	resp, err := client.Generate(context.Background(), provider.PromptRequest{
		Messages: []provider.Message{{Role: "user", Content: "hello"}},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp.Text != "hello from zai" {
		t.Fatalf("unexpected response text: %q", resp.Text)
	}
	if resp.Usage.PromptTokens != 11 || resp.Usage.CompletionTokens != 7 || resp.Usage.TotalTokens != 18 {
		t.Fatalf("unexpected usage: %#v", resp.Usage)
	}
	if resp.ReasoningContent != "chain" {
		t.Fatalf("unexpected reasoning content: %q", resp.ReasoningContent)
	}
}

func TestClientGeneratePostsRuntimeOverrides(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode request: %v", err)
		}
		if body["model"] != "glm-4.5" {
			t.Fatalf("unexpected override model: %#v", body["model"])
		}
		thinking, ok := body["thinking"].(map[string]any)
		if !ok {
			t.Fatalf("expected thinking payload, got %#v", body["thinking"])
		}
		if thinking["type"] != "disabled" {
			t.Fatalf("unexpected thinking.type: %#v", thinking["type"])
		}
		if thinking["clear_thinking"] != false {
			t.Fatalf("unexpected thinking.clear_thinking: %#v", thinking["clear_thinking"])
		}
		if body["temperature"] != 0.7 {
			t.Fatalf("unexpected temperature: %#v", body["temperature"])
		}
		if body["top_p"] != 0.9 {
			t.Fatalf("unexpected top_p: %#v", body["top_p"])
		}
		if body["max_tokens"] != float64(512) {
			t.Fatalf("unexpected max_tokens: %#v", body["max_tokens"])
		}
		if body["do_sample"] != true {
			t.Fatalf("unexpected do_sample: %#v", body["do_sample"])
		}
		responseFormat, ok := body["response_format"].(map[string]any)
		if !ok || responseFormat["type"] != "json_object" {
			t.Fatalf("unexpected response_format: %#v", body["response_format"])
		}

		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"ok"}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}`))
	}))
	defer server.Close()

	clear := false
	temp := 0.7
	topP := 0.9
	maxTokens := 512
	doSample := true

	client := NewClient(server.URL, "test-key").WithModel("glm-5-turbo").WithThinking("enabled", true)
	_, err := client.Generate(context.Background(), provider.PromptRequest{
		Messages: []provider.Message{{Role: "user", Content: "hello"}},
		Config: provider.RequestConfig{
			Model:         "glm-4.5",
			ReasoningMode: "disabled",
			ClearThinking: &clear,
			Temperature:   &temp,
			TopP:          &topP,
			MaxTokens:     &maxTokens,
			DoSample:      &doSample,
			ResponseFormat:"json_object",
		},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestClientGenerateReportsRawBodiesToTraceCollector(t *testing.T) {
	collector := llmtrace.NewCollector(llmtrace.RunMeta{RunID: "run-1"})
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"ok"}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, "test-key")
	traced := llmtrace.TracingProvider{Base: client}

	_, err := traced.Generate(llmtrace.WithCollector(context.Background(), collector), provider.PromptRequest{
		Messages: []provider.Message{{Role: "user", Content: "hello"}},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	trace := collector.Snapshot()
	if len(trace.Calls) != 1 {
		t.Fatalf("unexpected calls: %#v", trace.Calls)
	}
	if !strings.Contains(trace.Calls[0].ProviderRequestBody, `"messages":[{"role":"user","content":"hello"}]`) {
		t.Fatalf("unexpected provider request body: %s", trace.Calls[0].ProviderRequestBody)
	}
	if trace.Calls[0].ProviderRequestHeaders["Authorization"][0] != "Bearer test-key" {
		t.Fatalf("unexpected provider request headers: %#v", trace.Calls[0].ProviderRequestHeaders)
	}
	if !strings.Contains(trace.Calls[0].ProviderResponseBody, `"content":"ok"`) {
		t.Fatalf("unexpected provider response body: %s", trace.Calls[0].ProviderResponseBody)
	}
	if !strings.Contains(strings.Join(trace.Calls[0].ProviderResponseHeaders["Content-Type"], ","), "application/json") {
		t.Fatalf("unexpected provider response headers: %#v", trace.Calls[0].ProviderResponseHeaders)
	}
	if trace.Calls[0].ProviderStatusCode != 200 {
		t.Fatalf("unexpected provider status: %d", trace.Calls[0].ProviderStatusCode)
	}
}

func TestClientGenerateReturnsErrorOnNonSuccess(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "bad auth", http.StatusUnauthorized)
	}))
	defer server.Close()

	client := NewClient(server.URL, "bad-key")

	_, err := client.Generate(context.Background(), provider.PromptRequest{
		Messages: []provider.Message{{Role: "user", Content: "hello"}},
	})
	if err == nil {
		t.Fatal("expected non-success response error")
	}
}

func TestClientGenerateRetriesTemporaryFailures(t *testing.T) {
	attempts := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		attempts++
		if attempts < 3 {
			http.Error(w, "retry later", http.StatusBadGateway)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"ok after retry"}}],"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5}}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, "test-key")

	resp, err := client.Generate(context.Background(), provider.PromptRequest{
		Messages: []provider.Message{{Role: "user", Content: "hello"}},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp.Text != "ok after retry" {
		t.Fatalf("unexpected response text: %q", resp.Text)
	}
	if attempts != 3 {
		t.Fatalf("expected 3 attempts, got %d", attempts)
	}
}

func TestClientGenerateAppliesTransportOverrides(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/custom/chat" {
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		if got := r.Header.Get("X-Debug-Mode"); got != "raw" {
			t.Fatalf("unexpected custom header: %q", got)
		}
		if got := r.Header.Get("Authorization"); got != "Token override-secret" {
			t.Fatalf("unexpected auth header: %q", got)
		}

		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"ok"}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}`))
	}))
	defer server.Close()

	client := NewClient("https://unused.example", "base-key")

	_, err := client.Generate(context.Background(), provider.PromptRequest{
		Messages: []provider.Message{{Role: "user", Content: "hello"}},
		Transport: provider.TransportConfig{
			BaseURL: server.URL,
			Path:    "/custom/chat",
			Headers: map[string]string{
				"X-Debug-Mode": "raw",
			},
			Auth: &provider.RequestAuth{
				Header: "Authorization",
				Prefix: "Token",
				Value:  "override-secret",
			},
		},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestClientGenerateAppliesPerRequestTimeout(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		time.Sleep(150 * time.Millisecond)
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"late"}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, "test-key")

	_, err := client.Generate(context.Background(), provider.PromptRequest{
		Messages: []provider.Message{{Role: "user", Content: "hello"}},
		Transport: provider.TransportConfig{
			Timeout: 50 * time.Millisecond,
		},
	})
	if err == nil {
		t.Fatal("expected timeout error")
	}
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("expected context deadline exceeded, got %v", err)
	}
}

func TestClientGeneratePostsStructuredMessages(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode request: %v", err)
		}

		messages, ok := body["messages"].([]any)
		if !ok || len(messages) != 2 {
			t.Fatalf("unexpected messages: %#v", body["messages"])
		}

		first, ok := messages[0].(map[string]any)
		if !ok {
			t.Fatalf("unexpected first message: %#v", messages[0])
		}
		second, ok := messages[1].(map[string]any)
		if !ok {
			t.Fatalf("unexpected second message: %#v", messages[1])
		}

		if first["role"] != "user" || second["role"] != "assistant" {
			t.Fatalf("unexpected roles: %#v", messages)
		}

		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"ok"}}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, "test-key")

	_, err := client.Generate(context.Background(), provider.PromptRequest{
		Messages: []provider.Message{
			{Role: "user", Content: "hello"},
			{Role: "assistant", Content: "hi"},
		},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestClientGeneratePostsToolsAndParsesToolCalls(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode request: %v", err)
		}

		if body["tool_choice"] != "auto" {
			t.Fatalf("unexpected tool_choice: %#v", body["tool_choice"])
		}

		tools, ok := body["tools"].([]any)
		if !ok || len(tools) != 1 {
			t.Fatalf("unexpected tools payload: %#v", body["tools"])
		}

		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{
		  "choices":[
		    {
		      "finish_reason":"tool_calls",
		      "message":{
		        "content":"",
		        "tool_calls":[
		          {
		            "id":"call_1",
		            "type":"function",
		            "function":{
		              "name":"filesystem.read_file",
		              "arguments":{"path":"/tmp/note.txt"}
		            }
		          }
		        ]
		      }
		    }
		  ],
		  "usage":{"prompt_tokens":9,"completion_tokens":3,"total_tokens":12}
		}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, "test-key")

	resp, err := client.Generate(context.Background(), provider.PromptRequest{
		Messages: []provider.Message{{Role: "user", Content: "read the file"}},
		Tools: []provider.ToolDefinition{{
			Name:        "filesystem.read_file",
			Description: "Read a file.",
			Parameters: map[string]any{
				"type": "object",
			},
		}},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp.FinishReason != "tool_calls" {
		t.Fatalf("unexpected finish reason: %q", resp.FinishReason)
	}
	if len(resp.ToolCalls) != 1 {
		t.Fatalf("unexpected tool calls: %#v", resp.ToolCalls)
	}
	if resp.ToolCalls[0].Name != "filesystem.read_file" {
		t.Fatalf("unexpected tool name: %#v", resp.ToolCalls[0])
	}
}

func TestClientGenerateParsesStringifiedToolArguments(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{
		  "choices":[
		    {
		      "finish_reason":"tool_calls",
		      "message":{
		        "tool_calls":[
		          {
		            "id":"call_1",
		            "type":"function",
		            "function":{
		              "name":"filesystem.read_file",
		              "arguments":"{\"path\":\"/etc/hosts\"}"
		            }
		          }
		        ]
		      }
		    }
		  ],
		  "usage":{"prompt_tokens":5,"completion_tokens":2,"total_tokens":7}
		}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, "test-key")

	resp, err := client.Generate(context.Background(), provider.PromptRequest{
		Messages: []provider.Message{{Role: "user", Content: "read hosts"}},
		Tools: []provider.ToolDefinition{{
			Name:        "filesystem.read_file",
			Description: "Read a file.",
			Parameters:  map[string]any{"type": "object"},
		}},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(resp.ToolCalls) != 1 {
		t.Fatalf("unexpected tool calls: %#v", resp.ToolCalls)
	}
	if got := resp.ToolCalls[0].Arguments["path"]; got != "/etc/hosts" {
		t.Fatalf("unexpected parsed args: %#v", resp.ToolCalls[0].Arguments)
	}
}

func TestClientGenerateOmitsNameForToolMessages(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode request: %v", err)
		}

		messages, ok := body["messages"].([]any)
		if !ok || len(messages) != 2 {
			t.Fatalf("unexpected messages: %#v", body["messages"])
		}

		toolMsg, ok := messages[1].(map[string]any)
		if !ok {
			t.Fatalf("unexpected tool message: %#v", messages[1])
		}
		if _, exists := toolMsg["name"]; exists {
			t.Fatalf("tool message should not include name: %#v", toolMsg)
		}
		if toolMsg["tool_call_id"] != "call_1" {
			t.Fatalf("unexpected tool_call_id: %#v", toolMsg)
		}

		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"ok"}}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, "test-key")

	_, err := client.Generate(context.Background(), provider.PromptRequest{
		Messages: []provider.Message{
			{Role: "user", Content: "hello"},
			{Role: "tool", Name: "filesystem.read_file", ToolCallID: "call_1", Content: "hosts content"},
		},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestClientGenerateSerializesAssistantToolCallsWithProviderSafeNames(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode request: %v", err)
		}

		messages := body["messages"].([]any)
		assistant := messages[0].(map[string]any)
		toolCalls := assistant["tool_calls"].([]any)
		call := toolCalls[0].(map[string]any)
		function := call["function"].(map[string]any)
		if function["name"] != "shell_exec" {
			t.Fatalf("unexpected function name: %#v", function)
		}
		if _, ok := function["arguments"].(string); !ok {
			t.Fatalf("expected stringified arguments, got %#v", function["arguments"])
		}

		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"ok"}}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, "test-key")

	_, err := client.Generate(context.Background(), provider.PromptRequest{
		Messages: []provider.Message{
			{
				Role:    "assistant",
				Content: "Checking tools",
				ToolCalls: []provider.ToolCall{{
					ID:   "call_1",
					Name: "shell.exec",
					Arguments: map[string]any{
						"command": "pwd",
					},
				}},
			},
		},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}
