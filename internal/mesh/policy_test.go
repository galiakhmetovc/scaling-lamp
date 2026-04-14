package mesh

import "testing"

func TestDefaultPolicyProfileValues(t *testing.T) {
	policy := DefaultPolicy()

	if policy.Profile != "direct" {
		t.Fatalf("expected direct profile, got %q", policy.Profile)
	}
	if policy.ClarificationMode != "off" {
		t.Fatalf("expected off clarification mode, got %q", policy.ClarificationMode)
	}
	if policy.MaxClarificationRounds != 1 {
		t.Fatalf("expected max clarification rounds 1, got %d", policy.MaxClarificationRounds)
	}
	if policy.ProposalMode != "off" {
		t.Fatalf("expected off proposal mode, got %q", policy.ProposalMode)
	}
	if policy.SampleK != 1 {
		t.Fatalf("expected sample_k 1, got %d", policy.SampleK)
	}
	if policy.ExecutionMode != "owner" {
		t.Fatalf("expected owner execution mode, got %q", policy.ExecutionMode)
	}
	if !policy.AllowToolExecution {
		t.Fatal("expected tool execution to be enabled")
	}
	if policy.CompositePlanning != "off" {
		t.Fatalf("expected off composite planning, got %q", policy.CompositePlanning)
	}
	if policy.JudgeMode != "owner" {
		t.Fatalf("expected owner judge mode, got %q", policy.JudgeMode)
	}
}

func TestPolicyProfileOverrides(t *testing.T) {
	policy, err := PolicyForProfile("deep")
	if err != nil {
		t.Fatalf("policy for profile: %v", err)
	}

	if policy.Profile != "deep" {
		t.Fatalf("expected deep profile, got %q", policy.Profile)
	}
	if policy.ClarificationMode != "sampled" {
		t.Fatalf("expected sampled clarification mode, got %q", policy.ClarificationMode)
	}
	if policy.ProposalMode != "all" {
		t.Fatalf("expected all proposal mode, got %q", policy.ProposalMode)
	}
	if policy.ExecutionMode != "winner" {
		t.Fatalf("expected winner execution mode, got %q", policy.ExecutionMode)
	}
	if policy.CompositePlanning != "auto" {
		t.Fatalf("expected auto composite planning, got %q", policy.CompositePlanning)
	}
}

func TestPolicyApplyOverride(t *testing.T) {
	policy := DefaultPolicy()

	updated, change, err := policy.ApplyOverride("sample_k", "4")
	if err != nil {
		t.Fatalf("apply override: %v", err)
	}
	if updated.SampleK != 4 {
		t.Fatalf("expected sample_k 4, got %d", updated.SampleK)
	}
	if change.Field != "sample_k" || change.OldValue != "1" || change.NewValue != "4" {
		t.Fatalf("unexpected change: %#v", change)
	}
}

func TestPolicyRejectsInvalidOverride(t *testing.T) {
	_, _, err := DefaultPolicy().ApplyOverride("execution_mode", "bogus")
	if err == nil {
		t.Fatal("expected invalid override error")
	}
}
