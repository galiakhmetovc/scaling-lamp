package projections

import (
	"encoding/json"
	"fmt"

	"teamd/internal/runtime/eventing"
)

type SessionPromptSnapshot struct {
	Overrides map[string]string `json:"overrides"`
}

type SessionPromptProjection struct {
	snapshot SessionPromptSnapshot
}

func NewSessionPromptProjection() *SessionPromptProjection {
	return &SessionPromptProjection{
		snapshot: SessionPromptSnapshot{Overrides: map[string]string{}},
	}
}

func (p *SessionPromptProjection) ID() string { return "session_prompt" }

func (p *SessionPromptProjection) Apply(event eventing.Event) error {
	if p.snapshot.Overrides == nil {
		p.snapshot.Overrides = map[string]string{}
	}
	switch event.Kind {
	case eventing.EventSessionPromptOverrideSet:
		override, _ := event.Payload["override"].(string)
		if override == "" {
			delete(p.snapshot.Overrides, event.AggregateID)
		} else {
			p.snapshot.Overrides[event.AggregateID] = override
		}
	case eventing.EventSessionDeleted:
		delete(p.snapshot.Overrides, event.AggregateID)
	}
	return nil
}

func (p *SessionPromptProjection) Snapshot() SessionPromptSnapshot { return p.snapshot }
func (p *SessionPromptProjection) SnapshotValue() any             { return p.snapshot }

func (p *SessionPromptProjection) RestoreSnapshot(raw []byte) error {
	var snapshot SessionPromptSnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore session prompt snapshot: %w", err)
	}
	if snapshot.Overrides == nil {
		snapshot.Overrides = map[string]string{}
	}
	p.snapshot = snapshot
	return nil
}

func (p *SessionPromptProjection) OverrideForSession(sessionID string) string {
	if p.snapshot.Overrides == nil {
		return ""
	}
	return p.snapshot.Overrides[sessionID]
}
