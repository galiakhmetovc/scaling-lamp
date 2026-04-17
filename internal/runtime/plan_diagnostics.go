package runtime

import (
	"slices"

	"teamd/internal/runtime/projections"
)

type planLintIssue struct {
	Code    string `json:"code"`
	TaskID  string `json:"task_id,omitempty"`
	Message string `json:"message"`
}

func buildPlanSnapshotPayload(active projections.ActivePlanSnapshot, head projections.PlanHeadSnapshot) map[string]any {
	ordered := orderedPlanTasks(active.Tasks)
	tasks := make([]map[string]any, 0, len(ordered))
	for _, task := range ordered {
		notes := make([]string, 0, len(task.Notes))
		for _, note := range task.Notes {
			notes = append(notes, note.Text)
		}
		tasks = append(tasks, map[string]any{
			"task_id":          task.ID,
			"description":      task.Description,
			"status":           task.Status,
			"order":            task.Order,
			"parent_task_id":   task.ParentTaskID,
			"depends_on":       slices.Clone(task.DependsOn),
			"blocked_reason":   task.BlockedReason,
			"notes":            notes,
			"ready":            head.Ready[task.ID],
			"waiting_on_deps":  head.WaitingOnDependencies[task.ID],
			"blocked_computed": head.Blocked[task.ID],
		})
	}
	return map[string]any{
		"status": "ok",
		"tool":   "plan_snapshot",
		"plan": map[string]any{
			"plan_id": active.Plan.ID,
			"goal":    active.Plan.Goal,
			"status":  active.Plan.Status,
			"tasks":   tasks,
		},
	}
}

func buildPlanLintPayload(active projections.ActivePlanSnapshot, head projections.PlanHeadSnapshot) map[string]any {
	issues := lintActivePlan(active, head)
	out := make([]map[string]any, 0, len(issues))
	for _, issue := range issues {
		out = append(out, map[string]any{
			"code":    issue.Code,
			"task_id": issue.TaskID,
			"message": issue.Message,
		})
	}
	return map[string]any{
		"status":        "ok",
		"tool":          "plan_lint",
		"issue_count":   len(out),
		"has_issues":    len(out) > 0,
		"issues":        out,
		"healthy":       len(out) == 0,
		"active_plan_id": active.Plan.ID,
	}
}

func lintActivePlan(active projections.ActivePlanSnapshot, head projections.PlanHeadSnapshot) []planLintIssue {
	issues := []planLintIssue{}
	if active.Plan.ID == "" {
		return append(issues, planLintIssue{Code: "no_active_plan", Message: "no active plan exists"})
	}
	inProgress := []projections.PlanTaskView{}
	for _, task := range orderedPlanTasks(active.Tasks) {
		switch task.Status {
		case "in_progress":
			inProgress = append(inProgress, task)
		case "blocked":
			if task.BlockedReason == "" {
				issues = append(issues, planLintIssue{Code: "blocked_without_reason", TaskID: task.ID, Message: "blocked task is missing blocked_reason"})
			}
		}
		for _, dep := range task.DependsOn {
			if dep == task.ID {
				issues = append(issues, planLintIssue{Code: "self_dependency", TaskID: task.ID, Message: "task depends on itself"})
				continue
			}
			if _, ok := active.Tasks[dep]; !ok {
				issues = append(issues, planLintIssue{Code: "dangling_dependency", TaskID: task.ID, Message: "task depends on missing task " + dep})
			}
		}
		if task.Status == "todo" && !head.Ready[task.ID] && !head.WaitingOnDependencies[task.ID] && head.Blocked[task.ID] == "" {
			issues = append(issues, planLintIssue{Code: "todo_without_computed_state", TaskID: task.ID, Message: "todo task is neither ready nor waiting on dependencies"})
		}
	}
	if len(inProgress) > 1 {
		for _, task := range inProgress {
			issues = append(issues, planLintIssue{Code: "multiple_in_progress", TaskID: task.ID, Message: "multiple tasks are marked in_progress"})
		}
	}
	return issues
}

func orderedPlanTasks(tasks map[string]projections.PlanTaskView) []projections.PlanTaskView {
	ordered := make([]projections.PlanTaskView, 0, len(tasks))
	for _, task := range tasks {
		ordered = append(ordered, task)
	}
	slices.SortFunc(ordered, func(a, b projections.PlanTaskView) int {
		if a.Order != b.Order {
			return a.Order - b.Order
		}
		if a.ID < b.ID {
			return -1
		}
		if a.ID > b.ID {
			return 1
		}
		return 0
	})
	return ordered
}
