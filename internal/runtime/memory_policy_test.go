package runtime

import "testing"

func TestNormalizeMemoryPolicyDefaults(t *testing.T) {
	policy := NormalizeMemoryPolicy(MemoryPolicy{})
	if policy.Profile != "conservative" {
		t.Fatalf("unexpected profile: %q", policy.Profile)
	}
	if policy.PromoteCheckpoint {
		t.Fatal("expected checkpoint promotion disabled by default")
	}
	if !policy.PromoteContinuity {
		t.Fatal("expected continuity promotion enabled by default")
	}
	if len(policy.AutomaticRecallKinds) != 1 || policy.AutomaticRecallKinds[0] != "continuity" {
		t.Fatalf("unexpected recall kinds: %#v", policy.AutomaticRecallKinds)
	}
}

func TestNormalizeMemoryPolicyKindsCanonicalized(t *testing.T) {
	policy := NormalizeMemoryPolicy(MemoryPolicy{
		AutomaticRecallKinds: []string{" continuity ", "checkpoint", "continuity", ""},
		MaxDocumentBodyChars: 10,
		MaxResolvedFacts:     2,
	})
	if got := len(policy.AutomaticRecallKinds); got != 2 {
		t.Fatalf("unexpected kinds count: %d", got)
	}
	if policy.AutomaticRecallKinds[0] != "continuity" || policy.AutomaticRecallKinds[1] != "checkpoint" {
		t.Fatalf("unexpected kinds: %#v", policy.AutomaticRecallKinds)
	}
}
