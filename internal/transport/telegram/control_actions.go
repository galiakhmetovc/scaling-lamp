package telegram

import (
	"context"

	runtimex "teamd/internal/runtime"
)

func (a *Adapter) executeRunControlAction(chatID int64, action runtimex.ControlAction) (runtimex.ControlActionResult, error) {
	if a.agentCore != nil {
		return a.agentCore.ExecuteControlAction(a.meshSessionID(chatID), runtimex.ControlActionRequest{
			Action: action,
			ChatID: chatID,
		})
	}
	return a.runtimeAPI.ExecuteControlAction(a.meshSessionID(chatID), chatID, a.runtimeDefaults, a.memoryPolicy, a.actionPolicy, action)
}

func (a *Adapter) sendRunControlAction(ctx context.Context, chatID int64, action runtimex.ControlAction) error {
	result, err := a.executeRunControlAction(chatID, action)
	if err != nil {
		return err
	}
	if result.Message != "" {
		if _, err := a.sendMessage(ctx, chatID, result.Message, nil); err != nil {
			return err
		}
	}
	if len(result.Pages) == 0 {
		return nil
	}
	for _, page := range result.Pages {
		if _, err := a.sendMessage(ctx, chatID, page, nil); err != nil {
			return err
		}
	}
	return nil
}
