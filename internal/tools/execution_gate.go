package tools

import (
	"fmt"
	"slices"

	"teamd/internal/contracts"
)

type ExecutionDecision struct {
	Allowed          bool
	ApprovalRequired bool
	Reason           string
	Sandbox          contracts.ToolSandboxParams
}

type ExecutionGate struct{}

func NewExecutionGate() *ExecutionGate {
	return &ExecutionGate{}
}

func (g *ExecutionGate) Evaluate(contract contracts.ToolExecutionContract, toolID string) (ExecutionDecision, error) {
	if g == nil {
		return ExecutionDecision{}, fmt.Errorf("tool execution gate is nil")
	}
	allowed, reason, err := g.evaluateAccess(contract.Access, toolID)
	if err != nil {
		return ExecutionDecision{}, err
	}
	decision := ExecutionDecision{
		Allowed: allowed,
		Reason:  reason,
	}
	if !allowed {
		return decision, nil
	}
	approvalRequired, err := g.evaluateApproval(contract.Approval, toolID)
	if err != nil {
		return ExecutionDecision{}, err
	}
	sandbox, err := g.evaluateSandbox(contract.Sandbox)
	if err != nil {
		return ExecutionDecision{}, err
	}
	decision.ApprovalRequired = approvalRequired
	decision.Sandbox = sandbox
	return decision, nil
}

func (g *ExecutionGate) evaluateAccess(policy contracts.ToolAccessPolicy, toolID string) (bool, string, error) {
	if !policy.Enabled {
		return true, "", nil
	}
	switch policy.Strategy {
	case "deny_all":
		return false, "tool access denied by policy", nil
	case "static_allowlist":
		if slices.Contains(policy.Params.ToolIDs, toolID) {
			return true, "", nil
		}
		return false, "tool not present in access allowlist", nil
	default:
		return false, "", fmt.Errorf("unsupported tool access strategy %q", policy.Strategy)
	}
}

func (g *ExecutionGate) evaluateApproval(policy contracts.ToolApprovalPolicy, toolID string) (bool, error) {
	if !policy.Enabled {
		return false, nil
	}
	switch policy.Strategy {
	case "always_allow":
		return false, nil
	case "always_require":
		return true, nil
	case "require_for_destructive":
		return slices.Contains(policy.Params.DestructiveToolIDs, toolID), nil
	default:
		return false, fmt.Errorf("unsupported tool approval strategy %q", policy.Strategy)
	}
}

func (g *ExecutionGate) evaluateSandbox(policy contracts.ToolSandboxPolicy) (contracts.ToolSandboxParams, error) {
	if !policy.Enabled {
		return contracts.ToolSandboxParams{}, nil
	}
	switch policy.Strategy {
	case "default_runtime", "read_only", "workspace_write", "deny_exec":
		return policy.Params, nil
	default:
		return contracts.ToolSandboxParams{}, fmt.Errorf("unsupported tool sandbox strategy %q", policy.Strategy)
	}
}
