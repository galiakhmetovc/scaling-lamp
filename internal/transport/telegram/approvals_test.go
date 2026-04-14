package telegram

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/mcp"
	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
)

func TestAdapterExecuteToolRequestsApprovalForGuardedTool(t *testing.T) {
	var sentText string
	var sentMarkup string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		sentText = r.PostForm.Get("text")
		sentMarkup = r.PostForm.Get("reply_markup")
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:      server.URL,
		Token:        "bot",
		HTTPClient:   server.Client(),
		Approvals:    approvals.New(approvals.TestDeps()),
		ActionPolicy: runtimex.ActionPolicy{ApprovalRequiredTools: []string{"shell.exec"}},
	})
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	done := make(chan error, 1)
	go func() {
		_, err := adapter.executeTool(ctx, 1001, provider.ToolCall{
			Name: providerToolName("shell.exec"),
			Arguments: map[string]any{
				"command": "echo hello",
			},
		})
		done <- err
	}()
	deadline := time.Now().Add(time.Second)
	for time.Now().Before(deadline) {
		if strings.Contains(sentText, "Approval required") {
			break
		}
		time.Sleep(10 * time.Millisecond)
	}
	if !strings.Contains(sentText, "Approval required") || !strings.Contains(sentText, "tool: shell.exec") {
		t.Fatalf("unexpected approval message: %q", sentText)
	}
	if !strings.Contains(sentMarkup, "approval:approve:approval-1") || !strings.Contains(sentMarkup, "approval:reject:approval-1") {
		t.Fatalf("unexpected approval keyboard: %q", sentMarkup)
	}
	cancel()
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("guarded tool did not stop after context cancel")
	}
}

func TestAdapterExecuteToolKeepsPendingApprovalWhenTelegramDeliveryFails(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusBadRequest)
		_, _ = w.Write([]byte(`{"ok":false,"error_code":400,"description":"Bad Request: chat not found"}`))
	}))
	defer server.Close()

	svc := approvals.New(approvals.TestDeps())
	adapter := New(Deps{
		BaseURL:      server.URL,
		Token:        "bot",
		HTTPClient:   server.Client(),
		Approvals:    svc,
		ActionPolicy: runtimex.ActionPolicy{ApprovalRequiredTools: []string{"shell.exec"}},
	})
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	done := make(chan error, 1)
	go func() {
		_, err := adapter.executeTool(ctx, 1001, provider.ToolCall{
			Name: providerToolName("shell.exec"),
			Arguments: map[string]any{
				"command": "echo hello",
			},
		})
		done <- err
	}()

	deadline := time.Now().Add(time.Second)
	for time.Now().Before(deadline) {
		pending := svc.PendingBySession("1001:default")
		if len(pending) == 1 {
			break
		}
		time.Sleep(10 * time.Millisecond)
	}
	pending := svc.PendingBySession("1001:default")
	if len(pending) != 1 {
		t.Fatalf("expected pending approval despite telegram delivery failure, got %d", len(pending))
	}
	select {
	case err := <-done:
		t.Fatalf("guarded tool returned early instead of waiting for approval: %v", err)
	default:
	}
	cancel()
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("guarded tool did not stop after context cancel")
	}
}

func TestAdapterReplyHandlesApprovalCallbackActions(t *testing.T) {
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

	svc := approvals.New(approvals.TestDeps())
	record, err := svc.Create(approvals.Request{
		WorkerID:  "shell.exec",
		SessionID: "1001:default",
		Payload:   "{}",
	})
	if err != nil {
		t.Fatalf("create approval: %v", err)
	}
	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Approvals:  svc,
	})

	err = adapter.Reply(context.Background(), Update{
		UpdateID:      77,
		ChatID:        1001,
		CallbackID:    "cb-1",
		CallbackData:  "approval:approve:" + record.ID,
		CallbackQuery: true,
	})
	if err != nil {
		t.Fatalf("callback reply: %v", err)
	}
	if !answered {
		t.Fatal("expected callback acknowledgement")
	}
	if len(texts) != 1 || !strings.Contains(texts[0], "approval updated: "+record.ID+" -> approved") {
		t.Fatalf("unexpected callback texts: %#v", texts)
	}
	got, ok := svc.Get(record.ID)
	if !ok || got.Status != approvals.StatusApproved {
		t.Fatalf("expected approved record, got %+v ok=%v", got, ok)
	}
}

func TestAdapterExecuteToolWaitsForApprovalAndResumes(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	tools := &fakeToolRuntime{
		result: mcp.CallResult{Content: "approved result"},
	}
	svc := approvals.New(approvals.TestDeps())
	adapter := New(Deps{
		BaseURL:      server.URL,
		Token:        "bot",
		HTTPClient:   server.Client(),
		Tools:        tools,
		Approvals:    svc,
		ActionPolicy: runtimex.ActionPolicy{ApprovalRequiredTools: []string{"shell.exec"}},
	})

	done := make(chan string, 1)
	go func() {
		out, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
			Name:      providerToolName("shell.exec"),
			Arguments: map[string]any{"command": "echo hello"},
		})
		if err != nil {
			t.Errorf("execute tool: %v", err)
			return
		}
		done <- out
	}()

	time.Sleep(20 * time.Millisecond)
	pending := svc.PendingBySession("1001:default")
	if len(pending) != 1 {
		t.Fatalf("expected one pending approval, got %d", len(pending))
	}
	if _, err := svc.HandleCallback(approvals.Callback{
		ApprovalID: pending[0].ID,
		Action:     approvals.ActionApprove,
		UpdateID:   "cb-resume-1",
	}); err != nil {
		t.Fatalf("handle callback: %v", err)
	}

	select {
	case out := <-done:
		if out != "approved result" {
			t.Fatalf("unexpected tool result: %q", out)
		}
	case <-time.After(time.Second):
		t.Fatal("tool did not resume after approval")
	}
	if len(tools.calls) != 1 {
		t.Fatalf("expected actual tool execution after approval, got %d", len(tools.calls))
	}
}

func TestAdapterApprovalCallbackResumesPersistedContinuation(t *testing.T) {
	var texts []string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/bottest-token/sendMessage":
			if err := r.ParseForm(); err != nil {
				t.Fatalf("parse form: %v", err)
			}
			texts = append(texts, r.PostForm.Get("text"))
		case "/bottest-token/answerCallbackQuery":
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	runStore, err := runtimex.NewSQLiteStore(t.TempDir() + "/runtime.db")
	if err != nil {
		t.Fatalf("new sqlite store: %v", err)
	}
	tools := &fakeToolRuntime{result: mcp.CallResult{Content: "approved result"}}
	svc := approvals.New(approvals.Deps{Store: runStore})
	record, err := svc.Create(approvals.Request{
		WorkerID:  "shell.exec",
		SessionID: "1001:default",
		Payload:   "{}",
	})
	if err != nil {
		t.Fatalf("create approval: %v", err)
	}
	if err := runStore.SaveApprovalContinuation(runtimex.ApprovalContinuation{
		ApprovalID:    record.ID,
		RunID:         "run-1",
		ChatID:        1001,
		SessionID:     "1001:default",
		Query:         "do thing",
		ToolCallID:    "call-1",
		ToolName:      "shell.exec",
		ToolArguments: map[string]any{"command": "echo hello"},
		RequestedAt:   time.Now().UTC(),
	}); err != nil {
		t.Fatalf("save continuation: %v", err)
	}

	sessionStore := NewSessionStore(16)
	if err := sessionStore.Append(1001, provider.Message{
		Role:    "assistant",
		Content: "",
		ToolCalls: []provider.ToolCall{{
			ID:        "call-1",
			Name:      providerToolName("shell.exec"),
			Arguments: map[string]any{"command": "echo hello"},
		}},
	}); err != nil {
		t.Fatalf("append assistant tool call: %v", err)
	}

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider:   provider.FakeProvider{},
		Store:      sessionStore,
		Tools:      tools,
		Approvals:  svc,
		RunStore:   runStore,
	})

	if err := adapter.Reply(context.Background(), Update{
		UpdateID:      88,
		ChatID:        1001,
		CallbackID:    "cb-2",
		CallbackData:  "approval:approve:" + record.ID,
		CallbackQuery: true,
	}); err != nil {
		t.Fatalf("callback reply: %v", err)
	}

	deadline := time.Now().Add(time.Second)
	for time.Now().Before(deadline) {
		messages, _ := sessionStore.Messages(1001)
		_, ok, _ := runStore.ApprovalContinuation(record.ID)
		if len(messages) >= 3 && !ok {
			break
		}
		time.Sleep(20 * time.Millisecond)
	}
	messages, _ := sessionStore.Messages(1001)
	if len(tools.calls) != 1 {
		t.Fatalf("expected resumed tool execution, got %d", len(tools.calls))
	}
	if len(messages) < 3 || messages[1].Role != "tool" || messages[1].Content != "approved result" {
		t.Fatalf("unexpected resumed history: %#v", messages)
	}
	if got, ok, err := runStore.ApprovalContinuation(record.ID); err != nil || ok || got.ApprovalID != "" {
		t.Fatalf("expected continuation deleted, got=%+v ok=%v err=%v", got, ok, err)
	}
	found := false
	for _, text := range texts {
		if strings.Contains(text, "resumed pending run") {
			found = true
			break
		}
	}
	if !found {
		t.Fatalf("unexpected callback texts: %#v", texts)
	}
}
