package runtime

import (
	"testing"

	"teamd/internal/provider"
)

func TestPromptContextAssemblerInjectsRuntimeOwnedFragments(t *testing.T) {
	assembler := PromptContextAssembler{
		WorkspaceContext: func(chatID int64) string { return "workspace context" },
		SessionHead:      func(chatID int64) (string, error) { return "session head", nil },
		MemoryRecall: func(chatID int64, messages []provider.Message) (string, error) {
			return "memory recall", nil
		},
		SkillsCatalog: func() (string, error) { return "skills catalog", nil },
		ActiveSkills:  func(chatID int64) (string, error) { return "active skills", nil },
	}
	out, err := assembler.Inject(1001, []provider.Message{{Role: "user", Content: "hello"}})
	if err != nil {
		t.Fatalf("inject: %v", err)
	}
	if len(out) != 6 {
		t.Fatalf("expected 6 messages, got %d", len(out))
	}
	if out[0].Content != "workspace context" || out[1].Content != "session head" || out[2].Content != "memory recall" || out[3].Content != "skills catalog" || out[4].Content != "active skills" {
		t.Fatalf("unexpected injected messages: %+v", out)
	}
	if out[5].Role != "user" || out[5].Content != "hello" {
		t.Fatalf("expected original message to remain last, got %+v", out[5])
	}
}

func TestPromptContextAssemblerBuildClassifiesLayers(t *testing.T) {
	assembler := PromptContextAssembler{
		WorkspaceContext: func(chatID int64) string { return "workspace context" },
		SessionHead:      func(chatID int64) (string, error) { return "session head", nil },
		RecentWork: func(chatID int64, messages []provider.Message) (string, error) {
			return "recent work", nil
		},
		MemoryRecall: func(chatID int64, messages []provider.Message) (string, error) {
			return "memory recall", nil
		},
		SkillsCatalog: func() (string, error) { return "skills catalog", nil },
		ActiveSkills:  func(chatID int64) (string, error) { return "active skills", nil },
	}
	build, err := assembler.Build(1001, []provider.Message{{Role: "user", Content: "hello"}})
	if err != nil {
		t.Fatalf("build: %v", err)
	}
	if len(build.Layers) != 6 {
		t.Fatalf("expected 6 layers, got %+v", build.Layers)
	}
	got := map[string]PromptContextResidency{}
	for _, layer := range build.Layers {
		got[layer.Name] = layer.Residency
	}
	if got["workspace"] != PromptContextAlwaysLoaded ||
		got["session_head"] != PromptContextAlwaysLoaded ||
		got["recent_work"] != PromptContextTriggerLoaded ||
		got["memory_recall"] != PromptContextTriggerLoaded ||
		got["skills_catalog"] != PromptContextAlwaysLoaded ||
		got["active_skills"] != PromptContextTriggerLoaded {
		t.Fatalf("unexpected layer classes: %+v", got)
	}
}

func TestPromptContextAssemblerInjectsRecentWorkFragment(t *testing.T) {
	assembler := PromptContextAssembler{
		SessionHead: func(chatID int64) (string, error) { return "session head", nil },
		RecentWork: func(chatID int64, messages []provider.Message) (string, error) {
			return "recent work", nil
		},
	}
	out, err := assembler.Inject(1001, []provider.Message{{Role: "user", Content: "continue"}})
	if err != nil {
		t.Fatalf("inject: %v", err)
	}
	if len(out) != 3 {
		t.Fatalf("expected 3 messages, got %d", len(out))
	}
	if out[1].Content != "recent work" {
		t.Fatalf("expected recent work fragment before user message, got %+v", out)
	}
}
