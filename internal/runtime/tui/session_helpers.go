package tui

import (
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
)

func (m *model) currentPlanHead() (projections.PlanHeadSnapshot, bool) {
	state := m.currentSessionState()
	if state == nil {
		return projections.PlanHeadSnapshot{}, false
	}
	return state.Snapshot.Plan, state.Snapshot.Plan.Plan.ID != ""
}

func (m *model) currentApprovals() []shell.PendingApprovalView {
	state := m.currentSessionState()
	if state == nil {
		return nil
	}
	approvals := append([]shell.PendingApprovalView(nil), state.Snapshot.PendingApprovals...)
	if len(approvals) == 0 {
		m.approvalCursor = 0
		state.ApprovalInFlightID = ""
		state.ApprovalMenu.ComposeMode = false
		state.ApprovalMenu.ActionIndex = 0
		return nil
	}
	if state.ApprovalInFlightID != "" {
		found := false
		for _, approval := range approvals {
			if approval.ApprovalID == state.ApprovalInFlightID {
				found = true
				break
			}
		}
		if !found {
			state.ApprovalInFlightID = ""
			state.ApprovalMenu.ComposeMode = false
			if state.ApprovalMenu.ActionIndex >= len(approvalMenuActions) {
				state.ApprovalMenu.ActionIndex = 0
			}
		}
	}
	if m.approvalCursor >= len(approvals) && len(approvals) > 0 {
		m.approvalCursor = len(approvals) - 1
	}
	return approvals
}

func (m *model) currentRunningCommands() []projections.ShellCommandView {
	state := m.currentSessionState()
	if state == nil {
		return nil
	}
	commands := append([]projections.ShellCommandView(nil), state.Snapshot.RunningCommands...)
	if m.commandCursor >= len(commands) && len(commands) > 0 {
		m.commandCursor = len(commands) - 1
	}
	return commands
}
