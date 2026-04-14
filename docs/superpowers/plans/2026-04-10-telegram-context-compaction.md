# Telegram Context Budget And Session Compaction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add budget-aware prompt assembly and durable session compaction so long-running Telegram sessions keep working under `z.ai` context limits instead of degrading or failing as history and tool output grow.

**Architecture:** Introduce a dedicated context-management layer between Telegram session storage and provider requests. The first layer enforces a prompt budget by estimating token cost, reducing oversized tool output, and selecting only the summary plus recent raw turns that fit the budget. The second layer persists a structured checkpoint per Telegram session and refreshes it automatically once raw history crosses a compaction threshold. This keeps the current Telegram UX and `z.ai` tool-calling loop intact while moving context-window logic into reusable Go-only runtime components.

**Tech Stack:** Go 1.24+, existing Telegram adapter and Postgres session store, current `z.ai` provider contract, existing `worker.Checkpoint`, existing `internal/compaction` package, Go tests.

---

## File Structure

- Create: `internal/compaction/budget.go`
  Purpose: estimate prompt size, define budget thresholds, and expose reusable accounting helpers.
- Create: `internal/compaction/budget_test.go`
  Purpose: verify token estimation and budget-trigger behavior deterministically.
- Create: `internal/compaction/assembler.go`
  Purpose: build provider-ready message windows from summary + recent transcript within a fixed budget.
- Create: `internal/compaction/assembler_test.go`
  Purpose: verify sliding-window selection, summary insertion, and tool-output reduction.
- Modify: `internal/compaction/service.go`
  Purpose: replace the current string-join placeholder with structured checkpoint generation suitable for persisted session summaries.
- Modify: `internal/provider/provider.go`
  Purpose: add optional prompt-budget metadata to request/response types only if needed for transport-visible diagnostics.
- Modify: `internal/config/config.go`
  Purpose: load context-budget and compaction thresholds from environment with safe defaults.
- Modify: `internal/transport/telegram/store.go`
  Purpose: extend the Telegram session storage contract to persist and load the current compacted checkpoint for the active named session.
- Modify: `internal/transport/telegram/session.go`
  Purpose: add in-memory support for session checkpoints so tests and local fallback match the store contract.
- Modify: `internal/transport/telegram/postgres_store.go`
  Purpose: persist per-session checkpoint summaries and any metadata needed for compaction decisions.
- Modify: `internal/transport/telegram/postgres_store_test.go`
  Purpose: verify checkpoint persistence, replacement, and restart-safe loading.
- Modify: `internal/transport/telegram/adapter.go`
  Purpose: replace raw `store.Messages()` prompt assembly with budget-aware assembly, trigger compaction when thresholds are crossed, and keep final reply behavior unchanged.
- Modify: `internal/transport/telegram/adapter_test.go`
  Purpose: cover context trimming, summary-backed prompts, tool-output reduction, and auto-compaction trigger behavior.
- Modify: `internal/transport/telegram/footer.go`
  Purpose: expose context-budget and compaction diagnostics without turning footer into the primary UX.
- Modify: `README.md`
  Purpose: document context limits, compaction behavior, and the fact that large tool output is reduced before being reused as model context.

---

## Budgeting Model

- Use a conservative token estimator rather than exact provider tokenization in this slice.
- Add a fixed safety margin so underestimation does not push requests over the real provider limit.
- Track three thresholds:
  - provider context window
  - prompt assembly budget
  - compaction trigger threshold
- Prompt assembly order:
  1. system-level context already implicit in runtime
  2. compacted session checkpoint if present
  3. newest raw turns in reverse until the budget is full
  4. drop older raw turns that no longer fit
- Tool output policy:
  - full output may still be sent to Telegram for live visibility
  - oversized tool output must be reduced before it is stored back into reusable model context
  - reduced output must clearly indicate truncation

For MVP, keep the estimator simple and deterministic. Do not block on exact `z.ai` tokenizer support before landing the safety layer.

---

## Task 1: Add Prompt Budget Accounting Primitives

**Files:**
- Create: `internal/compaction/budget.go`
- Create: `internal/compaction/budget_test.go`
- Modify: `internal/config/config.go`

- [ ] **Step 1: Write the failing test for budget estimation**

```go
func TestEstimateMessagesReturnsStableNonZeroCost(t *testing.T) {
	messages := []provider.Message{
		{Role: "user", Content: "hello"},
		{Role: "assistant", Content: "world"},
	}

	got := compaction.EstimateMessages(messages)
	if got <= 0 {
		t.Fatalf("expected positive estimate, got %d", got)
	}
}
```

- [ ] **Step 2: Write the failing test for threshold configuration**

```go
func TestLoadContextBudgetConfig(t *testing.T) {
	t.Setenv("TEAMD_CONTEXT_WINDOW_TOKENS", "32000")
	t.Setenv("TEAMD_PROMPT_BUDGET_TOKENS", "24000")
	t.Setenv("TEAMD_COMPACTION_TRIGGER_TOKENS", "16000")

	cfg := Load()
	if cfg.ContextWindowTokens != 32000 || cfg.PromptBudgetTokens != 24000 || cfg.CompactionTriggerTokens != 16000 {
		t.Fatalf("unexpected config: %#v", cfg)
	}
}
```

- [ ] **Step 3: Run focused tests to verify they fail**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/compaction ./internal/config -run 'TestEstimateMessagesReturnsStableNonZeroCost|TestLoadContextBudgetConfig' -v`

Expected: FAIL because budget helpers and config fields do not exist yet.

- [ ] **Step 4: Write minimal implementation**

Implement in `internal/compaction/budget.go`:
- `type Budget struct { ContextWindowTokens int; PromptBudgetTokens int; CompactionTriggerTokens int; MaxToolContextChars int }`
- `func EstimateMessage(msg provider.Message) int`
- `func EstimateMessages(messages []provider.Message) int`
- `func (b Budget) NeedsCompaction(tokens int) bool`

Implement in `internal/config/config.go`:
- defaults for the three thresholds
- optional `TEAMD_MAX_TOOL_CONTEXT_CHARS`

Keep the estimator explicit and deterministic; for example, base it on character count plus per-message overhead and a fixed safety margin, and document that it is an approximation.

- [ ] **Step 5: Run focused tests to verify they pass**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/compaction ./internal/config -run 'TestEstimateMessagesReturnsStableNonZeroCost|TestLoadContextBudgetConfig' -v`

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/compaction/budget.go internal/compaction/budget_test.go internal/config/config.go
git commit -m "feat: add prompt budget accounting primitives"
```

### Task 2: Build Budget-Aware Prompt Assembly

**Files:**
- Create: `internal/compaction/assembler.go`
- Create: `internal/compaction/assembler_test.go`
- Modify: `internal/provider/provider.go`

- [ ] **Step 1: Write the failing test for sliding-window prompt assembly**

```go
func TestAssemblePromptUsesSummaryAndNewestTurnsWithinBudget(t *testing.T) {
	budget := compaction.Budget{
		PromptBudgetTokens: 60,
		MaxToolContextChars: 80,
	}

	checkpoint := worker.Checkpoint{
		SessionID: "telegram:1/default",
		WhatHappened: "Earlier the user established important deployment context.",
		WhatMattersNow: "Keep the deployment target and rollback requirement in mind.",
	}

	raw := []provider.Message{
		{Role: "user", Content: strings.Repeat("old-", 20)},
		{Role: "assistant", Content: strings.Repeat("older-", 20)},
		{Role: "user", Content: "recent question"},
		{Role: "assistant", Content: "recent answer"},
	}

	got := compaction.AssemblePrompt(budget, checkpoint, raw)
	if len(got) == 0 {
		t.Fatal("expected assembled messages")
	}
	if got[0].Role != "system" {
		t.Fatalf("expected checkpoint summary first, got %#v", got[0])
	}
	if got[len(got)-1].Content != "recent answer" {
		t.Fatalf("expected newest raw turn to survive, got %#v", got[len(got)-1])
	}
}
```

- [ ] **Step 2: Write the failing test for tool-output reduction**

```go
func TestAssemblePromptReducesOversizedToolOutput(t *testing.T) {
	budget := compaction.Budget{
		PromptBudgetTokens: 200,
		MaxToolContextChars: 32,
	}

	raw := []provider.Message{
		{Role: "tool", Content: strings.Repeat("abcdef", 20), ToolCallID: "tool-1"},
	}

	got := compaction.AssemblePrompt(budget, worker.Checkpoint{}, raw)
	if len(got) != 1 {
		t.Fatalf("unexpected output: %#v", got)
	}
	if !strings.Contains(got[0].Content, "truncated") {
		t.Fatalf("expected truncation marker, got %q", got[0].Content)
	}
}
```

- [ ] **Step 3: Run focused tests to verify they fail**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/compaction -run 'TestAssemblePromptUsesSummaryAndNewestTurnsWithinBudget|TestAssemblePromptReducesOversizedToolOutput' -v`

Expected: FAIL because prompt assembly does not exist yet.

- [ ] **Step 4: Write minimal implementation**

Implement in `internal/compaction/assembler.go`:
- `func AssemblePrompt(b Budget, checkpoint worker.Checkpoint, raw []provider.Message) []provider.Message`
- a helper that turns `worker.Checkpoint` into one synthetic summary message
- a helper that reduces oversized `role=tool` content before estimation
- reverse scan of raw history so the newest turns win when the budget is tight

Keep provider-specific fields intact where possible:
- preserve `ToolCallID`
- preserve assistant `ToolCalls`
- do not inject provider-specific JSON here

- [ ] **Step 5: Run focused tests to verify they pass**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/compaction -run 'TestAssemblePromptUsesSummaryAndNewestTurnsWithinBudget|TestAssemblePromptReducesOversizedToolOutput' -v`

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/compaction/assembler.go internal/compaction/assembler_test.go internal/provider/provider.go
git commit -m "feat: add budget-aware prompt assembly"
```

### Task 3: Persist Session Checkpoints In Telegram Stores

**Files:**
- Modify: `internal/transport/telegram/store.go`
- Modify: `internal/transport/telegram/session.go`
- Modify: `internal/transport/telegram/postgres_store.go`
- Modify: `internal/transport/telegram/postgres_store_test.go`

- [ ] **Step 1: Write the failing test for durable checkpoint persistence**

```go
func TestPostgresStorePersistsCheckpointPerNamedSession(t *testing.T) {
	db := openTestDB(t)
	store := NewPostgresStore(db, 16)

	if err := store.CreateSession(12001, "deploy"); err != nil {
		t.Fatalf("create session: %v", err)
	}
	if err := store.UseSession(12001, "deploy"); err != nil {
		t.Fatalf("use session: %v", err)
	}

	want := worker.Checkpoint{
		SessionID: "telegram:12001/deploy",
		WhatHappened: "Compacted history",
		WhatMattersNow: "Remember deployment target",
	}
	if err := store.SaveCheckpoint(12001, want); err != nil {
		t.Fatalf("save checkpoint: %v", err)
	}

	got, ok, err := store.Checkpoint(12001)
	if err != nil || !ok {
		t.Fatalf("checkpoint load failed: ok=%v err=%v", ok, err)
	}
	if got.WhatMattersNow != want.WhatMattersNow {
		t.Fatalf("unexpected checkpoint: %#v", got)
	}
}
```

- [ ] **Step 2: Run focused test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestPostgresStorePersistsCheckpointPerNamedSession -v`

Expected: FAIL because the store contract has no checkpoint support yet.

- [ ] **Step 3: Write minimal implementation**

Extend `internal/transport/telegram/store.go` with:

```go
Checkpoint(chatID int64) (worker.Checkpoint, bool, error)
SaveCheckpoint(chatID int64, checkpoint worker.Checkpoint) error
```

Implement in-memory support in `session.go` by storing one checkpoint per active named session.

Implement Postgres support in `postgres_store.go` with a table like:

```sql
CREATE TABLE IF NOT EXISTS telegram_session_checkpoints (
  chat_id BIGINT NOT NULL,
  session_key TEXT NOT NULL,
  what_happened TEXT NOT NULL,
  what_matters_now TEXT NOT NULL,
  unresolved_items JSONB NOT NULL DEFAULT '[]'::jsonb,
  next_actions JSONB NOT NULL DEFAULT '[]'::jsonb,
  source_artifacts JSONB NOT NULL DEFAULT '[]'::jsonb,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (chat_id, session_key)
);
```

Use upsert semantics so a new checkpoint replaces the old one for the same `(chat_id, session_key)`.

Also extend `worker.Checkpoint` with:

```go
CompactionMethod string
```

Use a stable value such as `heuristic-v1` so later compaction upgrades can distinguish old checkpoints from new ones.

- [ ] **Step 4: Run focused tests to verify they pass**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestPostgresStorePersistsCheckpointPerNamedSession -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/store.go internal/transport/telegram/session.go internal/transport/telegram/postgres_store.go internal/transport/telegram/postgres_store_test.go
git commit -m "feat: persist telegram session checkpoints"
```

### Task 4: Upgrade Compaction Service From Placeholder To Checkpoint Builder

**Files:**
- Modify: `internal/compaction/service.go`
- Modify: `internal/compaction/assembler_test.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing test for structured checkpoint generation**

```go
func TestCompactProducesStructuredCheckpointFromTranscript(t *testing.T) {
	svc := compaction.New(compaction.TestDeps())
	out, err := svc.Compact(context.Background(), compaction.Input{
		SessionID: "telegram:1/default",
		Transcript: []string{
			"user: deploy service api to cluster blue",
			"assistant: confirmed blue cluster target",
			"tool: kubectl output truncated",
		},
		ArtifactRefs: []string{"artifact://tool-output/1"},
	})
	if err != nil {
		t.Fatalf("compact: %v", err)
	}
	if out.WhatHappened == "" || out.WhatMattersNow == "" {
		t.Fatalf("expected structured checkpoint fields, got %#v", out)
	}
	if len(out.SourceArtifacts) != 1 {
		t.Fatalf("expected source artifacts to survive, got %#v", out)
	}
}
```

- [ ] **Step 2: Run focused test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/compaction -run TestCompactProducesStructuredCheckpointFromTranscript -v`

Expected: FAIL or produce obviously placeholder output because the current implementation only joins strings.

- [ ] **Step 3: Write minimal implementation**

Update `internal/compaction/service.go` so `Compact(...)`:
- preserves `SessionID`
- separates:
  - `WhatHappened`
  - `WhatMattersNow`
  - `UnresolvedItems`
  - `NextActions`
- keeps `ArtifactRefs` as `SourceArtifacts`

For MVP, heuristics are acceptable:
- recent user asks and assistant conclusions feed `WhatMattersNow`
- lines mentioning `todo`, `next`, `need`, `pending`, or `unresolved` feed `UnresolvedItems`
- action-like phrases feed `NextActions`

Do not add a second provider dependency just for summary generation in this slice.
Add at least one negative test so substring false-positives such as `todoist` do not populate `UnresolvedItems`.

- [ ] **Step 4: Run focused tests to verify they pass**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/compaction -run TestCompactProducesStructuredCheckpointFromTranscript -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/compaction/service.go internal/compaction/assembler_test.go internal/transport/telegram/adapter_test.go
git commit -m "feat: upgrade session compaction checkpoints"
```

### Task 5: Wire Automatic Compaction Into Telegram Prompt Assembly

**Files:**
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/adapter_test.go`
- Modify: `internal/transport/telegram/footer.go`

- [ ] **Step 1: Write the failing test for summary-backed provider requests**

```go
func TestAdapterReplyUsesCheckpointAndRecentTurnsWhenHistoryIsTooLarge(t *testing.T) {
	store := NewSessionStore(64)
	for i := 0; i < 20; i++ {
		_ = store.Append(1001, provider.Message{Role: "user", Content: strings.Repeat("message ", 20)})
	}
	_ = store.SaveCheckpoint(1001, worker.Checkpoint{
		SessionID: "telegram:1001/default",
		WhatHappened: "Earlier conversation compacted",
		WhatMattersNow: "Preserve the key deployment requirement",
	})

	var got provider.PromptRequest
	adapter := New(Deps{
		Provider: captureProvider{onGenerate: func(req provider.PromptRequest) {
			got = req
		}},
		Store: store,
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "latest question"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if len(got.Messages) == 0 {
		t.Fatal("expected provider request messages")
	}
	if got.Messages[0].Role != "system" {
		t.Fatalf("expected checkpoint summary in assembled prompt, got %#v", got.Messages[0])
	}
}
```

- [ ] **Step 2: Write the failing test for auto-compaction trigger**

```go
func TestAdapterReplyCompactsAndPersistsCheckpointWhenThresholdExceeded(t *testing.T) {
	store := NewSessionStore(128)
	for i := 0; i < 30; i++ {
		_ = store.Append(1001, provider.Message{Role: "tool", Content: strings.Repeat("abcdef", 40), ToolCallID: fmt.Sprintf("tool-%d", i)})
	}

	adapter := New(Deps{
		Provider: provider.FakeProvider{},
		Store: store,
	})

	if err := adapter.Reply(context.Background(), Update{ChatID: 1001, Text: "continue"}); err != nil {
		t.Fatalf("reply: %v", err)
	}
	if _, ok, err := store.Checkpoint(1001); err != nil || !ok {
		t.Fatalf("expected checkpoint after compaction, ok=%v err=%v", ok, err)
	}
}
```

- [ ] **Step 3: Run focused tests to verify they fail**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run 'TestAdapterReplyUsesCheckpointAndRecentTurnsWhenHistoryIsTooLarge|TestAdapterReplyCompactsAndPersistsCheckpointWhenThresholdExceeded' -v`

Expected: FAIL because adapter still forwards raw `store.Messages()` directly to the provider and never saves checkpoints automatically.

- [ ] **Step 4: Write minimal implementation**

In `internal/transport/telegram/adapter.go`:
- load the configured `compaction.Budget`
- keep one per-chat/session compaction guard so parallel requests do not run duplicate compactions
- before each provider call, load:
  - current active-session checkpoint
  - raw messages
- estimate raw history cost
- if over compaction trigger:
  - re-check the threshold after taking the compaction guard in case another request already compacted the session
  - build transcript lines from the older portion of history
  - call `compaction.Service.Compact(...)` under a bounded timeout
  - save checkpoint through `store.SaveCheckpoint(...)`
  - keep only recent raw turns in active history
- if compaction fails:
  - log the failure
  - fall back to hard budget trimming with `compaction.AssemblePrompt(...)`
  - do not drop the user message or abort the request solely because checkpoint generation failed
- always call `compaction.AssemblePrompt(...)` instead of forwarding `store.Messages()` directly

In `internal/transport/telegram/footer.go`:
- add low-noise diagnostics such as:
  - `context_estimate=<n>`
  - `compacted=true|false`

Do not change the visible live tool messages in Telegram in this slice; only change what re-enters provider context.

- [ ] **Step 5: Run focused tests to verify they pass**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run 'TestAdapterReplyUsesCheckpointAndRecentTurnsWhenHistoryIsTooLarge|TestAdapterReplyCompactsAndPersistsCheckpointWhenThresholdExceeded' -v`

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/transport/telegram/adapter.go internal/transport/telegram/adapter_test.go internal/transport/telegram/footer.go
git commit -m "feat: add telegram auto-compaction and prompt budgeting"
```

### Task 6: Verify End-To-End Behavior And Document Operational Limits

**Files:**
- Modify: `README.md`
- Modify: `internal/provider/zai/client_test.go`
- Modify: `tests/integration/coordinator_flow_test.go`

- [ ] **Step 1: Write the failing integration test for long-session survival**

```go
func TestCoordinatorFlowSurvivesLongTelegramSessionWithCompaction(t *testing.T) {
	// Build a long transcript with repeated tool output and confirm
	// the provider still receives a bounded prompt plus checkpoint summary.
}
```

- [ ] **Step 2: Run focused integration test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./tests/integration -run TestCoordinatorFlowSurvivesLongTelegramSessionWithCompaction -v`

Expected: FAIL because the current flow has no compaction-aware integration coverage.

- [ ] **Step 3: Write minimal implementation**

Add documentation in `README.md` for:
- configured context thresholds
- summary/checkpoint behavior
- tool-output truncation in prompt context versus full Telegram display
- operational caveat that compaction is heuristic and provider token counts remain authoritative

Extend integration coverage so the coordinator path proves:
- large tool outputs no longer explode prompt assembly
- summaries survive restart through Postgres checkpoint persistence
- follow-up questions still use the compacted summary

- [ ] **Step 4: Run full verification**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./...`

Expected: PASS

- [ ] **Step 5: Manual verification**

In Telegram:
- create or switch to a dedicated session
- generate a long tool-heavy conversation
- verify the bot still answers after many rounds
- verify footer shows compaction diagnostics
- restart coordinator
- verify the session still answers coherently from the persisted checkpoint

- [ ] **Step 6: Commit**

```bash
git add README.md internal/provider/zai/client_test.go tests/integration/coordinator_flow_test.go
git commit -m "test: cover telegram context compaction end to end"
```

---

## Non-Goals

- No full semantic-memory publication of compaction outputs in this slice
- No exact provider tokenizer integration
- No Telegram UX redesign here beyond footer diagnostics already present
- No MCP policy work or tool permission changes
- No artifact-store refactor for full raw tool-output archival
- No observability metrics export in this slice

---

## Follow-On Work

After this plan lands, the likely next slices are:
- move oversized raw tool output into artifact references instead of only truncating it
- export compaction and context-budget metrics to observability
- connect session stats UI to real compacted-session metadata
- tighten provider-specific budgeting once `z.ai` exposes stable tokenizer or context introspection APIs
- add containerized Postgres test setup if local DB coupling becomes painful in CI
