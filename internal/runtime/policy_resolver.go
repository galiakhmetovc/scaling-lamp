package runtime

import (
	"fmt"
	"strings"
	"time"

	"teamd/internal/provider"
)

func DefaultMCPPolicy() MCPPolicy {
	return NormalizeMCPPolicy(MCPPolicy{
		Mode:           MCPPolicyAllowlist,
		AllowedTools:   []string{"filesystem.read_file", "filesystem.write_file", "filesystem.list_dir", "shell.exec"},
		ShellTimeout:   15 * time.Second,
		MaxOutputBytes: 16 * 1024,
		MaxOutputLines: 200,
	})
}

func NormalizeMCPPolicy(policy MCPPolicy) MCPPolicy {
	defaults := DefaultMCPPolicyValues()
	if policy.Mode == "" {
		policy.Mode = defaults.Mode
	}
	if len(policy.AllowedTools) == 0 {
		policy.AllowedTools = append([]string(nil), defaults.AllowedTools...)
	}
	if policy.ShellTimeout <= 0 {
		policy.ShellTimeout = defaults.ShellTimeout
	}
	if policy.MaxOutputBytes <= 0 {
		policy.MaxOutputBytes = defaults.MaxOutputBytes
	}
	if policy.MaxOutputLines <= 0 {
		policy.MaxOutputLines = defaults.MaxOutputLines
	}
	policy.AllowedTools = normalizeToolNames(policy.AllowedTools)
	return policy
}

func DefaultMCPPolicyValues() MCPPolicy {
	return MCPPolicy{
		Mode:           MCPPolicyAllowlist,
		AllowedTools:   []string{"filesystem.read_file", "filesystem.write_file", "filesystem.list_dir", "shell.exec"},
		ShellTimeout:   15 * time.Second,
		MaxOutputBytes: 16 * 1024,
		MaxOutputLines: 200,
	}
}

func ResolveEffectivePolicy(sessionID string, runtimeConfig provider.RequestConfig, memoryPolicy MemoryPolicy, actionPolicy ActionPolicy, mcpPolicy MCPPolicy, overrides SessionOverrides) EffectivePolicy {
	return EffectivePolicy{
		Summary: ApplySessionOverrides(sessionID, runtimeConfig, memoryPolicy, actionPolicy, overrides),
		MCP:     NormalizeMCPPolicy(mcpPolicy),
	}
}

func EffectivePolicyForSummary(summary RuntimeSummary, mcpPolicy MCPPolicy) EffectivePolicy {
	return EffectivePolicy{
		Summary: summary,
		MCP:     NormalizeMCPPolicy(mcpPolicy),
	}
}

func (p EffectivePolicy) DecideTool(toolName string) ToolExecutionDecision {
	toolName = strings.TrimSpace(strings.ToLower(toolName))
	if toolName == "" {
		return ToolExecutionDecision{Allowed: false, Reason: "tool name is required"}
	}
	normalized := NormalizeMCPPolicy(p.MCP)
	if normalized.Mode == MCPPolicyAllowlist && !containsTool(normalized.AllowedTools, toolName) {
		return ToolExecutionDecision{
			Allowed: false,
			Reason:  fmt.Sprintf("tool %q is not allowed by mcp policy", toolName),
		}
	}
	decision := ToolExecutionDecision{
		Allowed:          true,
		RequiresApproval: p.Summary.ActionPolicy.RequiresApproval(toolName),
		Policy: MCPToolPolicy{
			Name:           toolName,
			MaxOutputBytes: normalized.MaxOutputBytes,
			MaxOutputLines: normalized.MaxOutputLines,
		},
	}
	if toolName == "shell.exec" {
		decision.Policy.Timeout = normalized.ShellTimeout
	}
	return decision
}

func normalizeToolNames(items []string) []string {
	out := make([]string, 0, len(items))
	seen := map[string]struct{}{}
	for _, item := range items {
		item = strings.TrimSpace(strings.ToLower(item))
		if item == "" {
			continue
		}
		if _, ok := seen[item]; ok {
			continue
		}
		seen[item] = struct{}{}
		out = append(out, item)
	}
	return out
}

func containsTool(items []string, target string) bool {
	target = strings.TrimSpace(strings.ToLower(target))
	for _, item := range items {
		if item == target {
			return true
		}
	}
	return false
}
