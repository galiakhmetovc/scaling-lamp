package telegram

import (
	"context"
	"fmt"

	"database/sql"
	_ "github.com/jackc/pgx/v5/stdlib"
)

type PostgresStore struct {
	db    *sql.DB
	limit int
}

func NewPostgresStore(db *sql.DB, limit int) *PostgresStore {
	if limit <= 0 {
		limit = 1
	}
	return &PostgresStore{
		db:    db,
		limit: limit,
	}
}

func (s *PostgresStore) ensureSchema(ctx context.Context) error {
	const schema = `
CREATE TABLE IF NOT EXISTS telegram_session_messages (
  chat_id BIGINT NOT NULL,
  session_key TEXT NOT NULL DEFAULT 'default',
  seq BIGSERIAL PRIMARY KEY,
  role TEXT NOT NULL,
  content TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '',
  tool_call_id TEXT NOT NULL DEFAULT '',
  tool_calls JSONB NOT NULL DEFAULT '[]'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TABLE IF NOT EXISTS telegram_chat_sessions (
  chat_id BIGINT NOT NULL,
  session_key TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (chat_id, session_key)
);
CREATE TABLE IF NOT EXISTS telegram_chat_active_sessions (
  chat_id BIGINT PRIMARY KEY,
  session_key TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS telegram_session_checkpoints (
  chat_id BIGINT NOT NULL,
  session_key TEXT NOT NULL,
  compaction_method TEXT NOT NULL DEFAULT '',
  what_happened TEXT NOT NULL,
  what_matters_now TEXT NOT NULL,
  unresolved_items JSONB NOT NULL DEFAULT '[]'::jsonb,
  next_actions JSONB NOT NULL DEFAULT '[]'::jsonb,
  archive_refs JSONB NOT NULL DEFAULT '[]'::jsonb,
  source_artifacts JSONB NOT NULL DEFAULT '[]'::jsonb,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (chat_id, session_key)
);
ALTER TABLE telegram_session_messages
  ADD COLUMN IF NOT EXISTS session_key TEXT NOT NULL DEFAULT 'default';
ALTER TABLE telegram_session_messages
  ADD COLUMN IF NOT EXISTS name TEXT NOT NULL DEFAULT '';
ALTER TABLE telegram_session_messages
  ADD COLUMN IF NOT EXISTS tool_call_id TEXT NOT NULL DEFAULT '';
ALTER TABLE telegram_session_messages
  ADD COLUMN IF NOT EXISTS tool_calls JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE telegram_session_checkpoints
  ADD COLUMN IF NOT EXISTS archive_refs JSONB NOT NULL DEFAULT '[]'::jsonb;
CREATE INDEX IF NOT EXISTS idx_tsm_chat_seq
  ON telegram_session_messages(chat_id, seq);
CREATE INDEX IF NOT EXISTS idx_tsm_chat_session_seq
  ON telegram_session_messages(chat_id, session_key, seq);
CREATE INDEX IF NOT EXISTS idx_tsm_created
  ON telegram_session_messages(created_at);
`
	_, err := s.db.ExecContext(ctx, schema)
	return err
}

func (s *PostgresStore) ActiveSession(chatID int64) (string, error) {
	ctx := context.Background()
	if err := s.ensureSchema(ctx); err != nil {
		return "", err
	}
	if err := s.ensureDefaultSession(ctx, chatID); err != nil {
		return "", err
	}

	var session string
	err := s.db.QueryRowContext(ctx,
		`SELECT session_key FROM telegram_chat_active_sessions WHERE chat_id = $1`,
		chatID,
	).Scan(&session)
	if err == sql.ErrNoRows {
		return "default", nil
	}
	if err != nil {
		return "", err
	}
	return session, nil
}

func (s *PostgresStore) CreateSession(chatID int64, session string) error {
	ctx := context.Background()
	if err := s.ensureSchema(ctx); err != nil {
		return err
	}
	session = normalizeSessionName(session)
	if session == "" {
		return fmt.Errorf("session name is required")
	}
	if err := s.ensureDefaultSession(ctx, chatID); err != nil {
		return err
	}
	_, err := s.db.ExecContext(ctx,
		`INSERT INTO telegram_chat_sessions (chat_id, session_key) VALUES ($1, $2) ON CONFLICT DO NOTHING`,
		chatID, session,
	)
	return err
}

func (s *PostgresStore) UseSession(chatID int64, session string) error {
	ctx := context.Background()
	if err := s.ensureSchema(ctx); err != nil {
		return err
	}
	session = normalizeSessionName(session)
	if session == "" {
		return fmt.Errorf("session name is required")
	}
	if err := s.ensureDefaultSession(ctx, chatID); err != nil {
		return err
	}

	var exists bool
	if err := s.db.QueryRowContext(ctx,
		`SELECT EXISTS(SELECT 1 FROM telegram_chat_sessions WHERE chat_id = $1 AND session_key = $2)`,
		chatID, session,
	).Scan(&exists); err != nil {
		return err
	}
	if !exists {
		return fmt.Errorf("session %q not found", session)
	}
	_, err := s.db.ExecContext(ctx,
		`INSERT INTO telegram_chat_active_sessions (chat_id, session_key) VALUES ($1, $2)
		 ON CONFLICT (chat_id) DO UPDATE SET session_key = EXCLUDED.session_key`,
		chatID, session,
	)
	return err
}

func (s *PostgresStore) ListSessions(chatID int64) ([]string, error) {
	ctx := context.Background()
	if err := s.ensureSchema(ctx); err != nil {
		return nil, err
	}
	if err := s.ensureDefaultSession(ctx, chatID); err != nil {
		return nil, err
	}

	rows, err := s.db.QueryContext(ctx,
		`SELECT session_key FROM telegram_chat_sessions WHERE chat_id = $1 ORDER BY session_key ASC`,
		chatID,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []string
	for rows.Next() {
		var session string
		if err := rows.Scan(&session); err != nil {
			return nil, err
		}
		out = append(out, session)
	}
	return out, rows.Err()
}

func (s *PostgresStore) ensureDefaultSession(ctx context.Context, chatID int64) error {
	if _, err := s.db.ExecContext(ctx,
		`INSERT INTO telegram_chat_sessions (chat_id, session_key) VALUES ($1, 'default') ON CONFLICT DO NOTHING`,
		chatID,
	); err != nil {
		return err
	}
	if _, err := s.db.ExecContext(ctx,
		`INSERT INTO telegram_chat_active_sessions (chat_id, session_key) VALUES ($1, 'default')
		 ON CONFLICT (chat_id) DO NOTHING`,
		chatID,
	); err != nil {
		return err
	}
	return nil
}
