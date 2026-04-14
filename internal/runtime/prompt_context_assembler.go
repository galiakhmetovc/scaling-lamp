package runtime

import (
	"strings"

	"teamd/internal/provider"
)

type PromptContextAssembler struct {
	WorkspaceContext func(chatID int64) string
	SessionHead      func(chatID int64) (string, error)
	RecentWork       func(chatID int64, messages []provider.Message) (string, error)
	MemoryRecall     func(chatID int64, messages []provider.Message) (string, error)
	SkillsCatalog    func() (string, error)
	ActiveSkills     func(chatID int64) (string, error)
}

func (a PromptContextAssembler) Build(chatID int64, messages []provider.Message) (PromptContextBuild, error) {
	out := make([]provider.Message, 0, len(messages)+4)
	var parts PromptContextParts
	layers := make([]PromptContextLayer, 0, 5)
	if a.WorkspaceContext != nil {
		if text := strings.TrimSpace(a.WorkspaceContext(chatID)); text != "" {
			parts.Workspace = text
			layers = append(layers, PromptContextLayer{Name: "workspace", Residency: PromptContextAlwaysLoaded, Content: text})
			out = append(out, provider.Message{Role: "system", Content: text})
		}
	}
	if a.SessionHead != nil {
		text, err := a.SessionHead(chatID)
		if err != nil {
			return PromptContextBuild{}, err
		}
		if text = strings.TrimSpace(text); text != "" {
			parts.SessionHead = text
			layers = append(layers, PromptContextLayer{Name: "session_head", Residency: PromptContextAlwaysLoaded, Content: text})
			out = append(out, provider.Message{Role: "system", Content: text})
		}
	}
	if a.RecentWork != nil {
		text, err := a.RecentWork(chatID, messages)
		if err != nil {
			return PromptContextBuild{}, err
		}
		if text = strings.TrimSpace(text); text != "" {
			layers = append(layers, PromptContextLayer{Name: "recent_work", Residency: PromptContextTriggerLoaded, Content: text})
			out = append(out, provider.Message{Role: "system", Content: text})
		}
	}
	if a.MemoryRecall != nil {
		text, err := a.MemoryRecall(chatID, messages)
		if err != nil {
			return PromptContextBuild{}, err
		}
		if text = strings.TrimSpace(text); text != "" {
			parts.MemoryRecall = text
			layers = append(layers, PromptContextLayer{Name: "memory_recall", Residency: PromptContextTriggerLoaded, Content: text})
			out = append(out, provider.Message{Role: "system", Content: text})
		}
	}
	if a.SkillsCatalog != nil {
		text, err := a.SkillsCatalog()
		if err != nil {
			return PromptContextBuild{}, err
		}
		if text = strings.TrimSpace(text); text != "" {
			parts.SkillsCatalog = text
			layers = append(layers, PromptContextLayer{Name: "skills_catalog", Residency: PromptContextAlwaysLoaded, Content: text})
			out = append(out, provider.Message{Role: "system", Content: text})
		}
	}
	if a.ActiveSkills != nil {
		text, err := a.ActiveSkills(chatID)
		if err != nil {
			return PromptContextBuild{}, err
		}
		if text = strings.TrimSpace(text); text != "" {
			parts.ActiveSkills = text
			layers = append(layers, PromptContextLayer{Name: "active_skills", Residency: PromptContextTriggerLoaded, Content: text})
			out = append(out, provider.Message{Role: "system", Content: text})
		}
	}
	out = append(out, messages...)
	return PromptContextBuild{Messages: out, Parts: parts, Layers: layers}, nil
}

func (a PromptContextAssembler) Inject(chatID int64, messages []provider.Message) ([]provider.Message, error) {
	build, err := a.Build(chatID, messages)
	if err != nil {
		return nil, err
	}
	return build.Messages, nil
}
