package runtime

import (
	"testing"
	"time"

	"teamd/internal/provider"
)

func TestResolveEffectivePolicyAppliesSessionOverrides(t *testing.T) {
	baseRuntime := provider.RequestConfig{Model: "glm-5-air"}
	baseMemory := MemoryPolicy{Profile: "conservative", PromoteContinuity: true}
	baseAction := ActionPolicy{ApprovalRequiredTools: []string{"shell.exec"}}
	baseMCP := MCPPolicy{
		Mode:           MCPPolicyAllowlist,
		AllowedTools:   []string{"shell.exec", "filesystem.read_file"},
		ShellTimeout:   15 * time.Second,
		MaxOutputBytes: 4096,
		MaxOutputLines: 80,
	}
	temperature := 0.2
	maxBody := 1200
	overrides := SessionOverrides{
		SessionID: "1001:debug",
		Runtime: provider.RequestConfig{
			Model:       "glm-5",
			Temperature: &temperature,
		},
		MemoryPolicy: MemoryPolicyOverride{
			Profile:              "aggressive",
			MaxDocumentBodyChars: &maxBody,
		},
		ActionPolicy: ActionPolicyOverride{
			ApprovalRequiredTools: []string{"filesystem.write_file"},
		},
	}

	effective := ResolveEffectivePolicy("1001:debug", baseRuntime, baseMemory, baseAction, baseMCP, overrides)

	if effective.Summary.Runtime.Model != "glm-5" {
		t.Fatalf("expected runtime override model, got %q", effective.Summary.Runtime.Model)
	}
	if effective.Summary.MemoryPolicy.Profile != "aggressive" {
		t.Fatalf("expected memory override profile, got %q", effective.Summary.MemoryPolicy.Profile)
	}
	if len(effective.Summary.ActionPolicy.ApprovalRequiredTools) != 1 || effective.Summary.ActionPolicy.ApprovalRequiredTools[0] != "filesystem.write_file" {
		t.Fatalf("expected action override, got %#v", effective.Summary.ActionPolicy.ApprovalRequiredTools)
	}
	if effective.MCP.MaxOutputBytes != 4096 || effective.MCP.ShellTimeout != 15*time.Second {
		t.Fatalf("unexpected mcp policy: %#v", effective.MCP)
	}
}

func TestEffectivePolicyDecideToolDeniesUnknownToolByDefault(t *testing.T) {
	effective := ResolveEffectivePolicy(
		"1001:default",
		provider.RequestConfig{Model: "glm-5"},
		DefaultMemoryPolicy(),
		DefaultActionPolicy(),
		DefaultMCPPolicy(),
		SessionOverrides{SessionID: "1001:default"},
	)

	decision := effective.DecideTool("unknown.tool")
	if decision.Allowed {
		t.Fatalf("expected unknown tool to be denied")
	}
	if decision.Reason == "" {
		t.Fatal("expected denial reason")
	}
}

func TestEffectivePolicyDecideToolCarriesApprovalAndLimits(t *testing.T) {
	effective := ResolveEffectivePolicy(
		"1001:default",
		provider.RequestConfig{Model: "glm-5"},
		DefaultMemoryPolicy(),
		ActionPolicy{ApprovalRequiredTools: []string{"shell.exec"}},
		MCPPolicy{
			Mode:           MCPPolicyAllowlist,
			AllowedTools:   []string{"shell.exec"},
			ShellTimeout:   30 * time.Second,
			MaxOutputBytes: 8192,
			MaxOutputLines: 100,
		},
		SessionOverrides{SessionID: "1001:default"},
	)

	decision := effective.DecideTool("shell.exec")
	if !decision.Allowed {
		t.Fatalf("expected shell.exec to be allowed")
	}
	if !decision.RequiresApproval {
		t.Fatalf("expected shell.exec to require approval")
	}
	if decision.Policy.Timeout != 30*time.Second || decision.Policy.MaxOutputBytes != 8192 || decision.Policy.MaxOutputLines != 100 {
		t.Fatalf("unexpected tool policy: %#v", decision.Policy)
	}
}
