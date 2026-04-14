package runtime

import (
	"strings"
	"testing"
)

func TestBuildRecentWorkPromptForContinuation(t *testing.T) {
	head := SessionHead{
		LastCompletedRunID: "run-prev",
		CurrentGoal:        "обновить шаблон астры",
		LastResultSummary:  "шаблон обновлён и выключен",
		RecentArtifactRefs: []string{"artifact://run/run-prev/report"},
		CurrentProject:     "projects/astra-template-update",
	}
	got := BuildRecentWorkPrompt("продолжай", head)
	if !strings.Contains(got, "recent completed work") {
		t.Fatalf("expected recent work guidance, got %q", got)
	}
	if !strings.Contains(got, "run-prev") || !strings.Contains(got, "artifact://run/run-prev/report") {
		t.Fatalf("expected recent run details, got %q", got)
	}
}

func TestBuildRecentWorkPromptForProjectCapture(t *testing.T) {
	head := SessionHead{
		LastCompletedRunID: "run-prev",
		CurrentGoal:        "обновить шаблон астры",
		LastResultSummary:  "шаблон обновлён и выключен",
	}
	got := BuildRecentWorkPrompt("запиши это как проект", head)
	if !strings.Contains(got, "formalize or save recent work") {
		t.Fatalf("expected project capture guidance, got %q", got)
	}
	if !strings.Contains(got, "Do not ask the user to restate details") {
		t.Fatalf("expected no-reask guidance, got %q", got)
	}
	if !strings.Contains(got, "Only ask for the target project path or name if that specific target is still missing") {
		t.Fatalf("expected narrow missing-target guidance, got %q", got)
	}
	if !strings.Contains(got, "Bind this request to the recent completed run") {
		t.Fatalf("expected explicit recent-run binding, got %q", got)
	}
	if !strings.Contains(got, "Use project_capture_recent before the final response") {
		t.Fatalf("expected explicit project capture tool guidance, got %q", got)
	}
}

func TestBuildRecentWorkPromptSkipsDetailedQuery(t *testing.T) {
	head := SessionHead{
		LastCompletedRunID: "run-prev",
		CurrentGoal:        "обновить шаблон астры",
		LastResultSummary:  "шаблон обновлён и выключен",
	}
	if got := BuildRecentWorkPrompt("сделай diff между docs/architecture.md и docs/decisions.md", head); got != "" {
		t.Fatalf("expected no recent work prompt for detailed query, got %q", got)
	}
}
