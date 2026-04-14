package runtime

import (
	"testing"
	"time"
)

func TestResolveContextPolicyPrefersRunThenSessionThenGlobal(t *testing.T) {
	global := ContextPolicy{
		Transport: TransportPolicy{
			Enabled:  true,
			Strategy: "fixed",
			BaseURL:  "https://global.example",
			Path:     "/global",
		},
		RequestShape: RequestShapePolicy{
			Enabled:  true,
			Strategy: "fixed",
			Model:    "glm-global",
		},
		Tools: ToolPolicy{
			Enabled:      true,
			Strategy:     "allow_selected",
			AllowedTools: []string{"shell.exec"},
		},
	}
	session := ContextPolicy{
		Transport: TransportPolicy{
			Enabled:  true,
			Strategy: "fixed",
			BaseURL:  "https://session.example",
		},
		RequestShape: RequestShapePolicy{
			Enabled:  true,
			Strategy: "fixed",
			Model:    "glm-session",
		},
	}
	run := ContextPolicy{
		Transport: TransportPolicy{
			Enabled:  true,
			Strategy: "fixed",
			Path:     "/run",
			Timeout:  5 * time.Second,
		},
	}

	resolved := ResolveContextPolicy(global, session, run)

	if resolved.Transport.BaseURL != "https://session.example" {
		t.Fatalf("unexpected transport base url: %#v", resolved.Transport)
	}
	if resolved.Transport.Path != "/run" {
		t.Fatalf("unexpected transport path: %#v", resolved.Transport)
	}
	if resolved.Transport.Timeout != 5*time.Second {
		t.Fatalf("unexpected transport timeout: %#v", resolved.Transport)
	}
	if resolved.RequestShape.Model != "glm-session" {
		t.Fatalf("unexpected request shape model: %#v", resolved.RequestShape)
	}
	if len(resolved.Tools.AllowedTools) != 1 || resolved.Tools.AllowedTools[0] != "shell.exec" {
		t.Fatalf("unexpected allowed tools: %#v", resolved.Tools.AllowedTools)
	}
}

func TestResolveContextContractsMapsEffectivePolicies(t *testing.T) {
	resolved := ResolveContextPolicy(
		ContextPolicy{
			Transport: TransportPolicy{
				Enabled:  true,
				Strategy: "fixed",
				BaseURL:  "https://api.example",
				Path:     "/chat/completions",
			},
			RequestShape: RequestShapePolicy{
				Enabled:        true,
				Strategy:       "fixed",
				Model:          "glm-5-turbo",
				ReasoningMode:  "enabled",
				ResponseFormat: "json_object",
			},
			Tools: ToolPolicy{
				Enabled:      true,
				Strategy:     "allow_selected",
				AllowedTools: []string{"shell.exec"},
				AutoApprove:  true,
			},
		},
		ContextPolicy{},
		ContextPolicy{},
	)

	contracts := ResolveContextContracts(resolved)
	if contracts.ProviderRequest.Transport.BaseURL != "https://api.example" {
		t.Fatalf("unexpected provider request transport: %#v", contracts.ProviderRequest.Transport)
	}
	if contracts.ProviderRequest.RequestShape.Model != "glm-5-turbo" {
		t.Fatalf("unexpected provider request shape: %#v", contracts.ProviderRequest.RequestShape)
	}
	if contracts.Execution.Tools.AutoApprove != true {
		t.Fatalf("unexpected execution contract: %#v", contracts.Execution.Tools)
	}
}

func TestValidateContextPolicyRejectsMissingOffloadPreviewWhenForceOffloadEnabled(t *testing.T) {
	err := ValidateContextPolicy(ContextPolicy{
		Offload: OffloadPolicy{
			Enabled:           true,
			Strategy:          "tool_aware",
			OffloadLastResult: true,
			PreviewMode:       "none",
		},
	})
	if err == nil {
		t.Fatal("expected validation error")
	}
}
