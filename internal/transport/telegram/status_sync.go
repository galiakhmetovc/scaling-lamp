package telegram

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"strings"
	"time"
)

func (a *Adapter) refreshRunStatus(ctx context.Context, chatID int64, stop <-chan struct{}) {
	ticker := time.NewTicker(a.refreshInterval)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-stop:
			return
		case <-ticker.C:
			if err := a.syncStatusCard(ctx, chatID); err != nil {
				slog.Debug("telegram status card sync failed", "chat_id", chatID, "err", err)
			}
		}
	}
}

func (a *Adapter) syncStatusCard(ctx context.Context, chatID int64) error {
	run, ok := a.runs.Active(chatID)
	if !ok || run.StatusMessageID == 0 {
		return nil
	}
	now := time.Now().UTC()
	if !run.Completed && !run.Failed && !run.CancelRequested {
		if !run.StatusRetryAfterUntil.IsZero() && now.Before(run.StatusRetryAfterUntil) {
			return nil
		}
		if !run.LastStatusSyncAt.IsZero() && now.Sub(run.LastStatusSyncAt) < minStatusSyncInterval {
			return nil
		}
	}
	err := a.editMessage(ctx, chatID, run.StatusMessageID, formatStatusCard(run), runKeyboard(run))
	if err == nil {
		a.runs.Update(chatID, func(run *RunState) {
			run.LastStatusSyncAt = now
			run.StatusRetryAfterUntil = time.Time{}
		})
		return nil
	}
	var tgErr *telegramAPIError
	if errors.As(err, &tgErr) {
		if tgErr.StatusCode == http.StatusTooManyRequests {
			retryAfter := tgErr.RetryAfter
			if retryAfter <= 0 {
				retryAfter = 60 * time.Second
			}
			a.runs.Update(chatID, func(run *RunState) {
				run.StatusRetryAfterUntil = now.Add(retryAfter)
			})
			slog.Warn("telegram_status_rate_limited", "chat_id", chatID, "retry_after", retryAfter.String())
			return nil
		}
		if strings.Contains(strings.ToLower(tgErr.Description), "message is not modified") {
			a.runs.Update(chatID, func(run *RunState) {
				run.LastStatusSyncAt = now
			})
			return nil
		}
	}
	return err
}

type telegramAPIError struct {
	StatusCode  int
	Body        string
	Description string
	RetryAfter  time.Duration
}

func (e *telegramAPIError) Error() string {
	return fmt.Sprintf("telegram api error: status=%d body=%s", e.StatusCode, strings.TrimSpace(e.Body))
}

type telegramErrorPayload struct {
	OK          bool   `json:"ok"`
	ErrorCode   int    `json:"error_code"`
	Description string `json:"description"`
	Parameters  struct {
		RetryAfter int `json:"retry_after"`
	} `json:"parameters"`
}

func parseTelegramAPIError(statusCode int, body []byte) error {
	apiErr := &telegramAPIError{
		StatusCode: statusCode,
		Body:       string(body),
	}
	var payload telegramErrorPayload
	if err := json.Unmarshal(body, &payload); err == nil {
		apiErr.Description = payload.Description
		if payload.Parameters.RetryAfter > 0 {
			apiErr.RetryAfter = time.Duration(payload.Parameters.RetryAfter) * time.Second
		}
	}
	return apiErr
}
