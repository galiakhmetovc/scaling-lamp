package mesh

import (
	"context"
	"testing"
)

func TestInMemoryIdentityRegistryPersistsAndRebindsSession(t *testing.T) {
	registry := NewInMemoryIdentityRegistry()
	ctx := context.Background()

	identity := AgentIdentity{
		AgentID:               "agent-a",
		MemoryNamespace:       "agent:agent-a:memory:v1",
		SessionNamespace:      "agent:agent-a:sessions:v1",
		PreferredModelProfile: "glm-4.5",
	}
	if err := registry.Save(ctx, identity); err != nil {
		t.Fatalf("save: %v", err)
	}
	if err := registry.BindSession(ctx, "agent-a", "session-123"); err != nil {
		t.Fatalf("bind session: %v", err)
	}

	got, ok, err := registry.Get(ctx, "agent-a")
	if err != nil {
		t.Fatalf("get: %v", err)
	}
	if !ok {
		t.Fatal("expected identity to exist")
	}
	if got.MemoryNamespace != identity.MemoryNamespace || got.SessionNamespace != identity.SessionNamespace {
		t.Fatalf("unexpected identity: %#v", got)
	}

	sessionID, ok, err := registry.ActiveSession(ctx, "agent-a")
	if err != nil {
		t.Fatalf("active session: %v", err)
	}
	if !ok || sessionID != "session-123" {
		t.Fatalf("unexpected active session: %q ok=%v", sessionID, ok)
	}
}

func TestInMemoryIdentityRegistryListsSavedIdentities(t *testing.T) {
	registry := NewInMemoryIdentityRegistry()
	ctx := context.Background()

	if err := registry.Save(ctx, AgentIdentity{AgentID: "agent-b"}); err != nil {
		t.Fatalf("save agent-b: %v", err)
	}
	if err := registry.Save(ctx, AgentIdentity{AgentID: "agent-a"}); err != nil {
		t.Fatalf("save agent-a: %v", err)
	}

	identities, err := registry.List(ctx)
	if err != nil {
		t.Fatalf("list: %v", err)
	}
	if len(identities) != 2 {
		t.Fatalf("expected 2 identities, got %d", len(identities))
	}
	if identities[0].AgentID != "agent-a" || identities[1].AgentID != "agent-b" {
		t.Fatalf("expected sorted identities, got %#v", identities)
	}
}
