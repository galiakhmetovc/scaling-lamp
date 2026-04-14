package telegram

import (
	"fmt"
	"time"

	runtimex "teamd/internal/runtime"
)

func (a *Adapter) deleteApprovalContinuationAndFailRun(approvalID, reason string) error {
	if a.runStore == nil {
		return nil
	}
	cont, ok, err := a.runStore.ApprovalContinuation(approvalID)
	if err != nil || !ok {
		return err
	}
	endedAt := time.Now().UTC()
	if saveErr := a.runStore.SaveRun(runtimex.RunRecord{
		RunID:         cont.RunID,
		ChatID:        cont.ChatID,
		SessionID:     cont.SessionID,
		Query:         cont.Query,
		Status:        runtimex.StatusFailed,
		StartedAt:     cont.RequestedAt,
		EndedAt:       &endedAt,
		FailureReason: reason,
	}); saveErr != nil {
		return saveErr
	}
	if err := a.runStore.DeleteApprovalContinuation(approvalID); err != nil {
		return fmt.Errorf("delete continuation: %w", err)
	}
	return nil
}
