package runtime

import (
	"strings"
	"unicode/utf8"
)

type RecentWorkIntent string

const (
	RecentWorkIntentNone        RecentWorkIntent = ""
	RecentWorkIntentContinue    RecentWorkIntent = "continue"
	RecentWorkIntentProjectSave RecentWorkIntent = "project_save"
)

func BuildRecentWorkPrompt(query string, head SessionHead) string {
	intent := DetectRecentWorkIntent(query)
	if intent == RecentWorkIntentNone {
		return ""
	}
	if strings.TrimSpace(head.LastCompletedRunID) == "" && strings.TrimSpace(head.LastResultSummary) == "" {
		return ""
	}

	lines := []string{"Recent work follow-up."}
	switch intent {
	case RecentWorkIntentContinue:
		lines = append(lines, "Treat the user's short follow-up as a continuation of the recent completed work in this session.")
		lines = append(lines, "Use the recent completed work as the primary source of truth before broader memory recall.")
	case RecentWorkIntentProjectSave:
		lines = append(lines, "The user is asking to formalize or save recent work.")
		lines = append(lines, "Bind this request to the recent completed run in this session instead of asking the user what work they mean.")
		lines = append(lines, "Use the recent completed work as the source of truth before broader memory recall.")
		lines = append(lines, "Use project_capture_recent before the final response when you already have enough information to create or update project files.")
		lines = append(lines, "Do not ask the user to restate details already present in the recent run, replay, or artifacts unless a required fact is still missing.")
		lines = append(lines, "Only ask for the target project path or name if that specific target is still missing.")
	}
	appendRecentWorkFacts(&lines, head)
	return strings.Join(lines, "\n")
}

func appendRecentWorkFacts(lines *[]string, head SessionHead) {
	if text := strings.TrimSpace(head.LastCompletedRunID); text != "" {
		*lines = append(*lines, "Recent run: "+text)
	}
	if text := strings.TrimSpace(head.CurrentGoal); text != "" {
		*lines = append(*lines, "Recent goal: "+text)
	}
	if text := strings.TrimSpace(head.LastResultSummary); text != "" {
		*lines = append(*lines, "Recent result: "+text)
	}
	if len(head.ResolvedEntities) > 0 {
		*lines = append(*lines, "Resolved entities: "+strings.Join(head.ResolvedEntities, ", "))
	}
	if len(head.RecentArtifactRefs) > 0 {
		*lines = append(*lines, "Recent artifacts: "+strings.Join(head.RecentArtifactRefs, ", "))
	}
	if text := strings.TrimSpace(head.CurrentProject); text != "" {
		*lines = append(*lines, "Current project: "+text)
	}
	if len(head.OpenLoops) > 0 {
		*lines = append(*lines, "Open loops: "+strings.Join(head.OpenLoops, ", "))
	}
}

func DetectRecentWorkIntent(query string) RecentWorkIntent {
	normalized := normalizeRecentWorkQuery(query)
	if normalized == "" {
		return RecentWorkIntentNone
	}
	if isProjectCaptureQuery(normalized) {
		return RecentWorkIntentProjectSave
	}
	if isContinuationQuery(normalized) {
		return RecentWorkIntentContinue
	}
	return RecentWorkIntentNone
}

func normalizeRecentWorkQuery(query string) string {
	return strings.Join(strings.Fields(strings.ToLower(strings.TrimSpace(query))), " ")
}

func isContinuationQuery(query string) bool {
	if query == "" {
		return false
	}
	short := utf8.RuneCountInString(query) <= 48
	switch query {
	case "continue", "go on", "carry on", "next", "дальше", "продолжай", "продолжить", "что дальше":
		return true
	}
	if short {
		for _, prefix := range []string{
			"continue ",
			"go on ",
			"carry on ",
			"дальше ",
			"продолж",
			"что дальше",
		} {
			if strings.HasPrefix(query, prefix) {
				return true
			}
		}
	}
	return false
}

func isProjectCaptureQuery(query string) bool {
	if utf8.RuneCountInString(query) > 120 {
		return false
	}
	verbs := []string{
		"оформи", "запиши", "сохрани", "документ", "formalize", "document", "write down", "save",
	}
	targets := []string{
		"проект", "кейс", "процедур", "runbook", "project", "case", "procedure", "readme", "docs", "note",
	}
	hasVerb := false
	for _, verb := range verbs {
		if strings.Contains(query, verb) {
			hasVerb = true
			break
		}
	}
	if !hasVerb {
		return false
	}
	for _, target := range targets {
		if strings.Contains(query, target) {
			return true
		}
	}
	return false
}
