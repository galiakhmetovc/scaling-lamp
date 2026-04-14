package telegram

import (
	"context"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync"
	"testing"
	"time"

	"teamd/internal/mcp"
	"teamd/internal/memory"
	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
	"teamd/internal/worker"
)

func TestAdapterReplyInjectsMemoryRecallIntoProviderPrompt(t *testing.T) {
	workspaceDir := t.TempDir()
	if err := os.WriteFile(workspaceDir+"/AGENTS.md", []byte("Agent rule: stay concise."), 0o644); err != nil {
		t.Fatal(err)
	}
	mem := memory.NewInMemorySemanticStore()
	if err := mem.UpsertDocument(memory.Document{
		DocKey:    "continuity:1001:default",
		Scope:     memory.ScopeSession,
		ChatID:    1001,
		SessionID: "1001:default",
		Kind:      "continuity",
		Title:     "Проверка памяти сервера",
		Body:      "RAM почти заполнена, swap тоже почти заполнен.",
	}); err != nil {
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
		Memory:        mem,
		Provider: captureProvider{
			onGenerate: func(req provider.PromptRequest) { got = req },
			response: provider.PromptResponse{Text: "ok", Model: "glm-5"},
		},
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "что с памятью на сервере?"}); err != nil {
		t.Fatalf("reply: %v", err)
	}

	joined := joinPromptContents(got.Messages)
	if !strings.Contains(joined, "Relevant memory recall.") {
		t.Fatalf("expected memory recall in prompt, got %q", joined)
	}
	if !strings.Contains(joined, "RAM почти заполнена") {
		t.Fatalf("expected recalled fact in prompt, got %q", joined)
	}
}

func TestAdapterReplyAddsSkillsToolsToProviderRequest(t *testing.T) {
	workspaceDir := t.TempDir()
	if err := os.MkdirAll(workspaceDir+"/skills/deploy", 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(workspaceDir+"/skills/deploy/SKILL.md", []byte("---\nname: deploy\ndescription: Safe deploy workflow\n---\n\nUse deploy flow."), 0o644); err != nil {
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

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "hello bot"}); err != nil {
		t.Fatalf("reply: %v", err)
	}

	var sawList, sawRead, sawActivate bool
	for _, tool := range got.Tools {
		if tool.Name == skillsToolListName {
			sawList = true
		}
		if tool.Name == skillsToolReadName {
			sawRead = true
		}
		if tool.Name == skillsToolActivateName {
			sawActivate = true
		}
	}
	if !sawList || !sawRead || !sawActivate {
		t.Fatalf("expected skills tools in provider request, got %#v", got.Tools)
	}
}

func TestAdapterReplyAddsMemoryToolsToProviderRequest(t *testing.T) {
	mem := memory.NewInMemorySemanticStore()
	if err := mem.UpsertDocument(memory.Document{
		DocKey:    "continuity:1001:default",
		Scope:     memory.ScopeSession,
		ChatID:    1001,
		SessionID: "1001:default",
		Kind:      "continuity",
		Title:     "Server memory",
		Body:      "RAM almost full.",
	}); err != nil {
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
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Memory:     mem,
		Provider: captureProvider{
			onGenerate: func(req provider.PromptRequest) { got = req },
			response:   provider.PromptResponse{Text: "ok", Model: "glm-5"},
		},
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "remember this"}); err != nil {
		t.Fatalf("reply: %v", err)
	}

	var sawSearch, sawRead bool
	for _, tool := range got.Tools {
		if tool.Name == memoryToolSearchName {
			sawSearch = true
		}
		if tool.Name == memoryToolReadName {
			sawRead = true
		}
	}
	if !sawSearch || !sawRead {
		t.Fatalf("expected memory tools in provider request, got %#v", got.Tools)
	}
}

func TestAdapterExecuteMemorySearchToolReturnsFullDocKeysAndSummary(t *testing.T) {
	mem := memory.NewInMemorySemanticStore()
	if err := mem.UpsertDocument(memory.Document{
		DocKey:    "checkpoint:1001:default",
		Scope:     memory.ScopeSession,
		ChatID:    1001,
		SessionID: "1001:default",
		Kind:      "checkpoint",
		Title:     "SearXNG setup",
		Body:      "Local SearXNG runs on localhost:8888 and should be queried with format=json.",
		Source:    "runtime_checkpoint",
	}); err != nil {
		t.Fatal(err)
	}

	adapter := New(Deps{Memory: mem})
	out, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: memoryToolSearchName,
		Arguments: map[string]any{
			"query": "searxng localhost format json",
			"limit": 3,
		},
	})
	if err != nil {
		t.Fatalf("execute memory search: %v", err)
	}
	if !strings.Contains(out, "doc_key=checkpoint:1001:default") {
		t.Fatalf("expected doc key in search output, got %q", out)
	}
	if !strings.Contains(out, "format=json") {
		t.Fatalf("expected body summary in search output, got %q", out)
	}
}

func TestAdapterExecuteMemorySearchToolPrefersRecentWorkSnapshotForProjectCapture(t *testing.T) {
	mem := memory.NewInMemorySemanticStore()
	if err := mem.UpsertDocument(memory.Document{
		DocKey:    "continuity:1001:default",
		Scope:     memory.ScopeSession,
		ChatID:    1001,
		SessionID: "1001:default",
		Kind:      "continuity",
		Title:     "Old note",
		Body:      "Старое общее воспоминание по проекту.",
		Source:    "runtime_continuity",
	}); err != nil {
		t.Fatal(err)
	}
	runtimeStore, err := runtimex.NewSQLiteStore(t.TempDir() + "/runtime.db")
	if err != nil {
		t.Fatalf("new runtime store: %v", err)
	}
	now := time.Now().UTC()
	if err := runtimeStore.SaveRun(runtimex.RunRecord{
		RunID:         "run-prev",
		ChatID:        1001,
		SessionID:     "1001:default",
		Query:         "обновить шаблон астры",
		Status:        runtimex.StatusCompleted,
		FinalResponse: "шаблон обновлён и выключен",
		StartedAt:     now,
	}); err != nil {
		t.Fatalf("save run: %v", err)
	}
	if err := runtimeStore.SaveSessionHead(runtimex.SessionHead{
		ChatID:             1001,
		SessionID:          "1001:default",
		LastCompletedRunID: "run-prev",
		CurrentGoal:        "обновить шаблон астры",
		LastResultSummary:  "шаблон обновлён и выключен",
		RecentArtifactRefs: []string{"artifact://run/run-prev/report"},
		UpdatedAt:          now,
	}); err != nil {
		t.Fatalf("save session head: %v", err)
	}

	adapter := New(Deps{Memory: mem, RunStore: runtimeStore})
	out, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: memoryToolSearchName,
		Arguments: map[string]any{
			"query": "запиши это как проект",
			"limit": 3,
		},
	})
	if err != nil {
		t.Fatalf("execute memory search: %v", err)
	}
	if !strings.Contains(out, "recent work snapshot") {
		t.Fatalf("expected recent work snapshot in output, got %q", out)
	}
	if !strings.Contains(out, "last_completed_run_id=run-prev") || !strings.Contains(out, "replay_run=run-prev") {
		t.Fatalf("expected recent run details before memory recall, got %q", out)
	}
	if !strings.Contains(out, "only ask for the target project path or name if that specific target is missing") {
		t.Fatalf("expected narrow project-target guidance in snapshot, got %q", out)
	}
}

func TestAdapterExecuteProjectCaptureRecentToolCreatesProjectFiles(t *testing.T) {
	workspaceDir := t.TempDir()
	runtimeStore, err := runtimex.NewSQLiteStore(t.TempDir() + "/runtime.db")
	if err != nil {
		t.Fatalf("new runtime store: %v", err)
	}
	now := time.Now().UTC()
	if err := runtimeStore.SaveRun(runtimex.RunRecord{
		RunID:         "run-prev",
		ChatID:        1001,
		SessionID:     "1001:default",
		Query:         "обновить шаблон астры",
		Status:        runtimex.StatusCompleted,
		FinalResponse: "шаблон обновлён и выключен",
		StartedAt:     now,
	}); err != nil {
		t.Fatalf("save run: %v", err)
	}
	if err := runtimeStore.SaveSessionHead(runtimex.SessionHead{
		ChatID:             1001,
		SessionID:          "1001:default",
		LastCompletedRunID: "run-prev",
		CurrentGoal:        "обновить шаблон астры",
		LastResultSummary:  "шаблон обновлён и выключен",
		RecentArtifactRefs: []string{"artifact://run/run-prev/report"},
		UpdatedAt:          now,
	}); err != nil {
		t.Fatalf("save session head: %v", err)
	}

	adapter := New(Deps{RunStore: runtimeStore, WorkspaceRoot: workspaceDir})
	out, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: projectCaptureRecentToolName,
		Arguments: map[string]any{
			"project_path": "projects/astra-template-update",
			"title":        "Astra Template Update",
		},
	})
	if err != nil {
		t.Fatalf("execute project capture: %v", err)
	}
	if !strings.Contains(out, "project captured:") {
		t.Fatalf("expected capture summary, got %q", out)
	}
	for _, path := range []string{
		workspaceDir + "/projects/astra-template-update/README.md",
		workspaceDir + "/projects/astra-template-update/docs/architecture.md",
		workspaceDir + "/projects/astra-template-update/state/current.md",
		workspaceDir + "/projects/index.md",
	} {
		if _, err := os.Stat(path); err != nil {
			t.Fatalf("expected project file %s: %v", path, err)
		}
	}
	head, ok, err := runtimeStore.SessionHead(1001, "1001:default")
	if err != nil || !ok {
		t.Fatalf("session head after project capture: ok=%v err=%v", ok, err)
	}
	if head.CurrentProject != "projects/astra-template-update" {
		t.Fatalf("expected CurrentProject to be updated, got %+v", head)
	}
}

func TestAdapterExecuteMemoryReadToolReturnsFullDocument(t *testing.T) {
	mem := memory.NewInMemorySemanticStore()
	if err := mem.UpsertDocument(memory.Document{
		DocKey:    "continuity:1001:default",
		Scope:     memory.ScopeSession,
		ChatID:    1001,
		SessionID: "1001:default",
		Kind:      "continuity",
		Title:     "Memory pressure",
		Body:      "RAM almost full. Swap almost full.",
		Source:    "runtime_continuity",
	}); err != nil {
		t.Fatal(err)
	}

	adapter := New(Deps{Memory: mem})
	out, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: memoryToolReadName,
		Arguments: map[string]any{
			"doc_key": "continuity:1001:default",
		},
	})
	if err != nil {
		t.Fatalf("execute memory read: %v", err)
	}
	if !strings.Contains(out, "doc_key: continuity:1001:default") {
		t.Fatalf("expected full document header, got %q", out)
	}
	if !strings.Contains(out, "RAM almost full. Swap almost full.") {
		t.Fatalf("expected full body, got %q", out)
	}
}

func TestCheckpointMemoryDocumentRejectsNoisySearchLikeContent(t *testing.T) {
	doc, ok := runtimex.BuildCheckpointDocument(1001, "1001:default", "Запомни - пароль qlbch7h2v", worker.Checkpoint{
		WhatHappened:   "results count: 22\nanswers count: 0\ninfoboxes count: 0\n---\ntitle: Quelle est la température maximale pour aujourd’hui à Moscou ?\ncontent: La température maximale...",
		WhatMattersNow: "title: Quelle est la température maximale pour aujourd’hui à Moscou ?",
	}, time.Now().UTC())
	if ok {
		t.Fatalf("expected noisy checkpoint to be rejected, got %+v", doc)
	}
}

func TestCheckpointMemoryDocumentAcceptsDurableSummary(t *testing.T) {
	doc, ok := runtimex.BuildCheckpointDocumentWithPolicy(runtimex.MemoryPolicy{
		PromoteCheckpoint: true,
		MaxDocumentBodyChars: 600,
		MaxResolvedFacts: 3,
	}, 1001, "1001:default", "Напиши пароль", worker.Checkpoint{
		WhatHappened:   "Пользователь попросил сохранить пароль qlbch7h2v для последующего восстановления.",
		WhatMattersNow: "Пароль qlbch7h2v должен быть доступен для следующей сессии.",
	}, time.Now().UTC())
	if !ok {
		t.Fatal("expected durable checkpoint summary to be promoted")
	}
	if doc.Title != "Напиши пароль" {
		t.Fatalf("unexpected title: %q", doc.Title)
	}
	if !strings.Contains(doc.Body, "qlbch7h2v") {
		t.Fatalf("expected durable body, got %q", doc.Body)
	}
}

func TestAdapterReplyAppliesSessionRuntimeConfigToProviderRequest(t *testing.T) {
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
		Provider: captureProvider{
			onGenerate: func(req provider.PromptRequest) { got = req },
			response:   provider.PromptResponse{Text: "ok", Model: "glm-4.5"},
		},
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/model set glm-4.5"}); err != nil {
		t.Fatalf("model set: %v", err)
	}
	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/reasoning mode disabled"}); err != nil {
		t.Fatalf("reasoning mode: %v", err)
	}
	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/params set top_p=0.8"}); err != nil {
		t.Fatalf("params set: %v", err)
	}
	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "hello bot"}); err != nil {
		t.Fatalf("reply: %v", err)
	}

	if got.Config.Model != "glm-4.5" {
		t.Fatalf("unexpected request model: %#v", got.Config)
	}
	if got.Config.ReasoningMode != "disabled" {
		t.Fatalf("unexpected reasoning mode: %#v", got.Config)
	}
	if got.Config.TopP == nil || *got.Config.TopP != 0.8 {
		t.Fatalf("unexpected top_p: %#v", got.Config)
	}
}

func TestAdapterReplySendsFirstModelAnswerWithoutCompletionPass(t *testing.T) {
	serverTexts := []string{}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		serverTexts = append(serverTexts, r.PostForm.Get("text"))
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	prov := &scriptedProvider{
		responses: []provider.PromptResponse{
			{
				Text:  "Только про скиллы.",
				Model: "glm-5",
			},
		},
	}

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider:   prov,
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "что у тебя в контексте? какие тулзы доступны? какие скиллы доступны?"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if len(prov.requests) != 1 {
		t.Fatalf("expected single provider request, got %d requests", len(prov.requests))
	}
	last := serverTexts[len(serverTexts)-1]
	if !strings.Contains(last, "Только про скиллы.") {
		t.Fatalf("unexpected final text: %q", last)
	}
}

func TestAdapterDispatchRunsConversationAsyncAndAllowsCancel(t *testing.T) {
	var (
		mu      sync.Mutex
		texts   []string
		markups []string
	)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		mu.Lock()
		texts = append(texts, r.PostForm.Get("text"))
		markups = append(markups, r.PostForm.Get("reply_markup"))
		mu.Unlock()
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	provider := &blockingProvider{started: make(chan struct{}), released: make(chan struct{})}
	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider:   provider,
	})

	if err := adapter.Dispatch(context.Background(), Update{ChatID: 1001, Text: "hello bot"}); err != nil {
		t.Fatalf("dispatch run: %v", err)
	}

	select {
	case <-provider.started:
	case <-time.After(time.Second):
		t.Fatal("provider generate did not start")
	}

	if err := adapter.Dispatch(context.Background(), Update{ChatID: 1001, Text: "/cancel"}); err != nil {
		t.Fatalf("dispatch cancel: %v", err)
	}

	select {
	case <-provider.released:
	case <-time.After(time.Second):
		t.Fatal("provider generate was not cancelled")
	}

	deadline := time.Now().Add(time.Second)
	for {
		mu.Lock()
		joined := strings.Join(texts, "\n")
		mu.Unlock()
		if strings.Contains(joined, "Отмена запрошена") && strings.Contains(joined, "Выполнение отменено") {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("unexpected async responses: %q", joined)
		}
		time.Sleep(10 * time.Millisecond)
	}
}

func TestAdapterDispatchRejectsSecondUserRunWhileChatIsBusy(t *testing.T) {
	var (
		mu      sync.Mutex
		texts   []string
		markups []string
	)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		mu.Lock()
		texts = append(texts, r.PostForm.Get("text"))
		markups = append(markups, r.PostForm.Get("reply_markup"))
		mu.Unlock()
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	provider := &blockingProvider{started: make(chan struct{}), released: make(chan struct{})}
	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider:   provider,
	})

	if err := adapter.Dispatch(context.Background(), Update{ChatID: 1001, Text: "first"}); err != nil {
		t.Fatalf("dispatch first run: %v", err)
	}
	select {
	case <-provider.started:
	case <-time.After(time.Second):
		t.Fatal("provider generate did not start")
	}

	if err := adapter.Dispatch(context.Background(), Update{ChatID: 1001, Text: "second"}); err != nil {
		t.Fatalf("dispatch second run: %v", err)
	}

	mu.Lock()
	joined := strings.Join(texts, "\n")
	joinedMarkup := strings.Join(markups, "\n")
	mu.Unlock()
	if !strings.Contains(joined, "Уже выполняю предыдущий запрос") {
		t.Fatalf("expected busy message, got %q", joined)
	}
	if !strings.Contains(joinedMarkup, "busy:queue") || !strings.Contains(joinedMarkup, "busy:interrupt") {
		t.Fatalf("expected busy keyboard, got %q", joinedMarkup)
	}

	if err := adapter.Dispatch(context.Background(), Update{ChatID: 1001, Text: "/cancel"}); err != nil {
		t.Fatalf("dispatch cancel: %v", err)
	}
	select {
	case <-provider.released:
	case <-time.After(time.Second):
		t.Fatal("provider generate was not cancelled")
	}
}

func TestAdapterBusyQueueStartsQueuedMessageAfterCurrentRun(t *testing.T) {
	var mu sync.Mutex
	var texts []string
	firstStarted := make(chan struct{})
	firstRelease := make(chan struct{})
	secondStarted := make(chan struct{}, 1)
	callIndex := 0

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		if r.URL.Path == "/bottest-token/sendMessage" {
			mu.Lock()
			texts = append(texts, r.PostForm.Get("text"))
			mu.Unlock()
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	provider := captureProvider{
		onGenerate: func(req provider.PromptRequest) {
			mu.Lock()
			idx := callIndex
			callIndex++
			mu.Unlock()
			if idx == 0 {
				close(firstStarted)
				<-firstRelease
				return
			}
			if last := req.Messages[len(req.Messages)-1].Content; last != "second" {
				t.Errorf("expected queued message to run next, got %q", last)
			}
			secondStarted <- struct{}{}
		},
		response: provider.PromptResponse{Text: "ok", Model: "glm-5"},
	}
	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider:   provider,
	})

	if err := adapter.Dispatch(context.Background(), Update{ChatID: 1001, Text: "first"}); err != nil {
		t.Fatalf("dispatch first: %v", err)
	}
	select {
	case <-firstStarted:
	case <-time.After(time.Second):
		t.Fatal("first run did not start")
	}

	if err := adapter.Dispatch(context.Background(), Update{ChatID: 1001, Text: "second"}); err != nil {
		t.Fatalf("dispatch second: %v", err)
	}
	if err := adapter.Reply(context.Background(), Update{
		ChatID:        1001,
		CallbackID:    "cb-busy-1",
		CallbackData:  "busy:queue",
		CallbackQuery: true,
	}); err != nil {
		t.Fatalf("busy queue callback: %v", err)
	}
	close(firstRelease)
	select {
	case <-secondStarted:
	case <-time.After(3 * time.Second):
		mu.Lock()
		defer mu.Unlock()
		t.Fatalf("expected queued message to run after current run, texts=%#v", texts)
	}
}

func TestAdapterDispatchIgnoresDuplicateTelegramUpdateIDs(t *testing.T) {
	runtimeStore, err := runtimex.NewSQLiteStore(t.TempDir() + "/runtime.db")
	if err != nil {
		t.Fatalf("new runtime store: %v", err)
	}
	var texts []string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Fatalf("parse form: %v", err)
		}
		if r.URL.Path == "/bottest-token/sendMessage" {
			texts = append(texts, r.PostForm.Get("text"))
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":8}}`))
	}))
	defer server.Close()

	adapter := New(Deps{
		BaseURL:    server.URL,
		Token:      "test-token",
		HTTPClient: server.Client(),
		Provider:   fakeProvider{text: "ok", model: "glm-5"},
		RunStore:   runtimeStore,
	})
	update := Update{UpdateID: 42, ChatID: 1001, Text: "hello"}

	if err := adapter.Dispatch(context.Background(), update); err != nil {
		t.Fatalf("first dispatch: %v", err)
	}
	deadline := time.Now().Add(time.Second)
	for len(texts) < 2 && time.Now().Before(deadline) {
		time.Sleep(10 * time.Millisecond)
	}
	if err := adapter.Dispatch(context.Background(), update); err != nil {
		t.Fatalf("duplicate dispatch: %v", err)
	}
	time.Sleep(50 * time.Millisecond)

	if len(texts) != 2 {
		t.Fatalf("expected duplicate update to be ignored after first ack+reply, got %#v", texts)
	}
}

func TestAdapterRunConversationReturnsProviderRoundTimeout(t *testing.T) {
	adapter := New(Deps{
		Provider:             &blockingProvider{started: make(chan struct{}), released: make(chan struct{})},
		ProviderRoundTimeout: 20 * time.Millisecond,
	})
	if err := adapter.store.Append(1001, provider.Message{Role: "user", Content: "hello bot"}); err != nil {
		t.Fatalf("append user: %v", err)
	}

	_, err := adapter.runConversation(context.Background(), 1001)
	if err == nil || !strings.Contains(err.Error(), "llm round timed out after 20ms") {
		t.Fatalf("expected provider timeout error, got %v", err)
	}
}

func TestAdapterRunConversationBreaksRepeatedIdenticalToolCalls(t *testing.T) {
	prov := &scriptedProvider{
		responses: []provider.PromptResponse{
			{
				FinishReason: "tool_calls",
				ToolCalls: []provider.ToolCall{
					{ID: "call-1", Name: providerToolName("shell.exec"), Arguments: map[string]any{"command": "echo hi"}},
				},
			},
			{
				FinishReason: "tool_calls",
				ToolCalls: []provider.ToolCall{
					{ID: "call-2", Name: providerToolName("shell.exec"), Arguments: map[string]any{"command": "echo hi"}},
				},
			},
			{
				Text:  "Переключаюсь на другой подход.",
				Model: "glm-5",
			},
		},
	}
	tools := &fakeToolRuntime{
		tools: []mcp.Tool{{Name: "shell.exec", Description: "exec shell"}},
		result: mcp.CallResult{Content: "ok"},
	}
	adapter := New(Deps{Provider: prov, Tools: tools})
	if err := adapter.store.Append(1001, provider.Message{Role: "user", Content: "проверь"}); err != nil {
		t.Fatalf("append user: %v", err)
	}

	resp, err := adapter.runConversation(context.Background(), 1001)
	if err != nil {
		t.Fatalf("runConversation: %v", err)
	}
	if resp.Text != "Переключаюсь на другой подход." {
		t.Fatalf("unexpected final text: %q", resp.Text)
	}
	if len(tools.calls) != 1 {
		t.Fatalf("expected repeated tool call to be short-circuited, got %d tool executions", len(tools.calls))
	}
	messages, err := adapter.store.Messages(1001)
	if err != nil {
		t.Fatalf("messages: %v", err)
	}
	joined := ""
	for _, msg := range messages {
		joined += "\n" + msg.Content
	}
	if !strings.Contains(joined, "tool guard triggered") {
		t.Fatalf("expected synthetic loop breaker tool output, got %q", joined)
	}
}

func TestAdapterRunConversationStopsOnAdvisoryDraftBeforeGeneralResearchTool(t *testing.T) {
	prov := &scriptedProvider{
		responses: []provider.PromptResponse{
			{
				Text:         "Я бы рекомендовал оставить SearXNG локальным и не усложнять hardening.",
				FinishReason: "tool_calls",
				ToolCalls: []provider.ToolCall{
					{ID: "call-1", Name: providerToolName("shell.exec"), Arguments: map[string]any{"command": "curl -s http://localhost:8888/search?q=searxng+best+practices"}},
				},
			},
		},
	}
	tools := &fakeToolRuntime{
		tools: []mcp.Tool{{Name: "shell.exec", Description: "exec shell"}},
		result: mcp.CallResult{Content: "ok"},
	}
	adapter := New(Deps{Provider: prov, Tools: tools})
	if err := adapter.store.Append(1001, provider.Message{Role: "user", Content: "Ну ты что посоветуешь? Он только для тебя, доступа из интернета к нему нет"}); err != nil {
		t.Fatalf("append user: %v", err)
	}

	resp, err := adapter.runConversation(context.Background(), 1001)
	if err != nil {
		t.Fatalf("runConversation: %v", err)
	}
	if !strings.Contains(resp.Text, "Я бы рекомендовал") {
		t.Fatalf("expected advisory draft to be returned, got %q", resp.Text)
	}
	if len(tools.calls) != 0 {
		t.Fatalf("expected general research tool call to be skipped, got %d calls", len(tools.calls))
	}
}
