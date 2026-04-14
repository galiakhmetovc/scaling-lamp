package runtime

import (
	"testing"

	"teamd/internal/provider"
)

func TestApplySessionOverridesMergesPolicies(t *testing.T) {
	baseRuntime := provider.RequestConfig{Model: "glm-5-turbo"}
	baseMemory := MemoryPolicy{Profile: "conservative", PromoteContinuity: true, AutomaticRecallKinds: []string{"continuity"}}
	baseAction := ActionPolicy{ApprovalRequiredTools: []string{"shell.exec"}}
	maxFacts := 7

	summary := ApplySessionOverrides("1001:default", baseRuntime, baseMemory, baseAction, SessionOverrides{
		SessionID: "1001:default",
		Runtime: provider.RequestConfig{
			Model: "glm-5.1",
		},
		MemoryPolicy: MemoryPolicyOverride{
			Profile:          "standard",
			MaxResolvedFacts: &maxFacts,
		},
		ActionPolicy: ActionPolicyOverride{
			ApprovalRequiredTools: []string{"shell.exec", "filesystem.write_file"},
		},
	})

	if summary.Runtime.Model != "glm-5.1" {
		t.Fatalf("unexpected runtime summary: %+v", summary.Runtime)
	}
	if summary.MemoryPolicy.Profile != "standard" || summary.MemoryPolicy.MaxResolvedFacts != 7 {
		t.Fatalf("unexpected memory policy: %+v", summary.MemoryPolicy)
	}
	if len(summary.ActionPolicy.ApprovalRequiredTools) != 2 {
		t.Fatalf("unexpected action policy: %+v", summary.ActionPolicy)
	}
	if !summary.HasOverrides {
		t.Fatal("expected overrides flag")
	}
}
