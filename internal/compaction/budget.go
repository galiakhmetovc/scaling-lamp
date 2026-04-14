package compaction

import "teamd/internal/provider"

type Budget struct {
	ContextWindowTokens    int
	PromptBudgetTokens     int
	CompactionTriggerTokens int
	MaxToolContextChars    int
}

func EstimateMessage(msg provider.Message) int {
	contentTokens := len(msg.Content) / 4
	roleOverhead := 8
	nameOverhead := len(msg.Name) / 8
	toolCallOverhead := len(msg.ToolCalls) * 12

	estimate := contentTokens + roleOverhead + nameOverhead + toolCallOverhead
	estimate = (estimate * 12) / 10
	if estimate < 1 {
		return 1
	}
	return estimate
}

func EstimateMessages(messages []provider.Message) int {
	total := 0
	for _, msg := range messages {
		total += EstimateMessage(msg)
	}
	return total
}

func (b Budget) NeedsCompaction(tokens int) bool {
	return tokens >= b.CompactionTriggerTokens
}
