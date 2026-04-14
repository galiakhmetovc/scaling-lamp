package telegram

import (
	"context"
	"strings"

	"teamd/internal/provider"
)

const btwNotePrefix = "Operator note (out-of-band): "

func (a *Adapter) handleBtwCommand(ctx context.Context, chatID int64, text string) error {
	note := strings.TrimSpace(strings.TrimPrefix(strings.TrimSpace(text), "/btw"))
	if note == "" {
		_, err := a.sendMessage(ctx, chatID, "usage: /btw <заметка>", nil)
		return err
	}
	run, ok := a.runs.Active(chatID)
	if !ok || run.Completed || run.Failed {
		_, err := a.sendMessage(ctx, chatID, "Нет активного выполнения для /btw", nil)
		return err
	}
	if err := a.store.Append(chatID, provider.Message{
		Role:    "user",
		Content: btwNotePrefix + note,
	}); err != nil {
		return err
	}
	_, err := a.sendMessage(ctx, chatID, "Заметка добавлена к активному run", nil)
	return err
}
