package skills

import (
	"strings"
	"testing"
)

func TestParseSkillMarkdownExtractsNameDescriptionAndPrompt(t *testing.T) {
	raw := "---\nname: deploy\ndescription: Safe deploy workflow\nversion: 1\n---\n\n# Deploy\n\nUse this workflow."
	bundle, err := ParseMarkdown("skills/deploy/SKILL.md", raw)
	if err != nil {
		t.Fatal(err)
	}
	if bundle.Name != "deploy" || bundle.Description != "Safe deploy workflow" || bundle.Version != "1" {
		t.Fatalf("unexpected metadata: %#v", bundle)
	}
	if !strings.Contains(bundle.Prompt, "Use this workflow.") {
		t.Fatalf("expected prompt body, got %q", bundle.Prompt)
	}
}

func TestParseSkillMarkdownFallsBackToDirectoryName(t *testing.T) {
	bundle, err := ParseMarkdown("skills/incident/SKILL.md", "# Incident\n\nHandle incidents.")
	if err != nil {
		t.Fatal(err)
	}
	if bundle.Name != "incident" {
		t.Fatalf("expected fallback name, got %q", bundle.Name)
	}
	if bundle.Description != "Handle incidents." {
		t.Fatalf("expected fallback description, got %q", bundle.Description)
	}
}

func TestParseSkillMarkdownParsesCanonicalYAMLFrontmatter(t *testing.T) {
	raw := "---\nname: deploy-safe\nversion: 1.2.0\ndescription: \"Safe deploy workflow\"\nlicense: Apache-2.0\nallowed-tools:\n  - shell.exec\n  - filesystem.read_file\n---\n\n# Deploy\n\nFollow the deploy checklist."
	bundle, err := ParseMarkdown("skills/deploy-safe/SKILL.md", raw)
	if err != nil {
		t.Fatal(err)
	}
	if bundle.Name != "deploy-safe" {
		t.Fatalf("unexpected name: %#v", bundle)
	}
	if bundle.Description != "Safe deploy workflow" {
		t.Fatalf("unexpected description: %#v", bundle)
	}
	if bundle.Version != "1.2.0" {
		t.Fatalf("unexpected version: %#v", bundle)
	}
	if bundle.License != "Apache-2.0" {
		t.Fatalf("unexpected license: %#v", bundle)
	}
	if got, want := bundle.AllowedTools, []string{"shell.exec", "filesystem.read_file"}; !equalStrings(got, want) {
		t.Fatalf("unexpected allowed tools: got=%#v want=%#v", got, want)
	}
	if !strings.Contains(bundle.Prompt, "Follow the deploy checklist.") {
		t.Fatalf("expected prompt body, got %q", bundle.Prompt)
	}
}

func equalStrings(got []string, want []string) bool {
	if len(got) != len(want) {
		return false
	}
	for i := range got {
		if got[i] != want[i] {
			return false
		}
	}
	return true
}
