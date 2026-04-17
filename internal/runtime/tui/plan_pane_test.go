package tui

import (
	"regexp"
	"strings"
	"testing"

	"teamd/internal/runtime/projections"
)

var ansiPattern = regexp.MustCompile(`\x1b\[[0-9;]*m`)

func TestPlanTaskLineUsesColoredStatusTokens(t *testing.T) {
	head := projections.PlanHeadSnapshot{
		Ready:                 map[string]bool{"ready-task": true},
		WaitingOnDependencies: map[string]bool{"waiting-task": true},
	}

	cases := []struct {
		name     string
		task     projections.PlanTaskView
		wantText string
	}{
		{name: "todo", task: projections.PlanTaskView{ID: "todo-task", Description: "Todo task"}, wantText: "[todo] Todo task"},
		{name: "ready", task: projections.PlanTaskView{ID: "ready-task", Description: "Ready task"}, wantText: "[ready] Ready task"},
		{name: "waiting", task: projections.PlanTaskView{ID: "waiting-task", Description: "Waiting task"}, wantText: "[waiting] Waiting task"},
		{name: "doing", task: projections.PlanTaskView{ID: "doing-task", Description: "Doing task", Status: "in_progress"}, wantText: "[doing] Doing task"},
		{name: "done", task: projections.PlanTaskView{ID: "done-task", Description: "Done task", Status: "done"}, wantText: "[done] Done task"},
		{name: "blocked", task: projections.PlanTaskView{ID: "blocked-task", Description: "Blocked task", Status: "blocked"}, wantText: "[blocked] Blocked task"},
		{name: "cancelled", task: projections.PlanTaskView{ID: "cancelled-task", Description: "Cancelled task", Status: "cancelled"}, wantText: "[cancelled] Cancelled task"},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := planTaskLine(head, tc.task, 0)
			if plain := ansiPattern.ReplaceAllString(got, ""); plain != tc.wantText {
				t.Fatalf("plain line = %q, want %q", plain, tc.wantText)
			}
			if !strings.Contains(got, "\x1b[") {
				t.Fatalf("line = %q, want ANSI color escape", got)
			}
		})
	}
}
