package projections

import (
	"encoding/json"
	"fmt"
	"sort"
	"time"

	"teamd/internal/runtime/eventing"
)

type SessionCatalogEntry struct {
	SessionID    string    `json:"session_id"`
	CreatedAt    time.Time `json:"created_at"`
	LastActivity time.Time `json:"last_activity"`
	MessageCount int       `json:"message_count"`
}

type SessionCatalogSnapshot struct {
	Sessions map[string]SessionCatalogEntry `json:"sessions"`
}

type SessionCatalogProjection struct {
	snapshot SessionCatalogSnapshot
}

func NewSessionCatalogProjection() *SessionCatalogProjection {
	return &SessionCatalogProjection{
		snapshot: SessionCatalogSnapshot{Sessions: map[string]SessionCatalogEntry{}},
	}
}

func (p *SessionCatalogProjection) ID() string { return "session_catalog" }

func (p *SessionCatalogProjection) Apply(event eventing.Event) error {
	switch event.Kind {
	case eventing.EventSessionCreated:
		if p.snapshot.Sessions == nil {
			p.snapshot.Sessions = map[string]SessionCatalogEntry{}
		}
		p.snapshot.Sessions[event.AggregateID] = SessionCatalogEntry{
			SessionID:    event.AggregateID,
			CreatedAt:    event.OccurredAt,
			LastActivity: event.OccurredAt,
		}
	case eventing.EventMessageRecorded:
		sessionID, _ := event.Payload["session_id"].(string)
		if sessionID == "" {
			return nil
		}
		if p.snapshot.Sessions == nil {
			p.snapshot.Sessions = map[string]SessionCatalogEntry{}
		}
		entry := p.snapshot.Sessions[sessionID]
		if entry.SessionID == "" {
			entry.SessionID = sessionID
			entry.CreatedAt = event.OccurredAt
		}
		entry.LastActivity = event.OccurredAt
		entry.MessageCount++
		p.snapshot.Sessions[sessionID] = entry
	}
	return nil
}

func (p *SessionCatalogProjection) Snapshot() SessionCatalogSnapshot { return p.snapshot }
func (p *SessionCatalogProjection) SnapshotValue() any               { return p.snapshot }

func (p *SessionCatalogProjection) RestoreSnapshot(raw []byte) error {
	var snapshot SessionCatalogSnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore session catalog snapshot: %w", err)
	}
	if snapshot.Sessions == nil {
		snapshot.Sessions = map[string]SessionCatalogEntry{}
	}
	p.snapshot = snapshot
	return nil
}

func SortedSessionEntries(snapshot SessionCatalogSnapshot) []SessionCatalogEntry {
	out := make([]SessionCatalogEntry, 0, len(snapshot.Sessions))
	for _, entry := range snapshot.Sessions {
		out = append(out, entry)
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].LastActivity.Equal(out[j].LastActivity) {
			return out[i].SessionID < out[j].SessionID
		}
		return out[i].LastActivity.After(out[j].LastActivity)
	})
	return out
}
