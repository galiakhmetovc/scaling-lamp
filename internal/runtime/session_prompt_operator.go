package runtime

import (
	"context"
	"fmt"
	"os"
	"strings"

	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func (a *Agent) sessionPromptProjection() *projections.SessionPromptProjection {
	for _, projection := range a.Projections {
		prompt, ok := projection.(*projections.SessionPromptProjection)
		if ok {
			return prompt
		}
	}
	return nil
}

func (a *Agent) CurrentSessionPromptOverride(sessionID string) string {
	if !a.sessionExists(sessionID) {
		return ""
	}
	if projection := a.sessionPromptProjection(); projection != nil {
		return projection.OverrideForSession(sessionID)
	}
	return ""
}

func (a *Agent) DefaultSystemPrompt() (string, error) {
	if a == nil {
		return "", fmt.Errorf("agent is nil")
	}
	policy := a.Contracts.PromptAssembly.SystemPrompt
	if !policy.Enabled || strings.TrimSpace(policy.Params.Path) == "" {
		return "", nil
	}
	body, err := os.ReadFile(policy.Params.Path)
	if err != nil {
		if policy.Params.Required {
			return "", fmt.Errorf("read system prompt file: %w", err)
		}
		return "", nil
	}
	content := string(body)
	if policy.Params.TrimTrailingWhitespace {
		content = strings.TrimRight(content, " \t\r\n")
	}
	return content, nil
}

func (a *Agent) EffectiveSystemPrompt(sessionID string) (string, error) {
	override := a.CurrentSessionPromptOverride(sessionID)
	if strings.TrimSpace(override) != "" {
		return override, nil
	}
	return a.DefaultSystemPrompt()
}

func (a *Agent) SetSessionPromptOverride(ctx context.Context, sessionID, override string) error {
	if a == nil {
		return fmt.Errorf("agent is nil")
	}
	if strings.TrimSpace(sessionID) == "" {
		return fmt.Errorf("session id is empty")
	}
	if !a.sessionExists(sessionID) {
		return fmt.Errorf("session %q not found", sessionID)
	}
	override = strings.TrimSpace(override)
	if override == "" {
		return fmt.Errorf("session prompt override is empty")
	}
	return a.RecordEvent(ctx, eventing.Event{
		ID:               a.newID("evt-session-prompt-override-set"),
		Kind:             eventing.EventSessionPromptOverrideSet,
		OccurredAt:       a.now(),
		AggregateID:      sessionID,
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 3,
		CorrelationID:    sessionID,
		Source:           "runtime.session",
		ActorID:          a.Config.ID,
		ActorType:        "agent",
		TraceSummary:     "session prompt override set",
		Payload: map[string]any{
			"session_id": sessionID,
			"override":   override,
		},
	})
}

func (a *Agent) ClearSessionPromptOverride(ctx context.Context, sessionID string) error {
	if a == nil {
		return fmt.Errorf("agent is nil")
	}
	if strings.TrimSpace(sessionID) == "" {
		return fmt.Errorf("session id is empty")
	}
	if !a.sessionExists(sessionID) {
		return fmt.Errorf("session %q not found", sessionID)
	}
	return a.RecordEvent(ctx, eventing.Event{
		ID:               a.newID("evt-session-prompt-override-cleared"),
		Kind:             eventing.EventSessionPromptOverrideSet,
		OccurredAt:       a.now(),
		AggregateID:      sessionID,
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 4,
		CorrelationID:    sessionID,
		Source:           "runtime.session",
		ActorID:          a.Config.ID,
		ActorType:        "agent",
		TraceSummary:     "session prompt override cleared",
		Payload: map[string]any{
			"session_id": sessionID,
			"override":   "",
		},
	})
}
