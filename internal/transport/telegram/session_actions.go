package telegram

import (
	"strings"

	runtimex "teamd/internal/runtime"
)

func (a *Adapter) executeSessionAction(chatID int64, action runtimex.SessionAction, sessionName string) (string, error) {
	req := runtimex.SessionActionRequest{
		ChatID:      chatID,
		Action:      action,
		SessionName: sessionName,
	}
	var (
		result runtimex.SessionActionResult
		err    error
	)
	if a.agentCore != nil {
		result, err = a.agentCore.ExecuteSessionAction(req)
	} else {
		result, err = a.sessionActions.Execute(chatID, req)
	}
	if err != nil {
		return "", err
	}
	return a.formatSessionActionResult(chatID, result, sessionName), nil
}

func (a *Adapter) formatSessionActionResult(chatID int64, result runtimex.SessionActionResult, sessionName string) string {
	switch result.Action {
	case runtimex.SessionActionShow:
		return "session active: " + result.ActiveSession
	case runtimex.SessionActionCreate:
		return a.sessionSummary(chatID, "session created: "+normalizeSessionName(sessionName))
	case runtimex.SessionActionUse:
		return "session active: " + result.ActiveSession
	case runtimex.SessionActionList:
		lines := make([]string, 0, len(result.Sessions))
		for _, session := range result.Sessions {
			prefix := "  "
			if session == result.ActiveSession {
				prefix = "* "
			}
			lines = append(lines, prefix+session)
		}
		return strings.Join(lines, "\n")
	case runtimex.SessionActionStats:
		return a.sessionSummary(chatID, "session stats")
	case runtimex.SessionActionReset:
		return a.sessionSummary(chatID, "session reset")
	default:
		return "unknown action"
	}
}
