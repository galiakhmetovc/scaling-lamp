package runtime

import (
	"context"
	"fmt"
	"time"

	"teamd/internal/contracts"
	"teamd/internal/provider"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

type SmokeInput struct {
	SessionID            string
	Prompt               string
	PromptAssetSelection []string
}

func (a *Agent) Smoke(ctx context.Context, input SmokeInput) (provider.ClientResult, error) {
	if a == nil {
		return provider.ClientResult{}, fmt.Errorf("agent is nil")
	}
	if a.ProviderClient == nil {
		return provider.ClientResult{}, fmt.Errorf("agent provider client is nil")
	}
	if input.Prompt == "" {
		return provider.ClientResult{}, fmt.Errorf("smoke prompt is empty")
	}

	now := a.now()
	sessionID := input.SessionID
	if sessionID == "" {
		sessionID = "smoke:" + a.Config.ID
	}
	runID := a.newID("run-smoke")
	correlationID := runID

	if !a.sessionExists(sessionID) {
		if err := a.RecordEvent(ctx, eventing.Event{
			ID:               a.newID("evt-session-created"),
			Kind:             eventing.EventSessionCreated,
			OccurredAt:       now,
			AggregateID:      sessionID,
			AggregateType:    eventing.AggregateSession,
			AggregateVersion: 1,
			CorrelationID:    correlationID,
			Source:           "agent.smoke",
			ActorID:          a.Config.ID,
			ActorType:        "agent",
			TraceSummary:     "smoke session bootstrap",
			Payload: map[string]any{
				"session_id": sessionID,
			},
		}); err != nil {
			return provider.ClientResult{}, fmt.Errorf("record session bootstrap: %w", err)
		}
	}

	if err := a.RecordEvent(ctx, eventing.Event{
		ID:               a.newID("evt-run-started"),
		Kind:             eventing.EventRunStarted,
		OccurredAt:       now,
		AggregateID:      runID,
		AggregateType:    eventing.AggregateRun,
		AggregateVersion: 1,
		CorrelationID:    correlationID,
		Source:           "agent.smoke",
		ActorID:          a.Config.ID,
		ActorType:        "agent",
		TraceSummary:     "smoke provider request started",
		Payload: map[string]any{
			"session_id": sessionID,
			"prompt":     input.Prompt,
		},
	}); err != nil {
		return provider.ClientResult{}, fmt.Errorf("record run started: %w", err)
	}

	result, err := a.ProviderClient.Execute(ctx, a.Contracts, provider.ClientInput{
		PromptAssetSelection: input.PromptAssetSelection,
		Messages: []contracts.Message{
			{Role: "user", Content: input.Prompt},
		},
	})
	if err != nil {
		if recordErr := a.recordProviderRequestEvent(ctx, runID, sessionID, correlationID, "agent.smoke", result.RequestBody); recordErr != nil {
			return provider.ClientResult{}, fmt.Errorf("execute smoke request: %v; record provider request: %w", err, recordErr)
		}
		if recordErr := a.recordTransportAttemptEvents(ctx, runID, sessionID, correlationID, result.TransportAttempts); recordErr != nil {
			return provider.ClientResult{}, fmt.Errorf("execute smoke request: %v; record transport attempts: %w", err, recordErr)
		}
		recordErr := a.RecordEvent(ctx, eventing.Event{
			ID:               a.newID("evt-run-failed"),
			Kind:             eventing.EventRunFailed,
			OccurredAt:       a.now(),
			AggregateID:      runID,
			AggregateType:    eventing.AggregateRun,
			AggregateVersion: 2,
			CorrelationID:    correlationID,
			CausationID:      runID,
			Source:           "agent.smoke",
			ActorID:          a.Config.ID,
			ActorType:        "agent",
			TraceSummary:     "smoke provider request failed",
			Payload: map[string]any{
				"session_id": sessionID,
				"error":      err.Error(),
			},
		})
		if recordErr != nil {
			return provider.ClientResult{}, fmt.Errorf("execute smoke request: %v; record failure event: %w", err, recordErr)
		}
		return provider.ClientResult{}, fmt.Errorf("execute smoke request: %w", err)
	}

	if err := a.recordProviderRequestEvent(ctx, runID, sessionID, correlationID, "agent.smoke", result.RequestBody); err != nil {
		return provider.ClientResult{}, fmt.Errorf("record provider request: %w", err)
	}
	if err := a.recordTransportAttemptEvents(ctx, runID, sessionID, correlationID, result.TransportAttempts); err != nil {
		return provider.ClientResult{}, fmt.Errorf("record transport attempts: %w", err)
	}

	if err := a.RecordEvent(ctx, eventing.Event{
		ID:               a.newID("evt-run-completed"),
		Kind:             eventing.EventRunCompleted,
		OccurredAt:       a.now(),
		AggregateID:      runID,
		AggregateType:    eventing.AggregateRun,
		AggregateVersion: 2,
		CorrelationID:    correlationID,
		CausationID:      runID,
		Source:           "agent.smoke",
		ActorID:          a.Config.ID,
		ActorType:        "agent",
		TraceSummary:     "smoke provider request completed",
		Payload: map[string]any{
			"session_id":    sessionID,
			"provider_id":   result.Provider.ID,
			"model":         result.Provider.Model,
			"finish_reason": result.Provider.FinishReason,
			"input_tokens":  result.Provider.Usage.InputTokens,
			"output_tokens": result.Provider.Usage.OutputTokens,
			"total_tokens":  result.Provider.Usage.TotalTokens,
		},
	}); err != nil {
		return provider.ClientResult{}, fmt.Errorf("record run completed: %w", err)
	}

	return result, nil
}

func (a *Agent) now() time.Time {
	if a.Now != nil {
		return a.Now().UTC()
	}
	return time.Now().UTC()
}

func (a *Agent) newID(prefix string) string {
	if a.NewID != nil {
		return a.NewID(prefix)
	}
	return fmt.Sprintf("%s-%d", prefix, a.now().UnixNano())
}

func (a *Agent) sessionExists(sessionID string) bool {
	for _, projection := range a.Projections {
		sessionProjection, ok := projection.(*projections.SessionProjection)
		if !ok {
			continue
		}
		return sessionProjection.Snapshot().SessionID == sessionID
	}
	return false
}

func (a *Agent) recordTransportAttemptEvents(ctx context.Context, runID, sessionID, correlationID string, attempts []provider.AttemptTrace) error {
	for _, attempt := range attempts {
		payload := map[string]any{
			"session_id":         sessionID,
			"attempt":            attempt.Attempt,
			"attempt_started_at": attempt.StartedAt,
			"attempt_finished_at": attempt.FinishedAt,
			"duration_ms":        attempt.Duration.Milliseconds(),
			"status_code":        attempt.StatusCode,
			"error":              attempt.Error,
			"attempt_timeout_ms": attempt.AttemptTimeout.Milliseconds(),
			"operation_budget_ms": attempt.OperationBudget.Milliseconds(),
			"retry_decision":     attempt.RetryDecision,
			"retry_reason":       attempt.RetryReason,
			"computed_backoff_ms": attempt.ComputedBackoff.Milliseconds(),
			"final_attempt_count": attempt.FinalAttemptCount,
		}
		if err := a.RecordEvent(ctx, eventing.Event{
			ID:               a.newID("evt-transport-attempt"),
			Kind:             eventing.EventTransportAttemptCompleted,
			OccurredAt:       attempt.FinishedAt,
			AggregateID:      runID,
			AggregateType:    eventing.AggregateRun,
			AggregateVersion: 2,
			CorrelationID:    correlationID,
			CausationID:      runID,
			Source:           "agent.smoke",
			ActorID:          a.Config.ID,
			ActorType:        "agent",
			TraceSummary:     "transport attempt completed",
			Payload:          payload,
		}); err != nil {
			return err
		}
	}
	return nil
}
