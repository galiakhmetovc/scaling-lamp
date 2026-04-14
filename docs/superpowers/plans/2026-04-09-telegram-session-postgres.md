# Telegram Session Postgres Storage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist Telegram chat session history in Postgres so context survives bot restarts and `/reset` clears durable state instead of only process memory.

**Architecture:** Introduce a session storage abstraction used by the Telegram adapter instead of a hardcoded in-memory map. Keep the current in-memory implementation for tests, add a Postgres-backed implementation for production, and wire the coordinator startup to choose Postgres storage when a DSN is present. Postgres writes must use parameterized queries and keep append plus trimming in one transaction so the per-chat message limit remains deterministic under concurrent writes.

**Tech Stack:** Go 1.24+, standard library, current Telegram adapter, `database/sql`, Postgres, current runtime config.

---

## File Structure

- Create: `internal/transport/telegram/store.go`
  Purpose: define the session storage interface shared by in-memory and Postgres implementations.
- Modify: `internal/transport/telegram/session.go`
  Purpose: make the current in-memory store satisfy the shared interface.
- Create: `internal/transport/telegram/postgres_store.go`
  Purpose: implement durable Telegram session storage in Postgres.
- Create: `internal/transport/telegram/postgres_store_test.go`
  Purpose: verify SQL-backed session persistence logic with focused tests.
- Modify: `README.md`
  Purpose: document automatic schema bootstrap, manual migration stance for MVP, and local test prerequisites.
- Modify: `internal/transport/telegram/adapter.go`
  Purpose: depend on the storage interface instead of directly on `SessionStore`.
- Modify: `internal/transport/telegram/adapter_test.go`
  Purpose: verify adapter behavior with injected storage.
- Modify: `internal/config/config.go`
  Purpose: add a switch or default path for Telegram session persistence configuration if needed.
- Modify: `cmd/coordinator/main.go`
  Purpose: construct the appropriate Telegram session store at runtime.

### Task 1: Introduce Telegram Session Storage Interface

**Files:**
- Create: `internal/transport/telegram/store.go`
- Modify: `internal/transport/telegram/session.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing test for adapter storage injection**

```go
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestAdapterUsesInjectedSessionStore -v`
Expected: FAIL because `Deps.Store` and adapter storage abstraction do not exist yet.

- [ ] **Step 3: Write minimal implementation**

Create `internal/transport/telegram/store.go`:

```go
type Store interface {
	Append(chatID int64, msg provider.Message) error
	Messages(chatID int64) ([]provider.Message, error)
	Reset(chatID int64) error
}
```

Update `session.go` so the in-memory store implements this interface. Update adapter construction to accept an injected store and default to in-memory when omitted.

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestAdapterUsesInjectedSessionStore -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/store.go internal/transport/telegram/session.go internal/transport/telegram/adapter_test.go
git commit -m "feat: add telegram session store interface"
```

### Task 2: Add Postgres Session Store

**Files:**
- Create: `internal/transport/telegram/postgres_store.go`
- Create: `internal/transport/telegram/postgres_store_test.go`

- [ ] **Step 1: Write the failing test for persistent append and load**

```go
func TestPostgresStorePersistsMessages(t *testing.T) {
	db := openTestDB(t)
	store := NewPostgresStore(db, 16)

	err := store.Append(1001, provider.Message{Role: "user", Content: "hello"})
	if err != nil {
		t.Fatalf("append: %v", err)
	}

	got, err := store.Messages(1001)
	if err != nil {
		t.Fatalf("messages: %v", err)
	}
	if len(got) != 1 || got[0].Content != "hello" {
		t.Fatalf("unexpected messages: %#v", got)
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestPostgresStorePersistsMessages -v`
Expected: FAIL because the Postgres store does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Create `internal/transport/telegram/postgres_store.go` with:
- constructor `NewPostgresStore(db *sql.DB, limit int) *PostgresStore`
- schema bootstrap helper for a table like:

```sql
CREATE TABLE IF NOT EXISTS telegram_session_messages (
  chat_id BIGINT NOT NULL,
  seq BIGSERIAL PRIMARY KEY,
  role TEXT NOT NULL,
  content TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tsm_chat_seq
  ON telegram_session_messages(chat_id, seq);

CREATE INDEX IF NOT EXISTS idx_tsm_created
  ON telegram_session_messages(created_at);
```

Implement:
- `Append`
- `Messages`
- `Reset`
- trimming oldest rows beyond the configured message limit

Use a deterministic `ORDER BY seq ASC` on reads.
Keep append plus trimming in one transaction. For MVP, document that schema bootstrap is automatic at startup and later schema changes are manual rather than managed by a migration framework.

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestPostgresStorePersistsMessages -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/postgres_store.go internal/transport/telegram/postgres_store_test.go
git commit -m "feat: add postgres telegram session store"
```

### Task 3: Make Reset Durable

**Files:**
- Modify: `internal/transport/telegram/postgres_store_test.go`
- Modify: `internal/transport/telegram/adapter_test.go`
- Modify: `internal/transport/telegram/adapter.go`

- [ ] **Step 1: Write the failing test for durable reset**

```go
func TestPostgresStoreResetRemovesSessionMessages(t *testing.T) {
	db := openTestDB(t)
	store := NewPostgresStore(db, 16)
	_ = store.Append(1001, provider.Message{Role: "user", Content: "hello"})

	if err := store.Reset(1001); err != nil {
		t.Fatalf("reset: %v", err)
	}

	got, _ := store.Messages(1001)
	if len(got) != 0 {
		t.Fatalf("expected empty history after reset, got %#v", got)
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestPostgresStoreResetRemovesSessionMessages -v`
Expected: FAIL because durable reset behavior is not fully covered yet.

- [ ] **Step 3: Write minimal implementation**

In the Postgres store:
- implement `DELETE FROM telegram_session_messages WHERE chat_id = $1`

In the adapter:
- call `store.Reset(chatID)` on `/reset`
- stop assuming reset is only in-memory
- if `Append` or `Messages` fails, log and return an error rather than building a partial provider request from stale state

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestPostgresStoreResetRemovesSessionMessages -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/postgres_store.go internal/transport/telegram/postgres_store_test.go internal/transport/telegram/adapter.go internal/transport/telegram/adapter_test.go
git commit -m "feat: make telegram session reset durable"
```

### Task 4: Wire Runtime To Use Postgres Store

**Files:**
- Modify: `internal/config/config.go`
- Modify: `cmd/coordinator/main.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing test for runtime store selection**

```go
func TestLoadIncludesTelegramSessionStorageConfig(t *testing.T) {
	t.Setenv("TEAMD_POSTGRES_DSN", "postgres://teamd:teamd@localhost:5432/teamd_test?sslmode=disable")
	cfg := Load()
	if cfg.PostgresDSN == "" {
		t.Fatal("expected postgres dsn")
	}
}
```

- [ ] **Step 2: Run test to verify it fails only if new config is missing**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/config -run TestLoadIncludesTelegramSessionStorageConfig -v`
Expected: PASS if existing config is enough, otherwise FAIL and add only the missing knob.

- [ ] **Step 3: Write minimal implementation**

In `cmd/coordinator/main.go`:
- open `sql.DB` when `TEAMD_POSTGRES_DSN` is set
- set conservative pool settings such as `SetMaxOpenConns`, `SetMaxIdleConns`, and `SetConnMaxLifetime`
- create `telegram.NewPostgresStore(db, 16)`
- pass it into `telegram.New(telegram.Deps{Store: ...})`
- otherwise keep the current in-memory fallback

Do not add extra config unless a real need appears during implementation.

- [ ] **Step 4: Run focused verification**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./cmd/coordinator ./internal/config -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add cmd/coordinator/main.go internal/config/config.go internal/transport/telegram/adapter_test.go
git commit -m "feat: wire telegram session storage to postgres"
```

### Task 5: Verify Restore-After-Restart Behavior

**Files:**
- Modify: `internal/transport/telegram/postgres_store_test.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing test for reloaded history**

```go
func TestPostgresStoreReloadsHistoryAcrossInstances(t *testing.T) {
	db := openTestDB(t)
	store1 := NewPostgresStore(db, 16)
	_ = store1.Append(1001, provider.Message{Role: "user", Content: "hello"})

	store2 := NewPostgresStore(db, 16)
	got, err := store2.Messages(1001)
	if err != nil {
		t.Fatalf("messages: %v", err)
	}
	if len(got) != 1 || got[0].Content != "hello" {
		t.Fatalf("unexpected reloaded history: %#v", got)
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestPostgresStoreReloadsHistoryAcrossInstances -v`
Expected: FAIL because reload persistence has not been verified yet.

- [ ] **Step 3: Write minimal implementation**

Fix only what the failing test exposes. This should usually be SQL bootstrap or ordering bugs, not a redesign.

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestPostgresStoreReloadsHistoryAcrossInstances -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/postgres_store.go internal/transport/telegram/postgres_store_test.go
git commit -m "test: verify telegram session restore after restart"
```

### Task 6: Document DB Test Strategy And MVP Limits

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Write the failing documentation check**

Manual check:
- README must mention that Postgres schema is auto-created for MVP
- README must mention that later schema changes are manual
- README must mention local Postgres test prerequisites if `testcontainers-go` is not added in this slice
- README must mention that the session limit counts individual messages, not user/assistant pairs

- [ ] **Step 2: Verify the documentation gap exists**

Run: `rg -n "telegram session|schema|migration|Postgres|session limit" README.md`
Expected: missing or incomplete coverage

- [ ] **Step 3: Write minimal documentation**

Document:
- schema bootstrap behavior
- manual migration stance for MVP
- local Postgres test expectation or explicit deferral of containerized DB tests
- session limit semantics

- [ ] **Step 4: Verify documentation is present**

Run: `rg -n "schema|migration|session limit|Postgres" README.md`
Expected: matching lines for all points above

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: document telegram postgres session storage"
```

### Task 7: Final Verification

**Files:**
- Verify only

- [ ] **Step 1: Run full test suite**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./...`
Expected: PASS

- [ ] **Step 2: Manual runtime verification**

Run:

```bash
mkdir -p .tmp/go
set -a && . ./.env && set +a
GOTMPDIR=$PWD/.tmp/go go run ./cmd/coordinator
```

Expected:
- send a message in Telegram
- restart the process
- send a follow-up message
- observe that the bot still has prior session context
- send `/reset`
- observe that the next reply starts a clean session

- [ ] **Step 3: Commit final verified changes**

```bash
git add internal/transport/telegram/store.go internal/transport/telegram/session.go internal/transport/telegram/postgres_store.go internal/transport/telegram/postgres_store_test.go internal/transport/telegram/adapter.go internal/transport/telegram/adapter_test.go cmd/coordinator/main.go internal/config/config.go
git commit -m "feat: persist telegram session memory in postgres"
```
