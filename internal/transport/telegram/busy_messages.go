package telegram

import (
	"context"
	"fmt"
	"strings"
	"time"

	runtimex "teamd/internal/runtime"
)

type busyMessageState struct {
	Pending string
	Queued  string
}

func busyKeyboard() map[string]any {
	return map[string]any{
		"inline_keyboard": [][]map[string]string{
			{
				{"text": "В очередь", "callback_data": "busy:queue"},
				{"text": "Прервать", "callback_data": "busy:interrupt"},
			},
			{
				{"text": "Отмена", "callback_data": "busy:dismiss"},
			},
		},
	}
}

func formatBusyMessage(text string) string {
	text = strings.TrimSpace(text)
	if len(text) > 160 {
		text = text[:160] + "..."
	}
	lines := []string{
		"Уже выполняю предыдущий запрос.",
		"Что сделать с новым сообщением?",
		"",
		"Новое сообщение:",
		text,
	}
	return strings.Join(lines, "\n")
}

func (a *Adapter) saveBusyPendingMessage(chatID int64, text string) {
	state := a.busyMessageState(chatID)
	state.Pending = strings.TrimSpace(text)
	a.busyMessages.Store(chatID, state)
}

func (a *Adapter) busyMessageState(chatID int64) busyMessageState {
	if value, ok := a.busyMessages.Load(chatID); ok {
		if state, ok := value.(busyMessageState); ok {
			return state
		}
	}
	return busyMessageState{}
}

func (a *Adapter) updateBusyMessageState(chatID int64, fn func(*busyMessageState)) busyMessageState {
	state := a.busyMessageState(chatID)
	fn(&state)
	if strings.TrimSpace(state.Pending) == "" && strings.TrimSpace(state.Queued) == "" {
		a.busyMessages.Delete(chatID)
		return busyMessageState{}
	}
	a.busyMessages.Store(chatID, state)
	return state
}

func (a *Adapter) handleBusyRun(ctx context.Context, chatID int64, text string) error {
	a.saveBusyPendingMessage(chatID, text)
	_, err := a.sendMessage(ctx, chatID, formatBusyMessage(text), busyKeyboard())
	return err
}

func (a *Adapter) handleBusyCallback(ctx context.Context, update Update) error {
	if err := a.answerCallback(ctx, update.CallbackID); err != nil {
		return err
	}
	var reply string
	switch update.CallbackData {
	case "busy:queue":
		state := a.updateBusyMessageState(update.ChatID, func(state *busyMessageState) {
			state.Queued = state.Pending
			state.Pending = ""
		})
		if strings.TrimSpace(state.Queued) == "" {
			reply = "Нет нового сообщения для очереди"
		} else {
			reply = "Сообщение поставлено в очередь и будет запущено после текущего run"
		}
	case "busy:interrupt":
		state := a.updateBusyMessageState(update.ChatID, func(state *busyMessageState) {
			state.Queued = state.Pending
			state.Pending = ""
		})
		if strings.TrimSpace(state.Queued) == "" {
			reply = "Нет нового сообщения для запуска"
		} else if a.requestRunCancel(update.ChatID) {
			reply = "Отмена текущего run запрошена; новое сообщение будет запущено следующим"
		} else {
			reply = "Текущий run уже неактивен; новое сообщение будет запущено следующим"
		}
	case "busy:dismiss":
		a.updateBusyMessageState(update.ChatID, func(state *busyMessageState) {
			state.Pending = ""
		})
		reply = "Новое сообщение отброшено"
	default:
		return fmt.Errorf("unsupported busy callback: %s", update.CallbackData)
	}
	_, err := a.sendMessage(ctx, update.ChatID, reply, nil)
	return err
}

func (a *Adapter) maybeStartQueuedMessage(chatID int64) {
	state := a.updateBusyMessageState(chatID, func(state *busyMessageState) {
		if strings.TrimSpace(state.Pending) != "" && strings.TrimSpace(state.Queued) == "" {
			state.Queued = state.Pending
			state.Pending = ""
		}
	})
	if strings.TrimSpace(state.Queued) == "" || a.execution == nil {
		return
	}
	go func(queued string) {
		for i := 0; i < 40; i++ {
			if a.runtimeAPI == nil {
				break
			}
			if _, ok := a.runtimeAPI.ActiveRun(chatID); !ok {
				break
			}
			time.Sleep(50 * time.Millisecond)
		}
		a.updateBusyMessageState(chatID, func(state *busyMessageState) {
			if strings.TrimSpace(state.Queued) == queued {
				state.Queued = ""
			}
		})
		ctx := context.Background()
		_, ok, err := a.execution.StartDetached(ctx, runtimex.StartRunRequest{
			RunID:          a.runs.AllocateID(),
			ChatID:         chatID,
			SessionID:      a.meshSessionID(chatID),
			Query:          queued,
			PolicySnapshot: runtimex.PolicySnapshotForSummary(a.runtimeSummary(chatID)),
			Interactive:    true,
		})
		if err != nil || !ok {
			_, _ = a.sendMessage(ctx, chatID, "Не удалось запустить сообщение из очереди", nil)
		}
	}(state.Queued)
}
