package runtime

import "strings"

type MemoryPolicy struct {
	Profile              string
	PromoteCheckpoint    bool
	PromoteContinuity    bool
	AutomaticRecallKinds []string
	MaxDocumentBodyChars int
	MaxResolvedFacts     int
}

func DefaultMemoryPolicy() MemoryPolicy {
	return MemoryPolicy{
		Profile:              "conservative",
		PromoteCheckpoint:    false,
		PromoteContinuity:    true,
		AutomaticRecallKinds: []string{"continuity"},
		MaxDocumentBodyChars: 600,
		MaxResolvedFacts:     3,
	}
}

func NormalizeMemoryPolicy(policy MemoryPolicy) MemoryPolicy {
	defaults := DefaultMemoryPolicy()
	if strings.TrimSpace(policy.Profile) == "" &&
		!policy.PromoteCheckpoint &&
		!policy.PromoteContinuity &&
		len(policy.AutomaticRecallKinds) == 0 &&
		policy.MaxDocumentBodyChars == 0 &&
		policy.MaxResolvedFacts == 0 {
		return defaults
	}
	if strings.TrimSpace(policy.Profile) == "" {
		policy.Profile = defaults.Profile
	}
	if policy.MaxDocumentBodyChars <= 0 {
		policy.MaxDocumentBodyChars = defaults.MaxDocumentBodyChars
	}
	if policy.MaxResolvedFacts <= 0 {
		policy.MaxResolvedFacts = defaults.MaxResolvedFacts
	}
	if len(policy.AutomaticRecallKinds) == 0 {
		policy.AutomaticRecallKinds = append([]string(nil), defaults.AutomaticRecallKinds...)
	}
	policy.AutomaticRecallKinds = normalizeMemoryPolicyKinds(policy.AutomaticRecallKinds)
	return policy
}

func normalizeMemoryPolicyKinds(kinds []string) []string {
	out := make([]string, 0, len(kinds))
	seen := map[string]struct{}{}
	for _, kind := range kinds {
		kind = strings.ToLower(strings.TrimSpace(kind))
		if kind == "" {
			continue
		}
		if _, ok := seen[kind]; ok {
			continue
		}
		seen[kind] = struct{}{}
		out = append(out, kind)
	}
	return out
}
