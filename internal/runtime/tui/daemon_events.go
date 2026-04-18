package tui

import (
	"encoding/json"

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
			switch event.Kind {
			case runtime.UIEventStreamText:
				state.Streaming.WriteString(event.Text)
			case runtime.UIEventToolStarted, runtime.UIEventToolCompleted:
				state.ToolLog = append(state.ToolLog, toolLogEntry{Activity: event.Tool})
				if len(state.ToolLog) > 200 {
					state.ToolLog = state.ToolLog[len(state.ToolLog)-200:]
				}
			case runtime.UIEventStatusChanged:
				state.Status = event.Status
				if event.Status == "approval_pending" {
					_ = m.reloadSessionSnapshot(event.SessionID)
				}
			case runtime.UIEventRunCompleted:
				state.Status = "done"
				state.Streaming.Reset()
			}
			m.renderChatViewport(state)
			m.renderToolsViewport(state)
			if event.Kind == runtime.UIEventRunCompleted {
				_ = m.reloadSessionSnapshot(event.SessionID)
				m.renderChatViewport(state)
				m.renderToolsViewport(state)
			}
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
