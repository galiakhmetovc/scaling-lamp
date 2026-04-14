package tools_test

import (
	"testing"

	"teamd/internal/contracts"
	"teamd/internal/tools"
)

func TestExecutionGateEvaluatesAllowlistApprovalAndSandbox(t *testing.T) {
	t.Parallel()

	gate := tools.NewExecutionGate()
	decision, err := gate.Evaluate(contracts.ToolExecutionContract{
		Access: contracts.ToolAccessPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ToolAccessParams{ToolIDs: []string{"shell.exec"}},
		},
		Approval: contracts.ToolApprovalPolicy{
			Enabled:  true,
			Strategy: "always_allow",
		},
		Sandbox: contracts.ToolSandboxPolicy{
			Enabled:  true,
			Strategy: "default_runtime",
			Params: contracts.ToolSandboxParams{
				AllowNetwork: false,
				Timeout:      "30s",
			},
		},
	}, "shell.exec")
	if err != nil {
		t.Fatalf("Evaluate returned error: %v", err)
	}
	if !decision.Allowed {
		t.Fatalf("decision.Allowed = false, want true")
	}
	if decision.ApprovalRequired {
		t.Fatalf("decision.ApprovalRequired = true, want false")
	}
	if decision.Sandbox.Timeout != "30s" {
		t.Fatalf("sandbox timeout = %q, want 30s", decision.Sandbox.Timeout)
	}
}

func TestExecutionGateDenyAllRejectsTool(t *testing.T) {
	t.Parallel()

	gate := tools.NewExecutionGate()
	decision, err := gate.Evaluate(contracts.ToolExecutionContract{
		Access: contracts.ToolAccessPolicy{
			Enabled:  true,
			Strategy: "deny_all",
		},
	}, "shell.exec")
	if err != nil {
		t.Fatalf("Evaluate returned error: %v", err)
	}
	if decision.Allowed {
		t.Fatalf("decision.Allowed = true, want false")
	}
	if decision.Reason == "" {
		t.Fatalf("decision.Reason is empty")
	}
}
