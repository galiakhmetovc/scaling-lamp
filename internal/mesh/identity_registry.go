package mesh

import (
	"context"
	"sort"
	"sync"
)

type IdentityRegistry interface {
	Save(ctx context.Context, identity AgentIdentity) error
	Get(ctx context.Context, agentID string) (AgentIdentity, bool, error)
	BindSession(ctx context.Context, agentID, sessionID string) error
	ActiveSession(ctx context.Context, agentID string) (string, bool, error)
	List(ctx context.Context) ([]AgentIdentity, error)
}

type InMemoryIdentityRegistry struct {
	mu         sync.RWMutex
	identities map[string]AgentIdentity
	sessions   map[string]string
}

func NewInMemoryIdentityRegistry() *InMemoryIdentityRegistry {
	return &InMemoryIdentityRegistry{
		identities: map[string]AgentIdentity{},
		sessions:   map[string]string{},
	}
}

func (r *InMemoryIdentityRegistry) Save(_ context.Context, identity AgentIdentity) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.identities[identity.AgentID] = identity
	return nil
}

func (r *InMemoryIdentityRegistry) Get(_ context.Context, agentID string) (AgentIdentity, bool, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	identity, ok := r.identities[agentID]
	return identity, ok, nil
}

func (r *InMemoryIdentityRegistry) BindSession(_ context.Context, agentID, sessionID string) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.sessions[agentID] = sessionID
	return nil
}

func (r *InMemoryIdentityRegistry) ActiveSession(_ context.Context, agentID string) (string, bool, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	sessionID, ok := r.sessions[agentID]
	return sessionID, ok, nil
}

func (r *InMemoryIdentityRegistry) List(_ context.Context) ([]AgentIdentity, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	out := make([]AgentIdentity, 0, len(r.identities))
	for _, identity := range r.identities {
		out = append(out, identity)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].AgentID < out[j].AgentID })
	return out, nil
}
