package runtime

import (
	"context"
	"fmt"
	"strings"

	"teamd/internal/contracts"
	"teamd/internal/provider"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func (a *Agent) contextSummaryProjection() *projections.ContextSummaryProjection {
	for _, projection := range a.Projections {
		summary, ok := projection.(*projections.ContextSummaryProjection)
		if ok {
			return summary
		}
	}
	return nil
}

func (a *Agent) CurrentContextSummary(sessionID string) projections.ContextSummaryView {
	if projection := a.contextSummaryProjection(); projection != nil {
		return projection.SnapshotForSession(sessionID)
	}
	return projections.ContextSummaryView{}
}

func (a *Agent) preparePromptMessages(ctx context.Context, contractSet contracts.ResolvedContracts, sessionID string, rawMessages []contracts.Message, allowSummary bool) ([]contracts.Message, error) {
	if allowSummary {
		if err := a.maybeRefreshContextSummary(ctx, contractSet, sessionID, rawMessages); err != nil {
			return nil, err
		}
	}
	compacted := a.compactedMessagesForPrompt(contractSet, sessionID, rawMessages)
	return a.assemblePromptMessages(contractSet, sessionID, compacted)
}

func (a *Agent) compactedMessagesForPrompt(contractSet contracts.ResolvedContracts, sessionID string, rawMessages []contracts.Message) []contracts.Message {
	summary := a.CurrentContextSummary(sessionID)
	if strings.TrimSpace(summary.SummaryText) == "" || summary.CoveredMessages <= 0 {
		return append([]contracts.Message{}, rawMessages...)
	}
	covered := summary.CoveredMessages
	if covered > len(rawMessages) {
		covered = len(rawMessages)
	}
	out := make([]contracts.Message, 0, 1+len(rawMessages)-covered)
	out = append(out, contracts.Message{
		Role:    "system",
		Content: fmt.Sprintf("Conversation summary covering earlier context (messages 1-%d):\n%s", covered, summary.SummaryText),
	})
	out = append(out, rawMessages[covered:]...)
	return out
}

func (a *Agent) CompactedMessagesForSession(sessionID string, rawMessages []contracts.Message) []contracts.Message {
	if a == nil {
		return append([]contracts.Message{}, rawMessages...)
	}
	return a.compactedMessagesForPrompt(a.Contracts, sessionID, rawMessages)
}

func (a *Agent) maybeRefreshContextSummary(ctx context.Context, contractSet contracts.ResolvedContracts, sessionID string, rawMessages []contracts.Message) error {
	if a == nil || a.ProviderClient == nil {
		return nil
	}
	policy := contractSet.ContextBudget.Compaction
	if !policy.Enabled || policy.Strategy != "rolling_summary_v1" {
		return nil
	}
	params := policy.Params
	if params.CompactionTokens <= 0 || len(rawMessages) == 0 {
		return nil
	}
	if params.MinMessagesToSummarize <= 0 {
		params.MinMessagesToSummarize = 8
	}
	if params.KeepRecentMessages < 0 {
		params.KeepRecentMessages = 0
	}
	if params.RefreshEveryMessages <= 0 {
		params.RefreshEveryMessages = 1
	}
	if approximateMessagesTokens(rawMessages, contractSet.ContextBudget.Estimation.Params.CharsPerToken) < params.CompactionTokens {
		return nil
	}
	coverUntil := len(rawMessages) - params.KeepRecentMessages
	if coverUntil < params.MinMessagesToSummarize {
		return nil
	}
	existing := a.CurrentContextSummary(sessionID)
	if coverUntil <= existing.CoveredMessages {
		return nil
	}
	if existing.CoveredMessages > 0 && coverUntil-existing.CoveredMessages < params.RefreshEveryMessages {
		return nil
	}
	summaryText, usage, err := a.generateContextSummary(ctx, contractSet, existing, rawMessages[:coverUntil])
	if err != nil {
		return fmt.Errorf("generate context summary: %w", err)
	}
	summaryText = trimSummaryText(summaryText, params.MaxSummaryChars)
	if strings.TrimSpace(summaryText) == "" {
		return nil
	}
	summaryTokens := approximateTextTokens(summaryText, contractSet.ContextBudget.Estimation.Params.CharsPerToken)
	if usage.OutputTokens > 0 {
		summaryTokens = usage.OutputTokens
	}
	artifactRef := ""
	var artifactRefs []string
	if params.StoreArtifacts && a.ArtifactStore != nil {
		record, err := a.ArtifactStore.Write(ctx, "context_summary", summaryText, 240)
		if err == nil {
			artifactRef = record.Ref
			artifactRefs = []string{record.Ref}
		}
	}
	return a.RecordEvent(ctx, eventing.Event{
		ID:               a.newID("evt-context-summary"),
		Kind:             eventing.EventContextSummaryUpdated,
		OccurredAt:       a.now(),
		AggregateID:      sessionID,
		AggregateType:    eventing.AggregateSession,
		CorrelationID:    sessionID,
		Source:           "agent.context_budget",
		ActorID:          a.Config.ID,
		ActorType:        "agent",
		TraceSummary:     "rolling context summary updated",
		ArtifactRefs:     artifactRefs,
		Payload: map[string]any{
			"session_id":              sessionID,
			"summary_text":            summaryText,
			"covered_messages":        coverUntil,
			"summary_tokens":          summaryTokens,
			"summarization_count":     existing.SummarizationCount + 1,
			"compacted_message_count": coverUntil,
			"artifact_ref":            artifactRef,
		},
	})
}

func (a *Agent) generateContextSummary(ctx context.Context, contractSet contracts.ResolvedContracts, existing projections.ContextSummaryView, messages []contracts.Message) (string, provider.Usage, error) {
	if len(messages) == 0 {
		return "", provider.Usage{}, nil
	}
	instructions := contractSet.ContextBudget.Compaction.Params.Instructions
	if strings.TrimSpace(instructions) == "" {
		instructions = "Summarize earlier conversation faithfully for continued coding work. Preserve decisions, changed files, verification results, open risks, and unresolved follow-ups."
	}
	content := formatSummarySourceMessages(messages)
	userPrompt := "Earlier conversation to summarize:\n" + content
	if strings.TrimSpace(existing.SummaryText) != "" {
		userPrompt = "Existing rolling summary:\n" + existing.SummaryText + "\n\nNew full earlier conversation to reconcile:\n" + content
	}
	summaryContracts := disableBuiltinTools(contractSet)
	result, err := a.ProviderClient.Execute(ctx, summaryContracts, provider.ClientInput{
		Messages: []contracts.Message{
			{Role: "system", Content: instructions},
			{Role: "user", Content: userPrompt},
		},
	})
	if err != nil {
		return "", provider.Usage{}, err
	}
	return result.Provider.Message.Content, result.Provider.Usage, nil
}

func formatSummarySourceMessages(messages []contracts.Message) string {
	lines := make([]string, 0, len(messages))
	for _, message := range messages {
		role := strings.ToUpper(message.Role)
		if role == "" {
			role = "MESSAGE"
		}
		lines = append(lines, role+": "+strings.TrimSpace(message.Content))
	}
	return strings.Join(lines, "\n")
}

func trimSummaryText(text string, maxChars int) string {
	text = strings.TrimSpace(text)
	if maxChars <= 0 || len([]rune(text)) <= maxChars {
		return text
	}
	runes := []rune(text)
	return strings.TrimSpace(string(runes[:maxChars])) + "…"
}

func approximateMessagesTokens(messages []contracts.Message, charsPerToken int) int {
	chars := 0
	for _, message := range messages {
		chars += len([]rune(message.Content))
	}
	if charsPerToken <= 0 {
		charsPerToken = 4
	}
	if chars <= 0 {
		return 0
	}
	return (chars + charsPerToken - 1) / charsPerToken
}

func approximateTextTokens(text string, charsPerToken int) int {
	if charsPerToken <= 0 {
		charsPerToken = 4
	}
	chars := len([]rune(text))
	if chars <= 0 {
		return 0
	}
	return (chars + charsPerToken - 1) / charsPerToken
}
