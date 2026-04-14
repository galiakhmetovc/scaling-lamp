package telegram

import (
	"database/sql"
	"os"
	"strings"
	"testing"
	"unicode/utf8"

	"teamd/internal/provider"
	"teamd/internal/worker"
)

func openTestDB(t *testing.T) *sql.DB {
	t.Helper()

	dsn := os.Getenv("TEAMD_TEST_POSTGRES_DSN")
	if dsn == "" {
		dsn = "postgres://postgres:postgres@localhost:5432/postgres?sslmode=disable"
	}

	db, err := sql.Open("pgx", dsn)
	if err != nil {
		t.Fatalf("open db: %v", err)
	}
	t.Cleanup(func() {
		_ = db.Close()
	})

	return db
}

func resetChatRows(t *testing.T, db *sql.DB, chatID int64) {
	t.Helper()
	statements := []string{
		`DELETE FROM telegram_session_messages WHERE chat_id = $1`,
		`DELETE FROM telegram_session_checkpoints WHERE chat_id = $1`,
		`DELETE FROM telegram_chat_active_sessions WHERE chat_id = $1`,
		`DELETE FROM telegram_chat_sessions WHERE chat_id = $1`,
	}
	for _, stmt := range statements {
		if _, err := db.Exec(stmt, chatID); err != nil {
			if strings.Contains(err.Error(), `does not exist`) {
				continue
			}
			t.Fatalf("reset chat rows: %v", err)
		}
	}
}

func TestPostgresStorePersistsMessages(t *testing.T) {
	db := openTestDB(t)
	store := NewPostgresStore(db, 16)
	const chatID = 11001
	resetChatRows(t, db, chatID)

	if err := store.Reset(chatID); err != nil {
		t.Fatalf("reset before test: %v", err)
	}

	err := store.Append(chatID, provider.Message{Role: "user", Content: "hello"})
	if err != nil {
		t.Fatalf("append: %v", err)
	}

	got, err := store.Messages(chatID)
	if err != nil {
		t.Fatalf("messages: %v", err)
	}
	if len(got) != 1 || got[0].Content != "hello" {
		t.Fatalf("unexpected messages: %#v", got)
	}
}

func TestPostgresStoreSanitizesInvalidUTF8InMessageContent(t *testing.T) {
	db := openTestDB(t)
	store := NewPostgresStore(db, 16)
	const chatID = 11005
	resetChatRows(t, db, chatID)

	if err := store.Reset(chatID); err != nil {
		t.Fatalf("reset before test: %v", err)
	}

	invalid := "bad:\xe2\xa1"
	if utf8.ValidString(invalid) {
		t.Fatal("expected intentionally invalid utf-8 test string")
	}

	if err := store.Append(chatID, provider.Message{Role: "tool", Content: invalid, ToolCallID: "call-1"}); err != nil {
		t.Fatalf("append invalid utf-8: %v", err)
	}

	got, err := store.Messages(chatID)
	if err != nil {
		t.Fatalf("messages: %v", err)
	}
	if len(got) != 1 {
		t.Fatalf("unexpected messages: %#v", got)
	}
	if !utf8.ValidString(got[0].Content) {
		t.Fatalf("expected stored content to be valid utf-8, got %q", got[0].Content)
	}
	if strings.Contains(got[0].Content, "\xe2\xa1") {
		t.Fatalf("expected invalid bytes removed from stored content, got %q", got[0].Content)
	}
}

func TestPostgresStoreSanitizesInvalidUTF8InToolCallArguments(t *testing.T) {
	db := openTestDB(t)
	store := NewPostgresStore(db, 16)
	const chatID int64 = 11006
	resetChatRows(t, db, chatID)

	if err := store.Reset(chatID); err != nil {
		t.Fatalf("reset before test: %v", err)
	}

	invalid := string([]byte{0xd0, 0x2e})
	err := store.Append(chatID, provider.Message{
		Role:    "assistant",
		Content: "tool call",
		ToolCalls: []provider.ToolCall{
			{
				ID:   "call-1",
				Name: "shell.exec",
				Arguments: map[string]any{
					"command": invalid,
				},
			},
		},
	})
	if err != nil {
		t.Fatalf("append with invalid tool args: %v", err)
	}

	got, err := store.Messages(chatID)
	if err != nil {
		t.Fatalf("messages: %v", err)
	}
	if len(got) != 1 || len(got[0].ToolCalls) != 1 {
		t.Fatalf("unexpected messages: %#v", got)
	}
	if got[0].ToolCalls[0].Arguments["command"] != "." {
		t.Fatalf("expected sanitized tool arg, got %#v", got[0].ToolCalls[0].Arguments)
	}
}

func TestPostgresStoreSanitizesInvalidUTF8InCheckpoint(t *testing.T) {
	db := openTestDB(t)
	store := NewPostgresStore(db, 16)
	const chatID int64 = 11007
	resetChatRows(t, db, chatID)

	if err := store.Reset(chatID); err != nil {
		t.Fatalf("reset before test: %v", err)
	}

	invalid := string([]byte{0xd0, 0x2e})
	err := store.SaveCheckpoint(chatID, worker.Checkpoint{
		CompactionMethod: "heuristic",
		WhatHappened:     "bad:" + invalid,
		WhatMattersNow:   "keep context",
		UnresolvedItems:  []string{"item:" + invalid},
		NextActions:      []string{"next:" + invalid},
		ArchiveRefs:      []string{"archive:" + invalid},
		SourceArtifacts:  []string{"artifact:" + invalid},
	})
	if err != nil {
		t.Fatalf("save checkpoint with invalid utf-8: %v", err)
	}

	got, ok, err := store.Checkpoint(chatID)
	if err != nil {
		t.Fatalf("checkpoint: %v", err)
	}
	if !ok {
		t.Fatal("expected checkpoint")
	}
	if !utf8.ValidString(got.WhatHappened) || !utf8.ValidString(got.UnresolvedItems[0]) || !utf8.ValidString(got.NextActions[0]) || !utf8.ValidString(got.ArchiveRefs[0]) || !utf8.ValidString(got.SourceArtifacts[0]) {
		t.Fatalf("expected sanitized checkpoint, got %#v", got)
	}
}

func TestPostgresStoreResetRemovesSessionMessages(t *testing.T) {
	db := openTestDB(t)
	store := NewPostgresStore(db, 16)
	const chatID = 11002
	resetChatRows(t, db, chatID)

	if err := store.Reset(chatID); err != nil {
		t.Fatalf("reset before test: %v", err)
	}
	if err := store.Append(chatID, provider.Message{Role: "user", Content: "hello"}); err != nil {
		t.Fatalf("append: %v", err)
	}

	if err := store.Reset(chatID); err != nil {
		t.Fatalf("reset: %v", err)
	}

	got, err := store.Messages(chatID)
	if err != nil {
		t.Fatalf("messages: %v", err)
	}
	if len(got) != 0 {
		t.Fatalf("expected empty history after reset, got %#v", got)
	}
}

func TestPostgresStoreReloadsHistoryAcrossInstances(t *testing.T) {
	db := openTestDB(t)
	store1 := NewPostgresStore(db, 16)
	const chatID = 11003
	resetChatRows(t, db, chatID)

	if err := store1.Reset(chatID); err != nil {
		t.Fatalf("reset before test: %v", err)
	}
	if err := store1.Append(chatID, provider.Message{Role: "user", Content: "hello"}); err != nil {
		t.Fatalf("append: %v", err)
	}

	store2 := NewPostgresStore(db, 16)
	got, err := store2.Messages(chatID)
	if err != nil {
		t.Fatalf("messages: %v", err)
	}
	if len(got) != 1 || got[0].Content != "hello" {
		t.Fatalf("unexpected reloaded history: %#v", got)
	}
}

func TestPostgresStorePersistsNamedSessionsAndActivePointer(t *testing.T) {
	db := openTestDB(t)
	store := NewPostgresStore(db, 16)
	const chatID = 11004
	resetChatRows(t, db, chatID)

	if err := store.Reset(chatID); err != nil {
		t.Fatalf("reset default: %v", err)
	}
	if err := store.CreateSession(chatID, "deploy"); err != nil {
		t.Fatalf("create session: %v", err)
	}
	if err := store.UseSession(chatID, "deploy"); err != nil {
		t.Fatalf("use session: %v", err)
	}
	if err := store.Append(chatID, provider.Message{Role: "user", Content: "deploy ctx"}); err != nil {
		t.Fatalf("append deploy: %v", err)
	}
	if err := store.UseSession(chatID, "default"); err != nil {
		t.Fatalf("use default: %v", err)
	}
	if err := store.Append(chatID, provider.Message{Role: "user", Content: "default ctx"}); err != nil {
		t.Fatalf("append default: %v", err)
	}

	reloaded := NewPostgresStore(db, 16)
	active, err := reloaded.ActiveSession(chatID)
	if err != nil {
		t.Fatalf("active session: %v", err)
	}
	if active != "default" {
		t.Fatalf("unexpected active session: %q", active)
	}

	got, err := reloaded.Messages(chatID)
	if err != nil {
		t.Fatalf("messages default: %v", err)
	}
	if len(got) != 1 || got[0].Content != "default ctx" {
		t.Fatalf("unexpected default history: %#v", got)
	}

	if err := reloaded.UseSession(chatID, "deploy"); err != nil {
		t.Fatalf("use deploy reload: %v", err)
	}
	got, err = reloaded.Messages(chatID)
	if err != nil {
		t.Fatalf("messages deploy: %v", err)
	}
	if len(got) != 1 || got[0].Content != "deploy ctx" {
		t.Fatalf("unexpected deploy history: %#v", got)
	}
}

func TestPostgresStorePersistsCheckpointPerNamedSession(t *testing.T) {
	db := openTestDB(t)
	store := NewPostgresStore(db, 16)
	const chatID = 12001
	resetChatRows(t, db, chatID)

	if err := store.CreateSession(chatID, "deploy"); err != nil {
		t.Fatalf("create session: %v", err)
	}
	if err := store.UseSession(chatID, "deploy"); err != nil {
		t.Fatalf("use session: %v", err)
	}

	want := worker.Checkpoint{
		SessionID:        "telegram:12001/deploy",
		WhatHappened:     "Compacted history",
		WhatMattersNow:   "Remember deployment target",
		CompactionMethod: "heuristic-v1",
	}
	if err := store.SaveCheckpoint(chatID, want); err != nil {
		t.Fatalf("save checkpoint: %v", err)
	}

	got, ok, err := store.Checkpoint(chatID)
	if err != nil || !ok {
		t.Fatalf("checkpoint load failed: ok=%v err=%v", ok, err)
	}
	if got.WhatMattersNow != want.WhatMattersNow {
		t.Fatalf("unexpected checkpoint: %#v", got)
	}
	if got.CompactionMethod != want.CompactionMethod {
		t.Fatalf("unexpected checkpoint method: %#v", got)
	}
}

func TestPostgresStorePreservesActiveTurnWhenLimitExceeded(t *testing.T) {
	db := openTestDB(t)
	store := NewPostgresStore(db, 4)
	const chatID = 12002
	resetChatRows(t, db, chatID)

	messages := []provider.Message{
		{Role: "user", Content: "old user"},
		{Role: "assistant", Content: "old assistant"},
		{Role: "user", Content: "active user"},
		{Role: "assistant", ToolCalls: []provider.ToolCall{{ID: "call-1", Name: "shell_exec"}}},
		{Role: "tool", ToolCallID: "call-1", Content: "tool output"},
		{Role: "assistant", ToolCalls: []provider.ToolCall{{ID: "call-2", Name: "filesystem_read_file"}}},
		{Role: "tool", ToolCallID: "call-2", Content: "file output"},
	}
	for _, msg := range messages {
		if err := store.Append(chatID, msg); err != nil {
			t.Fatalf("append: %v", err)
		}
	}

	got, err := store.Messages(chatID)
	if err != nil {
		t.Fatalf("messages: %v", err)
	}
	if len(got) != 5 {
		t.Fatalf("expected active turn retained beyond nominal limit, got %#v", got)
	}
	if got[0].Role != "user" || got[0].Content != "active user" {
		t.Fatalf("expected active turn to start with current user, got %#v", got)
	}
	if got[1].Role != "assistant" || len(got[1].ToolCalls) != 1 || got[1].ToolCalls[0].ID != "call-1" {
		t.Fatalf("expected first tool call kept, got %#v", got[1])
	}
	if got[4].Role != "tool" || got[4].ToolCallID != "call-2" {
		t.Fatalf("expected final tool result kept, got %#v", got[4])
	}
}
