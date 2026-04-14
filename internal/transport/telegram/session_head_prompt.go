package telegram

import (
	"strings"

	runtimex "teamd/internal/runtime"
)

func (a *Adapter) sessionHeadPrompt(chatID int64) (string, error) {
	if a == nil || a.runStore == nil {
		return "", nil
	}
	head, ok, err := a.runStore.SessionHead(chatID, a.meshSessionID(chatID))
	if err != nil || !ok {
		return "", err
	}
	lines := []string{"Session head."}
	if text := strings.TrimSpace(head.CurrentGoal); text != "" {
		lines = append(lines, "Current goal: "+text)
	}
	if text := strings.TrimSpace(head.LastResultSummary); text != "" {
		lines = append(lines, "Last result: "+text)
	}
	if text := strings.TrimSpace(head.CurrentPlanTitle); text != "" {
		lines = append(lines, "Current plan: "+text)
	}
	if text := strings.TrimSpace(head.CurrentPlanID); text != "" {
		lines = append(lines, "Plan id: "+text)
	}
	if len(head.CurrentPlanItems) > 0 {
		lines = append(lines, "Plan items:")
		for _, item := range head.CurrentPlanItems {
			lines = append(lines, "- "+item)
		}
	}
	if text := strings.TrimSpace(head.LastCompletedRunID); text != "" {
		lines = append(lines, "Last completed run: "+text)
	}
	if len(head.ResolvedEntities) > 0 {
		lines = append(lines, "Resolved entities: "+strings.Join(head.ResolvedEntities, ", "))
	}
	if len(head.RecentArtifactRefs) > 0 {
		lines = append(lines, "Recent artifacts: "+strings.Join(head.RecentArtifactRefs, ", "))
	}
	if len(head.OpenLoops) > 0 {
		lines = append(lines, "Open loops: "+strings.Join(head.OpenLoops, ", "))
	}
	if text := strings.TrimSpace(head.CurrentProject); text != "" {
		lines = append(lines, "Current project: "+text)
	}
	return strings.Join(lines, "\n"), nil
}

var _ runtimex.SessionStateStore = (*runtimex.SQLiteStore)(nil)
