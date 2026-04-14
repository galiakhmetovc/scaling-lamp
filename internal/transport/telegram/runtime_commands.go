package telegram

import (
	"fmt"
	"strconv"
	"strings"

	runtimex "teamd/internal/runtime"
)

func (a *Adapter) handleRuntimeCommand(chatID int64, text string) (bool, string, error) {
	fields := strings.Fields(text)
	if len(fields) == 0 {
		return false, "", nil
	}

	switch fields[0] {
	case "/runtime":
		return true, a.formatRuntimeConfig(chatID), nil
	case "/approvals":
		return true, a.formatPendingApprovals(chatID), nil
	case "/memory":
		if len(fields) == 2 && fields[1] == "policy" {
			return true, a.formatMemoryPolicy(chatID), nil
		}
		if len(fields) >= 4 && fields[1] == "policy" && fields[2] == "set" && strings.Contains(fields[3], "=") {
			if err := a.applyMemoryPolicyOverride(chatID, fields[3]); err != nil {
				return true, "", err
			}
			return true, a.formatMemoryPolicy(chatID), nil
		}
		if len(fields) == 3 && fields[1] == "policy" && fields[2] == "clear" {
			if err := a.clearSessionOverrides(chatID); err != nil {
				return true, "", err
			}
			return true, a.formatRuntimeConfig(chatID), nil
		}
		return true, "usage: /memory policy | /memory policy set <key=value> | /memory policy clear", nil
	case "/model":
		if len(fields) == 1 {
			return true, a.formatRuntimeConfig(chatID), nil
		}
		if len(fields) >= 3 && fields[1] == "set" {
			cfg := a.runtimeConfig(chatID)
			cfg.Model = fields[2]
			if err := a.setRuntimeConfig(chatID, cfg); err != nil {
				return true, "", err
			}
			return true, a.formatRuntimeConfig(chatID), nil
		}
		return true, "usage: /model set <name>", nil
	case "/reasoning":
		if len(fields) == 1 {
			return true, a.formatRuntimeConfig(chatID), nil
		}
		cfg := a.runtimeConfig(chatID)
		switch fields[1] {
		case "mode":
			if len(fields) < 3 {
				return true, "usage: /reasoning mode <enabled|disabled>", nil
			}
			cfg.ReasoningMode = fields[2]
			if err := a.setRuntimeConfig(chatID, cfg); err != nil {
				return true, "", err
			}
			return true, a.formatRuntimeConfig(chatID), nil
		case "clear":
			if len(fields) < 3 {
				return true, "usage: /reasoning clear <on|off>", nil
			}
			value := strings.ToLower(fields[2]) == "on" || strings.ToLower(fields[2]) == "true"
			cfg.ClearThinking = &value
			if err := a.setRuntimeConfig(chatID, cfg); err != nil {
				return true, "", err
			}
			return true, a.formatRuntimeConfig(chatID), nil
		default:
			return true, "usage: /reasoning <mode|clear> ...", nil
		}
	case "/params":
		if len(fields) < 3 || fields[1] != "set" || !strings.Contains(fields[2], "=") {
			return true, "usage: /params set <temperature|top_p|max_tokens>=<value>", nil
		}
		parts := strings.SplitN(fields[2], "=", 2)
		cfg := a.runtimeConfig(chatID)
		switch strings.ToLower(parts[0]) {
		case "temperature":
			v, err := strconv.ParseFloat(parts[1], 64)
			if err != nil {
				return true, "", fmt.Errorf("invalid temperature %q", parts[1])
			}
			cfg.Temperature = &v
		case "top_p":
			v, err := strconv.ParseFloat(parts[1], 64)
			if err != nil {
				return true, "", fmt.Errorf("invalid top_p %q", parts[1])
			}
			cfg.TopP = &v
		case "max_tokens":
			v, err := strconv.Atoi(parts[1])
			if err != nil {
				return true, "", fmt.Errorf("invalid max_tokens %q", parts[1])
			}
			cfg.MaxTokens = &v
		default:
			return true, "", fmt.Errorf("unsupported param %q", parts[0])
		}
		if err := a.setRuntimeConfig(chatID, cfg); err != nil {
			return true, "", err
		}
		return true, a.formatRuntimeConfig(chatID), nil
	case "/policy":
		if len(fields) == 4 && fields[1] == "approval_tools" && fields[2] == "set" {
			if err := a.saveSessionOverrides(chatID, func(overrides *runtimex.SessionOverrides) {
				overrides.ActionPolicy.ApprovalRequiredTools = parseCSVValues(fields[3])
			}); err != nil {
				return true, "", err
			}
			return true, a.formatActionPolicy(chatID), nil
		}
		if len(fields) == 3 && fields[1] == "approval_tools" && fields[2] == "clear" {
			if err := a.saveSessionOverrides(chatID, func(overrides *runtimex.SessionOverrides) {
				overrides.ActionPolicy.ApprovalRequiredTools = nil
			}); err != nil {
				return true, "", err
			}
			return true, a.formatActionPolicy(chatID), nil
		}
		return true, "usage: /policy approval_tools <set csv|clear>", nil
	default:
		return false, "", nil
	}
}

func parseCSVValues(raw string) []string {
	parts := strings.Split(raw, ",")
	out := make([]string, 0, len(parts))
	for _, part := range parts {
		part = strings.TrimSpace(part)
		if part != "" {
			out = append(out, part)
		}
	}
	if len(out) == 0 {
		return nil
	}
	return out
}
