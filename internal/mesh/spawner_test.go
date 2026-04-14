package mesh

import (
	"context"
	"testing"
	"time"
)

type fakeRuntimeLauncher struct {
	starts []StartRuntimeRequest
	stops  []string
}

func (f *fakeRuntimeLauncher) Start(_ context.Context, req StartRuntimeRequest) error {
	f.starts = append(f.starts, req)
	return nil
}

func (f *fakeRuntimeLauncher) Stop(_ context.Context, runtimeID string) error {
	f.stops = append(f.stops, runtimeID)
	return nil
}

func TestSpawnerReusesWarmLeaseBeforeStartingNewRuntime(t *testing.T) {
	launcher := &fakeRuntimeLauncher{}
	spawner := NewSpawner(SpawnerDeps{
		OwnerAgentID: "owner",
		Launcher:     launcher,
		Leases:       NewLeaseManager(),
		Budget: ResourceBudget{
			MaxSpawnedPeersPerTask: 2,
			MaxWarmPeers:           2,
			MaxConcurrentAgents:    4,
			MaxLeaseLifetime:       time.Hour,
		},
	})

	identity := AgentIdentity{AgentID: "agent-a"}
	first, reused, err := spawner.Acquire(context.Background(), "owner", identity, SpawnSpec{
		IdleTTL:     10 * time.Minute,
		MaxLifetime: time.Hour,
	})
	if err != nil {
		t.Fatalf("acquire first: %v", err)
	}
	if reused {
		t.Fatal("expected first acquire to spawn new runtime")
	}
	if len(launcher.starts) != 1 {
		t.Fatalf("expected one runtime start, got %d", len(launcher.starts))
	}

	if _, err := spawner.Release(first.RuntimeID, time.Now()); err != nil {
		t.Fatalf("release: %v", err)
	}

	second, reused, err := spawner.Acquire(context.Background(), "owner", identity, SpawnSpec{
		IdleTTL:     10 * time.Minute,
		MaxLifetime: time.Hour,
	})
	if err != nil {
		t.Fatalf("acquire second: %v", err)
	}
	if !reused {
		t.Fatal("expected second acquire to reuse warm/idle runtime")
	}
	if second.RuntimeID != first.RuntimeID {
		t.Fatalf("expected reused runtime %q, got %q", first.RuntimeID, second.RuntimeID)
	}
	if len(launcher.starts) != 1 {
		t.Fatalf("expected still one runtime start, got %d", len(launcher.starts))
	}
}

func TestSpawnerEnforcesOwnerOnlySpawnAuthority(t *testing.T) {
	spawner := NewSpawner(SpawnerDeps{
		OwnerAgentID: "owner",
		Launcher:     &fakeRuntimeLauncher{},
		Leases:       NewLeaseManager(),
		Budget: ResourceBudget{MaxSpawnedPeersPerTask: 1, MaxWarmPeers: 1, MaxConcurrentAgents: 2},
	})

	_, _, err := spawner.Acquire(context.Background(), "peer-a", AgentIdentity{AgentID: "agent-a"}, SpawnSpec{})
	if err == nil {
		t.Fatal("expected non-owner spawn request to be rejected")
	}
}

func TestSpawnerExpiresIdleLeaseAndStopsRuntime(t *testing.T) {
	launcher := &fakeRuntimeLauncher{}
	spawner := NewSpawner(SpawnerDeps{
		OwnerAgentID: "owner",
		Launcher:     launcher,
		Leases:       NewLeaseManager(),
		Budget: ResourceBudget{
			MaxSpawnedPeersPerTask: 1,
			MaxWarmPeers:           1,
			MaxConcurrentAgents:    2,
			MaxLeaseLifetime:       time.Hour,
		},
	})
	now := time.Date(2026, 4, 10, 12, 0, 0, 0, time.UTC)

	lease, _, err := spawner.Acquire(context.Background(), "owner", AgentIdentity{AgentID: "agent-a"}, SpawnSpec{
		StartedAt:    now,
		LastUsedAt:   now,
		IdleTTL:      time.Minute,
		MaxLifetime:  time.Hour,
	})
	if err != nil {
		t.Fatalf("acquire: %v", err)
	}
	if _, err := spawner.Release(lease.RuntimeID, now.Add(10*time.Second)); err != nil {
		t.Fatalf("release: %v", err)
	}
	if err := spawner.Reap(context.Background(), now.Add(2*time.Minute)); err != nil {
		t.Fatalf("reap: %v", err)
	}
	if len(launcher.stops) != 1 || launcher.stops[0] != lease.RuntimeID {
		t.Fatalf("expected runtime stop for idle lease, got %#v", launcher.stops)
	}
}
