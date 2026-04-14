package telegram

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"teamd/internal/compaction"
	"teamd/internal/llmtrace"
	"teamd/internal/mcp"
	"teamd/internal/mesh"
	"teamd/internal/provider"
	"teamd/internal/worker"
)

type fakeProvider struct {
	text      string
	model     string
	reasoning provider.ReasoningSettings
	usage     provider.Usage
	err       error
	got       *[]provider.Message
}

func (f fakeProvider) Generate(_ context.Context, req provider.PromptRequest) (provider.PromptResponse, error) {
	if f.got != nil {
		copied := make([]provider.Message, len(req.Messages))
		copy(copied, req.Messages)
		*f.got = copied
	}
	if f.err != nil {
		return provider.PromptResponse{}, f.err
	}
	if len(req.Messages) == 0 || req.Messages[len(req.Messages)-1].Content != "hello bot" {
		return provider.PromptResponse{}, nil
	}
	return provider.PromptResponse{
		Text:             f.text,
		Model:            f.model,
		Reasoning:        f.reasoning,
		Usage:            f.usage,
		ReasoningContent: "chain",
	}, nil
}

type scriptedProvider struct {
	responses []provider.PromptResponse
	requests  []provider.PromptRequest
}

type blockingProvider struct {
	started  chan struct{}
	released chan struct{}
}

func (p *blockingProvider) Generate(ctx context.Context, req provider.PromptRequest) (provider.PromptResponse, error) {
	select {
	case <-p.started:
	default:
		close(p.started)
	}
	<-ctx.Done()
	select {
	case <-p.released:
	default:
		close(p.released)
	}
	return provider.PromptResponse{}, ctx.Err()
}

func (s *scriptedProvider) Generate(_ context.Context, req provider.PromptRequest) (provider.PromptResponse, error) {
	s.requests = append(s.requests, req)
	if len(s.responses) == 0 {
		return provider.PromptResponse{}, nil
	}
	resp := s.responses[0]
	s.responses = s.responses[1:]
	return resp, nil
}

type captureProvider struct {
	onGenerate func(provider.PromptRequest)
	response   provider.PromptResponse
	err        error
}

func (c captureProvider) Generate(_ context.Context, req provider.PromptRequest) (provider.PromptResponse, error) {
	if c.onGenerate != nil {
		c.onGenerate(req)
	}
	if c.err != nil {
		return provider.PromptResponse{}, c.err
	}
	return c.response, nil
}

func joinPromptContents(messages []provider.Message) string {
	parts := make([]string, 0, len(messages))
	for _, msg := range messages {
		if strings.TrimSpace(msg.Content) == "" {
			continue
		}
		parts = append(parts, msg.Content)
	}
	return strings.Join(parts, "\n---\n")
}

type fakeToolRuntime struct {
	tools []mcp.Tool
	calls []struct {
		name string
		args map[string]any
	}
	result mcp.CallResult
	err    error
}

func (f *fakeToolRuntime) ListTools(string) ([]mcp.Tool, error) {
	return f.tools, nil
}

func (f *fakeToolRuntime) CallTool(_ context.Context, name string, args map[string]any) (mcp.CallResult, error) {
	f.calls = append(f.calls, struct {
		name string
		args map[string]any
	}{name: name, args: args})
	if f.err != nil {
		return mcp.CallResult{}, f.err
	}
	return f.result, nil
}

type fakeMeshRuntime struct {
	reply mesh.CandidateReply
	err   error
	calls []struct {
		sessionID string
		prompt    string
		policy    mesh.OrchestrationPolicy
	}
}

func (f *fakeMeshRuntime) HandleOwnerTask(_ context.Context, sessionID, prompt string, policy mesh.OrchestrationPolicy) (mesh.CandidateReply, error) {
	f.calls = append(f.calls, struct {
		sessionID string
		prompt    string
		policy    mesh.OrchestrationPolicy
	}{sessionID: sessionID, prompt: prompt, policy: policy})
	if f.err != nil {
		return mesh.CandidateReply{}, f.err
	}
	return f.reply, nil
}

func TestAdapterPollReadsTelegramUpdate(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/bottest-token/getUpdates" {
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		if got := r.URL.Query().Get("offset"); got != "0" {
			t.Fatalf("unexpected offset: %s", got)
		}

		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":[{"update_id":42,"message":{"message_id":7,"chat":{"id":1001},"text":"hello bot"}}]}`))
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
	})

	update, err := adapter.Poll(context.Background(), 0)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if update.UpdateID != 42 {
		t.Fatalf("unexpected update id: %d", update.UpdateID)
	}
	if update.ChatID != 1001 {
		t.Fatalf("unexpected chat id: %d", update.ChatID)
	}
	if update.Text != "hello bot" {
		t.Fatalf("unexpected text: %q", update.Text)
	}
}

func TestAdapterReplyGeneratesLLMResponseAndSendsMessage(t *testing.T) {
	var texts []string
	var parseModes []string

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/bottest-token/sendMessage":
			if err := r.ParseForm(); err != nil {
				t.Fatalf("parse form: %v", err)
			}
			texts = append(texts, r.PostForm.Get("text"))
			parseModes = append(parseModes, r.PostForm.Get("parse_mode"))
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
		case "/bottest-token/editMessageText":
			if err := r.ParseForm(); err != nil {
				t.Fatalf("parse form: %v", err)
			}
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider: fakeProvider{
			text:  "hello human",
			model: "glm-5",
			reasoning: provider.ReasoningSettings{
				Mode:          "enabled",
				ClearThinking: true,
			},
			usage: provider.Usage{
				PromptTokens:     11,
				CompletionTokens: 7,
				TotalTokens:      18,
			},
		},
	})

	err := adapter.Reply(context.Background(), Update{
		UpdateID: 42,
		ChatID:   1001,
		Text:     "hello bot",
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(texts) != 2 {
		t.Fatalf("expected combined run card and final reply, got %#v", texts)
	}
	if !strings.Contains(texts[0], "Запрос получен") || !strings.Contains(texts[0], "Агент работает") {
		t.Fatalf("unexpected run card: %q", texts[0])
	}
	if texts[1] != "hello human" {
		t.Fatalf("unexpected final reply text: %q", texts[1])
	}
	if parseModes[0] != "" || parseModes[1] != "HTML" {
		t.Fatalf("unexpected parse modes: %#v", parseModes)
	}
}

func TestAdapterReplyWritesLLMTraceFileWhenEnabled(t *testing.T) {
	var texts []string
	traceDir := t.TempDir()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/bottest-token/sendMessage":
			if err := r.ParseForm(); err != nil {
				t.Fatalf("parse form: %v", err)
			}
			texts = append(texts, r.PostForm.Get("text"))
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
		case "/bottest-token/editMessageText":
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider: llmtrace.TracingProvider{Base: fakeProvider{
			text:  "hello human",
			model: "glm-5",
		}},
		TraceEnabled: true,
		TraceDir:     traceDir,
	})

	err := adapter.Reply(context.Background(), Update{
		UpdateID: 42,
		ChatID:   1001,
		Text:     "hello bot",
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	entries, err := os.ReadDir(traceDir)
	if err != nil {
		t.Fatalf("read trace dir: %v", err)
	}
	if len(entries) != 1 {
		t.Fatalf("expected 1 trace file, got %d", len(entries))
	}
	data, err := os.ReadFile(filepath.Join(traceDir, entries[0].Name()))
	if err != nil {
		t.Fatalf("read trace file: %v", err)
	}
	if !strings.Contains(string(data), `"query": "hello bot"`) {
		t.Fatalf("trace missing query: %s", string(data))
	}
	if !strings.Contains(string(data), `"Content": "hello bot"`) {
		t.Fatalf("trace missing request content: %s", string(data))
	}
	if !strings.Contains(string(data), `"Text": "hello human"`) {
		t.Fatalf("trace missing response text: %s", string(data))
	}
}

func TestAdapterReplyTracksSessionMessageCount(t *testing.T) {
	var texts []string

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/bottest-token/sendMessage":
			if err := r.ParseForm(); err != nil {
				t.Fatalf("parse form: %v", err)
			}
			text := r.PostForm.Get("text")
			if text == "hello human" {
				texts = append(texts, text)
			}
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
		case "/bottest-token/editMessageText":
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider: fakeProvider{
			text:  "hello human",
			model: "glm-5",
			reasoning: provider.ReasoningSettings{
				Mode: "enabled",
			},
		},
	})

	for i := 0; i < 2; i++ {
		err := adapter.Reply(context.Background(), Update{
			UpdateID: int64(42 + i),
			ChatID:   1001,
			Text:     "hello bot",
		})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
	}

	if len(texts) != 2 {
		t.Fatalf("expected 2 final replies, got %d", len(texts))
	}
	if texts[0] != "hello human" {
		t.Fatalf("unexpected first final reply: %q", texts[0])
	}
	if texts[1] != "hello human" {
		t.Fatalf("unexpected second final reply: %q", texts[1])
	}
}

func TestAdapterReplyUsesMeshOwnerFlowWhenConfigured(t *testing.T) {
	var texts []string
	meshRuntime := &fakeMeshRuntime{
		reply: mesh.CandidateReply{
			AgentID: "peer-a",
			Stage:   "final",
			Text:    "mesh answer",
		},
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/bottest-token/sendMessage":
			if err := r.ParseForm(); err != nil {
				t.Fatalf("parse form: %v", err)
			}
			texts = append(texts, r.PostForm.Get("text"))
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
		case "/bottest-token/editMessageText":
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider: fakeProvider{
			text: "provider answer",
		},
		Mesh: meshRuntime,
	})

	if err := adapter.Reply(context.Background(), Update{
		ChatID: 1001,
		Text:   "hello bot",
	}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if len(meshRuntime.calls) != 0 {
		t.Fatalf("expected direct 1-1 path without mesh call, got %#v", meshRuntime.calls)
	}
	if got := texts[len(texts)-1]; got != "provider answer" {
		t.Fatalf("expected direct provider reply, got %q", got)
	}
}

func TestAdapterReplyUsesCheckpointAndRecentTurnsWhenHistoryIsTooLarge(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/bottest-token/sendMessage", "/bottest-token/editMessageText":
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	store := NewSessionStore(64)
	for i := 0; i < 20; i++ {
		_ = store.Append(1001, provider.Message{Role: "user", Content: strings.Repeat("message ", 20)})
	}
	_ = store.SaveCheckpoint(1001, worker.Checkpoint{
		SessionID:        "telegram:1001/default",
		CompactionMethod: "heuristic-v1",
		WhatHappened:     "Earlier conversation compacted",
		WhatMattersNow:   "Preserve the key deployment requirement",
	})

	var got provider.PromptRequest
	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider: captureProvider{
			onGenerate: func(req provider.PromptRequest) { got = req },
			response: provider.PromptResponse{
				Text:  "ok",
				Model: "glm-5",
			},
		},
		Store: store,
	})
	adapter.budget = compaction.Budget{
		ContextWindowTokens:     32000,
		PromptBudgetTokens:      120,
		CompactionTriggerTokens: 1000,
		MaxToolContextChars:     128,
	}

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "latest question"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if len(got.Messages) == 0 {
		t.Fatal("expected provider request messages")
	}
	if got.Messages[0].Role != "system" {
		t.Fatalf("expected checkpoint summary in assembled prompt, got %#v", got.Messages[0])
	}
	foundLatest := false
	for _, msg := range got.Messages {
		if msg.Content == "latest question" {
			foundLatest = true
		}
		if msg.Content == strings.Repeat("message ", 20) {
			t.Fatalf("expected old oversized history to be trimmed, got %#v", got.Messages)
		}
	}
	if !foundLatest {
		t.Fatalf("expected latest question in prompt, got %#v", got.Messages)
	}
}

func TestAdapterReplyCompactsAndPersistsCheckpointWhenThresholdExceeded(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/bottest-token/sendMessage", "/bottest-token/editMessageText":
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	store := NewSessionStore(128)
	for i := 0; i < 30; i++ {
		_ = store.Append(1001, provider.Message{Role: "tool", Content: strings.Repeat("abcdef", 40), ToolCallID: fmt.Sprintf("tool-%d", i)})
	}

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider: captureProvider{
			response: provider.PromptResponse{
				Text:  "ok",
				Model: "glm-5",
			},
		},
		Store: store,
	})
	adapter.budget = compaction.Budget{
		ContextWindowTokens:     32000,
		PromptBudgetTokens:      240,
		CompactionTriggerTokens: 120,
		MaxToolContextChars:     48,
	}

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "continue"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	got, ok, err := store.Checkpoint(1001)
	if err != nil || !ok {
		t.Fatalf("expected checkpoint after compaction, ok=%v err=%v", ok, err)
	}
	if got.CompactionMethod != "heuristic-v1" {
		t.Fatalf("unexpected checkpoint: %#v", got)
	}
}

func TestAdapterUsesConfiguredCompactionBudget(t *testing.T) {
	adapter := New(Deps{
		ContextWindowTokens:     12345,
		PromptBudgetTokens:      6789,
		CompactionTriggerTokens: 2222,
		MaxToolContextChars:     333,
	})

	if adapter.budget.ContextWindowTokens != 12345 {
		t.Fatalf("unexpected context budget: %d", adapter.budget.ContextWindowTokens)
	}
	if adapter.budget.PromptBudgetTokens != 6789 {
		t.Fatalf("unexpected prompt budget: %d", adapter.budget.PromptBudgetTokens)
	}
	if adapter.budget.CompactionTriggerTokens != 2222 {
		t.Fatalf("unexpected compaction trigger: %d", adapter.budget.CompactionTriggerTokens)
	}
	if adapter.budget.MaxToolContextChars != 333 {
		t.Fatalf("unexpected max tool chars: %d", adapter.budget.MaxToolContextChars)
	}
}

func TestSyncStatusCardSwallowsTelegram429(t *testing.T) {
	var sendCount int
	var editCount int
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/bottest-token/sendMessage":
			sendCount++
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
		case "/bottest-token/editMessageText":
			editCount++
			w.WriteHeader(http.StatusTooManyRequests)
			_, _ = w.Write([]byte(`{"ok":false,"error_code":429,"description":"Too Many Requests: retry after 60","parameters":{"retry_after":60}}`))
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider: fakeProvider{
			text: "hello human",
		},
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "hello bot"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if sendCount < 2 {
		t.Fatalf("expected ack and final message to survive 429, got sendCount=%d editCount=%d", sendCount, editCount)
	}
	if editCount == 0 {
		t.Fatalf("expected at least one edit attempt")
	}
}

func TestSyncStatusCardThrottlesRapidEdits(t *testing.T) {
	var editCount int
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/bottest-token/editMessageText":
			editCount++
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
		case "/bottest-token/sendMessage":
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider:   fakeProvider{text: "ok"},
	})
	adapter.runs.CreateWithID(1001, adapter.runs.AllocateID(), "q", time.Now().UTC())
	adapter.runs.Update(1001, func(run *RunState) {
		run.StatusMessageID = 8
	})

	if err := adapter.syncStatusCard(context.Background(), 1001); err != nil {
		t.Fatalf("first sync: %v", err)
	}
	if err := adapter.syncStatusCard(context.Background(), 1001); err != nil {
		t.Fatalf("second sync: %v", err)
	}
	if editCount != 1 {
		t.Fatalf("expected throttled edit count 1, got %d", editCount)
	}
}

func TestAdapterNormalizeUsesTelegramChatAsSessionID(t *testing.T) {
	evt, err := New(TestDeps()).Normalize(Update{
		UpdateID: 1,
		ChatID:   77,
		Text:     "ping",
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if evt.Source != "telegram" {
		t.Fatalf("unexpected source: %q", evt.Source)
	}
	if evt.SessionID != "telegram:77" {
		t.Fatalf("unexpected session id: %q", evt.SessionID)
	}
}

func TestAdapterNormalizeAllowsCallbackUpdates(t *testing.T) {
	evt, err := New(TestDeps()).Normalize(Update{
		UpdateID:      2,
		ChatID:        77,
		CallbackID:    "cb-1",
		CallbackData:  "session:list",
		CallbackQuery: true,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if evt.Source != "telegram" {
		t.Fatalf("unexpected source: %q", evt.Source)
	}
	if evt.SessionID != "telegram:77" {
		t.Fatalf("unexpected session id: %q", evt.SessionID)
	}
	if evt.Text != "session:list" {
		t.Fatalf("unexpected callback text: %q", evt.Text)
	}
}

func TestAdapterPollReturnsErrorOnTelegramFailure(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
		_, _ = w.Write([]byte(`{"ok":false}`))
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
	})

	_, err := adapter.Poll(context.Background(), 0)
	if err == nil {
		t.Fatal("expected poll error")
	}
	if !strings.Contains(err.Error(), "telegram api error") {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestAdapterPollSkipsNonMessageUpdate(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{
			"ok": true,
			"result": []map[string]any{
				{"update_id": 5},
			},
		})
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
	})

	update, err := adapter.Poll(context.Background(), 0)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if update.UpdateID != 5 {
		t.Fatalf("unexpected update id: %d", update.UpdateID)
	}
	if update.Text != "" {
		t.Fatalf("expected empty text for non-message update, got %q", update.Text)
	}
}

func TestSessionStoreTracksHistoryAndReset(t *testing.T) {
	store := NewSessionStore(4)
	store.Append(1001, provider.Message{Role: "user", Content: "u1"})
	store.Append(1001, provider.Message{Role: "assistant", Content: "a1"})

	messages, err := store.Messages(1001)
	if err != nil {
		t.Fatalf("messages: %v", err)
	}
	if len(messages) != 2 {
		t.Fatalf("expected 2 messages")
	}

	if err := store.Reset(1001); err != nil {
		t.Fatalf("reset: %v", err)
	}
	messages, err = store.Messages(1001)
	if err != nil {
		t.Fatalf("messages after reset: %v", err)
	}
	if len(messages) != 0 {
		t.Fatalf("expected empty history after reset")
	}
}

func TestAdapterReplySendsSessionHistoryToProvider(t *testing.T) {
	var got []provider.Message

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider: fakeProvider{
			text: "hello human",
			got:  &got,
		},
	})

	err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "hello bot"})
	if err != nil {
		t.Fatalf("unexpected error on first reply: %v", err)
	}
	err = adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "hello bot"})
	if err != nil {
		t.Fatalf("unexpected error on second reply: %v", err)
	}

	if len(got) < 2 {
		t.Fatalf("expected history in provider request, got %#v", got)
	}
}

func TestAdapterReplyDoesNotStoreFailedProviderResponse(t *testing.T) {
	adapter := New(Deps{
		Provider: fakeProvider{
			err: errors.New("provider down"),
		},
	})

	err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "hello bot"})
	if err == nil {
		t.Fatal("expected provider error")
	}
}

func TestAdapterReplyResetsSessionOnResetCommand(t *testing.T) {
	var sent url.Values
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		sent = r.PostForm
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":1}}`))
	}))
	defer server.Close()

	adapter := New(Deps{BaseURL: server.URL, Token: "test-token", HTTPClient: server.Client()})
	if err := adapter.store.Append(1001, provider.Message{Role: "user", Content: "old"}); err != nil {
		t.Fatalf("seed user: %v", err)
	}
	if err := adapter.store.Append(1001, provider.Message{Role: "assistant", Content: "old-reply"}); err != nil {
		t.Fatalf("seed assistant: %v", err)
	}

	err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/reset"})
	if err != nil {
		t.Fatalf("unexpected reset error: %v", err)
	}

	if !strings.Contains(sent.Get("text"), "session reset") {
		t.Fatalf("unexpected reset reply: %q", sent.Get("text"))
	}
	messages, err := adapter.store.Messages(1001)
	if err != nil {
		t.Fatalf("messages after reset: %v", err)
	}
	if len(messages) != 0 {
		t.Fatalf("expected empty session after reset, got %#v", messages)
	}
}

func TestFormatFooterUsesStructuredMetrics(t *testing.T) {
	text := formatFooter(FooterMetrics{
		Model:            "glm-5",
		Thinking:         "enabled",
		ClearThinking:    true,
		ContextTokens:    18,
		PromptTokens:     11,
		CompletionTokens: 7,
		SessionMessages:  4,
		ContextEstimate:  30,
		Compacted:        true,
	})

	if !strings.Contains(text, "session_messages=4") {
		t.Fatalf("unexpected footer: %q", text)
	}
	if !strings.Contains(text, "context_estimate=30") {
		t.Fatalf("unexpected footer: %q", text)
	}
	if !strings.Contains(text, "compacted=true") {
		t.Fatalf("unexpected footer: %q", text)
	}
}

func TestSessionStoreTrimsHistoryWhenLimitExceeded(t *testing.T) {
	store := NewSessionStore(3)
	store.Append(1001, provider.Message{Role: "user", Content: "u1"})
	store.Append(1001, provider.Message{Role: "assistant", Content: "a1"})
	store.Append(1001, provider.Message{Role: "user", Content: "u2"})
	store.Append(1001, provider.Message{Role: "assistant", Content: "a2"})

	got, err := store.Messages(1001)
	if err != nil {
		t.Fatalf("messages: %v", err)
	}
	if len(got) != 3 {
		t.Fatalf("expected 3 messages, got %d", len(got))
	}
	if got[0].Content != "a1" || got[1].Content != "u2" || got[2].Content != "a2" {
		t.Fatalf("unexpected trimmed history: %#v", got)
	}
}

func TestSessionStoreSupportsNamedSessionsPerChat(t *testing.T) {
	store := NewSessionStore(4)

	if session, err := store.ActiveSession(1001); err != nil || session != "default" {
		t.Fatalf("unexpected active session: %q err=%v", session, err)
	}
	if err := store.CreateSession(1001, "deploy"); err != nil {
		t.Fatalf("create session: %v", err)
	}
	if err := store.UseSession(1001, "deploy"); err != nil {
		t.Fatalf("use session: %v", err)
	}
	if err := store.Append(1001, provider.Message{Role: "user", Content: "deploy ctx"}); err != nil {
		t.Fatalf("append deploy: %v", err)
	}
	if err := store.UseSession(1001, "default"); err != nil {
		t.Fatalf("use default: %v", err)
	}
	if err := store.Append(1001, provider.Message{Role: "user", Content: "default ctx"}); err != nil {
		t.Fatalf("append default: %v", err)
	}

	got, err := store.Messages(1001)
	if err != nil {
		t.Fatalf("messages default: %v", err)
	}
	if len(got) != 1 || got[0].Content != "default ctx" {
		t.Fatalf("unexpected default messages: %#v", got)
	}

	if err := store.UseSession(1001, "deploy"); err != nil {
		t.Fatalf("use deploy again: %v", err)
	}
	got, err = store.Messages(1001)
	if err != nil {
		t.Fatalf("messages deploy: %v", err)
	}
	if len(got) != 1 || got[0].Content != "deploy ctx" {
		t.Fatalf("unexpected deploy messages: %#v", got)
	}

	sessions, err := store.ListSessions(1001)
	if err != nil {
		t.Fatalf("list sessions: %v", err)
	}
	if len(sessions) != 2 || sessions[0] != "default" || sessions[1] != "deploy" {
		t.Fatalf("unexpected sessions: %#v", sessions)
	}
}

func TestAdapterReplyIgnoresRepeatedResetGracefully(t *testing.T) {
	var texts []string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		texts = append(texts, r.PostForm.Get("text"))
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":1}}`))
	}))
	defer server.Close()

	adapter := New(Deps{BaseURL: server.URL, Token: "test-token", HTTPClient: server.Client()})

	for i := 0; i < 2; i++ {
		err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/reset"})
		if err != nil {
			t.Fatalf("unexpected reset error: %v", err)
		}
	}

	if len(texts) != 2 {
		t.Fatalf("expected 2 reset replies, got %d", len(texts))
	}
	if texts[0] != "session reset" || texts[1] != "session reset" {
		t.Fatalf("unexpected reset replies: %#v", texts)
	}
}

func TestAdapterUsesInjectedSessionStore(t *testing.T) {
	store := NewSessionStore(4)
	adapter := New(Deps{
		Provider: provider.FakeProvider{},
		Store:    store,
	})

	if adapter.store != store {
		t.Fatal("expected injected store to be used")
	}
}

func TestAdapterReplyHandlesSessionCommands(t *testing.T) {
	var texts []string
	var rawBodies []string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		texts = append(texts, r.PostForm.Get("text"))
		rawBodies = append(rawBodies, r.PostForm.Get("reply_markup"))
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
	})

	for _, text := range []string{"/session new deploy", "/session use deploy", "/session list"} {
		if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: text}); err != nil {
			t.Fatalf("reply %q: %v", text, err)
		}
	}

	if len(texts) != 3 {
		t.Fatalf("unexpected replies: %#v", texts)
	}
	if !strings.Contains(texts[0], "session created: deploy") || !strings.Contains(texts[0], "active: deploy") {
		t.Fatalf("unexpected create reply: %q", texts[0])
	}
	if texts[1] != "session active: deploy" {
		t.Fatalf("unexpected use reply: %q", texts[1])
	}
	if !strings.Contains(texts[2], "* deploy") || !strings.Contains(texts[2], "default") {
		t.Fatalf("unexpected list reply: %q", texts[2])
	}
	if rawBodies[0] == "" {
		t.Fatalf("expected inline keyboard on create reply")
	}
}

func TestAdapterReplyHandlesNaturalLanguageSessionSwitching(t *testing.T) {
	var texts []string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		texts = append(texts, r.PostForm.Get("text"))
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
	})

	for _, text := range []string{"создай сессию deploy", "переключись на сессию deploy"} {
		if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: text}); err != nil {
			t.Fatalf("reply %q: %v", text, err)
		}
	}

	if len(texts) != 2 {
		t.Fatalf("unexpected replies: %#v", texts)
	}
	if !strings.Contains(texts[0], "session created: deploy") || !strings.Contains(texts[0], "active: deploy") {
		t.Fatalf("unexpected create reply: %q", texts[0])
	}
	if texts[1] != "session active: deploy" {
		t.Fatalf("unexpected switch reply: %q", texts[1])
	}
}

func TestAdapterReplyHandlesSessionCallbackActions(t *testing.T) {
	var texts []string
	var answered bool
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/bottest-token/sendMessage":
			if err := r.ParseForm(); err != nil {
				t.Fatalf("parse form: %v", err)
			}
			texts = append(texts, r.PostForm.Get("text"))
		case "/bottest-token/answerCallbackQuery":
			answered = true
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	adapter := New(Deps{BaseURL: server.URL, Token: "test-token", HTTPClient: server.Client()})
	if err := adapter.store.CreateSession(1001, "deploy"); err != nil {
		t.Fatalf("create deploy: %v", err)
	}

	err := adapter.Reply(context.Background(), Update{
		ChatID:        1001,
		CallbackID:    "cb-1",
		CallbackData:  "session:use:deploy",
		CallbackQuery: true,
	})
	if err != nil {
		t.Fatalf("callback reply: %v", err)
	}

	if !answered {
		t.Fatalf("expected callback acknowledgement")
	}
	if len(texts) != 1 || texts[0] != "session active: deploy" {
		t.Fatalf("unexpected callback texts: %#v", texts)
	}
}

func TestAdapterReplyDeletesTechnicalRunMessageOnDeleteCallback(t *testing.T) {
	var deleted bool
	var answered bool
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/bottest-token/answerCallbackQuery":
			answered = true
		case "/bottest-token/deleteMessage":
			deleted = true
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	adapter := New(Deps{BaseURL: server.URL, Token: "test-token", HTTPClient: server.Client()})
	adapter.runs.CreateWithID(1001, adapter.runs.AllocateID(), "deploy", time.Now().UTC())
	adapter.runs.Update(1001, func(run *RunState) {
		run.StatusMessageID = 77
		run.Completed = true
		run.Stage = "Ответ отправлен"
	})

	err := adapter.Reply(context.Background(), Update{
		ChatID:        1001,
		CallbackID:    "cb-1",
		CallbackData:  "run:delete",
		CallbackQuery: true,
	})
	if err != nil {
		t.Fatalf("callback reply: %v", err)
	}
	if !answered || !deleted {
		t.Fatalf("expected callback acknowledgement and deleteMessage, answered=%v deleted=%v", answered, deleted)
	}
	if _, ok := adapter.runs.Active(1001); ok {
		t.Fatalf("expected run state to be removed after delete")
	}
}

func TestAdapterReplyExecutesModelRequestedToolAndSendsFinalReply(t *testing.T) {
	var sent url.Values
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		sent = r.PostForm
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	providerClient := &scriptedProvider{
		responses: []provider.PromptResponse{
			{
				Model:        "glm-5",
				FinishReason: "tool_calls",
				ToolCalls: []provider.ToolCall{{
					ID:   "call_1",
					Name: "filesystem_read_file",
					Arguments: map[string]any{
						"path": "/tmp/note.txt",
					},
				}},
			},
			{
				Text:  "file says hello",
				Model: "glm-5",
				Usage: provider.Usage{
					PromptTokens:     21,
					CompletionTokens: 8,
					TotalTokens:      29,
				},
			},
		},
	}
	tools := &fakeToolRuntime{
		tools: []mcp.Tool{{
			Name:        "filesystem.read_file",
			Description: "Read a file from the local filesystem.",
			Parameters: map[string]any{
				"type": "object",
			},
		}},
		result: mcp.CallResult{Content: "hello from file"},
	}

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider:   providerClient,
		Tools:      tools,
	})

	err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "прочитай файл"})
	if err != nil {
		t.Fatalf("unexpected reply error: %v", err)
	}
	if got := sent.Get("text"); !strings.Contains(got, "file says hello") {
		t.Fatalf("unexpected final reply: %q", got)
	}
	if len(tools.calls) != 1 || tools.calls[0].name != "filesystem.read_file" {
		t.Fatalf("unexpected tool calls: %#v", tools.calls)
	}
	if len(providerClient.requests) != 2 {
		t.Fatalf("expected 2 provider calls, got %d", len(providerClient.requests))
	}
	if len(providerClient.requests[0].Tools) != 1 {
		t.Fatalf("expected tools in first provider request, got %#v", providerClient.requests[0].Tools)
	}
	if providerClient.requests[0].Tools[0].Name != "filesystem_read_file" {
		t.Fatalf("expected provider-safe tool alias, got %#v", providerClient.requests[0].Tools)
	}
	secondMessages := providerClient.requests[1].Messages
	if len(secondMessages) < 3 || secondMessages[len(secondMessages)-1].Role != "tool" {
		t.Fatalf("expected tool message before second model call, got %#v", secondMessages)
	}
}

func TestAdapterReplyStreamsToolActivityBeforeFinalReply(t *testing.T) {
	var texts []string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		texts = append(texts, r.PostForm.Get("text"))
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	providerClient := &scriptedProvider{
		responses: []provider.PromptResponse{
			{
				Model:        "glm-5",
				FinishReason: "tool_calls",
				ToolCalls: []provider.ToolCall{{
					ID:   "call_1",
					Name: "shell_exec",
					Arguments: map[string]any{
						"command": "pwd",
					},
				}},
			},
			{
				Text:  "done",
				Model: "glm-5",
			},
		},
	}
	tools := &fakeToolRuntime{
		tools: []mcp.Tool{{
			Name:        "shell.exec",
			Description: "Execute a shell command locally.",
			Parameters:  map[string]any{"type": "object"},
		}},
		result: mcp.CallResult{Content: "/tmp/worktree\n"},
	}

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider:   providerClient,
		Tools:      tools,
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "run pwd"}); err != nil {
		t.Fatalf("unexpected reply error: %v", err)
	}

	if len(texts) != 4 {
		t.Fatalf("expected combined run card, tool status update, final status and final reply, got %#v", texts)
	}
	if !strings.Contains(texts[0], "Запрос получен") || !strings.Contains(texts[0], "Агент работает") {
		t.Fatalf("expected combined ack and status card, got %q", texts[0])
	}
	if !strings.Contains(texts[1], "🖥️ shell.exec") {
		t.Fatalf("expected shell tool in status update, got %q", texts[1])
	}
	if strings.Contains(texts[1], "/tmp/worktree") || !strings.Contains(texts[1], "command=pwd") {
		t.Fatalf("expected only summarized tool params in progress block, got %q", texts[1])
	}
	if !strings.Contains(texts[3], "done") {
		t.Fatalf("expected final reply, got %q", texts[3])
	}
}

func TestAdapterReplyAppliesWorkspaceDefaultCWDToShellTool(t *testing.T) {
	var texts []string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		texts = append(texts, r.PostForm.Get("text"))
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	providerClient := &scriptedProvider{
		responses: []provider.PromptResponse{
			{
				Model:        "glm-5",
				FinishReason: "tool_calls",
				ToolCalls: []provider.ToolCall{{
					ID:        "call_1",
					Name:      "shell_exec",
					Arguments: map[string]any{"command": "pwd"},
				}},
			},
			{Text: "done", Model: "glm-5"},
		},
	}
	tools := &fakeToolRuntime{
		tools: []mcp.Tool{{
			Name:        "shell.exec",
			Description: "Execute a shell command locally.",
			Parameters:  map[string]any{"type": "object"},
		}},
		result: mcp.CallResult{Content: "/tmp/workspace\n"},
	}

	adapter := New(Deps{
		BaseURL:       server.URL,
		Token:         "test-token",
		HTTPClient:    server.Client(),
		Provider:      providerClient,
		Tools:         tools,
		WorkspaceRoot: "/tmp/workspace",
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "run pwd"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if len(tools.calls) != 1 {
		t.Fatalf("expected one tool call, got %#v", tools.calls)
	}
	if got, _ := tools.calls[0].args["cwd"].(string); got != "/tmp/workspace" {
		t.Fatalf("expected workspace cwd to be injected, got %#v", tools.calls[0].args)
	}
}

func TestAdapterReplyAllowsMultiRoundToolSequenceBeforeFinalReply(t *testing.T) {
	var texts []string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		texts = append(texts, r.PostForm.Get("text"))
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	providerClient := &scriptedProvider{
		responses: []provider.PromptResponse{
			{FinishReason: "tool_calls", ToolCalls: []provider.ToolCall{{ID: "call_1", Name: "shell_exec", Arguments: map[string]any{"command": "echo one"}}}},
			{FinishReason: "tool_calls", ToolCalls: []provider.ToolCall{{ID: "call_2", Name: "filesystem_list_dir", Arguments: map[string]any{"path": "/"}}}},
			{FinishReason: "tool_calls", ToolCalls: []provider.ToolCall{{ID: "call_3", Name: "filesystem_write_file", Arguments: map[string]any{"path": "/tmp/x", "content": "ok"}}}},
			{FinishReason: "tool_calls", ToolCalls: []provider.ToolCall{{ID: "call_4", Name: "filesystem_read_file", Arguments: map[string]any{"path": "/tmp/x"}}}},
			{Text: "all checks done", Model: "glm-5"},
		},
	}
	tools := &fakeToolRuntime{
		tools: []mcp.Tool{
			{Name: "shell.exec", Parameters: map[string]any{"type": "object"}},
			{Name: "filesystem.list_dir", Parameters: map[string]any{"type": "object"}},
			{Name: "filesystem.write_file", Parameters: map[string]any{"type": "object"}},
			{Name: "filesystem.read_file", Parameters: map[string]any{"type": "object"}},
		},
		result: mcp.CallResult{Content: "ok"},
	}

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider:   providerClient,
		Tools:      tools,
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "check safely"}); err != nil {
		t.Fatalf("unexpected reply error: %v", err)
	}

	if len(texts) < 3 {
		t.Fatalf("expected ack, final status snapshot and final reply, got %#v", texts)
	}
	if !strings.Contains(texts[0], "Запрос получен") {
		t.Fatalf("expected initial run card, got %#v", texts)
	}
	if !strings.Contains(texts[len(texts)-2], "Выполнение завершено") {
		t.Fatalf("expected final status snapshot before reply, got %#v", texts)
	}
	if !strings.Contains(texts[len(texts)-1], "all checks done") {
		t.Fatalf("expected final reply after multi-round tools, got %#v", texts)
	}
}
