package mesh

import (
	"fmt"
	"sync"
	"time"
)

type RuntimeState string

const (
	RuntimeStateStarting RuntimeState = "starting"
	RuntimeStateWarm     RuntimeState = "warm"
	RuntimeStateIdle     RuntimeState = "idle"
	RuntimeStateDraining RuntimeState = "draining"
	RuntimeStateStopped  RuntimeState = "stopped"
)

type AgentIdentity struct {
	AgentID               string
	MemoryNamespace       string
	SessionNamespace      string
	PreferredModelProfile string
}

type RuntimeLease struct {
	RuntimeID    string
	AgentID      string
	StartedAt    time.Time
	LastUsedAt   time.Time
	IdleTTL      time.Duration
	MaxLifetime  time.Duration
	Pinned       bool
	State        RuntimeState
}

type LeaseSpec struct {
	RuntimeID   string
	IdleTTL     time.Duration
	MaxLifetime time.Duration
	StartedAt   time.Time
	LastUsedAt  time.Time
	Pinned      bool
}

type LeaseManager struct {
	mu     sync.Mutex
	leases map[string]RuntimeLease
}

func NewLeaseManager() *LeaseManager {
	return &LeaseManager{leases: map[string]RuntimeLease{}}
}

func (m *LeaseManager) Start(identity AgentIdentity, spec LeaseSpec) RuntimeLease {
	m.mu.Lock()
	defer m.mu.Unlock()

	startedAt := spec.StartedAt
	if startedAt.IsZero() {
		startedAt = time.Now()
	}
	lastUsedAt := spec.LastUsedAt
	if lastUsedAt.IsZero() {
		lastUsedAt = startedAt
	}
	lease := RuntimeLease{
		RuntimeID:   spec.RuntimeID,
		AgentID:     identity.AgentID,
		StartedAt:   startedAt,
		LastUsedAt:  lastUsedAt,
		IdleTTL:     spec.IdleTTL,
		MaxLifetime: spec.MaxLifetime,
		Pinned:      spec.Pinned,
		State:       RuntimeStateWarm,
	}
	m.leases[lease.RuntimeID] = lease
	return lease
}

func (m *LeaseManager) Assign(runtimeID string, at time.Time) (RuntimeLease, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	lease, ok := m.leases[runtimeID]
	if !ok {
		return RuntimeLease{}, fmt.Errorf("runtime %q not found", runtimeID)
	}
	if lease.State == RuntimeStateDraining || lease.State == RuntimeStateStopped {
		return RuntimeLease{}, fmt.Errorf("runtime %q is not assignable", runtimeID)
	}
	if at.IsZero() {
		at = time.Now()
	}
	lease.LastUsedAt = at
	lease.State = RuntimeStateWarm
	m.leases[runtimeID] = lease
	return lease, nil
}

func (m *LeaseManager) MarkIdle(runtimeID string, at time.Time) (RuntimeLease, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	lease, ok := m.leases[runtimeID]
	if !ok {
		return RuntimeLease{}, fmt.Errorf("runtime %q not found", runtimeID)
	}
	if at.IsZero() {
		at = time.Now()
	}
	lease.LastUsedAt = at
	if !lease.Pinned {
		lease.State = RuntimeStateIdle
	} else {
		lease.State = RuntimeStateWarm
	}
	m.leases[runtimeID] = lease
	return lease, nil
}

func (m *LeaseManager) BeginDrain(runtimeID string) (RuntimeLease, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	lease, ok := m.leases[runtimeID]
	if !ok {
		return RuntimeLease{}, fmt.Errorf("runtime %q not found", runtimeID)
	}
	if lease.State == RuntimeStateStopped {
		return RuntimeLease{}, fmt.Errorf("runtime %q already stopped", runtimeID)
	}
	lease.State = RuntimeStateDraining
	m.leases[runtimeID] = lease
	return lease, nil
}

func (m *LeaseManager) Stop(runtimeID string, at time.Time) (RuntimeLease, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	lease, ok := m.leases[runtimeID]
	if !ok {
		return RuntimeLease{}, fmt.Errorf("runtime %q not found", runtimeID)
	}
	if at.IsZero() {
		at = time.Now()
	}
	lease.LastUsedAt = at
	lease.State = RuntimeStateStopped
	m.leases[runtimeID] = lease
	return lease, nil
}

func (m *LeaseManager) FindReusable(agentID string, at time.Time) (RuntimeLease, bool) {
	m.mu.Lock()
	defer m.mu.Unlock()

	if at.IsZero() {
		at = time.Now()
	}
	for _, lease := range m.leases {
		if lease.AgentID != agentID {
			continue
		}
		if lease.State != RuntimeStateWarm && lease.State != RuntimeStateIdle {
			continue
		}
		if lease.MaxLifetime > 0 && at.Sub(lease.StartedAt) > lease.MaxLifetime {
			continue
		}
		if lease.State == RuntimeStateIdle && lease.IdleTTL > 0 && at.Sub(lease.LastUsedAt) > lease.IdleTTL {
			continue
		}
		lease.State = RuntimeStateWarm
		lease.LastUsedAt = at
		m.leases[lease.RuntimeID] = lease
		return lease, true
	}
	return RuntimeLease{}, false
}

func (m *LeaseManager) ExpireIdle(at time.Time) []RuntimeLease {
	m.mu.Lock()
	defer m.mu.Unlock()

	if at.IsZero() {
		at = time.Now()
	}
	expired := make([]RuntimeLease, 0)
	for runtimeID, lease := range m.leases {
		if lease.Pinned || lease.State != RuntimeStateIdle {
			continue
		}
		if lease.IdleTTL > 0 && at.Sub(lease.LastUsedAt) <= lease.IdleTTL {
			continue
		}
		lease.State = RuntimeStateStopped
		m.leases[runtimeID] = lease
		expired = append(expired, lease)
	}
	return expired
}
