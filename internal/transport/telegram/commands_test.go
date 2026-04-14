package telegram

import (
	"context"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
)

func TestAdapterSyncCommandsReplacesTelegramCommandMenu(t *testing.T) {
	var calls []struct {
		path     string
		commands string
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/bottest-token/deleteMyCommands", "/bottest-token/setMyCommands":
			if err := r.ParseForm(); err != nil {
				t.Fatalf("parse form: %v", err)
			}
			calls = append(calls, struct {
				path     string
				commands string
			}{
				path:     r.URL.Path,
				commands: r.PostForm.Get("commands"),
			})
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":true}`))
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
	})

	if err := adapter.SyncCommands(context.Background()); err != nil {
		t.Fatalf("sync commands: %v", err)
	}
	if len(calls) != 2 {
		t.Fatalf("expected delete+set calls, got %#v", calls)
	}
	if calls[0].path != "/bottest-token/deleteMyCommands" {
		t.Fatalf("expected deleteMyCommands first, got %#v", calls)
	}
	if calls[1].path != "/bottest-token/setMyCommands" {
		t.Fatalf("expected setMyCommands second, got %#v", calls)
	}
	if !strings.Contains(calls[1].commands, `"command":"reset"`) {
		t.Fatalf("expected reset command in payload, got %q", calls[1].commands)
	}
	if !strings.Contains(calls[1].commands, `"command":"status"`) {
		t.Fatalf("expected status command in payload, got %q", calls[1].commands)
	}
	if !strings.Contains(calls[1].commands, `"command":"btw"`) {
		t.Fatalf("expected btw command in payload, got %q", calls[1].commands)
	}
	if !strings.Contains(calls[1].commands, `"command":"session"`) {
		t.Fatalf("expected session command in payload, got %q", calls[1].commands)
	}
	if !strings.Contains(calls[1].commands, `"command":"mesh"`) {
		t.Fatalf("expected mesh command in payload, got %q", calls[1].commands)
	}
	if !strings.Contains(calls[1].commands, `"command":"skills"`) {
		t.Fatalf("expected skills command in payload, got %q", calls[1].commands)
	}
}

func TestAdapterReplyReservesStatusCommand(t *testing.T) {
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

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider:   fakeProvider{text: "should not be used"},
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/status"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if len(texts) != 1 || !strings.Contains(texts[0], "Нет активного выполнения") {
		t.Fatalf("unexpected status response: %#v", texts)
	}
}

func TestAdapterReplyReservesCancelCommand(t *testing.T) {
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

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider:   fakeProvider{text: "should not be used"},
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/cancel"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if len(texts) != 1 || !strings.Contains(texts[0], "Нет активного выполнения") {
		t.Fatalf("unexpected cancel response: %#v", texts)
	}
}

func TestAdapterReplyReservesBtwCommandWithoutActiveRun(t *testing.T) {
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

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider:   fakeProvider{text: "should not be used"},
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/btw проверь позже"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if len(texts) != 1 || !strings.Contains(texts[0], "Нет активного выполнения") {
		t.Fatalf("unexpected btw response: %#v", texts)
	}
}

func TestAdapterReplyStoresBtwNoteForActiveRun(t *testing.T) {
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

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider:   fakeProvider{text: "should not be used"},
	})
	adapter.runs.CreateWithID(1001, adapter.runs.AllocateID(), "deploy", time.Now().UTC())

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/btw проверь журналы после ответа"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if len(texts) != 1 || !strings.Contains(texts[0], "Заметка добавлена") {
		t.Fatalf("unexpected btw ack: %#v", texts)
	}

	messages, err := adapter.store.Messages(1001)
	if err != nil {
		t.Fatalf("messages: %v", err)
	}
	if len(messages) != 1 {
		t.Fatalf("expected one stored note, got %#v", messages)
	}
	if messages[0].Role != "user" {
		t.Fatalf("expected user role, got %#v", messages)
	}
	if !strings.Contains(messages[0].Content, "Operator note (out-of-band):") {
		t.Fatalf("expected operator note marker, got %#v", messages)
	}
	if !strings.Contains(messages[0].Content, "проверь журналы после ответа") {
		t.Fatalf("expected note content, got %#v", messages)
	}
}

func TestAdapterReplyRejectsUnknownSlashCommandWithoutInvokingProvider(t *testing.T) {
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

	var providerCalled bool
	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider: captureProvider{
			onGenerate: func(req provider.PromptRequest) { providerCalled = true },
			response:   provider.PromptResponse{Text: "should not be used", Model: "glm-5"},
		},
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/skill list"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if providerCalled {
		t.Fatal("expected unknown slash command to be handled before provider call")
	}
	if len(texts) != 1 || !strings.Contains(texts[0], "unknown command") {
		t.Fatalf("unexpected response: %#v", texts)
	}
}

func TestAdapterReplyInjectsWorkspaceAgentsContext(t *testing.T) {
	workspaceDir := t.TempDir()
	agentsPath := workspaceDir + "/AGENTS.md"
	if err := os.WriteFile(agentsPath, []byte("Agent rule: stay concise."), 0o644); err != nil {
		t.Fatalf("write AGENTS: %v", err)
	}

	var got provider.PromptRequest
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:       server.URL,
		Token:         "test-token",
		HTTPClient:    server.Client(),
		WorkspaceRoot: workspaceDir,
		Provider: captureProvider{
			onGenerate: func(req provider.PromptRequest) { got = req },
			response:   provider.PromptResponse{Text: "ok", Model: "glm-5"},
		},
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "hello bot"}); err != nil {
		t.Fatalf("reply: %v", err)
	}

	found := false
	for _, msg := range got.Messages {
		if msg.Role == "system" && strings.Contains(msg.Content, "AGENTS.md") && strings.Contains(msg.Content, "stay concise") {
			found = true
			break
		}
	}
	if !found {
		t.Fatalf("expected AGENTS.md workspace context in provider messages, got %#v", got.Messages)
	}
}

func TestAdapterReplyInjectsRecentWorkPromptFromSessionHead(t *testing.T) {
	runtimeStore, err := runtimex.NewSQLiteStore(t.TempDir() + "/runtime.db")
	if err != nil {
		t.Fatalf("new runtime store: %v", err)
	}
	if err := runtimeStore.SaveSessionHead(runtimex.SessionHead{
		ChatID:             1001,
		SessionID:          "1001:default",
		LastCompletedRunID: "run-prev",
		CurrentGoal:        "обновить шаблон астры",
		LastResultSummary:  "шаблон обновлён и выключен",
		RecentArtifactRefs: []string{"artifact://run/run-prev/report"},
	}); err != nil {
		t.Fatalf("save session head: %v", err)
	}

	var got provider.PromptRequest
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
		RunStore:   runtimeStore,
		Provider: captureProvider{
			onGenerate: func(req provider.PromptRequest) { got = req },
			response:   provider.PromptResponse{Text: "ok", Model: "glm-5"},
		},
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "продолжай"}); err != nil {
		t.Fatalf("reply: %v", err)
	}

	prompt := joinPromptContents(got.Messages)
	if !strings.Contains(prompt, "recent completed work") {
		t.Fatalf("expected recent work prompt in provider messages, got %#v", got.Messages)
	}
	if !strings.Contains(prompt, "run-prev") {
		t.Fatalf("expected last completed run in prompt, got %#v", got.Messages)
	}
}

func TestAdapterReplyInjectsPlanFromSessionHead(t *testing.T) {
	runtimeStore, err := runtimex.NewSQLiteStore(t.TempDir() + "/runtime.db")
	if err != nil {
		t.Fatalf("new runtime store: %v", err)
	}
	if err := runtimeStore.SaveSessionHead(runtimex.SessionHead{
		ChatID:            1001,
		SessionID:         "1001:default",
		CurrentGoal:       "отладить prompt assembly",
		CurrentPlanID:     "plan-1",
		CurrentPlanTitle:  "Inspect prompt assembly",
		CurrentPlanItems:  []string{"[in_progress] Inspect transcript", "[pending] Inspect memory recall"},
		LastResultSummary: "runtime responds",
	}); err != nil {
		t.Fatalf("save session head: %v", err)
	}

	var got provider.PromptRequest
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":9}}`))
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		RunStore:   runtimeStore,
		Provider: captureProvider{
			onGenerate: func(req provider.PromptRequest) { got = req },
			response:   provider.PromptResponse{Text: "ok", Model: "glm-5"},
		},
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "что дальше"}); err != nil {
		t.Fatalf("reply: %v", err)
	}

	prompt := joinPromptContents(got.Messages)
	if !strings.Contains(prompt, "Current plan: Inspect prompt assembly") || !strings.Contains(prompt, "[in_progress] Inspect transcript") {
		t.Fatalf("expected plan in session head prompt, got %#v", got.Messages)
	}
}

func TestAdapterRuntimeCommandsUpdateSessionRuntimeConfig(t *testing.T) {
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

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/model set glm-4.5"}); err != nil {
		t.Fatalf("model set: %v", err)
	}
	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/reasoning mode disabled"}); err != nil {
		t.Fatalf("reasoning mode: %v", err)
	}
	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/params set temperature=0.7"}); err != nil {
		t.Fatalf("params set: %v", err)
	}
	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/runtime"}); err != nil {
		t.Fatalf("runtime: %v", err)
	}

	last := texts[len(texts)-1]
	if !strings.Contains(last, "model: glm-4.5") || !strings.Contains(last, "reasoning_mode: disabled") || !strings.Contains(last, "temperature: 0.7") {
		t.Fatalf("unexpected runtime summary: %q", last)
	}
	if !strings.Contains(last, "Action policy") || !strings.Contains(last, "approval_required_tools: shell.exec,filesystem.write_file") {
		t.Fatalf("expected action policy in runtime summary: %q", last)
	}
	if !strings.Contains(last, "Memory policy") || !strings.Contains(last, "promote_continuity: true") {
		t.Fatalf("expected memory policy in runtime summary: %q", last)
	}
}

func TestAdapterApprovalsCommandShowsPendingApprovals(t *testing.T) {
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

	svc := approvals.New(approvals.TestDeps())
	if _, err := svc.Create(approvals.Request{
		WorkerID:  "shell.exec",
		SessionID: "1001:default",
		Payload:   "{}",
	}); err != nil {
		t.Fatalf("create approval: %v", err)
	}

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Approvals:  svc,
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/approvals"}); err != nil {
		t.Fatalf("approvals: %v", err)
	}

	last := texts[len(texts)-1]
	if !strings.Contains(last, "Approvals") || !strings.Contains(last, "approval-1") || !strings.Contains(last, "shell.exec") {
		t.Fatalf("unexpected approvals summary: %q", last)
	}
}

func TestAdapterMemoryPolicyCommandShowsConfiguredPolicy(t *testing.T) {
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

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		MemoryPolicy: runtimex.MemoryPolicy{
			Profile:              "standard",
			PromoteCheckpoint:    true,
			PromoteContinuity:    true,
			AutomaticRecallKinds: []string{"continuity", "checkpoint"},
			MaxDocumentBodyChars: 900,
			MaxResolvedFacts:     5,
		},
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/memory policy"}); err != nil {
		t.Fatalf("memory policy: %v", err)
	}

	last := texts[len(texts)-1]
	if !strings.Contains(last, "profile: standard") ||
		!strings.Contains(last, "promote_checkpoint: true") ||
		!strings.Contains(last, "recall_kinds: continuity,checkpoint") {
		t.Fatalf("unexpected memory policy summary: %q", last)
	}
}

func TestAdapterSkillsCommandsManageSessionSkills(t *testing.T) {
	workspaceDir := t.TempDir()
	if err := os.MkdirAll(workspaceDir+"/skills/deploy", 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(workspaceDir+"/skills/deploy/SKILL.md", []byte("---\nname: deploy\ndescription: Safe deploy workflow\n---\n\nUse deploy flow."), 0o644); err != nil {
		t.Fatal(err)
	}

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

	adapter := New(Deps{
		BaseURL:       server.URL,
		Token:         "test-token",
		HTTPClient:    server.Client(),
		WorkspaceRoot: workspaceDir,
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/skills list"}); err != nil {
		t.Fatalf("skills list: %v", err)
	}
	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/skills use deploy"}); err != nil {
		t.Fatalf("skills use: %v", err)
	}
	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/skills"}); err != nil {
		t.Fatalf("skills: %v", err)
	}
	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/skills drop deploy"}); err != nil {
		t.Fatalf("skills drop: %v", err)
	}

	joined := strings.Join(texts, "\n")
	if !strings.Contains(joined, "deploy") {
		t.Fatalf("expected deploy in skills responses, got %q", joined)
	}
	if !strings.Contains(joined, "active skills") {
		t.Fatalf("expected active skills summary, got %q", joined)
	}
}

func TestAdapterSkillsCommandRejectsUnexpectedExtraArgs(t *testing.T) {
	workspaceDir := t.TempDir()
	if err := os.MkdirAll(workspaceDir+"/skills/deploy", 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(workspaceDir+"/skills/deploy/SKILL.md", []byte("---\nname: deploy\ndescription: Safe deploy workflow\n---\n\nUse deploy flow."), 0o644); err != nil {
		t.Fatal(err)
	}

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

	adapter := New(Deps{
		BaseURL:       server.URL,
		Token:         "test-token",
		HTTPClient:    server.Client(),
		WorkspaceRoot: workspaceDir,
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/skills list show"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if len(texts) != 1 || !strings.Contains(texts[0], "usage: /skills list") {
		t.Fatalf("unexpected response: %#v", texts)
	}
}

func TestAdapterReplyInjectsSkillsCatalogAndActiveSkillIntoProviderPrompt(t *testing.T) {
	workspaceDir := t.TempDir()
	if err := os.MkdirAll(workspaceDir+"/skills/deploy", 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(workspaceDir+"/skills/deploy/SKILL.md", []byte("---\nname: deploy\ndescription: Safe deploy workflow\nversion: 1\n---\n\nUse deploy flow."), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(workspaceDir+"/AGENTS.md", []byte("Agent rule: stay concise."), 0o644); err != nil {
		t.Fatal(err)
	}

	var got provider.PromptRequest
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:       server.URL,
		Token:         "test-token",
		HTTPClient:    server.Client(),
		WorkspaceRoot: workspaceDir,
		Provider: captureProvider{
			onGenerate: func(req provider.PromptRequest) { got = req },
			response:   provider.PromptResponse{Text: "ok", Model: "glm-5"},
		},
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/skills use deploy"}); err != nil {
		t.Fatalf("skills use: %v", err)
	}
	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "hello bot"}); err != nil {
		t.Fatalf("reply: %v", err)
	}

	var foundAgents, foundCatalog, foundSkill bool
	for _, msg := range got.Messages {
		if msg.Role != "system" {
			continue
		}
		if strings.Contains(msg.Content, "AGENTS.md") {
			foundAgents = true
		}
		if strings.Contains(msg.Content, "## Available skills") && strings.Contains(msg.Content, "deploy") {
			foundCatalog = true
		}
		if strings.Contains(msg.Content, "Use deploy flow.") {
			foundSkill = true
		}
	}
	if !foundAgents || !foundCatalog || !foundSkill {
		t.Fatalf("expected AGENTS+catalog+skill injection, got %#v", got.Messages)
	}
}
