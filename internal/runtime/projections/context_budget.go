package projections

import (
	"encoding/json"
	"fmt"

	"teamd/internal/runtime/eventing"
)

type ContextBudgetView struct {
	LastInputTokens    int    `json:"last_input_tokens"`
	LastOutputTokens   int    `json:"last_output_tokens"`
	LastTotalTokens    int    `json:"last_total_tokens"`
	SummaryTokens      int    `json:"summary_tokens"`
	SummarizationCount int    `json:"summarization_count"`
	CompactedMessageCount int `json:"compacted_message_count"`
	Source             string `json:"source,omitempty"`
}

type ContextBudgetSnapshot struct {
	Sessions map[string]ContextBudgetView `json:"sessions"`
}

type ContextBudgetProjection struct {
	snapshot ContextBudgetSnapshot
}

func NewContextBudgetProjection() *ContextBudgetProjection {
	return &ContextBudgetProjection{
		snapshot: ContextBudgetSnapshot{Sessions: map[string]ContextBudgetView{}},
	}
}

func (p *ContextBudgetProjection) ID() string { return "context_budget" }

func (p *ContextBudgetProjection) Apply(event eventing.Event) error {
	switch event.Kind {
	case eventing.EventRunCompleted:
		sessionID, _ := event.Payload["session_id"].(string)
		if sessionID == "" {
			return nil
		}
		p.ensureSessions()
		view := p.snapshot.Sessions[sessionID]
		view.LastInputTokens = contextBudgetIntPayload(event.Payload, "input_tokens")
		view.LastOutputTokens = contextBudgetIntPayload(event.Payload, "output_tokens")
		view.LastTotalTokens = contextBudgetIntPayload(event.Payload, "total_tokens")
		view.Source = "provider"
		p.snapshot.Sessions[sessionID] = view
	case eventing.EventMessageRecorded:
		sessionID, _ := event.Payload["session_id"].(string)
		if sessionID == "" {
			return nil
		}
		p.ensureSessions()
		view := p.snapshot.Sessions[sessionID]
		if count, ok := contextBudgetOptionalIntPayload(event.Payload, "summarization_count"); ok {
			view.SummarizationCount = count
		}
		if tokens, ok := contextBudgetOptionalIntPayload(event.Payload, "summary_tokens"); ok {
			view.SummaryTokens = tokens
		}
		p.snapshot.Sessions[sessionID] = view
	case eventing.EventContextSummaryUpdated:
		sessionID, _ := event.Payload["session_id"].(string)
		if sessionID == "" {
			return nil
		}
		p.ensureSessions()
		view := p.snapshot.Sessions[sessionID]
		if count, ok := contextBudgetOptionalIntPayload(event.Payload, "summarization_count"); ok {
			view.SummarizationCount = count
		}
		if tokens, ok := contextBudgetOptionalIntPayload(event.Payload, "summary_tokens"); ok {
			view.SummaryTokens = tokens
		}
		if compacted, ok := contextBudgetOptionalIntPayload(event.Payload, "compacted_message_count"); ok {
			view.CompactedMessageCount = compacted
		}
		if view.Source == "" {
			view.Source = "mixed"
		}
		p.snapshot.Sessions[sessionID] = view
	}
	return nil
}

func (p *ContextBudgetProjection) Snapshot() ContextBudgetSnapshot { return p.snapshot }
func (p *ContextBudgetProjection) SnapshotValue() any              { return p.snapshot }

func (p *ContextBudgetProjection) SnapshotForSession(sessionID string) ContextBudgetView {
	if p.snapshot.Sessions == nil {
		return ContextBudgetView{}
	}
	return p.snapshot.Sessions[sessionID]
}

func (p *ContextBudgetProjection) RestoreSnapshot(raw []byte) error {
	var snapshot ContextBudgetSnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore context budget snapshot: %w", err)
	}
	if snapshot.Sessions == nil {
		snapshot.Sessions = map[string]ContextBudgetView{}
	}
	p.snapshot = snapshot
	return nil
}

func (p *ContextBudgetProjection) ensureSessions() {
	if p.snapshot.Sessions == nil {
		p.snapshot.Sessions = map[string]ContextBudgetView{}
	}
}

func contextBudgetIntPayload(payload map[string]any, key string) int {
	if value, ok := contextBudgetOptionalIntPayload(payload, key); ok {
		return value
	}
	return 0
}

func contextBudgetOptionalIntPayload(payload map[string]any, key string) (int, bool) {
	switch value := payload[key].(type) {
	case int:
		return value, true
	case int64:
		return int(value), true
	case float64:
		return int(value), true
	default:
		return 0, false
	}
}
