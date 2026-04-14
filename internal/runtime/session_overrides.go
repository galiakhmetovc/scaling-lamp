package runtime

import (
	"strings"

	"teamd/internal/provider"
)

func MergeRequestConfig(base, override provider.RequestConfig) provider.RequestConfig {
	out := base
	if strings.TrimSpace(override.Model) != "" {
		out.Model = override.Model
	}
	if strings.TrimSpace(override.ReasoningMode) != "" {
		out.ReasoningMode = override.ReasoningMode
	}
	if override.ClearThinking != nil {
		out.ClearThinking = override.ClearThinking
	}
	if override.Temperature != nil {
		out.Temperature = override.Temperature
	}
	if override.TopP != nil {
		out.TopP = override.TopP
	}
	if override.MaxTokens != nil {
		out.MaxTokens = override.MaxTokens
	}
	if override.DoSample != nil {
		out.DoSample = override.DoSample
	}
	if strings.TrimSpace(override.ResponseFormat) != "" {
		out.ResponseFormat = strings.TrimSpace(override.ResponseFormat)
	}
	return out
}

func MergeMemoryPolicy(base MemoryPolicy, override MemoryPolicyOverride) MemoryPolicy {
	out := NormalizeMemoryPolicy(base)
	if strings.TrimSpace(override.Profile) != "" {
		out.Profile = strings.TrimSpace(override.Profile)
	}
	if override.PromoteCheckpoint != nil {
		out.PromoteCheckpoint = *override.PromoteCheckpoint
	}
	if override.PromoteContinuity != nil {
		out.PromoteContinuity = *override.PromoteContinuity
	}
	if override.AutomaticRecallKinds != nil {
		out.AutomaticRecallKinds = append([]string(nil), override.AutomaticRecallKinds...)
	}
	if override.MaxDocumentBodyChars != nil {
		out.MaxDocumentBodyChars = *override.MaxDocumentBodyChars
	}
	if override.MaxResolvedFacts != nil {
		out.MaxResolvedFacts = *override.MaxResolvedFacts
	}
	return NormalizeMemoryPolicy(out)
}

func MergeActionPolicy(base ActionPolicy, override ActionPolicyOverride) ActionPolicy {
	out := NormalizeActionPolicy(base)
	if override.ApprovalRequiredTools != nil {
		out.ApprovalRequiredTools = append([]string(nil), override.ApprovalRequiredTools...)
	}
	return NormalizeActionPolicy(out)
}

func ApplySessionOverrides(sessionID string, runtimeConfig provider.RequestConfig, memoryPolicy MemoryPolicy, actionPolicy ActionPolicy, overrides SessionOverrides) RuntimeSummary {
	return RuntimeSummary{
		SessionID:    sessionID,
		Runtime:      MergeRequestConfig(runtimeConfig, overrides.Runtime),
		MemoryPolicy: MergeMemoryPolicy(memoryPolicy, overrides.MemoryPolicy),
		ActionPolicy: MergeActionPolicy(actionPolicy, overrides.ActionPolicy),
		HasOverrides: hasSessionOverrides(overrides),
		Overrides:    overrides,
	}
}

func hasSessionOverrides(overrides SessionOverrides) bool {
	if strings.TrimSpace(overrides.SessionID) == "" {
		return false
	}
	if strings.TrimSpace(overrides.Runtime.Model) != "" ||
		strings.TrimSpace(overrides.Runtime.ReasoningMode) != "" ||
		overrides.Runtime.ClearThinking != nil ||
		overrides.Runtime.Temperature != nil ||
		overrides.Runtime.TopP != nil ||
		overrides.Runtime.MaxTokens != nil ||
		overrides.Runtime.DoSample != nil ||
		strings.TrimSpace(overrides.Runtime.ResponseFormat) != "" {
		return true
	}
	if strings.TrimSpace(overrides.MemoryPolicy.Profile) != "" ||
		overrides.MemoryPolicy.PromoteCheckpoint != nil ||
		overrides.MemoryPolicy.PromoteContinuity != nil ||
		overrides.MemoryPolicy.AutomaticRecallKinds != nil ||
		overrides.MemoryPolicy.MaxDocumentBodyChars != nil ||
		overrides.MemoryPolicy.MaxResolvedFacts != nil {
		return true
	}
	if overrides.ActionPolicy.ApprovalRequiredTools != nil {
		return true
	}
	return false
}
