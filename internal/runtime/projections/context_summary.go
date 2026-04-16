package projections

import (
	"encoding/json"
	"fmt"

	"teamd/internal/runtime/eventing"
)

type ContextSummaryView struct {
	SummaryText           string `json:"summary_text"`
	CoveredMessages       int    `json:"covered_messages"`
	ArtifactRef           string `json:"artifact_ref,omitempty"`
	SummarizationCount    int    `json:"summarization_count"`
	CompactedMessageCount int    `json:"compacted_message_count"`
	LastGuardPercent      int    `json:"last_guard_percent"`
}

type ContextSummarySnapshot struct {
	Sessions map[string]ContextSummaryView `json:"sessions"`
}

type ContextSummaryProjection struct {
	snapshot ContextSummarySnapshot
}

func NewContextSummaryProjection() *ContextSummaryProjection {
	return &ContextSummaryProjection{
		snapshot: ContextSummarySnapshot{Sessions: map[string]ContextSummaryView{}},
	}
}

func (p *ContextSummaryProjection) ID() string { return "context_summary" }

func (p *ContextSummaryProjection) Apply(event eventing.Event) error {
	switch event.Kind {
	case eventing.EventContextSummaryUpdated:
		sessionID, _ := event.Payload["session_id"].(string)
		if sessionID == "" {
			return nil
		}
		if p.snapshot.Sessions == nil {
			p.snapshot.Sessions = map[string]ContextSummaryView{}
		}
		view := p.snapshot.Sessions[sessionID]
		if summaryText, ok := event.Payload["summary_text"].(string); ok {
			view.SummaryText = summaryText
		}
		if covered, ok := contextBudgetOptionalIntPayload(event.Payload, "covered_messages"); ok {
			view.CoveredMessages = covered
		}
		if artifactRef, ok := event.Payload["artifact_ref"].(string); ok {
			view.ArtifactRef = artifactRef
		}
		if count, ok := contextBudgetOptionalIntPayload(event.Payload, "summarization_count"); ok {
			view.SummarizationCount = count
		}
		if compacted, ok := contextBudgetOptionalIntPayload(event.Payload, "compacted_message_count"); ok {
			view.CompactedMessageCount = compacted
		}
		view.LastGuardPercent = 0
		p.snapshot.Sessions[sessionID] = view
	case eventing.EventContextGuardTriggered:
		sessionID, _ := event.Payload["session_id"].(string)
		if sessionID == "" {
			return nil
		}
		if p.snapshot.Sessions == nil {
			p.snapshot.Sessions = map[string]ContextSummaryView{}
		}
		view := p.snapshot.Sessions[sessionID]
		if percent, ok := contextBudgetOptionalIntPayload(event.Payload, "guard_percent"); ok {
			view.LastGuardPercent = percent
		}
		p.snapshot.Sessions[sessionID] = view
	}
	return nil
}

func (p *ContextSummaryProjection) Snapshot() ContextSummarySnapshot { return p.snapshot }
func (p *ContextSummaryProjection) SnapshotValue() any              { return p.snapshot }

func (p *ContextSummaryProjection) SnapshotForSession(sessionID string) ContextSummaryView {
	if p.snapshot.Sessions == nil {
		return ContextSummaryView{}
	}
	return p.snapshot.Sessions[sessionID]
}

func (p *ContextSummaryProjection) RestoreSnapshot(raw []byte) error {
	var snapshot ContextSummarySnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore context summary snapshot: %w", err)
	}
	if snapshot.Sessions == nil {
		snapshot.Sessions = map[string]ContextSummaryView{}
	}
	p.snapshot = snapshot
	return nil
}
