package telegram

import (
	"strings"

	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
)

func (a *Adapter) recentWorkPrompt(chatID int64, messages []provider.Message) (string, error) {
	if a == nil || a.runStore == nil {
		return "", nil
	}
	head, ok, err := a.runStore.SessionHead(chatID, a.meshSessionID(chatID))
	if err != nil || !ok {
		return "", err
	}
	query := latestUserContent(messages)
	if strings.TrimSpace(query) == "" {
		return "", nil
	}
	return runtimex.BuildRecentWorkPrompt(query, head), nil
}

func latestUserContent(messages []provider.Message) string {
	for i := len(messages) - 1; i >= 0; i-- {
		if messages[i].Role == "user" && strings.TrimSpace(messages[i].Content) != "" {
			return messages[i].Content
		}
	}
	return ""
}
