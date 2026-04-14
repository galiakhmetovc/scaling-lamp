package telegram

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"

	"teamd/internal/provider"
)

func (s *PostgresStore) Append(chatID int64, msg provider.Message) error {
	ctx := context.Background()
	if err := s.ensureSchema(ctx); err != nil {
		return err
	}
	msg = sanitizeMessage(msg)
	session, err := s.ActiveSession(chatID)
	if err != nil {
		return err
	}

	tx, err := s.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelReadCommitted})
	if err != nil {
		return err
	}
	defer func() {
		_ = tx.Rollback()
	}()

	toolCalls, err := json.Marshal(msg.ToolCalls)
	if err != nil {
		return err
	}

	if _, err := tx.ExecContext(ctx,
		`INSERT INTO telegram_chat_sessions (chat_id, session_key) VALUES ($1, $2) ON CONFLICT DO NOTHING`,
		chatID, session,
	); err != nil {
		return err
	}

	if _, err := tx.ExecContext(ctx,
		`INSERT INTO telegram_session_messages (chat_id, session_key, role, content, name, tool_call_id, tool_calls) VALUES ($1, $2, $3, $4, $5, $6, $7)`,
		chatID, session, msg.Role, msg.Content, msg.Name, msg.ToolCallID, toolCalls,
	); err != nil {
		return err
	}

	history, err := s.loadTranscriptRows(ctx, tx, chatID, session)
	if err != nil {
		return err
	}
	if len(history) > s.limit {
		messages := make([]provider.Message, 0, len(history))
		for _, item := range history {
			messages = append(messages, item.msg)
		}
		start := trimHistoryStart(messages, s.limit)
		for i := 0; i < start; i++ {
			if _, err := tx.ExecContext(ctx, `DELETE FROM telegram_session_messages WHERE seq = $1`, history[i].seq); err != nil {
				return err
			}
		}
	}

	return tx.Commit()
}

func (s *PostgresStore) Messages(chatID int64) ([]provider.Message, error) {
	ctx := context.Background()
	if err := s.ensureSchema(ctx); err != nil {
		return nil, err
	}
	session, err := s.ActiveSession(chatID)
	if err != nil {
		return nil, err
	}
	rows, err := s.loadTranscriptRows(ctx, s.db, chatID, session)
	if err != nil {
		return nil, err
	}
	out := make([]provider.Message, 0, len(rows))
	for _, item := range rows {
		out = append(out, item.msg)
	}
	return out, nil
}

func (s *PostgresStore) Reset(chatID int64) error {
	ctx := context.Background()
	if err := s.ensureSchema(ctx); err != nil {
		return err
	}
	session, err := s.ActiveSession(chatID)
	if err != nil {
		return err
	}
	if _, err := s.db.ExecContext(ctx, `DELETE FROM telegram_session_messages WHERE chat_id = $1 AND session_key = $2`, chatID, session); err != nil {
		return fmt.Errorf("reset chat %d: %w", chatID, err)
	}
	if _, err := s.db.ExecContext(ctx, `DELETE FROM telegram_session_checkpoints WHERE chat_id = $1 AND session_key = $2`, chatID, session); err != nil {
		return fmt.Errorf("reset checkpoint %d: %w", chatID, err)
	}
	return nil
}

type transcriptQuerier interface {
	QueryContext(ctx context.Context, query string, args ...any) (*sql.Rows, error)
}

type rowMessage struct {
	seq int64
	msg provider.Message
}

func (s *PostgresStore) loadTranscriptRows(ctx context.Context, q transcriptQuerier, chatID int64, session string) ([]rowMessage, error) {
	rows, err := q.QueryContext(ctx,
		`SELECT seq, role, content, name, tool_call_id, tool_calls
		   FROM telegram_session_messages
		  WHERE chat_id = $1 AND session_key = $2
		  ORDER BY seq ASC`,
		chatID, session,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var history []rowMessage
	for rows.Next() {
		var item rowMessage
		var rawToolCalls []byte
		if err := rows.Scan(&item.seq, &item.msg.Role, &item.msg.Content, &item.msg.Name, &item.msg.ToolCallID, &rawToolCalls); err != nil {
			return nil, err
		}
		if len(rawToolCalls) > 0 {
			if err := json.Unmarshal(rawToolCalls, &item.msg.ToolCalls); err != nil {
				return nil, err
			}
		}
		history = append(history, item)
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	return history, nil
}
