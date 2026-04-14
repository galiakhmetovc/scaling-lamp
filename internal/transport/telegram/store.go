package telegram

import (
	"strings"

	"teamd/internal/provider"
	"teamd/internal/worker"
)

type Store interface {
	TranscriptStore
	CheckpointStore
	SessionSelector
}

type TranscriptStore interface {
	Append(chatID int64, msg provider.Message) error
	Messages(chatID int64) ([]provider.Message, error)
	Reset(chatID int64) error
}

type CheckpointStore interface {
	Checkpoint(chatID int64) (worker.Checkpoint, bool, error)
	SaveCheckpoint(chatID int64, checkpoint worker.Checkpoint) error
}

type SessionSelector interface {
	ActiveSession(chatID int64) (string, error)
	CreateSession(chatID int64, session string) error
	UseSession(chatID int64, session string) error
	ListSessions(chatID int64) ([]string, error)
}

func sanitizeMessage(msg provider.Message) provider.Message {
	msg.Role = strings.ToValidUTF8(msg.Role, "")
	msg.Content = strings.ToValidUTF8(msg.Content, "")
	msg.Name = strings.ToValidUTF8(msg.Name, "")
	msg.ToolCallID = strings.ToValidUTF8(msg.ToolCallID, "")
	if len(msg.ToolCalls) == 0 {
		return msg
	}
	sanitized := make([]provider.ToolCall, len(msg.ToolCalls))
	for i, call := range msg.ToolCalls {
		call.ID = strings.ToValidUTF8(call.ID, "")
		call.Name = strings.ToValidUTF8(call.Name, "")
		call.Arguments = sanitizeValue(call.Arguments).(map[string]any)
		sanitized[i] = call
	}
	msg.ToolCalls = sanitized
	return msg
}

func sanitizeValue(v any) any {
	switch typed := v.(type) {
	case string:
		return strings.ToValidUTF8(typed, "")
	case []any:
		out := make([]any, len(typed))
		for i, item := range typed {
			out[i] = sanitizeValue(item)
		}
		return out
	case map[string]any:
		out := make(map[string]any, len(typed))
		for k, item := range typed {
			out[strings.ToValidUTF8(k, "")] = sanitizeValue(item)
		}
		return out
	default:
		return v
	}
}

func sanitizeCheckpoint(checkpoint worker.Checkpoint) worker.Checkpoint {
	checkpoint.SessionID = strings.ToValidUTF8(checkpoint.SessionID, "")
	checkpoint.CompactionMethod = strings.ToValidUTF8(checkpoint.CompactionMethod, "")
	checkpoint.WhatHappened = strings.ToValidUTF8(checkpoint.WhatHappened, "")
	checkpoint.WhatMattersNow = strings.ToValidUTF8(checkpoint.WhatMattersNow, "")
	for i, item := range checkpoint.UnresolvedItems {
		checkpoint.UnresolvedItems[i] = strings.ToValidUTF8(item, "")
	}
	for i, item := range checkpoint.NextActions {
		checkpoint.NextActions[i] = strings.ToValidUTF8(item, "")
	}
	for i, item := range checkpoint.ArchiveRefs {
		checkpoint.ArchiveRefs[i] = strings.ToValidUTF8(item, "")
	}
	for i, item := range checkpoint.SourceArtifacts {
		checkpoint.SourceArtifacts[i] = strings.ToValidUTF8(item, "")
	}
	return checkpoint
}

func trimHistory(history []provider.Message, limit int) []provider.Message {
	if limit <= 0 || len(history) <= limit {
		return history
	}
	start := trimHistoryStart(history, limit)
	return history[start:]
}

func trimHistoryStart(history []provider.Message, limit int) int {
	if limit <= 0 || len(history) <= limit {
		return 0
	}
	activeTurnStart := -1
	for i := len(history) - 1; i >= 0; i-- {
		if history[i].Role == "user" {
			activeTurnStart = i
			break
		}
	}
	if activeTurnStart < 0 {
		return len(history) - limit
	}

	protected := history[activeTurnStart:]
	if len(protected) >= limit {
		return activeTurnStart
	}

	older := history[:activeTurnStart]
	keepOlder := limit - len(protected)
	if keepOlder > len(older) {
		keepOlder = len(older)
	}
	return len(older) - keepOlder
}
