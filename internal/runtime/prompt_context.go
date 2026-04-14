package runtime

import (
	"context"
	"fmt"
	"time"

	"teamd/internal/compaction"
	"teamd/internal/provider"
)

func prepareConversationRound(ctx context.Context, chatID int64, hooks ConversationHooks) ([]provider.Message, string, PromptBudgetMetrics, error) {
	messages, err := hooks.Store.Messages(chatID)
	if err != nil {
		return nil, "", PromptBudgetMetrics{}, err
	}
	if err := maybeCompactConversation(ctx, chatID, hooks, messages); err != nil {
		return nil, "", PromptBudgetMetrics{}, err
	}
	messages, err = hooks.Store.Messages(chatID)
	if err != nil {
		return nil, "", PromptBudgetMetrics{}, err
	}
	prunedMessages := PrunePromptResidency(messages, hooks.Budget.MaxToolContextChars)
	checkpoint, _, err := hooks.Store.Checkpoint(chatID)
	if err != nil {
		return nil, "", PromptBudgetMetrics{}, err
	}
	base := compaction.AssemblePrompt(hooks.Budget, checkpoint, prunedMessages)
	var assembled []provider.Message
	build := PromptContextBuild{Messages: base}
	if hooks.BuildPromptContext != nil {
		var err error
		build, err = hooks.BuildPromptContext(chatID, base)
		if err != nil {
			return nil, "", PromptBudgetMetrics{}, err
		}
		assembled = build.Messages
	} else {
		assembled, err = hooks.InjectPromptContext(chatID, base)
		if err != nil {
			return nil, "", PromptBudgetMetrics{}, err
		}
		build.Messages = assembled
	}
	metrics := ComputePromptBudgetMetrics(hooks.Budget, messages, checkpoint, base, build)
	return assembled, hooks.LastUserMessage(messages), metrics, nil
}

func PreviewConversationRound(chatID int64, hooks ConversationHooks) ([]provider.Message, string, PromptBudgetMetrics, error) {
	return prepareConversationRound(context.Background(), chatID, hooks)
}

func maybeCompactConversation(ctx context.Context, chatID int64, hooks ConversationHooks, messages []provider.Message) error {
	needsCompaction, err := needsProjectedCompaction(chatID, hooks, messages)
	if err != nil {
		return err
	}
	if !needsCompaction {
		return nil
	}
	activeSession, err := hooks.Store.ActiveSession(chatID)
	if err != nil {
		return err
	}
	messages, err = hooks.Store.Messages(chatID)
	if err != nil {
		return err
	}
	needsCompaction, err = needsProjectedCompaction(chatID, hooks, messages)
	if err != nil {
		return err
	}
	if !needsCompaction {
		return nil
	}

	older := messages
	if len(messages) > 4 {
		older = messages[:len(messages)-4]
	}

	lines := make([]string, 0, len(older))
	for _, msg := range older {
		reduced := compaction.ReduceForCompaction(msg, hooks.Budget.MaxToolContextChars)
		lines = append(lines, fmt.Sprintf("%s: %s", msg.Role, reduced.Content))
	}

	compactCtx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	out, err := hooks.Compactor.Compact(compactCtx, compaction.Input{
		SessionID:    fmt.Sprintf("telegram:%d/%s", chatID, activeSession),
		Transcript:   lines,
		ArchiveRefs:  archiveRefsForTranscript(chatID, activeSession, len(older)),
		ArtifactRefs: nil,
	})
	if err != nil {
		return nil
	}
	if err := hooks.Store.SaveCheckpoint(chatID, out); err != nil {
		return err
	}
	if hooks.OnCheckpointSaved != nil {
		hooks.OnCheckpointSaved(chatID, out, hooks.LastUserMessage(messages))
	}
	return nil
}

func needsProjectedCompaction(chatID int64, hooks ConversationHooks, messages []provider.Message) (bool, error) {
	if hooks.Budget.NeedsCompaction(compaction.EstimateMessages(messages)) {
		return true, nil
	}
	checkpoint, _, err := hooks.Store.Checkpoint(chatID)
	if err != nil {
		return false, err
	}
	pruned := PrunePromptResidency(messages, hooks.Budget.MaxToolContextChars)
	base := compaction.AssemblePrompt(hooks.Budget, checkpoint, pruned)
	if hooks.BuildPromptContext != nil {
		build, err := hooks.BuildPromptContext(chatID, base)
		if err != nil {
			return false, err
		}
		metrics := ComputePromptBudgetMetrics(hooks.Budget, messages, checkpoint, base, build)
		return hooks.Budget.NeedsCompaction(metrics.FinalPromptTokens), nil
	}
	if hooks.InjectPromptContext != nil {
		assembled, err := hooks.InjectPromptContext(chatID, base)
		if err != nil {
			return false, err
		}
		return hooks.Budget.NeedsCompaction(compaction.EstimateMessages(assembled)), nil
	}
	return hooks.Budget.NeedsCompaction(compaction.EstimateMessages(base)), nil
}

func archiveRefsForTranscript(chatID int64, sessionID string, count int) []string {
	if count <= 0 {
		return nil
	}
	return []string{fmt.Sprintf("archive://telegram/%d/%s?messages=0-%d", chatID, sessionID, count-1)}
}

func providerRoundContext(ctx context.Context, timeout time.Duration) (context.Context, context.CancelFunc) {
	if timeout <= 0 {
		return ctx, func() {}
	}
	return context.WithTimeout(ctx, timeout)
}

func errorsIsProviderRoundTimeout(err error, parent context.Context) bool {
	if err == nil {
		return false
	}
	if parent != nil && parent.Err() != nil {
		return false
	}
	return err == context.DeadlineExceeded
}
