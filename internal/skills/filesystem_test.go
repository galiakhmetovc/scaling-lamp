package skills

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func TestFilesystemCatalogListsWorkspaceSkills(t *testing.T) {
	root := t.TempDir()
	if err := os.MkdirAll(filepath.Join(root, "skills", "deploy"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "skills", "deploy", "SKILL.md"), []byte("# Deploy\n\nUse deploy workflow."), 0o644); err != nil {
		t.Fatal(err)
	}

	catalog := NewFilesystemCatalog(root)
	bundles, err := catalog.List()
	if err != nil {
		t.Fatal(err)
	}
	if len(bundles) != 1 || bundles[0].Name != "deploy" {
		t.Fatalf("unexpected bundles: %#v", bundles)
	}
}

func TestFilesystemCatalogSkipsInvalidFoldersAndSupportsGet(t *testing.T) {
	root := t.TempDir()
	if err := os.MkdirAll(filepath.Join(root, "skills", "a"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(root, "skills", "b"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "skills", "b", "SKILL.md"), []byte("# B\n\nSecond."), 0o644); err != nil {
		t.Fatal(err)
	}

	catalog := NewFilesystemCatalog(root)
	bundles, err := catalog.List()
	if err != nil {
		t.Fatal(err)
	}
	if len(bundles) != 1 || bundles[0].Name != "b" {
		t.Fatalf("unexpected bundles: %#v", bundles)
	}
	bundle, ok, err := catalog.Get("b")
	if err != nil {
		t.Fatal(err)
	}
	if !ok || bundle.Name != "b" {
		t.Fatalf("unexpected bundle: %#v ok=%v", bundle, ok)
	}
}

func TestFilesystemCatalogIncludesSkillResources(t *testing.T) {
	root := t.TempDir()
	skillRoot := filepath.Join(root, "skills", "deploy")
	for _, dir := range []string{
		filepath.Join(skillRoot, "scripts"),
		filepath.Join(skillRoot, "references"),
		filepath.Join(skillRoot, "assets"),
	} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(filepath.Join(skillRoot, "SKILL.md"), []byte("---\nname: deploy\n---\n\nUse deploy workflow."), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(skillRoot, "scripts", "check.sh"), []byte("#!/bin/sh"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(skillRoot, "references", "README.md"), []byte("ref"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(skillRoot, "assets", "logo.txt"), []byte("asset"), 0o644); err != nil {
		t.Fatal(err)
	}

	catalog := NewFilesystemCatalog(root)
	bundle, ok, err := catalog.Get("deploy")
	if err != nil {
		t.Fatal(err)
	}
	if !ok {
		t.Fatal("expected deploy skill")
	}
	if got, want := bundle.Scripts, []string{"scripts/check.sh"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("unexpected scripts: got=%#v want=%#v", got, want)
	}
	if got, want := bundle.References, []string{"references/README.md"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("unexpected references: got=%#v want=%#v", got, want)
	}
	if got, want := bundle.Assets, []string{"assets/logo.txt"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("unexpected assets: got=%#v want=%#v", got, want)
	}
}

func TestFilesystemCatalogDiscoversCrossClientAgentsDirectory(t *testing.T) {
	root := t.TempDir()
	if err := os.MkdirAll(filepath.Join(root, ".agents", "skills", "incident"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, ".agents", "skills", "incident", "SKILL.md"), []byte("---\nname: incident\ndescription: Incident workflow\n---\n\nUse incident workflow."), 0o644); err != nil {
		t.Fatal(err)
	}

	catalog := NewFilesystemCatalog(root)
	bundle, ok, err := catalog.Get("incident")
	if err != nil {
		t.Fatal(err)
	}
	if !ok || bundle.Name != "incident" {
		t.Fatalf("unexpected bundle: %#v ok=%v", bundle, ok)
	}
}

func TestComposePromptOrdersAndBoundsSkillPrompts(t *testing.T) {
	bundles := []Bundle{
		{Name: "b", Prompt: "second"},
		{Name: "a", Prompt: "first"},
	}
	out := ComposePrompt(bundles)
	if !strings.Contains(out, "first") || !strings.Contains(out, "second") {
		t.Fatalf("missing prompts: %q", out)
	}
	if strings.Index(out, "first") > strings.Index(out, "second") {
		t.Fatalf("expected lexical order: %q", out)
	}
}

func TestComposePromptTruncatesLargePrompt(t *testing.T) {
	bundles := []Bundle{{Name: "a", Prompt: strings.Repeat("x", defaultPromptBudget+100)}}
	out := ComposePrompt(bundles)
	if !strings.Contains(out, "[truncated]") {
		t.Fatalf("expected truncation marker, got %q", out)
	}
}

func TestComposeCatalogBuildsCompactAvailableSkillsSection(t *testing.T) {
	bundles := []Bundle{
		{Name: "deploy", Description: "Safe deploy workflow"},
		{Name: "incident", Description: "Incident triage"},
	}
	out := ComposeCatalog(bundles)
	if !strings.Contains(out, "## Available skills") {
		t.Fatalf("missing header: %q", out)
	}
	if !strings.Contains(out, "`deploy` — Safe deploy workflow") || !strings.Contains(out, "`incident` — Incident triage") {
		t.Fatalf("missing generated skill index: %q", out)
	}
	if !strings.Contains(out, "skills.read") || !strings.Contains(out, "activate_skill") {
		t.Fatalf("missing protocol hints: %q", out)
	}
}

func TestCatalogSummariesReturnCompactList(t *testing.T) {
	bundles := []Bundle{
		{Name: "deploy", Description: "Safe deploy workflow"},
		{Name: "incident", Description: "Incident triage"},
	}
	got := Summaries(bundles)
	want := []Summary{
		{Name: "deploy", Description: "Safe deploy workflow"},
		{Name: "incident", Description: "Incident triage"},
	}
	if !reflect.DeepEqual(want, got) {
		t.Fatalf("unexpected summaries: %#v", got)
	}
}
