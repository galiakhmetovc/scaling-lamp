package runtime

import (
	"strings"
	"time"

	"teamd/internal/provider"
)

func BuildWorkerHandoff(workerID string, lastRun *RunView, messages []provider.Message, previous *WorkerHandoff) *WorkerHandoff {
	if strings.TrimSpace(workerID) == "" {
		return nil
	}
	summary := workerSummary(messages)
	if summary == "" && lastRun == nil {
		return previous
	}
	now := time.Now().UTC()
	handoff := WorkerHandoff{
		WorkerID:    workerID,
		CreatedAt:   now,
		UpdatedAt:   now,
		Summary:     summary,
		Artifacts:   nil,
		OpenQuestions: nil,
		PromotedFacts: nil,
	}
	if previous != nil {
		handoff.CreatedAt = previous.CreatedAt
	}
	if lastRun != nil {
		handoff.LastRunID = lastRun.RunID
		handoff.Artifacts = append([]string(nil), lastRun.ArtifactRefs...)
	}
	return &handoff
}

func workerSummary(messages []provider.Message) string {
	for i := len(messages) - 1; i >= 0; i-- {
		if messages[i].Role != "assistant" {
			continue
		}
		text := strings.TrimSpace(messages[i].Content)
		if text != "" {
			return text
		}
	}
	return ""
}
