package telegram

import (
	"fmt"
	"strings"

	"log/slog"

	"teamd/internal/mesh"
	runtimex "teamd/internal/runtime"
)

func (a *Adapter) handleMeshCommand(chatID int64, text string) (bool, string, error) {
	fields := strings.Fields(text)
	if len(fields) == 0 || fields[0] != "/mesh" {
		return false, "", nil
	}
	if len(fields) == 1 {
		return true, mesh.FormatPolicy(a.meshPolicy(chatID)), nil
	}

	switch fields[1] {
	case "help":
		return true, mesh.FormatPolicyHelp(), nil
	case "mode":
		if len(fields) < 3 {
			return true, "usage: /mesh mode <direct|fast|balanced|deep|composite>", nil
		}
		oldPolicy := a.meshPolicy(chatID)
		policy, err := mesh.PolicyForProfile(fields[2])
		if err != nil {
			return true, "", err
		}
		a.setMeshPolicy(chatID, policy)
		slog.Info("mesh policy profile changed",
			"chat_id", chatID,
			"user_id", chatID,
			"session_id", a.meshSessionID(chatID),
			"old_value", oldPolicy.Profile,
			"new_value", policy.Profile,
		)
		return true, mesh.FormatPolicy(policy), nil
	case "set":
		if len(fields) < 3 || !strings.Contains(fields[2], "=") {
			return true, "usage: /mesh set <field>=<value>", nil
		}
		parts := strings.SplitN(fields[2], "=", 2)
		current := a.meshPolicy(chatID)
		updated, change, err := current.ApplyOverride(parts[0], parts[1])
		if err != nil {
			return true, "", err
		}
		a.setMeshPolicy(chatID, updated)
		slog.Info("mesh policy override changed",
			"chat_id", chatID,
			"user_id", chatID,
			"session_id", a.meshSessionID(chatID),
			"field", change.Field,
			"old_value", change.OldValue,
			"new_value", change.NewValue,
		)
		return true, fmt.Sprintf("%s: %s -> %s\n\n%s", change.Field, change.OldValue, change.NewValue, mesh.FormatPolicy(updated)), nil
	default:
		return true, mesh.FormatPolicyHelp(), nil
	}
}
func (a *Adapter) applySessionCallback(chatID int64, data string) (string, error) {
	parts := strings.Split(data, ":")
	if len(parts) < 2 || parts[0] != "session" {
		return "unknown action", nil
	}
	switch parts[1] {
	case "list":
		return a.executeSessionAction(chatID, runtimex.SessionActionList, "")
	case "reset":
		return a.executeSessionAction(chatID, runtimex.SessionActionReset, "")
	case "stats":
		return a.executeSessionAction(chatID, runtimex.SessionActionStats, "")
	case "use":
		if len(parts) < 3 {
			return "usage: session:use:<name>", nil
		}
		name := normalizeSessionName(parts[2])
		return a.executeSessionAction(chatID, runtimex.SessionActionUse, name)
	default:
		return "unknown action", nil
	}
}
