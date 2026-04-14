package runtime

import (
	"strings"
	"testing"

	"teamd/internal/compaction"
	"teamd/internal/provider"
	"teamd/internal/worker"
)

func TestComputePromptBudgetMetricsBreaksDownLayers(t *testing.T) {
	budget := compaction.Budget{
		ContextWindowTokens:     200000,
		PromptBudgetTokens:      150000,
		CompactionTriggerTokens: 120000,
		MaxToolContextChars:     4096,
	}
	raw := []provider.Message{
		{Role: "user", Content: strings.Repeat("u", 120)},
		{Role: "assistant", Content: strings.Repeat("a", 120)},
	}
	checkpoint := worker.Checkpoint{
		WhatHappened:     "older discussion compacted",
		WhatMattersNow:   "need the deployment target",
		ArchiveRefs:      []string{"archive://telegram/1001/default?messages=0-4"},
		SourceArtifacts:  []string{"artifact://run/run-1/report"},
	}
	base := compaction.AssemblePrompt(budget, checkpoint, raw)
	build := PromptContextBuild{
		Parts: PromptContextParts{
			Workspace:     "Workspace context",
			SessionHead:   "Recent context from SessionHead",
			MemoryRecall:  "Recall block",
			SkillsCatalog: "Skills catalog",
			ActiveSkills:  "Active skill prompt",
		},
		Layers: []PromptContextLayer{
			{Name: "workspace", Residency: PromptContextAlwaysLoaded, Content: "Workspace context"},
			{Name: "session_head", Residency: PromptContextAlwaysLoaded, Content: "Recent context from SessionHead"},
			{Name: "memory_recall", Residency: PromptContextTriggerLoaded, Content: "Recall block"},
			{Name: "skills_catalog", Residency: PromptContextAlwaysLoaded, Content: "Skills catalog"},
			{Name: "active_skills", Residency: PromptContextTriggerLoaded, Content: "Active skill prompt"},
		},
	}

	metrics := ComputePromptBudgetMetrics(budget, raw, checkpoint, base, build)
	if metrics.ContextWindowTokens != 200000 || metrics.PromptBudgetTokens != 150000 {
		t.Fatalf("unexpected budget config: %+v", metrics)
	}
	if metrics.RawTranscriptTokens <= 0 {
		t.Fatalf("expected raw transcript tokens: %+v", metrics)
	}
	if metrics.CheckpointTokens <= 0 {
		t.Fatalf("expected checkpoint tokens: %+v", metrics)
	}
	if metrics.WorkspaceTokens <= 0 || metrics.SessionHeadTokens <= 0 || metrics.MemoryRecallTokens <= 0 {
		t.Fatalf("expected prompt layer tokens: %+v", metrics)
	}
	if metrics.SystemOverheadTokens <= 0 {
		t.Fatalf("expected system overhead tokens: %+v", metrics)
	}
	if metrics.FinalPromptTokens <= metrics.BasePromptTokens {
		t.Fatalf("expected final prompt to exceed base prompt: %+v", metrics)
	}
	if metrics.PromptBudgetPercent <= 0 || metrics.ContextWindowPercent <= 0 {
		t.Fatalf("expected visible percents: %+v", metrics)
	}
	if len(metrics.Layers) != 5 {
		t.Fatalf("expected prompt layers in metrics: %+v", metrics)
	}
}
