package tui

import (
	"context"
	"strings"
	"time"
)

func (m *model) traceApprovalMenuState(state *sessionState, reason string) {
	if state == nil {
		return
	}
	visible := m.chatApprovalMenuVisible(state)
	traceState := approvalMenuTraceState{Visible: visible}
	approvals := state.Snapshot.PendingApprovals
	if visible && len(approvals) > 0 {
		idx := min(max(m.approvalCursor, 0), len(approvals)-1)
		traceState.ApprovalID = approvals[idx].ApprovalID
		traceState.ActionIndex = state.ApprovalMenu.ActionIndex
		traceState.ComposeMode = state.ApprovalMenu.ComposeMode
		traceState.ApprovalCount = len(approvals)
	}
	prev := state.ApprovalTrace
	state.ApprovalTrace = traceState

	switch {
	case visible && !prev.Visible:
		m.sendApprovalTrace(state.SessionID, "tui.approval_menu.shown", map[string]any{
			"approval_id":    traceState.ApprovalID,
			"approval_count": traceState.ApprovalCount,
			"action_index":   traceState.ActionIndex,
			"compose_mode":   traceState.ComposeMode,
			"reason":         reason,
		})
	case !visible && prev.Visible:
		m.sendApprovalTrace(state.SessionID, "tui.approval_menu.hidden", map[string]any{
			"approval_id": prev.ApprovalID,
			"reason":      reason,
		})
	case visible && (prev.ApprovalID != traceState.ApprovalID || prev.ActionIndex != traceState.ActionIndex || prev.ComposeMode != traceState.ComposeMode):
		m.sendApprovalTrace(state.SessionID, "tui.approval_menu.selection_changed", map[string]any{
			"approval_id":    traceState.ApprovalID,
			"approval_count": traceState.ApprovalCount,
			"action_index":   traceState.ActionIndex,
			"compose_mode":   traceState.ComposeMode,
			"reason":         reason,
		})
	}
}

func (m *model) traceApprovalMenuAction(state *sessionState, traceName string, fields map[string]any) {
	if state == nil || strings.TrimSpace(traceName) == "" {
		return
	}
	payload := cloneTraceFields(fields)
	if len(state.Snapshot.PendingApprovals) > 0 {
		idx := min(max(m.approvalCursor, 0), len(state.Snapshot.PendingApprovals)-1)
		payload["approval_id"] = state.Snapshot.PendingApprovals[idx].ApprovalID
		payload["approval_count"] = len(state.Snapshot.PendingApprovals)
	}
	payload["action_index"] = state.ApprovalMenu.ActionIndex
	payload["compose_mode"] = state.ApprovalMenu.ComposeMode
	m.sendApprovalTrace(state.SessionID, traceName, payload)
}

func (m *model) sendApprovalTrace(sessionID, traceName string, fields map[string]any) {
	if strings.TrimSpace(sessionID) == "" || strings.TrimSpace(traceName) == "" || m.client == nil {
		return
	}
	ctx, cancel := context.WithTimeout(context.Background(), 500*time.Millisecond)
	defer cancel()
	_ = m.client.DebugTrace(ctx, sessionID, traceName, fields)
}
