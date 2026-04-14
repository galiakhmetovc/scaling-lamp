package telegram

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
)

func (a *Adapter) sendMessage(ctx context.Context, chatID int64, text string, keyboard any) (int64, error) {
	return a.sendMessageWithMode(ctx, chatID, text, keyboard, "")
}

func (a *Adapter) SyncCommands(ctx context.Context) error {
	if strings.TrimSpace(a.token) == "" {
		return nil
	}
	if err := a.postTelegramForm(ctx, "deleteMyCommands", url.Values{}); err != nil {
		return err
	}
	commands := []telegramBotCommand{
		{Command: "runtime", Description: "Show current runtime config"},
		{Command: "approvals", Description: "Show pending approvals"},
		{Command: "model", Description: "Change model for this session"},
		{Command: "reasoning", Description: "Change reasoning settings"},
		{Command: "params", Description: "Change z.ai generation params"},
		{Command: "skills", Description: "Inspect and manage session skills"},
		{Command: "btw", Description: "Add an out-of-band note to the active run"},
		{Command: "status", Description: "Show current run status"},
		{Command: "reset", Description: "Reset current session"},
		{Command: "session", Description: "Manage chat sessions"},
		{Command: "mesh", Description: "Show or change mesh mode"},
	}
	body, err := json.Marshal(commands)
	if err != nil {
		return err
	}
	form := url.Values{}
	form.Set("commands", string(body))
	return a.postTelegramForm(ctx, "setMyCommands", form)
}

func (a *Adapter) sendMessageWithMode(ctx context.Context, chatID int64, text string, keyboard any, parseMode string) (int64, error) {
	form := url.Values{}
	form.Set("chat_id", strconv.FormatInt(chatID, 10))
	form.Set("text", text)
	if strings.TrimSpace(parseMode) != "" {
		form.Set("parse_mode", parseMode)
	}
	if keyboard != nil {
		body, err := json.Marshal(keyboard)
		if err != nil {
			return 0, err
		}
		form.Set("reply_markup", string(body))
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, a.methodURL("sendMessage"), strings.NewReader(form.Encode()))
	if err != nil {
		return 0, err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")

	httpResp, err := a.httpClient.Do(req)
	if err != nil {
		return 0, err
	}
	defer httpResp.Body.Close()

	if httpResp.StatusCode < 200 || httpResp.StatusCode >= 300 {
		body, _ := io.ReadAll(httpResp.Body)
		return 0, parseTelegramAPIError(httpResp.StatusCode, body)
	}
	var payload telegramMutationResponse
	if err := json.NewDecoder(httpResp.Body).Decode(&payload); err != nil {
		return 0, err
	}
	return payload.Result.MessageID, nil
}

func (a *Adapter) postTelegramForm(ctx context.Context, method string, form url.Values) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, a.methodURL(method), strings.NewReader(form.Encode()))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	resp, err := a.httpClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		body, _ := io.ReadAll(resp.Body)
		return parseTelegramAPIError(resp.StatusCode, body)
	}
	return nil
}

func (a *Adapter) editMessage(ctx context.Context, chatID, messageID int64, text string, keyboard any) error {
	form := url.Values{}
	form.Set("chat_id", strconv.FormatInt(chatID, 10))
	form.Set("message_id", strconv.FormatInt(messageID, 10))
	form.Set("text", text)
	if keyboard != nil {
		body, err := json.Marshal(keyboard)
		if err != nil {
			return err
		}
		form.Set("reply_markup", string(body))
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, a.methodURL("editMessageText"), strings.NewReader(form.Encode()))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	resp, err := a.httpClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		body, _ := io.ReadAll(resp.Body)
		return parseTelegramAPIError(resp.StatusCode, body)
	}
	return nil
}

func (a *Adapter) deleteMessage(ctx context.Context, chatID, messageID int64) error {
	form := url.Values{}
	form.Set("chat_id", strconv.FormatInt(chatID, 10))
	form.Set("message_id", strconv.FormatInt(messageID, 10))
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, a.methodURL("deleteMessage"), strings.NewReader(form.Encode()))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	resp, err := a.httpClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		body, _ := io.ReadAll(resp.Body)
		return fmt.Errorf("telegram api error: status=%d body=%s", resp.StatusCode, strings.TrimSpace(string(body)))
	}
	return nil
}

func (a *Adapter) handleCallback(ctx context.Context, update Update) error {
	if strings.HasPrefix(update.CallbackData, "approval:") {
		return a.handleApprovalCallback(ctx, update)
	}
	if strings.HasPrefix(update.CallbackData, "timeout:") {
		return a.handleTimeoutCallback(ctx, update)
	}
	if strings.HasPrefix(update.CallbackData, "busy:") {
		return a.handleBusyCallback(ctx, update)
	}
	if strings.HasPrefix(update.CallbackData, "run:") {
		if err := a.answerCallback(ctx, update.CallbackID); err != nil {
			return err
		}
		switch update.CallbackData {
		case "run:cancel":
			if err := a.sendRunControlAction(ctx, update.ChatID, runtimex.ControlActionRunCancel); err != nil {
				return err
			}
			return a.syncStatusCard(ctx, update.ChatID)
		case "run:delete":
			run, ok := a.runs.Active(update.ChatID)
			if !ok || run.StatusMessageID == 0 {
				return nil
			}
			if err := a.deleteMessage(ctx, update.ChatID, run.StatusMessageID); err != nil {
				return err
			}
			a.runs.Finish(update.ChatID)
			return nil
		case "run:status":
			return a.sendRunControlAction(ctx, update.ChatID, runtimex.ControlActionRunStatus)
		default:
			return nil
		}
	}
	reply, err := a.applySessionCallback(update.ChatID, update.CallbackData)
	if err != nil {
		return err
	}
	if err := a.answerCallback(ctx, update.CallbackID); err != nil {
		return err
	}
	_, err = a.sendMessage(ctx, update.ChatID, reply, sessionKeyboard())
	return err
}

func (a *Adapter) handleTimeoutCallback(ctx context.Context, update Update) error {
	if err := a.answerCallback(ctx, update.CallbackID); err != nil {
		return err
	}
	parts := strings.Split(update.CallbackData, ":")
	if len(parts) != 3 {
		return fmt.Errorf("unsupported timeout callback: %s", update.CallbackData)
	}
	var action runtimex.TimeoutDecisionAction
	switch parts[1] {
	case "continue":
		action = runtimex.TimeoutDecisionActionContinue
	case "retry":
		action = runtimex.TimeoutDecisionActionRetry
	case "cancel":
		action = runtimex.TimeoutDecisionActionCancel
	case "fail":
		action = runtimex.TimeoutDecisionActionFail
	default:
		return fmt.Errorf("unsupported timeout callback action: %s", parts[1])
	}
	if a.runtimeAPI == nil {
		_, err := a.sendMessage(ctx, update.ChatID, "timeout decision service is not configured", nil)
		return err
	}
	record, _, err := a.runtimeAPI.ResolveTimeoutDecision(parts[2], action, "operator timeout decision")
	if err != nil {
		return err
	}
	a.runs.Update(update.ChatID, func(run *RunState) {
		run.WaitingOn = ""
		run.LastProgressAt = time.Now().UTC()
		switch action {
		case runtimex.TimeoutDecisionActionContinue:
			run.Stage = "Продолжаю после timeout"
		case runtimex.TimeoutDecisionActionRetry:
			run.Stage = "Повторяю раунд после timeout"
		case runtimex.TimeoutDecisionActionCancel:
			run.Stage = "Отменяю выполнение"
		case runtimex.TimeoutDecisionActionFail:
			run.Stage = "Завершаю timeout ошибкой"
		}
	})
	_ = a.syncStatusCard(ctx, update.ChatID)
	_, err = a.sendMessage(ctx, update.ChatID, "timeout decision updated: "+record.RunID+" -> "+string(record.Status), nil)
	return err
}

func (a *Adapter) handleApprovalCallback(ctx context.Context, update Update) error {
	if a.approvals == nil {
		if err := a.answerCallback(ctx, update.CallbackID); err != nil {
			return err
		}
		_, err := a.sendMessage(ctx, update.ChatID, "approval service is not configured", nil)
		return err
	}
	parts := strings.Split(update.CallbackData, ":")
	if len(parts) != 3 {
		return fmt.Errorf("unsupported approval callback: %s", update.CallbackData)
	}
	var action approvals.Action
	switch parts[1] {
	case "approve":
		action = approvals.ActionApprove
	case "reject":
		action = approvals.ActionReject
	default:
		return fmt.Errorf("unsupported approval action: %s", parts[1])
	}
	var (
		record runtimex.ApprovalView
		err    error
	)
	if a.agentCore != nil {
		switch action {
		case approvals.ActionApprove:
			record, _, err = a.agentCore.Approve(parts[2])
		case approvals.ActionReject:
			record, _, err = a.agentCore.Reject(parts[2])
		}
	} else {
		raw, callbackErr := a.approvals.HandleCallback(approvals.Callback{
			ApprovalID: parts[2],
			Action:     action,
			UpdateID:   strconv.FormatInt(update.UpdateID, 10),
		})
		record = approvalViewFromRecord(raw)
		err = callbackErr
	}
	if err != nil {
		return err
	}
	if err := a.answerCallback(ctx, update.CallbackID); err != nil {
		return err
	}
	text := "approval updated: " + record.ID + " -> " + string(record.Status)
	if !a.approvals.HasWaiter(record.ID) {
		switch record.Status {
		case approvals.StatusApproved:
			if a.execution == nil {
				text += "\nresume failed: runtime execution service is not configured"
			} else if resumed, resumeErr := a.execution.ResumeApprovalContinuation(context.WithoutCancel(ctx), record.ID); resumeErr != nil {
				text += "\nresume failed: " + resumeErr.Error()
			} else if resumed {
				text += "\nresumed pending run"
			}
		case approvals.StatusRejected:
			_ = a.deleteApprovalContinuationAndFailRun(record.ID, "approval rejected")
		}
	}
	_, err = a.sendMessage(ctx, update.ChatID, text, nil)
	return err
}

func (a *Adapter) answerCallback(ctx context.Context, callbackID string) error {
	if strings.TrimSpace(callbackID) == "" {
		return nil
	}
	form := url.Values{}
	form.Set("callback_query_id", callbackID)
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, a.methodURL("answerCallbackQuery"), strings.NewReader(form.Encode()))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	resp, err := a.httpClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		body, _ := io.ReadAll(resp.Body)
		return fmt.Errorf("telegram api error: status=%d body=%s", resp.StatusCode, strings.TrimSpace(string(body)))
	}
	return nil
}

func (a *Adapter) methodURL(method string) string {
	return fmt.Sprintf("%s/bot%s/%s", a.baseURL, a.token, method)
}

func (a *Adapter) formatReply(resp provider.PromptResponse, chatID int64) string {
	return FormatTelegramReply(resp.Text)
}
