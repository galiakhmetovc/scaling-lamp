package delegation

import (
	"fmt"
	"strings"

	"teamd/internal/contracts"
	"teamd/internal/tools"
)

type DefinitionExecutor struct{}

func NewDefinitionExecutor() *DefinitionExecutor {
	return &DefinitionExecutor{}
}

func (e *DefinitionExecutor) Build(contract contracts.DelegationToolContract) ([]tools.Definition, error) {
	if e == nil {
		return nil, fmt.Errorf("delegation tool executor is nil")
	}
	if !contract.Catalog.Enabled {
		return nil, nil
	}
	if contract.Catalog.Strategy != "static_allowlist" {
		return nil, fmt.Errorf("unsupported delegation catalog strategy %q", contract.Catalog.Strategy)
	}
	if !contract.Description.Enabled {
		return nil, fmt.Errorf("delegation description policy must be enabled")
	}
	if contract.Description.Strategy != "static_builtin_descriptions" {
		return nil, fmt.Errorf("unsupported delegation description strategy %q", contract.Description.Strategy)
	}

	all := defaultDefinitions(contract.Description.Params)
	byID := make(map[string]tools.Definition, len(all))
	for _, definition := range all {
		byID[definition.ID] = definition
	}
	out := make([]tools.Definition, 0, len(contract.Catalog.Params.ToolIDs))
	for _, id := range contract.Catalog.Params.ToolIDs {
		definition, ok := byID[id]
		if !ok {
			return nil, fmt.Errorf("delegation tool %q is not defined", id)
		}
		out = append(out, definition)
	}
	return out, nil
}

func defaultDefinitions(params contracts.DelegationDescriptionParams) []tools.Definition {
	return []tools.Definition{
		{
			ID:          "delegate_spawn",
			Name:        "delegate_spawn",
			Description: delegateSpawnDescription(params),
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"delegate_id": map[string]any{"type": "string"},
					"backend":     map[string]any{"type": "string"},
					"prompt":      map[string]any{"type": "string"},
				},
				"required": []string{"prompt"},
			},
		},
		{
			ID:          "delegate_message",
			Name:        "delegate_message",
			Description: "Send a follow-up message to an existing delegate.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"delegate_id": map[string]any{"type": "string"},
					"content":     map[string]any{"type": "string"},
				},
				"required": []string{"delegate_id", "content"},
			},
		},
		{
			ID:          "delegate_wait",
			Name:        "delegate_wait",
			Description: delegateWaitDescription(params),
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"delegate_id":    map[string]any{"type": "string"},
					"after_cursor":   map[string]any{"type": "integer"},
					"after_event_id": map[string]any{"type": "integer"},
					"event_limit":    map[string]any{"type": "integer"},
				},
				"required": []string{"delegate_id"},
			},
		},
		{
			ID:          "delegate_close",
			Name:        "delegate_close",
			Description: "Close an existing delegate and stop accepting further delegated work on it.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"delegate_id": map[string]any{"type": "string"},
				},
				"required": []string{"delegate_id"},
			},
		},
		{
			ID:          "delegate_handoff",
			Name:        "delegate_handoff",
			Description: "Fetch the canonical handoff for a delegate, including summarized outcome, artifacts, and next-step recommendations.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"delegate_id": map[string]any{"type": "string"},
				},
				"required": []string{"delegate_id"},
			},
		},
	}
}

func delegateSpawnDescription(params contracts.DelegationDescriptionParams) string {
	parts := []string{"Spawn a delegate for bounded subagent work and receive a canonical delegate_id you can use for follow-up lifecycle calls."}
	if params.IncludeBackendHints {
		parts = append(parts, "Use backend to request a local_worker delegate today; future remote_mesh delegates will use the same contract.")
	}
	if params.IncludeLifecycleNotes {
		parts = append(parts, "Continue with delegate_message, delegate_wait, delegate_handoff, and delegate_close instead of assuming one-shot execution.")
	}
	if params.IncludeExamples {
		parts = append(parts, `Example: {"backend":"local_worker","prompt":"Investigate failing tests and report findings."}`)
	}
	return strings.Join(parts, " ")
}

func delegateWaitDescription(params contracts.DelegationDescriptionParams) string {
	parts := []string{"Poll a delegate for new messages, lifecycle events, artifact references, and an optional canonical handoff."}
	if params.IncludeLifecycleNotes {
		parts = append(parts, "Use after_cursor and after_event_id to fetch incremental updates instead of replaying the full delegate transcript every time.")
	}
	return strings.Join(parts, " ")
}
