package telegram

import (
	"strings"

	runtimex "teamd/internal/runtime"
)

func (a *Adapter) handleSessionCommand(chatID int64, text string) (bool, string, error) {
	fields := strings.Fields(text)
	if len(fields) == 0 || fields[0] != "/session" {
		return false, "", nil
	}
	if len(fields) == 1 {
		reply, err := a.executeSessionAction(chatID, runtimex.SessionActionShow, "")
		return true, reply, err
	}

	switch fields[1] {
	case "new":
		if len(fields) < 3 {
			return true, "usage: /session new <name>", nil
		}
		name := normalizeSessionName(strings.Join(fields[2:], " "))
		reply, err := a.executeSessionAction(chatID, runtimex.SessionActionCreate, name)
		return true, reply, err
	case "use":
		if len(fields) < 3 {
			return true, "usage: /session use <name>", nil
		}
		name := normalizeSessionName(strings.Join(fields[2:], " "))
		reply, err := a.executeSessionAction(chatID, runtimex.SessionActionUse, name)
		return true, reply, err
	case "list":
		reply, err := a.executeSessionAction(chatID, runtimex.SessionActionList, "")
		return true, reply, err
	default:
		return true, "usage: /session [new|use|list] ...", nil
	}
}

func (a *Adapter) handleSessionIntent(chatID int64, text string) (bool, string, error) {
	normalized := normalizeSessionName(text)
	switch {
	case strings.HasPrefix(normalized, "создай сессию "):
		name := normalizeSessionName(strings.TrimPrefix(normalized, "создай сессию "))
		reply, err := a.executeSessionAction(chatID, runtimex.SessionActionCreate, name)
		return true, reply, err
	case strings.HasPrefix(normalized, "переключись на сессию "):
		name := normalizeSessionName(strings.TrimPrefix(normalized, "переключись на сессию "))
		reply, err := a.executeSessionAction(chatID, runtimex.SessionActionUse, name)
		return true, reply, err
	case strings.HasPrefix(normalized, "create session "):
		name := normalizeSessionName(strings.TrimPrefix(normalized, "create session "))
		reply, err := a.executeSessionAction(chatID, runtimex.SessionActionCreate, name)
		return true, reply, err
	case strings.HasPrefix(normalized, "switch to session "):
		name := normalizeSessionName(strings.TrimPrefix(normalized, "switch to session "))
		reply, err := a.executeSessionAction(chatID, runtimex.SessionActionUse, name)
		return true, reply, err
	default:
		return false, "", nil
	}
}
