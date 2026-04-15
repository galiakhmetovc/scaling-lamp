package tui

import (
	"fmt"
	"slices"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"teamd/internal/runtime/projections"
)

func (m *model) updatePlan(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	head, ok := m.agent.CurrentPlanHead(m.activeSessionID)
	flat := flattenedPlanTasks(head)
	selected, _ := m.selectedPlanTask(head)
	switch msg.String() {
	case "pgup":
		m.planView.LineUp(max(1, m.planView.Height/2))
	case "pgdown":
		m.planView.LineDown(max(1, m.planView.Height/2))
	case "up", "k":
		if m.planMode == planEditorBrowse {
			if m.planCursor > 0 {
				m.planCursor--
			}
			return m, nil
		}
	case "down", "j":
		if m.planMode == planEditorBrowse {
			if m.planCursor < len(flat)-1 {
				m.planCursor++
			}
			return m, nil
		}
	case "c":
		m.planMode = planEditorCreatePlan
		m.planGoalInput.Focus()
		m.planGoalInput.SetValue("")
		return m, nil
	case "a":
		m.planMode = planEditorAddTask
		m.planDescInput.Focus()
		m.planDescInput.SetValue("")
		return m, nil
	case "e":
		if ok {
			m.planMode = planEditorEditTask
			m.planDescInput.Focus()
			m.planDescInput.SetValue(selected.Description)
		}
		return m, nil
	case "d":
		if ok {
			m.planMode = planEditorEditDeps
			m.planDepsInput.Focus()
			m.planDepsInput.SetValue(strings.Join(selected.DependsOn, ","))
		}
		return m, nil
	case "n":
		if ok {
			m.planMode = planEditorNote
			m.planNoteInput.Focus()
			m.planNoteInput.SetValue("")
		}
		return m, nil
	case "s":
		if ok {
			m.planMode = planEditorStatus
			m.planStatusIndex = indexOfPlanStatus(selected.Status)
		}
		return m, nil
	case "enter":
		if m.planMode == planEditorBrowse && ok {
			m.planMode = planEditorEditTask
			m.planDescInput.Focus()
			m.planDescInput.SetValue(selected.Description)
			return m, nil
		}
	case "esc":
		m.planMode = planEditorBrowse
		return m, nil
	case "ctrl+s":
		var err error
		switch m.planMode {
		case planEditorCreatePlan:
			err = m.agent.CreatePlan(m.ctx, m.activeSessionID, strings.TrimSpace(m.planGoalInput.Value()))
		case planEditorAddTask:
			err = m.agent.AddPlanTask(m.ctx, m.activeSessionID, strings.TrimSpace(m.planDescInput.Value()), "", nil)
		case planEditorEditTask:
			if ok {
				err = m.agent.EditPlanTask(m.ctx, m.activeSessionID, selected.ID, strings.TrimSpace(m.planDescInput.Value()), selected.DependsOn)
			}
		case planEditorEditDeps:
			if ok {
				err = m.agent.EditPlanTask(m.ctx, m.activeSessionID, selected.ID, selected.Description, parseCSV(m.planDepsInput.Value()))
			}
		case planEditorStatus:
			if ok {
				err = m.agent.SetPlanTaskStatus(m.ctx, m.activeSessionID, selected.ID, planStatuses()[m.planStatusIndex], "")
			}
		case planEditorNote:
			if ok {
				err = m.agent.AddPlanTaskNote(m.ctx, m.activeSessionID, selected.ID, strings.TrimSpace(m.planNoteInput.Value()))
			}
		}
		if err != nil {
			m.errMessage = err.Error()
			return m, nil
		}
		m.planMode = planEditorBrowse
		if state := m.currentSessionState(); state != nil {
			m.renderChatViewport(state)
		}
		m.statusMessage = "plan updated"
		return m, nil
	case "left", "h":
		if m.planMode == planEditorStatus && m.planStatusIndex > 0 {
			m.planStatusIndex--
			return m, nil
		}
	case "right", "l":
		if m.planMode == planEditorStatus && m.planStatusIndex < len(planStatuses())-1 {
			m.planStatusIndex++
			return m, nil
		}
	}
	switch m.planMode {
	case planEditorCreatePlan:
		var cmd tea.Cmd
		m.planGoalInput, cmd = m.planGoalInput.Update(msg)
		return m, cmd
	case planEditorAddTask, planEditorEditTask:
		var cmd tea.Cmd
		m.planDescInput, cmd = m.planDescInput.Update(msg)
		return m, cmd
	case planEditorEditDeps:
		var cmd tea.Cmd
		m.planDepsInput, cmd = m.planDepsInput.Update(msg)
		return m, cmd
	case planEditorNote:
		var cmd tea.Cmd
		m.planNoteInput, cmd = m.planNoteInput.Update(msg)
		return m, cmd
	}
	return m, nil
}

func (m *model) viewPlan() string {
	head, ok := m.agent.CurrentPlanHead(m.activeSessionID)
	if !ok || head.Plan.ID == "" {
		left := "No active plan\n\nPress c to create one."
		right := m.renderPlanEditor(projections.PlanHeadSnapshot{}, false, projections.PlanTaskView{})
		return lipgloss.JoinHorizontal(lipgloss.Top, lipgloss.NewStyle().Width(max(30, m.width/2)).Render(left), lipgloss.NewStyle().Width(max(26, m.width/3)).Render(right))
	}
	lines := []string{"Plan", "", "goal: " + head.Plan.Goal}
	m.mousePlanTop = len(lines)
	ordered := orderedPlanTasks(head.Tasks)
	selected, hasSelection := m.selectedPlanTask(head)
	flatIndex := 0
	for _, task := range ordered {
		if task.ParentTaskID != "" {
			continue
		}
		renderPlanTaskWithSelection(&lines, head, task, ordered, 0, &flatIndex, m.planCursor)
	}
	m.planView.SetContent(strings.Join(lines, "\n"))
	left := m.planView.View()
	right := m.renderPlanEditor(head, hasSelection, selected)
	return lipgloss.JoinHorizontal(lipgloss.Top, lipgloss.NewStyle().Width(max(30, m.width/2)).Render(left), lipgloss.NewStyle().Width(max(26, m.width/3)).Render(right))
}

func (m *model) handleMousePlan(msg tea.MouseMsg) bool {
	switch msg.Button {
	case tea.MouseButtonWheelUp:
		m.planView.LineUp(3)
		return true
	case tea.MouseButtonWheelDown:
		m.planView.LineDown(3)
		return true
	case tea.MouseButtonLeft:
		if msg.Action != tea.MouseActionRelease {
			return false
		}
		head, ok := m.agent.CurrentPlanHead(m.activeSessionID)
		if !ok {
			return false
		}
		row := msg.Y - 4
		flat := flattenedPlanTasks(head)
		if row < 0 || row >= len(flat) {
			return false
		}
		m.planCursor = row
		return true
	}
	return false
}

func orderedPlanTasks(tasks map[string]projections.PlanTaskView) []projections.PlanTaskView {
	out := make([]projections.PlanTaskView, 0, len(tasks))
	for _, task := range tasks {
		out = append(out, task)
	}
	slices.SortFunc(out, func(a, b projections.PlanTaskView) int {
		if a.Order == b.Order {
			return strings.Compare(a.ID, b.ID)
		}
		return a.Order - b.Order
	})
	return out
}

func flattenedPlanTasks(head projections.PlanHeadSnapshot) []projections.PlanTaskView {
	ordered := orderedPlanTasks(head.Tasks)
	out := make([]projections.PlanTaskView, 0, len(ordered))
	var walk func(parent string)
	walk = func(parent string) {
		for _, task := range ordered {
			if task.ParentTaskID != parent {
				continue
			}
			out = append(out, task)
			walk(task.ID)
		}
	}
	walk("")
	return out
}

func renderPlanTaskWithSelection(lines *[]string, head projections.PlanHeadSnapshot, task projections.PlanTaskView, all []projections.PlanTaskView, depth int, flatIndex *int, selectedIndex int) {
	prefix := "  "
	if *flatIndex == selectedIndex {
		prefix = "> "
	}
	rendered := fmt.Sprintf("%s%s", prefix, planTaskLine(head, task, depth))
	*lines = append(*lines, rendered)
	*flatIndex++
	for _, child := range all {
		if child.ParentTaskID == task.ID {
			renderPlanTaskWithSelection(lines, head, child, all, depth+1, flatIndex, selectedIndex)
		}
	}
}

func planTaskLine(head projections.PlanHeadSnapshot, task projections.PlanTaskView, depth int) string {
	prefix := strings.Repeat("  ", depth)
	status := "[todo]"
	switch task.Status {
	case "done":
		status = "[done]"
	case "in_progress":
		status = "[doing]"
	case "blocked":
		status = "[blocked]"
	case "cancelled":
		status = "[cancelled]"
	default:
		if head.WaitingOnDependencies[task.ID] {
			status = "[waiting]"
		} else if head.Ready[task.ID] {
			status = "[ready]"
		}
	}
	return fmt.Sprintf("%s%s %s", prefix, status, task.Description)
}

func (m *model) selectedPlanTask(head projections.PlanHeadSnapshot) (projections.PlanTaskView, bool) {
	flat := flattenedPlanTasks(head)
	if len(flat) == 0 || m.planCursor < 0 || m.planCursor >= len(flat) {
		return projections.PlanTaskView{}, false
	}
	return flat[m.planCursor], true
}

func (m *model) renderPlanEditor(head projections.PlanHeadSnapshot, hasSelection bool, selected projections.PlanTaskView) string {
	lines := []string{
		"Plan Editor",
		"",
		"c=create plan  a=add task  e=edit task  d=deps  s=status  n=note  Esc=close  Ctrl+S=apply",
		"",
	}
	switch m.planMode {
	case planEditorCreatePlan:
		lines = append(lines, "Create Plan", "", "Goal:", m.planGoalInput.View())
	case planEditorAddTask:
		lines = append(lines, "Add Task", "", "Description:", m.planDescInput.View())
	case planEditorEditTask:
		lines = append(lines, "Edit Task", "", "Task ID: "+selected.ID, "Description:", m.planDescInput.View())
	case planEditorEditDeps:
		lines = append(lines, "Edit Dependencies", "", "Task ID: "+selected.ID, "Depends on (comma-separated ids):", m.planDepsInput.View())
	case planEditorStatus:
		status := "todo"
		if index := m.planStatusIndex; index >= 0 && index < len(planStatuses()) {
			status = planStatuses()[index]
		}
		lines = append(lines, "Set Status", "", "Task ID: "+selected.ID, "Use h/l then Ctrl+S", "Status: "+status)
	case planEditorNote:
		lines = append(lines, "Add Note", "", "Task ID: "+selected.ID, "Note:", m.planNoteInput.View())
	default:
		if hasSelection {
			computed := "none"
			switch {
			case selected.Status == "blocked":
				computed = "blocked"
			case head.WaitingOnDependencies[selected.ID]:
				computed = "waiting_on_dependencies"
			case head.Ready[selected.ID]:
				computed = "ready"
			}
			lines = append(lines,
				"Selected Task",
				"",
				"ID: "+selected.ID,
				"Description: "+selected.Description,
				"Status: "+selected.Status,
				"Computed: "+computed,
				"Depends on: "+strings.Join(selected.DependsOn, ", "),
			)
			if selected.BlockedReason != "" {
				lines = append(lines, "Blocked reason: "+selected.BlockedReason)
			}
			if notes := head.Notes[selected.ID]; len(notes) > 0 {
				lines = append(lines, fmt.Sprintf("Notes: %d", len(notes)), "Latest note: "+notes[len(notes)-1])
			}
		} else {
			lines = append(lines, "No task selected")
		}
	}
	return strings.Join(lines, "\n")
}

func planStatuses() []string { return []string{"todo", "in_progress", "done", "blocked", "cancelled"} }

func indexOfPlanStatus(status string) int {
	for idx, item := range planStatuses() {
		if item == status {
			return idx
		}
	}
	return 0
}

func parseCSV(input string) []string {
	if strings.TrimSpace(input) == "" {
		return nil
	}
	parts := strings.Split(input, ",")
	out := make([]string, 0, len(parts))
	for _, part := range parts {
		text := strings.TrimSpace(part)
		if text != "" {
			out = append(out, text)
		}
	}
	return out
}
