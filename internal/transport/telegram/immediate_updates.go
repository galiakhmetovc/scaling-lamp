package telegram

import (
	"context"
	"strings"

	runtimex "teamd/internal/runtime"
)

func (a *Adapter) handleImmediateUpdate(ctx context.Context, update Update) (bool, error) {
	if update.ChatID == 0 {
		return true, nil
	}
	if update.CallbackQuery {
		return true, a.handleCallback(ctx, update)
	}
	if strings.TrimSpace(update.Text) == "" {
		return true, nil
	}
	text := strings.TrimSpace(update.Text)
	if handled, reply, err := a.handleMeshCommand(update.ChatID, text); handled {
		return true, a.replyImmediate(ctx, update.ChatID, reply, nil, err)
	}
	if handled, reply, err := a.handleRuntimeCommand(update.ChatID, text); handled {
		return true, a.replyImmediate(ctx, update.ChatID, reply, nil, err)
	}
	if handled, reply, err := a.handleSkillsCommand(update.ChatID, text); handled {
		return true, a.replyImmediate(ctx, update.ChatID, reply, nil, err)
	}
	if handled, err := a.handleBuiltInImmediateCommand(ctx, update.ChatID, text); handled {
		return true, err
	}
	if handled, reply, err := a.handleSessionCommand(update.ChatID, text); handled {
		return true, a.replyImmediate(ctx, update.ChatID, reply, sessionKeyboard(), err)
	}
	if handled, reply, err := a.handleSessionIntent(update.ChatID, text); handled {
		return true, a.replyImmediate(ctx, update.ChatID, reply, sessionKeyboard(), err)
	}
	if strings.HasPrefix(text, "/") {
		_, err := a.sendMessage(ctx, update.ChatID, "unknown command", nil)
		return true, err
	}
	return false, nil
}

func (a *Adapter) handleBuiltInImmediateCommand(ctx context.Context, chatID int64, text string) (bool, error) {
	if text == "/btw" || strings.HasPrefix(text, "/btw ") {
		return true, a.handleBtwCommand(ctx, chatID, text)
	}
	switch text {
	case "/status":
		return true, a.sendRunControlAction(ctx, chatID, runtimex.ControlActionRunStatus)
	case "/cancel":
		return true, a.sendRunControlAction(ctx, chatID, runtimex.ControlActionRunCancel)
	case "/reset":
		if err := a.store.Reset(chatID); err != nil {
			return true, err
		}
		_, err := a.sendMessage(ctx, chatID, "session reset", nil)
		return true, err
	default:
		return false, nil
	}
}

func (a *Adapter) replyImmediate(ctx context.Context, chatID int64, text string, keyboard map[string]any, err error) error {
	if err != nil {
		return err
	}
	_, err = a.sendMessage(ctx, chatID, text, keyboard)
	return err
}
