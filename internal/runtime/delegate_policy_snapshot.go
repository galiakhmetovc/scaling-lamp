package runtime

import (
	"encoding/json"
	"fmt"

	"teamd/internal/contracts"
)

func (a *Agent) defaultDelegatePolicySnapshot() DelegatePolicySnapshot {
	return DelegatePolicySnapshot{
		Tools:               a.Contracts.Tools,
		FilesystemTools:     a.Contracts.FilesystemTools,
		FilesystemExecution: a.Contracts.FilesystemExecution,
		ShellTools:          a.Contracts.ShellTools,
		ShellExecution:      a.Contracts.ShellExecution,
		DelegationTools:     a.Contracts.DelegationTools,
		DelegationExecution: a.Contracts.DelegationExecution,
		PlanTools:           a.Contracts.PlanTools,
		ToolExecution:       a.Contracts.ToolExecution,
	}
}

func encodeDelegatePolicySnapshot(snapshot DelegatePolicySnapshot) (map[string]any, error) {
	body, err := json.Marshal(snapshot)
	if err != nil {
		return nil, fmt.Errorf("marshal delegate policy snapshot: %w", err)
	}
	var out map[string]any
	if err := json.Unmarshal(body, &out); err != nil {
		return nil, fmt.Errorf("decode delegate policy snapshot map: %w", err)
	}
	return out, nil
}

func decodeDelegatePolicySnapshot(raw map[string]any) (DelegatePolicySnapshot, error) {
	if raw == nil {
		return DelegatePolicySnapshot{}, nil
	}
	body, err := json.Marshal(raw)
	if err != nil {
		return DelegatePolicySnapshot{}, fmt.Errorf("marshal delegate policy snapshot map: %w", err)
	}
	var snapshot DelegatePolicySnapshot
	if err := json.Unmarshal(body, &snapshot); err != nil {
		return DelegatePolicySnapshot{}, fmt.Errorf("decode delegate policy snapshot: %w", err)
	}
	return snapshot, nil
}

func (s DelegatePolicySnapshot) Apply(base contracts.ResolvedContracts) contracts.ResolvedContracts {
	out := base
	out.Tools = s.Tools
	out.FilesystemTools = s.FilesystemTools
	out.FilesystemExecution = s.FilesystemExecution
	out.ShellTools = s.ShellTools
	out.ShellExecution = s.ShellExecution
	out.DelegationTools = s.DelegationTools
	out.DelegationExecution = s.DelegationExecution
	out.PlanTools = s.PlanTools
	out.ToolExecution = s.ToolExecution
	return out
}
