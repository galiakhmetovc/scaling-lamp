package telegram

import (
	"log/slog"
	"strings"
	"time"

	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
	"teamd/internal/worker"
)

func (a *Adapter) persistCheckpoint(chatID int64, checkpoint worker.Checkpoint, originatingIntent string) {
	if a.runStore == nil {
		return
	}
	record := runtimex.Checkpoint{
		ChatID:            chatID,
		SessionID:         a.meshSessionID(chatID),
		OriginatingIntent: strings.TrimSpace(originatingIntent),
		WhatHappened:      checkpoint.WhatHappened,
		WhatMattersNow:    checkpoint.WhatMattersNow,
		UpdatedAt:         time.Now().UTC(),
	}
	if err := a.runStore.SaveCheckpoint(record); err != nil {
		slog.Warn("runtime_store_save_checkpoint_failed", "chat_id", chatID, "err", err)
	}
	policy := a.memoryPolicyForChat(chatID)
	if a.memory != nil {
		doc, ok := runtimex.BuildCheckpointDocumentWithPolicy(policy, chatID, record.SessionID, originatingIntent, checkpoint, record.UpdatedAt)
		if !ok {
			return
		}
		if err := a.memory.UpsertDocument(doc); err != nil {
			slog.Warn("memory_store_upsert_checkpoint_failed", "chat_id", chatID, "err", err)
		}
	}
}

func (a *Adapter) persistContinuity(chatID int64, resp provider.PromptResponse) {
	if a.runStore == nil {
		return
	}
	messages, err := a.store.Messages(chatID)
	if err != nil {
		slog.Warn("runtime_store_load_messages_failed", "chat_id", chatID, "err", err)
		return
	}
	userGoal := strings.TrimSpace(lastUserMessage(messages))
	if userGoal == "" {
		return
	}
	record := runtimex.Continuity{
		ChatID:          chatID,
		SessionID:       a.meshSessionID(chatID),
		UserGoal:        userGoal,
		CurrentState:    "answer_sent",
		ResolvedFacts:   runtimex.CompactResolvedFactsWithPolicy(a.memoryPolicyForChat(chatID), resp.Text),
		UnresolvedItems: nil,
		UpdatedAt:       time.Now().UTC(),
	}
	if err := a.runStore.SaveContinuity(record); err != nil {
		slog.Warn("runtime_store_save_continuity_failed", "chat_id", chatID, "err", err)
	}
	if a.memory != nil {
		doc, ok := runtimex.BuildContinuityDocumentWithPolicy(a.memoryPolicyForChat(chatID), record)
		if !ok {
			return
		}
		if err := a.memory.UpsertDocument(doc); err != nil {
			slog.Warn("memory_store_upsert_continuity_failed", "chat_id", chatID, "err", err)
		}
	}
}
