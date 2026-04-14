package runtime

import (
	"context"
	"fmt"

	"teamd/internal/contracts"
	"teamd/internal/provider"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

type ChatSession struct {
	SessionID string
	Messages  []contracts.Message
}

type ChatTurnInput struct {
	Prompt               string
	PromptAssetSelection []string
	StreamObserver       func(provider.StreamEvent)
}

func (a *Agent) NewChatSession() (*ChatSession, error) {
	if a == nil {
		return nil, fmt.Errorf("agent is nil")
	}
	return &ChatSession{
		SessionID: a.newID("session-chat"),
		Messages:  []contracts.Message{},
	}, nil
}

func (a *Agent) ResumeChatSession(ctx context.Context, sessionID string) (*ChatSession, error) {
	if a == nil {
		return nil, fmt.Errorf("agent is nil")
	}
	if sessionID == "" {
		return nil, fmt.Errorf("resume session id is empty")
	}
	if transcript := a.transcriptProjection(); transcript != nil {
		messages, ok := transcript.Snapshot().Sessions[sessionID]
		if !ok {
			return nil, fmt.Errorf("session %q not found", sessionID)
		}
		return &ChatSession{
			SessionID: sessionID,
			Messages:  append([]contracts.Message{}, messages...),
		}, nil
	}
	events, err := a.EventLog.ListByAggregate(ctx, eventing.AggregateSession, sessionID)
	if err != nil {
		return nil, fmt.Errorf("load session events: %w", err)
	}
	if len(events) == 0 {
		return nil, fmt.Errorf("session %q not found", sessionID)
	}
	session := &ChatSession{SessionID: sessionID, Messages: []contracts.Message{}}
	for _, event := range events {
		if event.Kind != eventing.EventMessageRecorded {
			continue
		}
		role, _ := event.Payload["role"].(string)
		content, _ := event.Payload["content"].(string)
		if role == "" || content == "" {
			continue
		}
		session.Messages = append(session.Messages, contracts.Message{Role: role, Content: content})
	}
	return session, nil
}

func (a *Agent) ChatTurn(ctx context.Context, session *ChatSession, input ChatTurnInput) (provider.ClientResult, error) {
	if a == nil {
		return provider.ClientResult{}, fmt.Errorf("agent is nil")
	}
	if a.ProviderClient == nil {
		return provider.ClientResult{}, fmt.Errorf("agent provider client is nil")
	}
	if session == nil {
		return provider.ClientResult{}, fmt.Errorf("chat session is nil")
	}
	if input.Prompt == "" {
		return provider.ClientResult{}, fmt.Errorf("chat prompt is empty")
	}

	now := a.now()
	runID := a.newID("run-chat")
	correlationID := runID

	if !a.sessionExists(session.SessionID) {
		if err := a.RecordEvent(ctx, eventing.Event{
			ID:               a.newID("evt-session-created"),
			Kind:             eventing.EventSessionCreated,
			OccurredAt:       now,
			AggregateID:      session.SessionID,
			AggregateType:    eventing.AggregateSession,
			AggregateVersion: 1,
			CorrelationID:    correlationID,
			Source:           "agent.chat",
			ActorID:          a.Config.ID,
			ActorType:        "agent",
			TraceSummary:     "chat session bootstrap",
			Payload:          map[string]any{"session_id": session.SessionID},
		}); err != nil {
			return provider.ClientResult{}, fmt.Errorf("record session bootstrap: %w", err)
		}
	}

	userMessage := contracts.Message{Role: "user", Content: input.Prompt}
	if err := a.recordSessionMessage(ctx, session.SessionID, correlationID, userMessage); err != nil {
		return provider.ClientResult{}, fmt.Errorf("record user message: %w", err)
	}
	session.Messages = append(session.Messages, userMessage)

	if err := a.RecordEvent(ctx, eventing.Event{
		ID:               a.newID("evt-run-started"),
		Kind:             eventing.EventRunStarted,
		OccurredAt:       now,
		AggregateID:      runID,
		AggregateType:    eventing.AggregateRun,
		AggregateVersion: 1,
		CorrelationID:    correlationID,
		Source:           "agent.chat",
		ActorID:          a.Config.ID,
		ActorType:        "agent",
		TraceSummary:     "chat provider request started",
		Payload: map[string]any{
			"session_id": session.SessionID,
			"prompt":     input.Prompt,
		},
	}); err != nil {
		return provider.ClientResult{}, fmt.Errorf("record run started: %w", err)
	}

	result, err := a.ProviderClient.Execute(ctx, a.Contracts, provider.ClientInput{
		PromptAssetSelection: input.PromptAssetSelection,
		Messages:             append([]contracts.Message{}, session.Messages...),
		StreamObserver:       input.StreamObserver,
	})
	if err != nil {
		if recordErr := a.recordProviderRequestEvent(ctx, runID, session.SessionID, correlationID, "agent.chat", result.RequestBody); recordErr != nil {
			return provider.ClientResult{}, fmt.Errorf("execute chat turn: %v; record provider request: %w", err, recordErr)
		}
		if recordErr := a.recordTransportAttemptEvents(ctx, runID, session.SessionID, correlationID, result.TransportAttempts); recordErr != nil {
			return provider.ClientResult{}, fmt.Errorf("execute chat turn: %v; record transport attempts: %w", err, recordErr)
		}
		if recordErr := a.RecordEvent(ctx, eventing.Event{
			ID:               a.newID("evt-run-failed"),
			Kind:             eventing.EventRunFailed,
			OccurredAt:       a.now(),
			AggregateID:      runID,
			AggregateType:    eventing.AggregateRun,
			AggregateVersion: 2,
			CorrelationID:    correlationID,
			CausationID:      runID,
			Source:           "agent.chat",
			ActorID:          a.Config.ID,
			ActorType:        "agent",
			TraceSummary:     "chat provider request failed",
			Payload: map[string]any{
				"session_id": session.SessionID,
				"error":      err.Error(),
			},
		}); recordErr != nil {
			return provider.ClientResult{}, fmt.Errorf("execute chat turn: %v; record failure event: %w", err, recordErr)
		}
		return provider.ClientResult{}, fmt.Errorf("execute chat turn: %w", err)
	}

	if err := a.recordProviderRequestEvent(ctx, runID, session.SessionID, correlationID, "agent.chat", result.RequestBody); err != nil {
		return provider.ClientResult{}, fmt.Errorf("record provider request: %w", err)
	}
	if err := a.recordTransportAttemptEvents(ctx, runID, session.SessionID, correlationID, result.TransportAttempts); err != nil {
		return provider.ClientResult{}, fmt.Errorf("record transport attempts: %w", err)
	}

	if err := a.recordSessionMessage(ctx, session.SessionID, correlationID, result.Provider.Message); err != nil {
		return provider.ClientResult{}, fmt.Errorf("record assistant message: %w", err)
	}
	session.Messages = append(session.Messages, result.Provider.Message)

	if err := a.RecordEvent(ctx, eventing.Event{
		ID:               a.newID("evt-run-completed"),
		Kind:             eventing.EventRunCompleted,
		OccurredAt:       a.now(),
		AggregateID:      runID,
		AggregateType:    eventing.AggregateRun,
		AggregateVersion: 2,
		CorrelationID:    correlationID,
		CausationID:      runID,
		Source:           "agent.chat",
		ActorID:          a.Config.ID,
		ActorType:        "agent",
		TraceSummary:     "chat provider request completed",
		Payload: map[string]any{
			"session_id":     session.SessionID,
			"provider_id":    result.Provider.ID,
			"model":          result.Provider.Model,
			"finish_reason":  result.Provider.FinishReason,
			"assistant_text": result.Provider.Message.Content,
			"input_tokens":   result.Provider.Usage.InputTokens,
			"output_tokens":  result.Provider.Usage.OutputTokens,
			"total_tokens":   result.Provider.Usage.TotalTokens,
		},
	}); err != nil {
		return provider.ClientResult{}, fmt.Errorf("record run completed: %w", err)
	}

	return result, nil
}

func (a *Agent) transcriptProjection() *projections.TranscriptProjection {
	for _, projection := range a.Projections {
		transcript, ok := projection.(*projections.TranscriptProjection)
		if ok {
			return transcript
		}
	}
	return nil
}

func (a *Agent) recordSessionMessage(ctx context.Context, sessionID, correlationID string, message contracts.Message) error {
	return a.RecordEvent(ctx, eventing.Event{
		ID:               a.newID("evt-message-recorded"),
		Kind:             eventing.EventMessageRecorded,
		OccurredAt:       a.now(),
		AggregateID:      sessionID,
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 2,
		CorrelationID:    correlationID,
		Source:           "agent.chat",
		ActorID:          a.Config.ID,
		ActorType:        "agent",
		TraceSummary:     "chat message recorded",
		Payload: map[string]any{
			"session_id": sessionID,
			"role":       message.Role,
			"content":    message.Content,
		},
	})
}
