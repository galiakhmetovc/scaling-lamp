package skills

import (
	"strings"
	"testing"
)

func TestToolListReturnsCompactSummaries(t *testing.T) {
	listing := ToolList([]Bundle{
		{Name: "deploy", Description: "Safe deploy workflow"},
		{Name: "incident", Description: "Incident triage"},
	})
	if len(listing) != 2 || listing[0].Name != "deploy" {
		t.Fatalf("unexpected listing: %#v", listing)
	}
}

func TestToolListCompactReturnsFullNameAndDescriptionSet(t *testing.T) {
	text := ToolListCompact([]Bundle{
		{Name: "algorithmic-art", Description: "A"},
		{Name: "brand-guidelines", Description: "B"},
		{Name: "canvas-design", Description: "C"},
		{Name: "claude-api", Description: "D"},
	}, 2)
	for _, name := range []string{"algorithmic-art", "brand-guidelines", "canvas-design", "claude-api"} {
		if !strings.Contains(text, name) {
			t.Fatalf("missing skill %q in %q", name, text)
		}
	}
	if strings.Contains(text, "omitted_skills") || strings.Contains(text, "sample_skills") {
		t.Fatalf("expected full list output, got %q", text)
	}
}

func TestToolReadReturnsOneSkillDetail(t *testing.T) {
	detail := ToolRead(Bundle{
		Name:        "deploy",
		Description: "Safe deploy workflow",
		Version:     "1",
		License:     "Apache-2.0",
		Prompt:      "Full prompt",
		AllowedTools: []string{
			"shell.exec",
		},
		Scripts:    []string{"scripts/check.sh"},
		References: []string{"references/README.md"},
		Assets:     []string{"assets/logo.txt"},
	})
	if detail.Name != "deploy" || detail.Version != "1" || detail.Prompt != "Full prompt" || detail.License != "Apache-2.0" {
		t.Fatalf("unexpected detail: %#v", detail)
	}
	if len(detail.AllowedTools) != 1 || detail.AllowedTools[0] != "shell.exec" {
		t.Fatalf("unexpected allowed tools: %#v", detail)
	}
	if len(detail.Scripts) != 1 || detail.Scripts[0] != "scripts/check.sh" {
		t.Fatalf("unexpected scripts: %#v", detail)
	}
}

func TestToolActivateWrapsPromptAndResources(t *testing.T) {
	text := ToolActivate(Bundle{
		Name:        "deploy",
		Path:        "/tmp/skills/deploy/SKILL.md",
		Description: "Safe deploy workflow",
		Prompt:      "Use deploy flow.",
		Scripts:     []string{"scripts/check.sh"},
		References:  []string{"references/README.md"},
	})
	if !strings.Contains(text, `<skill_content name="deploy">`) {
		t.Fatalf("missing wrapper: %q", text)
	}
	if !strings.Contains(text, "Skill directory: /tmp/skills/deploy") {
		t.Fatalf("missing directory: %q", text)
	}
	if !strings.Contains(text, "<file>scripts/check.sh</file>") || !strings.Contains(text, "<file>references/README.md</file>") {
		t.Fatalf("missing resources: %q", text)
	}
}
