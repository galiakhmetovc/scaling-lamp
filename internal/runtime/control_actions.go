package runtime

import (
	"fmt"
	"strings"

	"teamd/internal/provider"
)

type ControlAction string

const (
	ControlActionRunStatus ControlAction = "run.status"
	ControlActionRunCancel ControlAction = "run.cancel"
)

type ControlActionRequest struct {
	Action ControlAction
	ChatID int64
}

type ControlActionResult struct {
	Action  ControlAction
	Message string
	Pages   []string
	Control ControlState
}

func (a *API) ExecuteControlAction(sessionID string, chatID int64, runtimeConfig provider.RequestConfig, memoryPolicy MemoryPolicy, actionPolicy ActionPolicy, action ControlAction) (ControlActionResult, error) {
	switch action {
	case ControlActionRunStatus:
		control, err := a.ControlState(sessionID, chatID, runtimeConfig, memoryPolicy, actionPolicy)
		if err != nil {
			return ControlActionResult{}, err
		}
		if !controlHasActiveExecution(control) {
			return ControlActionResult{
				Action:  action,
				Message: "Нет активного выполнения",
				Control: control,
			}, nil
		}
		return ControlActionResult{
			Action:  action,
			Pages:   FormatControlReport(control),
			Control: control,
		}, nil
	case ControlActionRunCancel:
		requested := a.CancelRun(chatID)
		control, err := a.ControlState(sessionID, chatID, runtimeConfig, memoryPolicy, actionPolicy)
		if err != nil {
			return ControlActionResult{}, err
		}
		if !requested {
			return ControlActionResult{
				Action:  action,
				Message: "Нет активного выполнения",
				Control: control,
			}, nil
		}
		return ControlActionResult{
			Action:  action,
			Message: "Отмена запрошена",
			Pages:   FormatControlReport(control),
			Control: control,
		}, nil
	default:
		return ControlActionResult{}, NewControlError(ErrValidation, "unsupported control action")
	}
}

func FormatControlReport(control ControlState) []string {
	lines := []string{"📊 Control status"}
	if strings.TrimSpace(control.Session.SessionID) != "" {
		lines = append(lines, "Session: "+control.Session.SessionID)
	}
	if run := control.Session.LatestRun; run != nil {
		line := fmt.Sprintf("Run: %s (%s)", run.RunID, run.Status)
		if run.CancelRequested {
			line += " [cancel requested]"
		}
		lines = append(lines, line)
		if strings.TrimSpace(run.Query) != "" {
			lines = append(lines, "Query: "+run.Query)
		}
		if run.PromptBudget.FinalPromptTokens > 0 {
			lines = append(lines, fmt.Sprintf("Prompt budget: %d%%", run.PromptBudget.PromptBudgetPercent))
			lines = append(lines, fmt.Sprintf("Context window: %d%%", run.PromptBudget.ContextWindowPercent))
			lines = append(lines, fmt.Sprintf("System overhead: %d", run.PromptBudget.SystemOverheadTokens))
			lines = append(lines, fmt.Sprintf("Final prompt tokens: %d", run.PromptBudget.FinalPromptTokens))
			if len(run.PromptBudget.Layers) > 0 {
				lines = append(lines, "Context layers:")
				for _, layer := range run.PromptBudget.Layers {
					lines = append(lines, fmt.Sprintf("- %s [%s]: %d", layer.Name, layer.Residency, layer.Tokens))
				}
			}
		}
	}
	if head := control.Session.Head; head != nil {
		lines = append(lines, "")
		lines = append(lines, "Recent context:")
		if strings.TrimSpace(head.CurrentGoal) != "" {
			lines = append(lines, "- goal: "+head.CurrentGoal)
		}
		if strings.TrimSpace(head.LastResultSummary) != "" {
			lines = append(lines, "- last result: "+head.LastResultSummary)
		}
		if strings.TrimSpace(head.LastCompletedRunID) != "" {
			lines = append(lines, "- last completed run: "+head.LastCompletedRunID)
		}
		if len(head.RecentArtifactRefs) > 0 {
			lines = append(lines, "- recent artifacts: "+strings.Join(head.RecentArtifactRefs, ", "))
		}
	}
	if len(control.Approvals) > 0 {
		lines = append(lines, "", "Approvals:")
		for _, item := range control.Approvals {
			line := fmt.Sprintf("- %s: %s", item.ID, item.Status)
			if item.WorkerID != "" {
				line += " tool=" + item.WorkerID
			}
			if item.TargetType != "" || item.TargetID != "" {
				line += fmt.Sprintf(" target=%s/%s", item.TargetType, item.TargetID)
			}
			lines = append(lines, line)
		}
	}
	if len(control.Workers) > 0 {
		lines = append(lines, "", "Workers:")
		for _, item := range control.Workers {
			line := fmt.Sprintf("- %s: %s", item.WorkerID, item.Status)
			if item.LastRunID != "" {
				line += " run=" + item.LastRunID
			}
			lines = append(lines, line)
		}
	}
	if len(control.Jobs) > 0 {
		lines = append(lines, "", "Jobs:")
		for _, item := range control.Jobs {
			line := fmt.Sprintf("- %s: %s", item.JobID, item.Status)
			if strings.TrimSpace(item.Command) != "" {
				line += " cmd=" + item.Command
			}
			lines = append(lines, line)
		}
	}
	return []string{strings.Join(lines, "\n")}
}

func controlHasActiveExecution(control ControlState) bool {
	if run := control.Session.LatestRun; run != nil {
		if run.Active || run.Status == StatusRunning {
			return true
		}
	}
	for _, item := range control.Approvals {
		if item.Status == "pending" {
			return true
		}
	}
	for _, item := range control.Workers {
		if item.Status == WorkerRunning || item.Status == WorkerWaitingApproval {
			return true
		}
	}
	for _, item := range control.Jobs {
		if item.Status == JobQueued || item.Status == JobRunning {
			return true
		}
	}
	return false
}
