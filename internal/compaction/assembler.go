package compaction

import (
	"fmt"
	"sort"
	"strings"
	"unicode"

	"teamd/internal/provider"
	"teamd/internal/worker"
)

func AssemblePrompt(b Budget, checkpoint worker.Checkpoint, raw []provider.Message) []provider.Message {
	var assembled []provider.Message
	total := 0

	if summary, ok := CheckpointPromptMessage(checkpoint); ok {
		assembled = append(assembled, summary)
		total += EstimateMessage(summary)
	}

	limit := b.PromptBudgetTokens
	if limit <= 0 {
		limit = total
	}

	activeTurnStart := lastUserIndex(raw)
	protectedStart := len(raw)
	if activeTurnStart >= 0 {
		protectedStart = activeTurnStart
	}

	tail := make([]provider.Message, 0, len(raw)-protectedStart)
	for i := protectedStart; i < len(raw); i++ {
		msg := ReduceForCompaction(raw[i], b.MaxToolContextChars)
		tail = append(tail, msg)
		total += EstimateMessage(msg)
	}

	prefix, used := selectPrefixMessages(raw[:protectedStart], total, limit, b.MaxToolContextChars)
	total += used
	for i := 0; i < len(prefix); i++ {
		assembled = append(assembled, prefix[i])
	}
	assembled = append(assembled, tail...)

	return assembled
}

type prefixCandidate struct {
	index int
	msg   provider.Message
	cost  int
	score int
}

func selectPrefixMessages(prefix []provider.Message, currentTotal, limit, maxChars int) ([]provider.Message, int) {
	if len(prefix) == 0 || limit <= 0 || currentTotal >= limit {
		return nil, 0
	}
	candidates := make([]prefixCandidate, 0, len(prefix))
	for i, raw := range prefix {
		msg := ReduceForCompaction(raw, maxChars)
		candidates = append(candidates, prefixCandidate{
			index: i,
			msg:   msg,
			cost:  EstimateMessage(msg),
			score: prefixScore(raw, i),
		})
	}
	sort.SliceStable(candidates, func(i, j int) bool {
		if candidates[i].score == candidates[j].score {
			return candidates[i].index > candidates[j].index
		}
		return candidates[i].score > candidates[j].score
	})
	remaining := limit - currentTotal
	selected := map[int]provider.Message{}
	used := 0
	for _, candidate := range candidates {
		if candidate.cost > remaining-used {
			continue
		}
		selected[candidate.index] = candidate.msg
		used += candidate.cost
	}
	if len(selected) == 0 {
		return nil, 0
	}
	out := make([]provider.Message, 0, len(selected))
	for i := 0; i < len(prefix); i++ {
		msg, ok := selected[i]
		if !ok {
			continue
		}
		out = append(out, msg)
	}
	return out, used
}

func prefixScore(msg provider.Message, index int) int {
	score := index
	switch msg.Role {
	case "user":
		score += 300
	case "assistant":
		if len(msg.ToolCalls) > 0 && strings.TrimSpace(msg.Content) == "" {
			score += 40
		} else {
			score += 220
		}
	case "system":
		score += 180
	case "tool":
		score += 120
	default:
		score += 80
	}
	return score
}

func lastUserIndex(raw []provider.Message) int {
	for i := len(raw) - 1; i >= 0; i-- {
		if raw[i].Role == "user" {
			return i
		}
	}
	return -1
}

func CheckpointPromptMessage(checkpoint worker.Checkpoint) (provider.Message, bool) {
	if checkpoint.WhatHappened == "" && checkpoint.WhatMattersNow == "" && len(checkpoint.ArchiveRefs) == 0 && len(checkpoint.SourceArtifacts) == 0 {
		return provider.Message{}, false
	}

	lines := []string{"Session checkpoint."}
	if text := strings.TrimSpace(checkpoint.WhatHappened); text != "" {
		lines = append(lines, "What happened: "+text)
	}
	if text := strings.TrimSpace(checkpoint.WhatMattersNow); text != "" {
		lines = append(lines, "What matters now: "+text)
	}
	if len(checkpoint.ArchiveRefs) > 0 {
		lines = append(lines, "Archive refs: "+strings.Join(checkpoint.ArchiveRefs, ", "))
	}
	if len(checkpoint.SourceArtifacts) > 0 {
		lines = append(lines, "Artifact refs: "+strings.Join(checkpoint.SourceArtifacts, ", "))
	}

	return provider.Message{
		Role:    "system",
		Content: strings.Join(lines, "\n"),
	}, true
}

func ReduceForCompaction(msg provider.Message, maxChars int) provider.Message {
	if msg.Role == "tool" && looksLikeNoisyToolOutput(msg.Content) {
		reduced := msg
		name := msg.Name
		if name == "" {
			name = "tool"
		}
		reduced.Content = fmt.Sprintf("%s tool output omitted: machine-generated or binary-derived output", name)
		return reduced
	}
	return reduceToolMessage(msg, maxChars)
}

func reduceToolMessage(msg provider.Message, maxChars int) provider.Message {
	if msg.Role != "tool" || maxChars <= 0 || len(msg.Content) <= maxChars {
		return msg
	}

	truncated := msg
	truncated.Content = fmt.Sprintf(
		"%s...[truncated %d chars]",
		msg.Content[:maxChars],
		len(msg.Content)-maxChars,
	)
	return truncated
}

func looksLikeNoisyToolOutput(content string) bool {
	content = strings.TrimSpace(content)
	if content == "" {
		return false
	}
	lines := strings.Split(content, "\n")
	longest := 0
	totalLetters := 0
	totalSpaces := 0
	totalPunct := 0
	for _, line := range lines {
		if len(line) > longest {
			longest = len(line)
		}
		for _, r := range line {
			switch {
			case unicode.IsLetter(r) || unicode.IsDigit(r):
				totalLetters++
			case unicode.IsSpace(r):
				totalSpaces++
			case unicode.IsPunct(r) || unicode.IsSymbol(r):
				totalPunct++
			}
		}
	}
	if len(lines) <= 3 && longest >= 220 {
		return true
	}
	if longest >= 160 && totalPunct > totalSpaces*2 && totalLetters > 0 {
		return true
	}
	return false
}
