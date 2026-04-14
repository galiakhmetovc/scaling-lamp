package telegram

import (
	"encoding/json"
	"fmt"
	"strconv"
	"strings"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/mesh"
	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
	"teamd/internal/skills"
)

func approvalViewFromRecord(record approvals.Record) runtimex.ApprovalView {
	return runtimex.ApprovalView{
		ID:               record.ID,
		WorkerID:         record.WorkerID,
		SessionID:        record.SessionID,
		Payload:          record.Payload,
		Status:           record.Status,
		Reason:           record.Reason,
		TargetType:       record.TargetType,
		TargetID:         record.TargetID,
		RequestedAt:      record.RequestedAt,
		DecidedAt:        record.DecidedAt,
		DecisionUpdateID: record.DecisionUpdateID,
	}
}

func (a *Adapter) meshPolicy(chatID int64) mesh.OrchestrationPolicy {
	key := a.meshSessionID(chatID)
	a.meshPolicyMu.RLock()
	policy, ok := a.meshPolicies[key]
	a.meshPolicyMu.RUnlock()
	if ok {
		return policy
	}
	return mesh.DefaultPolicy()
}

func (a *Adapter) runtimeConfig(chatID int64) provider.RequestConfig {
	summary := a.runtimeSummary(chatID)
	return summary.Runtime
}

func (a *Adapter) memoryPolicyForChat(chatID int64) runtimex.MemoryPolicy {
	return a.runtimeSummary(chatID).MemoryPolicy
}

func (a *Adapter) actionPolicyForChat(chatID int64) runtimex.ActionPolicy {
	return a.runtimeSummary(chatID).ActionPolicy
}

func (a *Adapter) effectivePolicy(chatID int64) runtimex.EffectivePolicy {
	return runtimex.EffectivePolicyForSummary(a.runtimeSummary(chatID), a.mcpPolicy)
}

func (a *Adapter) sessionOverrides(chatID int64) (runtimex.SessionOverrides, bool) {
	if a.agentCore != nil {
		overrides, ok, err := a.agentCore.SessionOverrides(a.meshSessionID(chatID))
		if err != nil || !ok {
			return runtimex.SessionOverrides{}, false
		}
		return overrides, true
	}
	if a.runtimeAPI == nil {
		return runtimex.SessionOverrides{}, false
	}
	sessionID := a.meshSessionID(chatID)
	overrides, ok, err := a.runtimeAPI.SessionOverrides(sessionID)
	if err != nil || !ok {
		return runtimex.SessionOverrides{}, false
	}
	return overrides, true
}

func (a *Adapter) runtimeSummary(chatID int64) runtimex.RuntimeSummary {
	sessionID := a.meshSessionID(chatID)
	a.runtimeConfigMu.RLock()
	localRuntime := a.runtimeConfigs[sessionID]
	a.runtimeConfigMu.RUnlock()
	baseRuntime := runtimex.MergeRequestConfig(a.runtimeDefaults, localRuntime)
	if a.agentCore != nil {
		summary, err := a.agentCore.RuntimeSummary(sessionID)
		if err == nil {
			return summary
		}
		return runtimex.ApplySessionOverrides(sessionID, baseRuntime, a.memoryPolicy, a.actionPolicy, runtimex.SessionOverrides{SessionID: sessionID})
	}
	if a.runtimeAPI == nil {
		return runtimex.ApplySessionOverrides(sessionID, baseRuntime, a.memoryPolicy, a.actionPolicy, runtimex.SessionOverrides{SessionID: sessionID})
	}
	summary, err := a.runtimeAPI.RuntimeSummary(sessionID, baseRuntime, a.memoryPolicy, a.actionPolicy)
	if err != nil {
		return runtimex.ApplySessionOverrides(sessionID, baseRuntime, a.memoryPolicy, a.actionPolicy, runtimex.SessionOverrides{SessionID: sessionID})
	}
	return summary
}

func (a *Adapter) saveSessionOverrides(chatID int64, mutate func(*runtimex.SessionOverrides)) error {
	sessionID := a.meshSessionID(chatID)
	var (
		overrides runtimex.SessionOverrides
		err       error
	)
	if a.agentCore != nil {
		overrides, _, err = a.agentCore.SessionOverrides(sessionID)
	} else {
		overrides, _, err = a.runtimeAPI.SessionOverrides(sessionID)
	}
	if err != nil {
		return err
	}
	if strings.TrimSpace(overrides.SessionID) == "" {
		overrides.SessionID = sessionID
	}
	mutate(&overrides)
	overrides.UpdatedAt = time.Now().UTC()
	if a.agentCore != nil {
		return a.agentCore.SaveSessionOverrides(overrides)
	}
	return a.runtimeAPI.SaveSessionOverrides(overrides)
}

func (a *Adapter) clearSessionOverrides(chatID int64) error {
	if a.agentCore != nil {
		return a.agentCore.ClearSessionOverrides(a.meshSessionID(chatID))
	}
	if a.runtimeAPI == nil {
		return nil
	}
	return a.runtimeAPI.ClearSessionOverrides(a.meshSessionID(chatID))
}

func (a *Adapter) applyMemoryPolicyOverride(chatID int64, assignment string) error {
	parts := strings.SplitN(assignment, "=", 2)
	if len(parts) != 2 {
		return fmt.Errorf("invalid assignment %q", assignment)
	}
	key := strings.ToLower(strings.TrimSpace(parts[0]))
	value := strings.TrimSpace(parts[1])
	switch key {
	case "profile":
		return a.saveSessionOverrides(chatID, func(overrides *runtimex.SessionOverrides) {
			overrides.MemoryPolicy.Profile = value
		})
	case "promote_checkpoint":
		v := strings.EqualFold(value, "true") || strings.EqualFold(value, "on") || value == "1"
		return a.saveSessionOverrides(chatID, func(overrides *runtimex.SessionOverrides) {
			overrides.MemoryPolicy.PromoteCheckpoint = &v
		})
	case "promote_continuity":
		v := strings.EqualFold(value, "true") || strings.EqualFold(value, "on") || value == "1"
		return a.saveSessionOverrides(chatID, func(overrides *runtimex.SessionOverrides) {
			overrides.MemoryPolicy.PromoteContinuity = &v
		})
	case "recall_kinds":
		kinds := parseCSVValues(value)
		return a.saveSessionOverrides(chatID, func(overrides *runtimex.SessionOverrides) {
			overrides.MemoryPolicy.AutomaticRecallKinds = kinds
		})
	case "max_body_chars":
		v, err := strconv.Atoi(value)
		if err != nil {
			return err
		}
		return a.saveSessionOverrides(chatID, func(overrides *runtimex.SessionOverrides) {
			overrides.MemoryPolicy.MaxDocumentBodyChars = &v
		})
	case "max_resolved_facts":
		v, err := strconv.Atoi(value)
		if err != nil {
			return err
		}
		return a.saveSessionOverrides(chatID, func(overrides *runtimex.SessionOverrides) {
			overrides.MemoryPolicy.MaxResolvedFacts = &v
		})
	default:
		return fmt.Errorf("unsupported memory policy key %q", key)
	}
}

func (a *Adapter) setRuntimeConfig(chatID int64, cfg provider.RequestConfig) error {
	key := a.meshSessionID(chatID)
	a.runtimeConfigMu.Lock()
	a.runtimeConfigs[key] = cfg
	a.runtimeConfigMu.Unlock()
	if a.runtimeAPI == nil {
		return nil
	}
	return a.saveSessionOverrides(chatID, func(overrides *runtimex.SessionOverrides) {
		overrides.Runtime = cfg
	})
}

func (a *Adapter) formatRuntimeConfig(chatID int64) string {
	summary := a.runtimeSummary(chatID)
	cfg := summary.Runtime
	lines := []string{
		"Runtime config",
		"session: " + summary.SessionID,
		"model: " + valueOrUnknown(cfg.Model),
		"reasoning_mode: " + valueOrUnknown(cfg.ReasoningMode),
		"clear_thinking: " + boolPtrString(cfg.ClearThinking),
		"temperature: " + floatPtrString(cfg.Temperature),
		"top_p: " + floatPtrString(cfg.TopP),
		"max_tokens: " + intPtrString(cfg.MaxTokens),
		"has_overrides: " + strconv.FormatBool(summary.HasOverrides),
		"",
		a.formatActionPolicy(chatID),
		"",
		a.formatMemoryPolicy(chatID),
	}
	return strings.Join(lines, "\n")
}

func (a *Adapter) formatActionPolicy(chatID int64) string {
	policy := a.actionPolicyForChat(chatID)
	lines := []string{
		"Action policy",
		"approval_required_tools: " + joinOrUnknown(policy.ApprovalRequiredTools),
	}
	return strings.Join(lines, "\n")
}

func (a *Adapter) formatMemoryPolicy(chatID int64) string {
	policy := a.memoryPolicyForChat(chatID)
	lines := []string{
		"Memory policy",
		"profile: " + valueOrUnknown(policy.Profile),
		"promote_checkpoint: " + strconv.FormatBool(policy.PromoteCheckpoint),
		"promote_continuity: " + strconv.FormatBool(policy.PromoteContinuity),
		"recall_kinds: " + joinOrUnknown(policy.AutomaticRecallKinds),
		"max_body_chars: " + strconv.Itoa(policy.MaxDocumentBodyChars),
		"max_resolved_facts: " + strconv.Itoa(policy.MaxResolvedFacts),
	}
	return strings.Join(lines, "\n")
}

func (a *Adapter) formatPendingApprovals(chatID int64) string {
	if a.agentCore == nil && a.approvals == nil {
		return "Approvals\nservice: disabled"
	}
	var pending []runtimex.ApprovalView
	if a.agentCore != nil {
		pending = a.agentCore.ListApprovals(a.meshSessionID(chatID))
	} else {
		records := a.approvals.PendingBySession(a.meshSessionID(chatID))
		pending = make([]runtimex.ApprovalView, 0, len(records))
		for _, record := range records {
			pending = append(pending, approvalViewFromRecord(record))
		}
	}
	lines := []string{"Approvals", "session: " + a.meshSessionID(chatID)}
	if len(pending) == 0 {
		lines = append(lines, "pending: none")
		return strings.Join(lines, "\n")
	}
	lines = append(lines, "pending:")
	for _, record := range pending {
		line := "- " + record.ID + " | " + record.WorkerID
		if strings.TrimSpace(record.Reason) != "" {
			line += " | " + record.Reason
		}
		lines = append(lines, line)
	}
	return strings.Join(lines, "\n")
}

func (a *Adapter) markRunWaitingApproval(chatID int64, approvalID string) {
	a.runs.Update(chatID, func(run *RunState) {
		run.Stage = "Ожидаю approval"
		run.WaitingOn = "approval"
		run.LastProgressAt = time.Now().UTC()
		run.Trace = append(run.Trace, TraceEntry{
			Section: "Approval",
			Summary: "approval_requested",
			Payload: approvalID,
		})
	})
	if a.runStore == nil {
		return
	}
	if active, ok := a.runtimeAPI.ActiveRun(chatID); ok {
		_ = a.runStore.SaveRun(runtimex.RunRecord{
			RunID:     active.RunID,
			ChatID:    active.ChatID,
			SessionID: active.SessionID,
			Query:     active.Query,
			Status:    runtimex.StatusWaitingApproval,
			StartedAt: active.StartedAt,
		})
	}
}

func (a *Adapter) markRunRunning(chatID int64) {
	a.runs.Update(chatID, func(run *RunState) {
		run.Stage = "Продолжаю после approval"
		run.WaitingOn = "tool"
		run.LastProgressAt = time.Now().UTC()
	})
	if a.runStore == nil {
		return
	}
	if active, ok := a.runtimeAPI.ActiveRun(chatID); ok {
		_ = a.runStore.SaveRun(runtimex.RunRecord{
			RunID:     active.RunID,
			ChatID:    active.ChatID,
			SessionID: active.SessionID,
			Query:     active.Query,
			Status:    runtimex.StatusRunning,
			StartedAt: active.StartedAt,
		})
	}
}

func joinOrUnknown(values []string) string {
	if len(values) == 0 {
		return "unknown"
	}
	return strings.Join(values, ",")
}

func (a *Adapter) skillBundles() ([]skills.Bundle, error) {
	if a.skills == nil {
		return nil, nil
	}
	return a.skills.List()
}

func (a *Adapter) skillsCatalogPrompt() (string, error) {
	bundles, err := a.skillBundles()
	if err != nil {
		return "", err
	}
	return skills.ComposeCatalog(bundles), nil
}

func (a *Adapter) activeSkillsPrompt(chatID int64) (string, error) {
	if a.skills == nil {
		return "", nil
	}
	active := a.skillState.Active(a.meshSessionID(chatID))
	if len(active) == 0 {
		return "", nil
	}
	bundles := make([]skills.Bundle, 0, len(active))
	for _, name := range active {
		bundle, ok, err := a.skills.Get(name)
		if err != nil {
			return "", err
		}
		if ok {
			bundles = append(bundles, bundle)
		}
	}
	return skills.ComposePrompt(bundles), nil
}

func (a *Adapter) executeSkillsListTool() (string, error) {
	bundles, err := a.skillBundles()
	if err != nil {
		return "", err
	}
	return skills.ToolListCompact(bundles, 6), nil
}

func (a *Adapter) executeSkillsReadTool(call provider.ToolCall) (string, error) {
	if a.skills == nil {
		return "", fmt.Errorf("no skills catalog configured")
	}
	name, _ := call.Arguments["name"].(string)
	if strings.TrimSpace(name) == "" {
		return "", fmt.Errorf("skills.read requires name")
	}
	bundle, ok, err := a.skills.Get(name)
	if err != nil {
		return "", err
	}
	if !ok {
		return "", fmt.Errorf("unknown skill: %s", name)
	}
	body, err := json.Marshal(skills.ToolRead(bundle))
	if err != nil {
		return "", err
	}
	return string(body), nil
}

func (a *Adapter) executeSkillsActivateTool(chatID int64, call provider.ToolCall) (string, error) {
	if a.skills == nil {
		return "", fmt.Errorf("no skills catalog configured")
	}
	name, _ := call.Arguments["name"].(string)
	if strings.TrimSpace(name) == "" {
		return "", fmt.Errorf("skills.activate requires name")
	}
	bundle, ok, err := a.skills.Get(name)
	if err != nil {
		return "", err
	}
	if !ok {
		return "", fmt.Errorf("unknown skill: %s", name)
	}
	a.skillState.Activate(a.meshSessionID(chatID), bundle.Name)
	return skills.ToolActivate(bundle), nil
}

func (a *Adapter) setMeshPolicy(chatID int64, policy mesh.OrchestrationPolicy) {
	key := a.meshSessionID(chatID)
	a.meshPolicyMu.Lock()
	a.meshPolicies[key] = policy
	a.meshPolicyMu.Unlock()
}

func (a *Adapter) meshSessionID(chatID int64) string {
	session, err := a.store.ActiveSession(chatID)
	if err != nil || strings.TrimSpace(session) == "" {
		return fmt.Sprintf("%d:default", chatID)
	}
	return fmt.Sprintf("%d:%s", chatID, session)
}
