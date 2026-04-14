package policies_test

import (
	"testing"

	"teamd/internal/policies"
)

func TestBuiltInRegistryExposesPolicyFamiliesAndStrategies(t *testing.T) {
	t.Parallel()

	registry := policies.NewBuiltInRegistry()

	endpointType, err := registry.Type("EndpointPolicyConfig")
	if err != nil {
		t.Fatalf("Type returned error: %v", err)
	}
	if endpointType.Family != policies.FamilyEndpoint {
		t.Fatalf("family = %q, want %q", endpointType.Family, policies.FamilyEndpoint)
	}
	if err := registry.ValidateStrategy("EndpointPolicyConfig", "static"); err != nil {
		t.Fatalf("ValidateStrategy returned error for built-in endpoint strategy: %v", err)
	}

	if err := registry.ValidateStrategy("RetryPolicyConfig", "exponential_jitter"); err != nil {
		t.Fatalf("ValidateStrategy returned error for built-in retry strategy: %v", err)
	}
	if err := registry.ValidateStrategy("RetryPolicyConfig", "does_not_exist"); err == nil {
		t.Fatal("ValidateStrategy error = nil, want error for unsupported retry strategy")
	}

	if err := registry.ValidateStrategy("SystemPromptPolicyConfig", "file_static"); err != nil {
		t.Fatalf("ValidateStrategy returned error for built-in system prompt strategy: %v", err)
	}
	if err := registry.ValidateStrategy("SessionHeadPolicyConfig", "projection_summary"); err != nil {
		t.Fatalf("ValidateStrategy returned error for built-in session head strategy: %v", err)
	}
	if err := registry.ValidateStrategy("ToolCatalogPolicyConfig", "static_allowlist"); err != nil {
		t.Fatalf("ValidateStrategy returned error for built-in tool catalog strategy: %v", err)
	}
	if err := registry.ValidateStrategy("ToolSerializationPolicyConfig", "openai_function_tools"); err != nil {
		t.Fatalf("ValidateStrategy returned error for built-in tool serialization strategy: %v", err)
	}
	if err := registry.ValidateStrategy("ToolAccessPolicyConfig", "deny_all"); err != nil {
		t.Fatalf("ValidateStrategy returned error for built-in tool access strategy: %v", err)
	}
	if err := registry.ValidateStrategy("ToolApprovalPolicyConfig", "always_allow"); err != nil {
		t.Fatalf("ValidateStrategy returned error for built-in tool approval strategy: %v", err)
	}
	if err := registry.ValidateStrategy("ToolSandboxPolicyConfig", "default_runtime"); err != nil {
		t.Fatalf("ValidateStrategy returned error for built-in tool sandbox strategy: %v", err)
	}
}
