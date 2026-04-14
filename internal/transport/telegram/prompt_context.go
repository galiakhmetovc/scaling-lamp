package telegram

import (
	"strings"

	"teamd/internal/memory"
	"teamd/internal/provider"
)

func (a *Adapter) injectPromptContext(chatID int64, messages []provider.Message) ([]provider.Message, error) {
	out := make([]provider.Message, 0, len(messages)+3)
	if strings.TrimSpace(a.workspaceContext) != "" {
		out = append(out, provider.Message{Role: "system", Content: a.workspaceContext})
	}
	if recall, err := a.memoryRecallPrompt(chatID, messages); err != nil {
		return nil, err
	} else if strings.TrimSpace(recall) != "" {
		out = append(out, provider.Message{Role: "system", Content: recall})
	}
	if catalog, err := a.skillsCatalogPrompt(); err != nil {
		return nil, err
	} else if strings.TrimSpace(catalog) != "" {
		out = append(out, provider.Message{Role: "system", Content: catalog})
	}
	if active, err := a.activeSkillsPrompt(chatID); err != nil {
		return nil, err
	} else if strings.TrimSpace(active) != "" {
		out = append(out, provider.Message{Role: "system", Content: active})
	}
	out = append(out, messages...)
	return out, nil
}

func (a *Adapter) memoryRecallPrompt(chatID int64, messages []provider.Message) (string, error) {
	if a.memory == nil {
		return "", nil
	}
	query := strings.TrimSpace(lastUserMessage(messages))
	if query == "" {
		return "", nil
	}
	items, err := a.memory.Search(memory.RecallQuery{
		ChatID:    chatID,
		SessionID: a.meshSessionID(chatID),
		Text:      query,
		Limit:     3,
		Kinds:     a.memoryPolicyForChat(chatID).AutomaticRecallKinds,
	})
	if err != nil {
		return "", err
	}
	return memory.FormatRecallBlock(items), nil
}
