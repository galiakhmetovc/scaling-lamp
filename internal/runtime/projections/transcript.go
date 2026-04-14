package projections

import (
	"encoding/json"
	"fmt"

	"teamd/internal/contracts"
	"teamd/internal/runtime/eventing"
)

type TranscriptSnapshot struct {
	Sessions map[string][]contracts.Message `json:"sessions"`
}

type TranscriptProjection struct {
	snapshot TranscriptSnapshot
}

func NewTranscriptProjection() *TranscriptProjection {
	return &TranscriptProjection{
		snapshot: TranscriptSnapshot{
			Sessions: map[string][]contracts.Message{},
		},
	}
}

func (p *TranscriptProjection) ID() string {
	return "transcript"
}

func (p *TranscriptProjection) Apply(event eventing.Event) error {
	if event.Kind != eventing.EventMessageRecorded {
		return nil
	}
	sessionID, _ := event.Payload["session_id"].(string)
	role, _ := event.Payload["role"].(string)
	content, _ := event.Payload["content"].(string)
	if sessionID == "" || role == "" || content == "" {
		return nil
	}
	if p.snapshot.Sessions == nil {
		p.snapshot.Sessions = map[string][]contracts.Message{}
	}
	p.snapshot.Sessions[sessionID] = append(p.snapshot.Sessions[sessionID], contracts.Message{
		Role:    role,
		Content: content,
	})
	return nil
}

func (p *TranscriptProjection) Snapshot() TranscriptSnapshot {
	return p.snapshot
}

func (p *TranscriptProjection) SnapshotValue() any {
	return p.snapshot
}

func (p *TranscriptProjection) RestoreSnapshot(raw []byte) error {
	var snapshot TranscriptSnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore transcript snapshot: %w", err)
	}
	if snapshot.Sessions == nil {
		snapshot.Sessions = map[string][]contracts.Message{}
	}
	p.snapshot = snapshot
	return nil
}
