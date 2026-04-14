package telegram

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"strings"
	"time"

	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
	"teamd/internal/worker"
)

type previewConversationStore struct {
	base       Store
	sessionID  string
	pending    provider.Message
	checkpoint bool
}

func (s previewConversationStore) Append(chatID int64, msg provider.Message) error { return nil }

func (s previewConversationStore) Messages(chatID int64) ([]provider.Message, error) {
	messages, err := sessionMessagesForPreview(s.base, chatID, s.sessionID)
	if err != nil {
		return nil, err
	}
	if strings.TrimSpace(s.pending.Content) != "" {
		messages = append(messages, s.pending)
	}
	out := make([]provider.Message, len(messages))
	copy(out, messages)
	return out, nil
}

func (s previewConversationStore) Checkpoint(chatID int64) (worker.Checkpoint, bool, error) {
	if !s.checkpoint {
		return worker.Checkpoint{}, false, nil
	}
	return sessionCheckpointForPreview(s.base, chatID, s.sessionID)
}

func (s previewConversationStore) SaveCheckpoint(chatID int64, checkpoint worker.Checkpoint) error { return nil }

func (s previewConversationStore) ActiveSession(chatID int64) (string, error) {
	if strings.TrimSpace(s.sessionID) == "" {
		return s.base.ActiveSession(chatID)
	}
	return s.sessionID, nil
}

func sessionMessagesForPreview(base Store, chatID int64, sessionID string) ([]provider.Message, error) {
	switch typed := base.(type) {
	case *SessionStore:
		typed.mu.RLock()
		defer typed.mu.RUnlock()
		state := typed.ensureChat(chatID)
		history := state.messages[sessionID]
		out := make([]provider.Message, len(history))
		copy(out, history)
		return out, nil
	case *PostgresStore:
		ctx := context.Background()
		if err := typed.ensureSchema(ctx); err != nil {
			return nil, err
		}
		rows, err := typed.loadTranscriptRows(ctx, typed.db, chatID, sessionID)
		if err != nil {
			return nil, err
		}
		out := make([]provider.Message, 0, len(rows))
		for _, item := range rows {
			out = append(out, item.msg)
		}
		return out, nil
	default:
		return nil, fmt.Errorf("preview_messages_unsupported_store")
	}
}

func sessionCheckpointForPreview(base Store, chatID int64, sessionID string) (worker.Checkpoint, bool, error) {
	switch typed := base.(type) {
	case *SessionStore:
		typed.mu.RLock()
		defer typed.mu.RUnlock()
		state := typed.ensureChat(chatID)
		checkpoint, ok := state.checkpoints[sessionID]
		return checkpoint, ok, nil
	case *PostgresStore:
		ctx := context.Background()
		if err := typed.ensureSchema(ctx); err != nil {
			return worker.Checkpoint{}, false, err
		}
		var checkpoint worker.Checkpoint
		var unresolved []byte
		var nextActions []byte
		var archiveRefs []byte
		var sourceArtifacts []byte
		err := typed.db.QueryRowContext(ctx, `
SELECT compaction_method, what_happened, what_matters_now, unresolved_items, next_actions, archive_refs, source_artifacts
FROM telegram_session_checkpoints
WHERE chat_id = $1 AND session_key = $2
`, chatID, sessionID).Scan(
			&checkpoint.CompactionMethod,
			&checkpoint.WhatHappened,
			&checkpoint.WhatMattersNow,
			&unresolved,
			&nextActions,
			&archiveRefs,
			&sourceArtifacts,
		)
		if err == sql.ErrNoRows {
			return worker.Checkpoint{}, false, nil
		}
		if err != nil {
			return worker.Checkpoint{}, false, err
		}
		checkpoint.SessionID = fmt.Sprintf("telegram:%d/%s", chatID, sessionID)
		if err := json.Unmarshal(unresolved, &checkpoint.UnresolvedItems); err != nil {
			return worker.Checkpoint{}, false, err
		}
		if err := json.Unmarshal(nextActions, &checkpoint.NextActions); err != nil {
			return worker.Checkpoint{}, false, err
		}
		if err := json.Unmarshal(archiveRefs, &checkpoint.ArchiveRefs); err != nil {
			return worker.Checkpoint{}, false, err
		}
		if err := json.Unmarshal(sourceArtifacts, &checkpoint.SourceArtifacts); err != nil {
			return worker.Checkpoint{}, false, err
		}
		return checkpoint, true, nil
	default:
		return worker.Checkpoint{}, false, fmt.Errorf("preview_checkpoint_unsupported_store")
	}
}

func (a *Adapter) DebugProviderPreview(ctx context.Context, chatID int64, sessionID, query string, runtimeConfig provider.RequestConfig, profile *runtimex.DebugExecutionProfile) (provider.PromptRequest, runtimex.PromptBudgetMetrics, error) {
	if strings.TrimSpace(sessionID) == "" {
		return provider.PromptRequest{}, runtimex.PromptBudgetMetrics{}, fmt.Errorf("missing session id")
	}
	runID := fmt.Sprintf("preview-%d", timeNowUnixNano())
	if profile != nil {
		a.rememberDebugProfile(runID, profile)
		defer a.forgetDebugProfile(runID)
	}
	hooks := a.conversationHooks(withDebugRunID(ctx, runID), chatID)
	hooks.Store = previewConversationStore{
		base:      a.store,
		sessionID: sessionID,
		pending: provider.Message{
			Role:    "user",
			Content: strings.TrimSpace(query),
		},
		checkpoint: profile == nil || profile.Checkpoint,
	}
	baseConfig := hooks.RequestConfig
	hooks.RequestConfig = func(chatID int64) provider.RequestConfig {
		return runtimex.MergeRequestConfig(baseConfig(chatID), runtimeConfig)
	}
	assembled, _, metrics, err := runtimex.PreviewConversationRound(chatID, hooks)
	if err != nil {
		return provider.PromptRequest{}, runtimex.PromptBudgetMetrics{}, err
	}
	role := "telegram"
	if hooks.ToolRole != nil {
		role = hooks.ToolRole(chatID)
	}
	tools, err := hooks.ProviderTools(role)
	if err != nil {
		return provider.PromptRequest{}, runtimex.PromptBudgetMetrics{}, err
	}
	return provider.PromptRequest{
		WorkerID: fmt.Sprintf("telegram:%d", chatID),
		Messages: assembled,
		Tools:    tools,
		Config:   hooks.RequestConfig(chatID),
	}, metrics, nil
}

func timeNowUnixNano() int64 {
	return time.Now().UTC().UnixNano()
}
