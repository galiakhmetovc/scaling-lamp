package runtime

import "testing"

func TestActionPolicyDefaultRequiresRiskyTools(t *testing.T) {
	policy := NormalizeActionPolicy(ActionPolicy{})
	if !policy.RequiresApproval("shell.exec") {
		t.Fatal("expected shell.exec to require approval")
	}
	if !policy.RequiresApproval("filesystem.write_file") {
		t.Fatal("expected filesystem.write_file to require approval")
	}
	if policy.RequiresApproval("filesystem.read_file") {
		t.Fatal("did not expect read-only tool to require approval")
	}
}

func TestActionPolicyEmptyListDisablesApprovals(t *testing.T) {
	policy := NormalizeActionPolicy(ActionPolicy{ApprovalRequiredTools: []string{}})
	if policy.RequiresApproval("shell.exec") {
		t.Fatal("did not expect shell.exec to require approval with explicit empty policy")
	}
	if policy.RequiresApproval("filesystem.write_file") {
		t.Fatal("did not expect filesystem.write_file to require approval with explicit empty policy")
	}
}
