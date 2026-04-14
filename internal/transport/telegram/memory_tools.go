package telegram

import (
	"fmt"
	"strings"

	"teamd/internal/memory"
	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
)

func (a *Adapter) executeMemorySearchTool(chatID int64, call provider.ToolCall) (string, error) {
	if a.memory == nil {
		return "memory search unavailable", nil
	}
	query, _ := call.Arguments["query"].(string)
	query = strings.TrimSpace(query)
	if query == "" {
		return "memory search requires query", nil
	}
	limit := 5
	switch v := call.Arguments["limit"].(type) {
	case float64:
		if int(v) > 0 {
			limit = int(v)
		}
	case int:
		if v > 0 {
			limit = v
		}
	}
	recent, err := a.recentWorkSnapshot(chatID, query)
	if err != nil {
		return "", err
	}
	items, err := a.memory.Search(memory.RecallQuery{
		ChatID:    chatID,
		SessionID: a.meshSessionID(chatID),
		Text:      query,
		Limit:     limit,
	})
	if err != nil {
		return "", err
	}
	if recent == "" && len(items) == 0 {
		return "no memory results", nil
	}
	lines := []string{}
	if recent != "" {
		lines = append(lines, recent, "")
	}
	lines = append(lines, fmt.Sprintf("memory search results for %q:", query))
	for _, item := range items {
		title := strings.TrimSpace(item.Title)
		if title == "" {
			title = item.Kind
		}
		body := strings.TrimSpace(item.Body)
		if len(body) > 180 {
			body = body[:180] + "..."
		}
		lines = append(lines, fmt.Sprintf("- doc_key=%s kind=%s title=%s score=%.2f", item.DocKey, item.Kind, title, item.Score))
		lines = append(lines, "  "+body)
	}
	return strings.Join(lines, "\n"), nil
}

func (a *Adapter) recentWorkSnapshot(chatID int64, query string) (string, error) {
	if runtimex.DetectRecentWorkIntent(query) != runtimex.RecentWorkIntentProjectSave {
		return "", nil
	}
	sessionID := a.meshSessionID(chatID)
	var (
		snapshot runtimex.RecentWorkSnapshot
		ok       bool
		err      error
	)
	switch {
	case a.agentCore != nil:
		snapshot, ok, err = a.agentCore.RecentWorkSnapshot(chatID, sessionID, query)
	case a.runtimeAPI != nil:
		snapshot, ok, err = a.runtimeAPI.RecentWorkSnapshot(chatID, sessionID, query)
	default:
		return "", nil
	}
	if err != nil || !ok {
		return "", err
	}
	return formatRecentWorkSnapshot(snapshot), nil
}

func formatRecentWorkSnapshot(snapshot runtimex.RecentWorkSnapshot) string {
	lines := []string{
		fmt.Sprintf("recent work snapshot for %q:", strings.TrimSpace(snapshot.Query)),
		fmt.Sprintf("- intent=%s", snapshot.Intent),
	}
	if text := strings.TrimSpace(snapshot.Head.LastCompletedRunID); text != "" {
		lines = append(lines, "- last_completed_run_id="+text)
	}
	if text := strings.TrimSpace(snapshot.Head.CurrentGoal); text != "" {
		lines = append(lines, "- current_goal="+text)
	}
	if text := strings.TrimSpace(snapshot.Head.LastResultSummary); text != "" {
		lines = append(lines, "- last_result="+text)
	}
	if text := strings.TrimSpace(snapshot.Head.CurrentProject); text != "" {
		lines = append(lines, "- current_project="+text)
	}
	if len(snapshot.Head.RecentArtifactRefs) > 0 {
		lines = append(lines, "- recent_artifacts="+strings.Join(snapshot.Head.RecentArtifactRefs, ","))
	}
	if snapshot.Replay != nil {
		lines = append(lines, fmt.Sprintf("- replay_run=%s", snapshot.Replay.Run.RunID))
		if text := strings.TrimSpace(snapshot.Replay.Run.FinalResponse); text != "" {
			lines = append(lines, "- replay_final_response="+text)
		}
		if len(snapshot.Replay.Steps) > 0 {
			lines = append(lines, fmt.Sprintf("- replay_steps=%d", len(snapshot.Replay.Steps)))
		}
	}
	if snapshot.Intent == runtimex.RecentWorkIntentProjectSave {
		lines = append(lines, "- bind this formalization request to the recent completed run")
		lines = append(lines, "- only ask for the target project path or name if that specific target is missing")
	}
	lines = append(lines, "- use this recent snapshot before broader memory recall for project formalization")
	return strings.Join(lines, "\n")
}

func (a *Adapter) executeMemoryReadTool(call provider.ToolCall) (string, error) {
	if a.memory == nil {
		return "memory read unavailable", nil
	}
	docKey, _ := call.Arguments["doc_key"].(string)
	docKey = strings.TrimSpace(docKey)
	if docKey == "" {
		return "memory read requires doc_key", nil
	}
	doc, ok, err := a.memory.Get(docKey)
	if err != nil {
		return "", err
	}
	if !ok {
		return "memory document not found", nil
	}
	lines := []string{
		fmt.Sprintf("doc_key: %s", doc.DocKey),
		fmt.Sprintf("scope: %s", doc.Scope),
		fmt.Sprintf("kind: %s", doc.Kind),
		fmt.Sprintf("title: %s", strings.TrimSpace(doc.Title)),
		fmt.Sprintf("source: %s", strings.TrimSpace(doc.Source)),
		"",
		strings.TrimSpace(doc.Body),
	}
	return strings.Join(lines, "\n"), nil
}
