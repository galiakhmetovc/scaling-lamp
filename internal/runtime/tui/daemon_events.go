package tui

import (
	"encoding/json"
	"strings"

	"teamd/internal/runtime"
	"teamd/internal/runtime/daemon"
)

func (m *model) handleDaemonEnvelope(envelope daemon.WebsocketEnvelope) {
	switch envelope.Type {
	case "ui_event":
		if envelope.Event == nil {
			return
		}
		event := *envelope.Event
		if state := m.sessions[event.SessionID]; state != nil {
			reloadSnapshot := false
			switch event.Kind {
			case runtime.UIEventStreamText:
				state.Streaming.WriteString(event.Text)
			case runtime.UIEventToolStarted, runtime.UIEventToolCompleted:
				state.ToolLog = append(state.ToolLog, toolLogEntry{Activity: event.Tool})
				if len(state.ToolLog) > 200 {
					state.ToolLog = state.ToolLog[len(state.ToolLog)-200:]
				}
				if event.Kind == runtime.UIEventToolCompleted && toolActivityNeedsApprovalReload(event.Tool) {
					reloadSnapshot = true
				}
			case runtime.UIEventStatusChanged:
				if uiEventStatusNeedsSnapshotReload(event.Status) {
					reloadSnapshot = true
				}
			case runtime.UIEventRunCompleted:
				state.Streaming.Reset()
				state.ApprovalInFlightID = ""
				reloadSnapshot = true
			}
			if reloadSnapshot {
				_ = m.reloadSessionSnapshot(event.SessionID)
			}
			m.renderChatViewport(state)
			m.renderToolsViewport(state)
		}
	case "draft_queued", "draft_recalled", "queue_draft_started", "queue_draft_completed", "queue_draft_failed", "shell_approval_updated", "shell_approval_failed":
		if sessionID, ok := envelopeSessionID(envelope.Payload); ok {
			_ = m.reloadSessionSnapshot(sessionID)
		}
		if envelope.Type == "shell_approval_failed" && envelope.Error != "" {
			m.errMessage = envelope.Error
			if sessionID, ok := envelopeSessionID(envelope.Payload); ok {
				if state := m.sessions[sessionID]; state != nil {
					state.LastError = envelope.Error
					m.renderChatViewport(state)
					m.renderToolsViewport(state)
				}
			}
		}
	case "settings_applied":
		settings, err := m.client.GetSettings(m.ctx)
		if err == nil {
			m.settingsSnapshot = settings
			m.resetFormDraft()
		}
	}
}

func (m *model) daemonEnvelopeApprovalReloadSession(envelope daemon.WebsocketEnvelope) (string, bool) {
	if envelope.Type != "ui_event" || envelope.Event == nil {
		return "", false
	}
	event := *envelope.Event
	switch event.Kind {
	case runtime.UIEventToolCompleted:
		if toolActivityNeedsApprovalReload(event.Tool) {
			return event.SessionID, true
		}
	case runtime.UIEventStatusChanged:
		if event.Status == "approval_pending" {
			return event.SessionID, true
		}
	}
	return "", false
}

func toolActivityNeedsApprovalReload(activity runtime.ToolActivity) bool {
	if strings.Contains(activity.ErrorText, "requires approval") {
		return true
	}
	return strings.Contains(activity.ResultText, `"approval_pending"`)
}

func uiEventStatusNeedsSnapshotReload(status string) bool {
	switch strings.TrimSpace(status) {
	case "approval_pending", "waiting_shell", "running", "resuming", "idle", "done", "failed":
		return true
	default:
		return false
	}
}

func envelopeSessionID(payload any) (string, bool) {
	body, err := json.Marshal(payload)
	if err != nil {
		return "", false
	}
	var decoded map[string]any
	if err := json.Unmarshal(body, &decoded); err != nil {
		return "", false
	}
	sessionID, ok := decoded["session_id"].(string)
	return sessionID, ok
}
