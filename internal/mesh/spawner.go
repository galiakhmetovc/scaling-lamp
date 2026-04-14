package mesh

import (
	"context"
	"fmt"
	"sync"
	"time"
)

type ResourceBudget struct {
	MaxSpawnedPeersPerTask int
	MaxWarmPeers           int
	MaxConcurrentAgents    int
	MaxLeaseLifetime       time.Duration
}

type StartRuntimeRequest struct {
	RuntimeID   string
	Identity    AgentIdentity
	IdleTTL     time.Duration
	MaxLifetime time.Duration
}

type SpawnSpec struct {
	StartedAt   time.Time
	LastUsedAt  time.Time
	IdleTTL     time.Duration
	MaxLifetime time.Duration
	Pinned      bool
}

type RuntimeLauncher interface {
	Start(context.Context, StartRuntimeRequest) error
	Stop(context.Context, string) error
}

type SpawnerDeps struct {
	OwnerAgentID string
	Launcher     RuntimeLauncher
	Leases       *LeaseManager
	Budget       ResourceBudget
}

type Spawner struct {
	ownerAgentID string
	launcher     RuntimeLauncher
	leases       *LeaseManager
	budget       ResourceBudget
	mu           sync.Mutex
	nextID       int
}

func NewSpawner(deps SpawnerDeps) *Spawner {
	return &Spawner{
		ownerAgentID: deps.OwnerAgentID,
		launcher:     deps.Launcher,
		leases:       deps.Leases,
		budget:       deps.Budget,
	}
}

func (s *Spawner) Acquire(ctx context.Context, requester string, identity AgentIdentity, spec SpawnSpec) (RuntimeLease, bool, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	if requester != s.ownerAgentID {
		return RuntimeLease{}, false, fmt.Errorf("only owner may spawn or assign runtimes")
	}

	now := spec.LastUsedAt
	if now.IsZero() {
		now = time.Now()
	}
	if lease, ok := s.leases.FindReusable(identity.AgentID, now); ok {
		return lease, true, nil
	}

	s.nextID++
	runtimeID := fmt.Sprintf("%s-%d", identity.AgentID, s.nextID)
	if spec.MaxLifetime == 0 {
		spec.MaxLifetime = s.budget.MaxLeaseLifetime
	}
	req := StartRuntimeRequest{
		RuntimeID:   runtimeID,
		Identity:    identity,
		IdleTTL:     spec.IdleTTL,
		MaxLifetime: spec.MaxLifetime,
	}
	if s.launcher != nil {
		if err := s.launcher.Start(ctx, req); err != nil {
			return RuntimeLease{}, false, err
		}
	}
	lease := s.leases.Start(identity, LeaseSpec{
		RuntimeID:   runtimeID,
		IdleTTL:     spec.IdleTTL,
		MaxLifetime: spec.MaxLifetime,
		StartedAt:   spec.StartedAt,
		LastUsedAt:  now,
		Pinned:      spec.Pinned,
	})
	return lease, false, nil
}

func (s *Spawner) Release(runtimeID string, at time.Time) (RuntimeLease, error) {
	return s.leases.MarkIdle(runtimeID, at)
}

func (s *Spawner) Reap(ctx context.Context, at time.Time) error {
	expired := s.leases.ExpireIdle(at)
	for _, lease := range expired {
		if s.launcher != nil {
			if err := s.launcher.Stop(ctx, lease.RuntimeID); err != nil {
				return err
			}
		}
	}
	return nil
}
