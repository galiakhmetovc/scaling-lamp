package mesh

import (
	"testing"
	"time"
)

func TestLeaseManagerTransitionsWarmIdleDrainingStopped(t *testing.T) {
	manager := NewLeaseManager()
	now := time.Date(2026, 4, 10, 12, 0, 0, 0, time.UTC)

	lease := manager.Start(AgentIdentity{AgentID: "agent-a"}, LeaseSpec{
		RuntimeID:    "rt-a",
		IdleTTL:      5 * time.Minute,
		MaxLifetime:  30 * time.Minute,
		StartedAt:    now,
		LastUsedAt:   now,
	})
	if lease.State != RuntimeStateWarm {
		t.Fatalf("expected warm lease after start, got %q", lease.State)
	}

	lease, err := manager.MarkIdle("rt-a", now.Add(1*time.Minute))
	if err != nil {
		t.Fatalf("mark idle: %v", err)
	}
	if lease.State != RuntimeStateIdle {
		t.Fatalf("expected idle state, got %q", lease.State)
	}

	lease, err = manager.BeginDrain("rt-a")
	if err != nil {
		t.Fatalf("begin drain: %v", err)
	}
	if lease.State != RuntimeStateDraining {
		t.Fatalf("expected draining state, got %q", lease.State)
	}

	lease, err = manager.Stop("rt-a", now.Add(2*time.Minute))
	if err != nil {
		t.Fatalf("stop: %v", err)
	}
	if lease.State != RuntimeStateStopped {
		t.Fatalf("expected stopped state, got %q", lease.State)
	}
}

func TestLeaseManagerKeepsPinnedLeaseWarmPastIdleTTL(t *testing.T) {
	manager := NewLeaseManager()
	now := time.Date(2026, 4, 10, 12, 0, 0, 0, time.UTC)

	lease := manager.Start(AgentIdentity{AgentID: "agent-a"}, LeaseSpec{
		RuntimeID:    "rt-a",
		IdleTTL:      1 * time.Minute,
		MaxLifetime:  30 * time.Minute,
		StartedAt:    now,
		LastUsedAt:   now,
		Pinned:       true,
	})
	lease, err := manager.MarkIdle("rt-a", now.Add(10*time.Second))
	if err != nil {
		t.Fatalf("mark idle: %v", err)
	}
	if lease.State != RuntimeStateWarm {
		t.Fatalf("expected pinned lease to remain warm, got %q", lease.State)
	}

	expired := manager.ExpireIdle(now.Add(2 * time.Minute))
	if len(expired) != 0 {
		t.Fatalf("expected no idle expiry for pinned lease, got %#v", expired)
	}
}

func TestLeaseManagerSerializesAssignAndDrain(t *testing.T) {
	manager := NewLeaseManager()
	now := time.Date(2026, 4, 10, 12, 0, 0, 0, time.UTC)

	manager.Start(AgentIdentity{AgentID: "agent-a"}, LeaseSpec{
		RuntimeID:    "rt-a",
		IdleTTL:      5 * time.Minute,
		MaxLifetime:  30 * time.Minute,
		StartedAt:    now,
		LastUsedAt:   now,
	})
	if _, err := manager.BeginDrain("rt-a"); err != nil {
		t.Fatalf("begin drain: %v", err)
	}

	if _, err := manager.Assign("rt-a", now.Add(30*time.Second)); err == nil {
		t.Fatal("expected assign to draining lease to fail")
	}
}
