package mesh

import (
	"context"
	"testing"
	"time"
)

type memoryRegistry struct {
	peers  map[string]PeerDescriptor
	scores []ScoreRecord
}

func newMemoryRegistry() *memoryRegistry {
	return &memoryRegistry{
		peers: make(map[string]PeerDescriptor),
	}
}

func (r *memoryRegistry) Register(_ context.Context, peer PeerDescriptor) error {
	r.peers[peer.AgentID] = peer
	return nil
}

func (r *memoryRegistry) Heartbeat(_ context.Context, agentID string, at time.Time) error {
	peer := r.peers[agentID]
	peer.LastSeenAt = at
	r.peers[agentID] = peer
	return nil
}

func (r *memoryRegistry) ListOnline(_ context.Context) ([]PeerDescriptor, error) {
	out := make([]PeerDescriptor, 0, len(r.peers))
	for _, peer := range r.peers {
		out = append(out, peer)
	}
	return out, nil
}

func (r *memoryRegistry) RecordScore(_ context.Context, score ScoreRecord) error {
	r.scores = append(r.scores, score)
	return nil
}

func (r *memoryRegistry) ListScores(_ context.Context, taskClass string) ([]ScoreRecord, error) {
	out := make([]ScoreRecord, 0, len(r.scores))
	for _, score := range r.scores {
		if score.TaskClass == taskClass {
			out = append(out, score)
		}
	}
	return out, nil
}

func TestRegistryContractRegistersListsAndRecordsScore(t *testing.T) {
	var reg Registry = newMemoryRegistry()
	peer := PeerDescriptor{
		AgentID: "agent-a",
		Model:   "glm-5",
		Status:  "idle",
	}
	if err := reg.Register(context.Background(), peer); err != nil {
		t.Fatalf("register: %v", err)
	}
	if err := reg.Heartbeat(context.Background(), peer.AgentID, time.Now().UTC()); err != nil {
		t.Fatalf("heartbeat: %v", err)
	}
	online, err := reg.ListOnline(context.Background())
	if err != nil {
		t.Fatalf("list online: %v", err)
	}
	if len(online) != 1 || online[0].AgentID != peer.AgentID {
		t.Fatalf("unexpected online peers: %#v", online)
	}
	if err := reg.RecordScore(context.Background(), ScoreRecord{AgentID: peer.AgentID, TaskClass: "coding"}); err != nil {
		t.Fatalf("record score: %v", err)
	}
	scores, err := reg.ListScores(context.Background(), "coding")
	if err != nil {
		t.Fatalf("list scores: %v", err)
	}
	if len(scores) != 1 || scores[0].AgentID != peer.AgentID {
		t.Fatalf("unexpected score rows: %#v", scores)
	}
}
