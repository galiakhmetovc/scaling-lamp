package runtime

import "strings"

type ActionPolicy struct {
	ApprovalRequiredTools []string
}

func DefaultActionPolicy() ActionPolicy {
	return ActionPolicy{
		ApprovalRequiredTools: []string{"shell.exec", "filesystem.write_file"},
	}
}

func NormalizeActionPolicy(policy ActionPolicy) ActionPolicy {
	if policy.ApprovalRequiredTools == nil {
		policy.ApprovalRequiredTools = append([]string(nil), DefaultActionPolicy().ApprovalRequiredTools...)
	}
	seen := map[string]struct{}{}
	out := make([]string, 0, len(policy.ApprovalRequiredTools))
	for _, tool := range policy.ApprovalRequiredTools {
		tool = strings.TrimSpace(strings.ToLower(tool))
		if tool == "" {
			continue
		}
		if _, ok := seen[tool]; ok {
			continue
		}
		seen[tool] = struct{}{}
		out = append(out, tool)
	}
	policy.ApprovalRequiredTools = out
	return policy
}

func (p ActionPolicy) RequiresApproval(toolName string) bool {
	toolName = strings.TrimSpace(strings.ToLower(toolName))
	for _, candidate := range NormalizeActionPolicy(p).ApprovalRequiredTools {
		if candidate == toolName {
			return true
		}
	}
	return false
}
