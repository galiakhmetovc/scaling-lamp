package mesh

import (
	"context"
	"testing"
)

func TestRouterSamplesPeersDuringColdStart(t *testing.T) {
	ctx := context.Background()
	reg := newMemoryRegistry()
	owner := PeerDescriptor{AgentID: "owner", Status: "idle"}
	peers := []PeerDescriptor{
		{AgentID: "peer-a", Status: "idle"},
		{AgentID: "peer-b", Status: "idle"},
		{AgentID: "peer-c", Status: "busy"},
	}
	for _, peer := range append([]PeerDescriptor{owner}, peers...) {
		if err := reg.Register(ctx, peer); err != nil {
			t.Fatalf("register %s: %v", peer.AgentID, err)
		}
	}

	router := NewRouter(reg, RouterConfig{
		AgentID:         owner.AgentID,
		ColdStartFanout: 2,
	})

	selected, err := router.SelectPeers(ctx, owner.AgentID, "coding")
	if err != nil {
		t.Fatalf("select peers: %v", err)
	}
	if len(selected) != 2 {
		t.Fatalf("expected 2 selected peers, got %d: %#v", len(selected), selected)
	}
	for _, peer := range selected {
		if peer.AgentID == owner.AgentID {
			t.Fatalf("owner must not be delegated to itself: %#v", selected)
		}
		if peer.Status != "idle" {
			t.Fatalf("expected only idle peers, got %#v", selected)
		}
	}
}

func TestRouterPrefersHigherScoredPeersAfterWarmup(t *testing.T) {
	ctx := context.Background()
	reg := newMemoryRegistry()
	owner := PeerDescriptor{AgentID: "owner", Status: "idle"}
	high := PeerDescriptor{AgentID: "peer-high", Status: "idle"}
	low := PeerDescriptor{AgentID: "peer-low", Status: "idle"}
	for _, peer := range []PeerDescriptor{owner, high, low} {
		if err := reg.Register(ctx, peer); err != nil {
			t.Fatalf("register %s: %v", peer.AgentID, err)
		}
	}
	if err := reg.RecordScore(ctx, ScoreRecord{AgentID: low.AgentID, TaskClass: "coding", TasksSeen: 8, TasksWon: 2, SuccessCount: 2, AvgLatencyMS: 100}); err != nil {
		t.Fatalf("record low score: %v", err)
	}
	if err := reg.RecordScore(ctx, ScoreRecord{AgentID: high.AgentID, TaskClass: "coding", TasksSeen: 8, TasksWon: 6, SuccessCount: 6, AvgLatencyMS: 120}); err != nil {
		t.Fatalf("record high score: %v", err)
	}

	router := NewRouter(reg, RouterConfig{
		AgentID:         owner.AgentID,
		ColdStartFanout: 2,
	})

	selected, err := router.SelectPeers(ctx, owner.AgentID, "coding")
	if err != nil {
		t.Fatalf("select peers: %v", err)
	}
	if len(selected) == 0 {
		t.Fatalf("expected selected peers")
	}
	if selected[0].AgentID != high.AgentID {
		t.Fatalf("expected highest scored peer first, got %#v", selected)
	}
}
