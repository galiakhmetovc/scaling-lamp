package telegram

import (
	"strconv"
	"strings"
)

type FooterMetrics struct {
	Session          string
	Model            string
	Thinking         string
	ClearThinking    bool
	ContextTokens    int
	PromptTokens     int
	CompletionTokens int
	SessionMessages  int
	ContextEstimate  int
	Compacted        bool
}

func formatFooter(metrics FooterMetrics) string {
	lines := []string{
		"session=" + valueOrUnknown(metrics.Session),
		"model=" + valueOrUnknown(metrics.Model),
		"thinking=" + valueOrUnknown(metrics.Thinking),
		"clear_thinking=" + strconv.FormatBool(metrics.ClearThinking),
		"context_tokens=" + intOrUnknown(metrics.ContextTokens),
		"prompt_tokens=" + intOrUnknown(metrics.PromptTokens),
		"completion_tokens=" + intOrUnknown(metrics.CompletionTokens),
		"session_messages=" + intOrUnknown(metrics.SessionMessages),
		"context_estimate=" + intOrUnknown(metrics.ContextEstimate),
		"compacted=" + strconv.FormatBool(metrics.Compacted),
	}

	return strings.Join(lines, "\n")
}
