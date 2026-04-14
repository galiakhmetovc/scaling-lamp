package runtime

import (
	"strings"

	"teamd/internal/compaction"
	"teamd/internal/provider"
	"teamd/internal/worker"
)

func ComputePromptBudgetMetrics(budget compaction.Budget, raw []provider.Message, checkpoint worker.Checkpoint, base []provider.Message, build PromptContextBuild) PromptBudgetMetrics {
	metrics := PromptBudgetMetrics{
		ContextWindowTokens:     budget.ContextWindowTokens,
		PromptBudgetTokens:      budget.PromptBudgetTokens,
		CompactionTriggerTokens: budget.CompactionTriggerTokens,
		RawTranscriptTokens:     compaction.EstimateMessages(raw),
		CheckpointTokens:        estimateCheckpointTokens(checkpoint),
		WorkspaceTokens:         estimateSystemText(build.Parts.Workspace),
		SessionHeadTokens:       estimateSystemText(build.Parts.SessionHead),
		MemoryRecallTokens:      estimateSystemText(build.Parts.MemoryRecall),
		SkillsCatalogTokens:     estimateSystemText(build.Parts.SkillsCatalog),
		ActiveSkillsTokens:      estimateSystemText(build.Parts.ActiveSkills),
		BasePromptTokens:        compaction.EstimateMessages(base),
	}
	metrics.SystemOverheadTokens = metrics.WorkspaceTokens + metrics.SessionHeadTokens + metrics.MemoryRecallTokens + metrics.SkillsCatalogTokens + metrics.ActiveSkillsTokens
	metrics.FinalPromptTokens = metrics.BasePromptTokens + metrics.SystemOverheadTokens
	if metrics.PromptBudgetTokens > 0 {
		metrics.PromptBudgetPercent = ceilPercent(metrics.FinalPromptTokens, metrics.PromptBudgetTokens)
	}
	if metrics.ContextWindowTokens > 0 {
		metrics.ContextWindowPercent = ceilPercent(metrics.FinalPromptTokens, metrics.ContextWindowTokens)
	}
	metrics.Layers = make([]PromptBudgetLayer, 0, len(build.Layers))
	for _, layer := range build.Layers {
		metrics.Layers = append(metrics.Layers, PromptBudgetLayer{
			Name:      layer.Name,
			Residency: layer.Residency,
			Tokens:    estimateSystemText(layer.Content),
		})
	}
	return metrics
}

func estimateSystemText(text string) int {
	text = strings.TrimSpace(text)
	if text == "" {
		return 0
	}
	return compaction.EstimateMessage(provider.Message{Role: "system", Content: text})
}

func estimateCheckpointTokens(checkpoint worker.Checkpoint) int {
	msg, ok := compaction.CheckpointPromptMessage(checkpoint)
	if !ok {
		return 0
	}
	return compaction.EstimateMessage(msg)
}

func ceilPercent(part, total int) int {
	if part <= 0 || total <= 0 {
		return 0
	}
	return ((part * 100) + total - 1) / total
}
