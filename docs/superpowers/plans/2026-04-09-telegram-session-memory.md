# Telegram Session Memory Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add in-process Telegram chat session memory so the bot sends recent dialogue history to `z.ai`, supports `/reset`, and reports session metrics that reflect the real chat context.

**Architecture:** Keep the current single-process Telegram runtime and add a small in-memory session store keyed by `chat_id`. The Telegram adapter owns session history for now, builds provider prompts from recent messages, supports explicit reset, and updates footer metrics from both `z.ai` usage and local session state. Session storage must be thread-safe, and footer formatting must be isolated from transport side effects so tests are robust.

**Tech Stack:** Go 1.24+, standard library collections, current Telegram adapter, current `z.ai` provider client, Go tests.

---

## File Structure

- Modify: `internal/provider/provider.go`
  Purpose: allow prompt messages with explicit roles instead of only raw strings.
- Modify: `internal/provider/zai/client.go`
  Purpose: convert structured prompt history into `z.ai` chat completion payload.
- Modify: `internal/provider/zai/client_test.go`
  Purpose: verify structured history and usage behavior.
- Create: `internal/transport/telegram/session.go`
  Purpose: hold in-memory chat session state and reset behavior.
- Modify: `internal/transport/telegram/adapter.go`
  Purpose: load/store chat history, handle `/reset`, build prompt history, and update footer counters.
- Modify: `internal/transport/telegram/adapter_test.go`
  Purpose: verify reset, history forwarding, and session metrics.
- Create: `internal/transport/telegram/footer.go`
  Purpose: format reply footer from structured metrics instead of raw string concatenation inside the adapter.
- Modify: `README.md`
  Purpose: document `/reset`, session-memory behavior, and footer fields if the repo has a suitable runtime usage section.

### Task 1: Add Structured Prompt Messages To Provider Contract

**Files:**
- Modify: `internal/provider/provider.go`
- Modify: `internal/provider/zai/client_test.go`

- [ ] **Step 1: Write the failing test for structured message roles**

```go
func TestClientGeneratePostsStructuredMessages(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		_ = json.NewDecoder(r.Body).Decode(&body)

		messages := body["messages"].([]any)
		first := messages[0].(map[string]any)
		second := messages[1].(map[string]any)

		if first["role"] != "user" || second["role"] != "assistant" {
			t.Fatalf("unexpected roles: %#v", messages)
		}

		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"ok"}}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, "test-key")
	_, _ = client.Generate(context.Background(), provider.PromptRequest{
		Messages: []provider.Message{
			{Role: "user", Content: "hello"},
			{Role: "assistant", Content: "hi"},
		},
	})
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/provider/zai -run TestClientGeneratePostsStructuredMessages -v`
Expected: FAIL because `provider.Message` does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Add to `internal/provider/provider.go`:

```go
type Message struct {
	Role    string
	Content string
}

type PromptRequest struct {
	WorkerID string
	Messages []Message
}
```

Update `FakeProvider.Generate` to read the last message content for its echo behavior.

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/provider/zai -run TestClientGeneratePostsStructuredMessages -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/provider/provider.go internal/provider/zai/client_test.go
git commit -m "feat: add structured provider prompt messages"
```

### Task 2: Add In-Memory Telegram Session Store

**Files:**
- Create: `internal/transport/telegram/session.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing test for session history and reset**

```go
func TestSessionStoreTracksHistoryAndReset(t *testing.T) {
	store := NewSessionStore(4)
	store.Append(1001, provider.Message{Role: "user", Content: "u1"})
	store.Append(1001, provider.Message{Role: "assistant", Content: "a1"})

	if len(store.Messages(1001)) != 2 {
		t.Fatalf("expected 2 messages")
	}

	store.Reset(1001)
	if len(store.Messages(1001)) != 0 {
		t.Fatalf("expected empty history after reset")
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestSessionStoreTracksHistoryAndReset -v`
Expected: FAIL because `SessionStore` does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Create `internal/transport/telegram/session.go` with:

```go
type SessionStore struct {
	mu       sync.RWMutex
	limit    int
	messages map[int64][]provider.Message
}
```

Methods:
- `NewSessionStore(limit int) *SessionStore`
- `Append(chatID int64, msg provider.Message)`
- `Messages(chatID int64) []provider.Message`
- `Reset(chatID int64)`

Trim stored history to the configured limit. Define and document that the limit counts individual `provider.Message` items, not user/assistant pairs.

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestSessionStoreTracksHistoryAndReset -v`
Expected: PASS

- [ ] **Step 5: Run race check for the store**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test -race ./internal/transport/telegram -run TestSessionStoreTracksHistoryAndReset -v`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/transport/telegram/session.go internal/transport/telegram/adapter_test.go
git commit -m "feat: add telegram in-memory session store"
```

### Task 3: Send Recent Dialogue History To z.ai

**Files:**
- Modify: `internal/provider/zai/client.go`
- Modify: `internal/provider/zai/client_test.go`
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing test for adapter history forwarding**

```go
func TestAdapterReplySendsSessionHistoryToProvider(t *testing.T) {
	var got []provider.Message

	adapter := New(Deps{
		Provider: captureProvider{onGenerate: func(req provider.PromptRequest) {
			got = req.Messages
		}},
	})

	_ = adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "first"})
	_ = adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "second"})

	if len(got) < 2 {
		t.Fatalf("expected history in provider request")
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestAdapterReplySendsSessionHistoryToProvider -v`
Expected: FAIL because the adapter still sends only one raw message.

- [ ] **Step 3: Write minimal implementation**

In `internal/transport/telegram/adapter.go`:
- add `sessions *SessionStore`
- optionally keep `inflight map[int64]context.CancelFunc` if reset cancellation is implemented in this slice
- before provider call, append current user message to session
- send `sessions.Messages(chatID)` in `provider.PromptRequest`
- after successful provider response, append assistant reply to session
- do not append assistant reply when provider call fails

In `internal/provider/zai/client.go`:
- map each `provider.Message` to the existing chat payload shape

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram ./internal/provider/zai -run 'TestAdapterReplySendsSessionHistoryToProvider|TestClientGeneratePostsStructuredMessages' -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/provider/zai/client.go internal/provider/zai/client_test.go internal/transport/telegram/adapter.go internal/transport/telegram/adapter_test.go
git commit -m "feat: send telegram session history to zai"
```

### Task 4: Add `/reset` Command And Reset Reply

**Files:**
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing test for `/reset`**

```go
func TestAdapterReplyResetsSessionOnResetCommand(t *testing.T) {
	var sent url.Values
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = r.ParseForm()
		sent = r.PostForm
		_, _ = w.Write([]byte(`{"ok":true,"result":{"message_id":1}}`))
	}))
	defer server.Close()

	adapter := New(Deps{BaseURL: server.URL, Token: "test-token", HTTPClient: server.Client()})
	_ = adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "/reset"})

	if !strings.Contains(sent.Get("text"), "session reset") {
		t.Fatalf("unexpected reset reply: %q", sent.Get("text"))
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestAdapterReplyResetsSessionOnResetCommand -v`
Expected: FAIL because `/reset` is currently treated like a normal prompt.

- [ ] **Step 3: Write minimal implementation**

In `internal/transport/telegram/adapter.go`:
- detect `strings.TrimSpace(update.Text) == "/reset"`
- clear session history for `chat_id`
- reset local message counter for `chat_id`
- if there is an active provider call for the same `chat_id`, cancel it before wiping the session
- send a fixed reply such as `session reset`
- skip provider call for this branch

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestAdapterReplyResetsSessionOnResetCommand -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/adapter.go internal/transport/telegram/adapter_test.go
git commit -m "feat: add telegram session reset command"
```

### Task 5: Make Footer Reflect Real Session State

**Files:**
- Create: `internal/transport/telegram/footer.go`
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing test for footer session counters**

```go
func TestFormatFooterUsesStructuredMetrics(t *testing.T) {
	text := formatFooter(FooterMetrics{
		Model:            "glm-5",
		Thinking:         "enabled",
		ClearThinking:    true,
		ContextTokens:    18,
		PromptTokens:     11,
		CompletionTokens: 7,
		SessionMessages:  4,
	})

	if !strings.Contains(text, "session_messages=4") {
		t.Fatalf("unexpected footer: %q", text)
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestAdapterReplyFooterShowsSessionHistorySize -v`
Expected: FAIL because footer formatting is still embedded in adapter logic and not exposed as structured metrics.

- [ ] **Step 3: Write minimal implementation**

Create `internal/transport/telegram/footer.go` with:

```go
type FooterMetrics struct {
	Model            string
	Thinking         string
	ClearThinking    bool
	ContextTokens    int
	PromptTokens     int
	CompletionTokens int
	SessionMessages  int
}
```

Add `formatFooter(metrics FooterMetrics) string`.

Update footer logic to report:
- `session_messages` as current stored message count for the chat
- `context_tokens` from `usage.total_tokens` when available
- `prompt_tokens` from `usage.prompt_tokens`
- `completion_tokens` from `usage.completion_tokens`

Keep `unknown` fallback when provider usage is absent.

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestAdapterReplyFooterShowsSessionHistorySize -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/footer.go internal/transport/telegram/adapter.go internal/transport/telegram/adapter_test.go
git commit -m "feat: report real telegram session metrics"
```

### Task 6: Add Edge-Case Coverage

**Files:**
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing tests for edge cases**

```go
func TestAdapterReplyTrimsHistoryWhenLimitExceeded(t *testing.T) { /* ... */ }
func TestAdapterReplyDoesNotStoreFailedProviderResponse(t *testing.T) { /* ... */ }
func TestAdapterReplyIgnoresRepeatedResetGracefully(t *testing.T) { /* ... */ }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run 'TestAdapterReplyTrimsHistoryWhenLimitExceeded|TestAdapterReplyDoesNotStoreFailedProviderResponse|TestAdapterReplyIgnoresRepeatedResetGracefully' -v`
Expected: FAIL because these edge cases are not covered yet.

- [ ] **Step 3: Write minimal implementation**

Implement only what the tests require:
- drop oldest messages once the session store exceeds its message limit
- never append assistant messages on provider failure
- keep `/reset` idempotent

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run 'TestAdapterReplyTrimsHistoryWhenLimitExceeded|TestAdapterReplyDoesNotStoreFailedProviderResponse|TestAdapterReplyIgnoresRepeatedResetGracefully' -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/adapter_test.go internal/transport/telegram/session.go internal/transport/telegram/adapter.go
git commit -m "test: cover telegram session edge cases"
```

### Task 7: Final Verification

**Files:**
- Verify only

- [ ] **Step 1: Run full test suite**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./...`
Expected: PASS

- [ ] **Step 2: Run live bot manually**

Run:

```bash
mkdir -p .tmp/go
set -a && . ./.env && set +a
GOTMPDIR=$PWD/.tmp/go go run ./cmd/coordinator
```

Expected:
- bot answers with chat-aware context
- `/reset` clears dialogue history
- footer shows model, thinking, usage, and session message count

- [ ] **Step 3: Commit final verification-safe changes**

```bash
git add README.md internal/provider/provider.go internal/provider/zai/client.go internal/provider/zai/client_test.go internal/transport/telegram/session.go internal/transport/telegram/footer.go internal/transport/telegram/adapter.go internal/transport/telegram/adapter_test.go
git commit -m "feat: add telegram session memory and reset flow"
```
