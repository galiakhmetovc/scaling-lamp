package daemon

import (
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"

	"gopkg.in/yaml.v3"

	"teamd/internal/contracts"
	"teamd/internal/runtime"
)

type ToolGovernanceSnapshot struct {
	AllowedTools          []string `json:"allowed_tools"`
	ApprovalRequiredTools []string `json:"approval_required_tools"`
	ApprovalMode          string   `json:"approval_mode"`
	ShellApprovalMode     string   `json:"shell_approval_mode"`
	ShellAllowPrefixes    []string `json:"shell_allow_prefixes,omitempty"`
	ShellDenyPrefixes     []string `json:"shell_deny_prefixes,omitempty"`
	ShellTimeout          string   `json:"shell_timeout,omitempty"`
	ShellMaxOutputBytes   int      `json:"shell_max_output_bytes,omitempty"`
	ShellAllowNetwork     bool     `json:"shell_allow_network"`
}

func buildToolGovernanceSnapshot(agent *runtime.Agent) ToolGovernanceSnapshot {
	toolExecution := agent.Contracts.ToolExecution
	shellExecution := agent.Contracts.ShellExecution
	approvalTools := append([]string(nil), toolExecution.Approval.Params.DestructiveToolIDs...)
	slices.Sort(approvalTools)
	allowedTools := append([]string(nil), toolExecution.Access.Params.ToolIDs...)
	slices.Sort(allowedTools)
	allowPrefixes := append([]string(nil), shellExecution.Approval.Params.AllowPrefixes...)
	slices.Sort(allowPrefixes)
	denyPrefixes := append([]string(nil), shellExecution.Approval.Params.DenyPrefixes...)
	slices.Sort(denyPrefixes)
	return ToolGovernanceSnapshot{
		AllowedTools:          allowedTools,
		ApprovalRequiredTools: approvalTools,
		ApprovalMode:          toolExecution.Approval.Strategy,
		ShellApprovalMode:     shellExecution.Approval.Strategy,
		ShellAllowPrefixes:    allowPrefixes,
		ShellDenyPrefixes:     denyPrefixes,
		ShellTimeout:          shellExecution.Runtime.Params.Timeout,
		ShellMaxOutputBytes:   shellExecution.Runtime.Params.MaxOutputBytes,
		ShellAllowNetwork:     shellExecution.Runtime.Params.AllowNetwork,
	}
}

func BuildToolGovernanceSnapshot(agent *runtime.Agent) ToolGovernanceSnapshot {
	return buildToolGovernanceSnapshot(agent)
}

func PersistShellApprovalRuleAndReload(configPath, action, prefix string) (*runtime.Agent, error) {
	prefix = strings.TrimSpace(prefix)
	if prefix == "" {
		return nil, fmt.Errorf("shell approval prefix is empty")
	}
	if action != "allow" && action != "deny" {
		return nil, fmt.Errorf("unsupported shell approval rule action %q", action)
	}
	root := filepath.Dir(configPath)
	policyPath := filepath.Join(root, "policies", "shell-execution", "approval.yaml")
	original, err := os.ReadFile(policyPath)
	if err != nil {
		return nil, fmt.Errorf("read shell approval policy: %w", err)
	}
	var doc shellApprovalPolicyDocument
	if err := yaml.Unmarshal(original, &doc); err != nil {
		return nil, fmt.Errorf("decode shell approval policy: %w", err)
	}
	allowPrefixes := append([]string(nil), doc.Spec.Params.AllowPrefixes...)
	denyPrefixes := append([]string(nil), doc.Spec.Params.DenyPrefixes...)
	allowPrefixes = removeString(allowPrefixes, prefix)
	denyPrefixes = removeString(denyPrefixes, prefix)
	switch action {
	case "allow":
		allowPrefixes = appendIfMissing(allowPrefixes, prefix)
	case "deny":
		denyPrefixes = appendIfMissing(denyPrefixes, prefix)
	}
	doc.Spec.Params.AllowPrefixes = allowPrefixes
	doc.Spec.Params.DenyPrefixes = denyPrefixes
	body, err := yaml.Marshal(doc)
	if err != nil {
		return nil, fmt.Errorf("encode shell approval policy: %w", err)
	}
	if err := os.WriteFile(policyPath, body, 0o644); err != nil {
		return nil, fmt.Errorf("write shell approval policy: %w", err)
	}
	reloaded, err := runtime.BuildAgent(configPath)
	if err != nil {
		_ = os.WriteFile(policyPath, original, 0o644)
		return nil, err
	}
	return reloaded, nil
}

type shellApprovalPolicyDocument struct {
	Kind    string `yaml:"kind"`
	Version string `yaml:"version"`
	ID      string `yaml:"id"`
	Spec    struct {
		Enabled  bool                          `yaml:"enabled"`
		Strategy string                        `yaml:"strategy"`
		Params   contracts.ShellApprovalParams `yaml:"params"`
	} `yaml:"spec"`
}

func shellApprovalPrefix(command string, args []string) string {
	command = strings.TrimSpace(command)
	if command == "" {
		return strings.TrimSpace(strings.Join(args, " "))
	}
	if extracted := daemonShellSnippetExecutable(command); extracted != "" {
		command = extracted
	}
	base := filepath.Base(command)
	if base == "." || base == string(filepath.Separator) {
		return command
	}
	return strings.TrimSpace(base)
}

func daemonShellSnippetExecutable(command string) string {
	command = strings.TrimSpace(command)
	if command == "" || !daemonIsShellSnippet(command) {
		return ""
	}
	separators := []string{"&&", "||", ";", "|", "\n"}
	start := 0
	for {
		segment := strings.TrimSpace(command[start:])
		if segment == "" {
			return ""
		}
		if strings.HasPrefix(segment, "cd ") || strings.HasPrefix(segment, "cd\t") {
			next := len(segment)
			for _, sep := range separators {
				if idx := strings.Index(segment, sep); idx >= 0 && idx < next {
					next = idx + len(sep)
				}
			}
			if next >= len(segment) {
				return ""
			}
			start = len(command) - len(segment) + next
			continue
		}
		return daemonFirstShellToken(segment)
	}
}

func daemonIsShellSnippet(command string) bool {
	return strings.Contains(command, "&&") ||
		strings.Contains(command, "||") ||
		strings.Contains(command, ";") ||
		strings.Contains(command, "\n") ||
		strings.Contains(command, "|")
}

func daemonFirstShellToken(command string) string {
	command = strings.TrimSpace(command)
	if command == "" {
		return ""
	}
	var (
		quote  rune
		escape bool
		token  []rune
	)
	for _, r := range command {
		if escape {
			token = append(token, r)
			escape = false
			continue
		}
		switch {
		case r == '\\':
			escape = true
		case quote != 0:
			if r == quote {
				quote = 0
				continue
			}
			token = append(token, r)
		case r == '\'' || r == '"':
			quote = r
		case r == ' ' || r == '\t':
			if len(token) > 0 {
				return string(token)
			}
		default:
			token = append(token, r)
		}
	}
	return string(token)
}

func removeString(values []string, target string) []string {
	out := make([]string, 0, len(values))
	for _, value := range values {
		if value == target {
			continue
		}
		out = append(out, value)
	}
	return out
}

func appendIfMissing(values []string, target string) []string {
	for _, value := range values {
		if value == target {
			return values
		}
	}
	return append(values, target)
}
