package projections

import (
	"encoding/json"
	"fmt"
	"sort"
	"time"

	"teamd/internal/runtime/eventing"
)

type DelegateArtifactRefView struct {
	Ref         string `json:"ref"`
	Kind        string `json:"kind"`
	Label       string `json:"label"`
	ContentType string `json:"content_type"`
}

type DelegateEventRefView struct {
	EventID int64  `json:"event_id"`
	Kind    string `json:"kind"`
}

type DelegateHandoffView struct {
	DelegateID          string                  `json:"delegate_id"`
	Backend             string                  `json:"backend"`
	LastRunID           string                  `json:"last_run_id"`
	Summary             string                  `json:"summary"`
	Artifacts           []DelegateArtifactRefView `json:"artifacts,omitempty"`
	PromotedFacts       []string                `json:"promoted_facts,omitempty"`
	OpenQuestions       []string                `json:"open_questions,omitempty"`
	RecommendedNextStep string                  `json:"recommended_next_step,omitempty"`
	CreatedAt           time.Time               `json:"created_at"`
	UpdatedAt           time.Time               `json:"updated_at"`
}

type DelegateView struct {
	DelegateID        string                 `json:"delegate_id"`
	Backend           string                 `json:"backend"`
	OwnerSessionID    string                 `json:"owner_session_id"`
	DelegateSessionID string                 `json:"delegate_session_id"`
	Status            string                 `json:"status"`
	LastRunID         string                 `json:"last_run_id,omitempty"`
	ArtifactRefs      []DelegateArtifactRefView `json:"artifact_refs,omitempty"`
	EventRefs         []DelegateEventRefView `json:"event_refs,omitempty"`
	PolicySnapshot    map[string]any         `json:"policy_snapshot,omitempty"`
	LastError         string                 `json:"last_error,omitempty"`
	CreatedAt         time.Time              `json:"created_at"`
	UpdatedAt         time.Time              `json:"updated_at"`
	LastMessageAt     *time.Time             `json:"last_message_at,omitempty"`
	ClosedAt          *time.Time             `json:"closed_at,omitempty"`
}

type DelegateSnapshot struct {
	Delegates map[string]DelegateView       `json:"delegates"`
	Handoffs  map[string]DelegateHandoffView `json:"handoffs"`
}

type DelegateProjection struct {
	snapshot DelegateSnapshot
}

func NewDelegateProjection() *DelegateProjection {
	return &DelegateProjection{
		snapshot: DelegateSnapshot{
			Delegates: map[string]DelegateView{},
			Handoffs:  map[string]DelegateHandoffView{},
		},
	}
}

func (p *DelegateProjection) ID() string { return "delegate" }

func (p *DelegateProjection) Apply(event eventing.Event) error {
	if p.snapshot.Delegates == nil {
		p.snapshot.Delegates = map[string]DelegateView{}
	}
	if p.snapshot.Handoffs == nil {
		p.snapshot.Handoffs = map[string]DelegateHandoffView{}
	}
	switch event.Kind {
	case eventing.EventDelegateSpawned:
		view := p.snapshot.Delegates[event.AggregateID]
		view.DelegateID = event.AggregateID
		view.Backend = stringValue(event.Payload["backend"])
		view.OwnerSessionID = stringValue(event.Payload["owner_session_id"])
		view.DelegateSessionID = stringValue(event.Payload["delegate_session_id"])
		view.Status = "queued"
		view.PolicySnapshot = cloneMap(event.Payload["policy_snapshot"])
		view.CreatedAt = event.OccurredAt
		view.UpdatedAt = event.OccurredAt
		p.snapshot.Delegates[event.AggregateID] = view
	case eventing.EventDelegateMessageReceived:
		view := p.snapshot.Delegates[event.AggregateID]
		view.UpdatedAt = event.OccurredAt
		view.Status = "queued"
		ts := event.OccurredAt
		view.LastMessageAt = &ts
		p.snapshot.Delegates[event.AggregateID] = view
	case eventing.EventDelegateRunStarted:
		view := p.snapshot.Delegates[event.AggregateID]
		view.Status = "running"
		view.LastRunID = stringValue(event.Payload["delegate_run_id"])
		view.UpdatedAt = event.OccurredAt
		view.EventRefs = append(view.EventRefs, DelegateEventRefView{EventID: int64(event.Sequence), Kind: string(event.Kind)})
		p.snapshot.Delegates[event.AggregateID] = view
	case eventing.EventDelegateCompleted:
		view := p.snapshot.Delegates[event.AggregateID]
		view.Status = "idle"
		view.LastRunID = stringValue(event.Payload["delegate_run_id"])
		view.LastError = ""
		view.UpdatedAt = event.OccurredAt
		view.ArtifactRefs = artifactRefs(event.Payload["artifacts"])
		view.EventRefs = append(view.EventRefs, DelegateEventRefView{EventID: int64(event.Sequence), Kind: string(event.Kind)})
		p.snapshot.Delegates[event.AggregateID] = view
	case eventing.EventDelegateFailed:
		view := p.snapshot.Delegates[event.AggregateID]
		view.Status = "failed"
		view.LastRunID = stringValue(event.Payload["delegate_run_id"])
		view.LastError = stringValue(event.Payload["error"])
		view.UpdatedAt = event.OccurredAt
		view.EventRefs = append(view.EventRefs, DelegateEventRefView{EventID: int64(event.Sequence), Kind: string(event.Kind)})
		p.snapshot.Delegates[event.AggregateID] = view
	case eventing.EventDelegateClosed:
		view := p.snapshot.Delegates[event.AggregateID]
		view.Status = "closed"
		view.UpdatedAt = event.OccurredAt
		ts := event.OccurredAt
		view.ClosedAt = &ts
		view.EventRefs = append(view.EventRefs, DelegateEventRefView{EventID: int64(event.Sequence), Kind: string(event.Kind)})
		p.snapshot.Delegates[event.AggregateID] = view
	case eventing.EventDelegateHandoffCreated:
		view := p.snapshot.Delegates[event.AggregateID]
		handoff := DelegateHandoffView{
			DelegateID:          event.AggregateID,
			Backend:             stringValue(event.Payload["backend"]),
			LastRunID:           stringValue(event.Payload["delegate_run_id"]),
			Summary:             stringValue(event.Payload["summary"]),
			Artifacts:           artifactRefs(event.Payload["artifacts"]),
			PromotedFacts:       stringSlice(event.Payload["promoted_facts"]),
			OpenQuestions:       stringSlice(event.Payload["open_questions"]),
			RecommendedNextStep: stringValue(event.Payload["recommended_next_step"]),
			CreatedAt:           timeOr(event.Payload["created_at"], event.OccurredAt),
			UpdatedAt:           event.OccurredAt,
		}
		view.ArtifactRefs = append([]DelegateArtifactRefView(nil), handoff.Artifacts...)
		view.UpdatedAt = event.OccurredAt
		view.EventRefs = append(view.EventRefs, DelegateEventRefView{EventID: int64(event.Sequence), Kind: string(event.Kind)})
		p.snapshot.Delegates[event.AggregateID] = view
		p.snapshot.Handoffs[event.AggregateID] = handoff
	}
	return nil
}

func (p *DelegateProjection) Snapshot() DelegateSnapshot { return p.snapshot }
func (p *DelegateProjection) SnapshotValue() any         { return p.snapshot }

func (p *DelegateProjection) RestoreSnapshot(raw []byte) error {
	var snapshot DelegateSnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore delegate snapshot: %w", err)
	}
	if snapshot.Delegates == nil {
		snapshot.Delegates = map[string]DelegateView{}
	}
	if snapshot.Handoffs == nil {
		snapshot.Handoffs = map[string]DelegateHandoffView{}
	}
	p.snapshot = snapshot
	return nil
}

func (p *DelegateProjection) SnapshotForOwnerSession(sessionID string) []DelegateView {
	if p.snapshot.Delegates == nil {
		return nil
	}
	out := make([]DelegateView, 0, len(p.snapshot.Delegates))
	for _, view := range p.snapshot.Delegates {
		if sessionID != "" && view.OwnerSessionID != sessionID {
			continue
		}
		out = append(out, view)
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].UpdatedAt.Equal(out[j].UpdatedAt) {
			return out[i].DelegateID < out[j].DelegateID
		}
		return out[i].UpdatedAt.After(out[j].UpdatedAt)
	})
	return out
}

func (p *DelegateProjection) View(delegateID string) (DelegateView, bool) {
	view, ok := p.snapshot.Delegates[delegateID]
	return view, ok
}

func (p *DelegateProjection) Handoff(delegateID string) (DelegateHandoffView, bool) {
	handoff, ok := p.snapshot.Handoffs[delegateID]
	return handoff, ok
}

func stringValue(value any) string {
	text, _ := value.(string)
	return text
}

func cloneMap(value any) map[string]any {
	m, _ := value.(map[string]any)
	if m == nil {
		return nil
	}
	out := make(map[string]any, len(m))
	for k, v := range m {
		out[k] = v
	}
	return out
}

func stringSlice(value any) []string {
	switch raw := value.(type) {
	case []string:
		return append([]string(nil), raw...)
	case []any:
		out := make([]string, 0, len(raw))
		for _, item := range raw {
			if text, ok := item.(string); ok && text != "" {
				out = append(out, text)
			}
		}
		return out
	default:
		return nil
	}
}

func artifactRefs(value any) []DelegateArtifactRefView {
	raw, ok := value.([]any)
	if !ok {
		return nil
	}
	out := make([]DelegateArtifactRefView, 0, len(raw))
	for _, item := range raw {
		body, ok := item.(map[string]any)
		if !ok {
			continue
		}
		out = append(out, DelegateArtifactRefView{
			Ref:         stringValue(body["ref"]),
			Kind:        stringValue(body["kind"]),
			Label:       stringValue(body["label"]),
			ContentType: stringValue(body["content_type"]),
		})
	}
	return out
}

func timeOr(value any, fallback time.Time) time.Time {
	if text, ok := value.(string); ok && text != "" {
		if parsed, err := time.Parse(time.RFC3339Nano, text); err == nil {
			return parsed
		}
	}
	return fallback
}
