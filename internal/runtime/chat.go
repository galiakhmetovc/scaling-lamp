package runtime

import (
	"context"
	"fmt"

	"teamd/internal/contracts"
	"teamd/internal/promptassembly"
	"teamd/internal/provider"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

type ChatSession struct {
	SessionID string
	Messages  []contracts.Message
}

type ChatTurnInput struct {
	Prompt                string
	PromptAssetSelection  []string
	StreamObserver        func(provider.StreamEvent)
	ToolObserver          func(ToolActivity)
	MaxToolRoundsOverride int
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
	if a.UIBus != nil {
		a.UIBus.Publish(UIEvent{Kind: UIEventSessionChanged, SessionID: session.SessionID, RunID: runID, Status: "turn_started"})
	}

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

	result, err := a.executeProviderLoop(ctx, session.SessionID, runID, correlationID, "agent.chat", provider.ClientInput{
		PromptAssetSelection: input.PromptAssetSelection,
		Messages:             append([]contracts.Message{}, session.Messages...),
		StreamObserver:       input.StreamObserver,
	}, input.ToolObserver, input.MaxToolRoundsOverride)
	if err != nil {
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
		if a.UIBus != nil {
			a.UIBus.Publish(UIEvent{Kind: UIEventStatusChanged, SessionID: session.SessionID, RunID: runID, Status: "failed"})
		}
		return provider.ClientResult{}, fmt.Errorf("execute chat turn: %w", err)
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
	if a.UIBus != nil {
		a.UIBus.Publish(UIEvent{
			Kind:      UIEventRunCompleted,
			SessionID: session.SessionID,
			RunID:     runID,
			Status:    result.Provider.FinishReason,
			Text:      result.Provider.Message.Content,
		})
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

func (a *Agent) assemblePromptMessages(sessionID string, fallback []contracts.Message) ([]contracts.Message, error) {
	if a == nil || a.PromptAssembly == nil {
		return fallback, nil
	}
	transcript := projections.TranscriptSnapshot{Sessions: map[string][]contracts.Message{}}
	planHead := projections.PlanHeadSnapshot{
		Tasks:                 map[string]projections.PlanTaskView{},
		Ready:                 map[string]bool{},
		WaitingOnDependencies: map[string]bool{},
		Blocked:               map[string]string{},
		Notes:                 map[string][]string{},
	}
	if projection := a.transcriptProjection(); projection != nil {
		transcript = projection.Snapshot()
	}
	if projection := a.planHeadProjection(); projection != nil {
		planHead = projection.SnapshotForSession(sessionID)
	}
	messages, err := a.PromptAssembly.Build(a.Contracts.PromptAssembly, promptassembly.Input{
		SessionID:   sessionID,
		Transcript:  transcript,
		PlanHead:    planHead,
		RawMessages: append([]contracts.Message{}, fallback...),
	})
	if err != nil {
		return nil, err
	}
	if len(messages) == 0 {
		return fallback, nil
	}
	return messages, nil
}

func (a *Agent) planHeadProjection() *projections.PlanHeadProjection {
	for _, projection := range a.Projections {
		planHead, ok := projection.(*projections.PlanHeadProjection)
		if ok {
			return planHead
		}
	}
	return nil
}

func (a *Agent) CurrentPlanHead(sessionID string) (projections.PlanHeadSnapshot, bool) {
	projection := a.planHeadProjection()
	if projection == nil {
		return projections.PlanHeadSnapshot{}, false
	}
	return projection.SnapshotForSession(sessionID), true
}

func (a *Agent) CurrentTranscript(sessionID string) []contracts.Message {
	if projection := a.transcriptProjection(); projection != nil {
		return append([]contracts.Message{}, projection.Snapshot().Sessions[sessionID]...)
	}
	return nil
}

func (a *Agent) sessionCatalogProjection() *projections.SessionCatalogProjection {
	for _, projection := range a.Projections {
		catalog, ok := projection.(*projections.SessionCatalogProjection)
		if ok {
			return catalog
		}
	}
	return nil
}

func (a *Agent) chatTimelineProjection() *projections.ChatTimelineProjection {
	for _, projection := range a.Projections {
		timeline, ok := projection.(*projections.ChatTimelineProjection)
		if ok {
			return timeline
		}
	}
	return nil
}

func (a *Agent) shellCommandProjection() *projections.ShellCommandProjection {
	for _, projection := range a.Projections {
		shellCommands, ok := projection.(*projections.ShellCommandProjection)
		if ok {
			return shellCommands
		}
	}
	return nil
}

func (a *Agent) ListSessions() []projections.SessionCatalogEntry {
	if projection := a.sessionCatalogProjection(); projection != nil {
		return projections.SortedSessionEntries(projection.Snapshot())
	}
	return nil
}

func (a *Agent) CurrentChatTimeline(sessionID string) []projections.ChatTimelineItem {
	if projection := a.chatTimelineProjection(); projection != nil {
		return projection.SnapshotForSession(sessionID)
	}
	return nil
}

func (a *Agent) CurrentRunningShellCommands(sessionID string) []projections.ShellCommandView {
	if projection := a.shellCommandProjection(); projection != nil {
		return projection.ActiveForSession(sessionID)
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
