package telegram

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"time"
	"unicode"
	"unicode/utf8"

	"teamd/internal/approvals"
	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
)

func (a *Adapter) providerTools(role string) ([]provider.ToolDefinition, error) {
	out := make([]provider.ToolDefinition, 0)
	effective := a.effectivePolicy(0)
	if a.tools != nil {
		tools, err := a.tools.ListTools(role)
		if err != nil {
			return nil, err
		}
		for _, tool := range tools {
			if !effective.DecideTool(tool.Name).Allowed {
				continue
			}
			out = append(out, provider.ToolDefinition{
				Name:        providerToolName(tool.Name),
				Description: tool.Description,
				Parameters:  tool.Parameters,
			})
		}
	}
	if bundles, err := a.skillBundles(); err != nil {
		return nil, err
	} else if len(bundles) > 0 {
		out = append(out,
			provider.ToolDefinition{
				Name:        skillsToolListName,
				Description: "List available skills with names and descriptions",
				Parameters:  map[string]any{"type": "object"},
			},
			provider.ToolDefinition{
				Name:        skillsToolReadName,
				Description: "Read one skill by name",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"name": map[string]any{"type": "string"},
					},
					"required": []string{"name"},
				},
			},
			provider.ToolDefinition{
				Name:        skillsToolActivateName,
				Description: "Activate one skill by name and load its full instructions with bundled resources",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"name": map[string]any{"type": "string"},
					},
					"required": []string{"name"},
				},
			},
		)
	}
	if a.memory != nil {
		out = append(out,
			provider.ToolDefinition{
				Name:        memoryToolSearchName,
				Description: "Search recalled memory from previous sessions, checkpoints, and continuity documents",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"query": map[string]any{"type": "string"},
						"limit": map[string]any{"type": "integer"},
					},
					"required": []string{"query"},
				},
			},
			provider.ToolDefinition{
				Name:        memoryToolReadName,
				Description: "Read one recalled memory document by doc_key",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"doc_key": map[string]any{"type": "string"},
					},
					"required": []string{"doc_key"},
				},
			},
		)
	}
	if a.jobControl != nil {
		out = append(out,
			provider.ToolDefinition{
				Name:        jobStartToolName,
				Description: "Start a detached background job in the current workspace",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"command": map[string]any{"type": "string"},
						"args":    map[string]any{"type": "array", "items": map[string]any{"type": "string"}},
						"cwd":     map[string]any{"type": "string"},
					},
					"required": []string{"command"},
				},
			},
			provider.ToolDefinition{
				Name:        jobStatusToolName,
				Description: "Read one background job status by job_id",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"job_id": map[string]any{"type": "string"},
					},
					"required": []string{"job_id"},
				},
			},
			provider.ToolDefinition{
				Name:        jobCancelToolName,
				Description: "Cancel one background job by job_id",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"job_id": map[string]any{"type": "string"},
					},
					"required": []string{"job_id"},
				},
			},
		)
	}
	if a.workerControl != nil {
		out = append(out,
			provider.ToolDefinition{
				Name:        agentSpawnToolName,
				Description: "Spawn a managed local worker with its own isolated worker session",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"prompt": map[string]any{"type": "string"},
					},
				},
			},
			provider.ToolDefinition{
				Name:        agentMessageToolName,
				Description: "Send a new message to an existing managed worker",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"worker_id": map[string]any{"type": "string"},
						"content":   map[string]any{"type": "string"},
					},
					"required": []string{"worker_id", "content"},
				},
			},
			provider.ToolDefinition{
				Name:        agentWaitToolName,
				Description: "Poll worker state, messages, and events without blocking",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"worker_id":      map[string]any{"type": "string"},
						"after_cursor":   map[string]any{"type": "integer"},
						"after_event_id": map[string]any{"type": "integer"},
					},
					"required": []string{"worker_id"},
				},
			},
		)
	}
	if a.agentCore != nil {
		out = append(out,
			provider.ToolDefinition{
				Name:        planCreateToolName,
				Description: "Create a persisted plan for the current run or worker",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"title":      map[string]any{"type": "string"},
						"owner_type": map[string]any{"type": "string"},
						"owner_id":   map[string]any{"type": "string"},
					},
					"required": []string{"title"},
				},
			},
			provider.ToolDefinition{
				Name:        planReplaceItemsToolName,
				Description: "Replace all items in a persisted plan",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"plan_id": map[string]any{"type": "string"},
						"items": map[string]any{
							"type": "array",
							"items": map[string]any{
								"type": "object",
								"properties": map[string]any{
									"item_id": map[string]any{"type": "string"},
									"content": map[string]any{"type": "string"},
									"status":  map[string]any{"type": "string"},
								},
							},
						},
					},
					"required": []string{"plan_id", "items"},
				},
			},
			provider.ToolDefinition{
				Name:        planAnnotateToolName,
				Description: "Append a note to a persisted plan",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"plan_id": map[string]any{"type": "string"},
						"note":    map[string]any{"type": "string"},
					},
					"required": []string{"plan_id", "note"},
				},
			},
			provider.ToolDefinition{
				Name:        planItemAddToolName,
				Description: "Append one item to the end of a persisted plan",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"plan_id": map[string]any{"type": "string"},
						"content": map[string]any{"type": "string"},
						"status":  map[string]any{"type": "string"},
					},
					"required": []string{"plan_id", "content"},
				},
			},
			provider.ToolDefinition{
				Name:        planItemInsertAfterToolName,
				Description: "Insert one plan item after an existing item",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"plan_id":       map[string]any{"type": "string"},
						"after_item_id": map[string]any{"type": "string"},
						"content":       map[string]any{"type": "string"},
						"status":        map[string]any{"type": "string"},
					},
					"required": []string{"plan_id", "after_item_id", "content"},
				},
			},
			provider.ToolDefinition{
				Name:        planItemUpdateToolName,
				Description: "Update one existing plan item without replacing the full plan",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"plan_id": map[string]any{"type": "string"},
						"item_id": map[string]any{"type": "string"},
						"content": map[string]any{"type": "string"},
						"status":  map[string]any{"type": "string"},
					},
					"required": []string{"plan_id", "item_id"},
				},
			},
			provider.ToolDefinition{
				Name:        planItemRemoveToolName,
				Description: "Remove one existing plan item",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"plan_id": map[string]any{"type": "string"},
						"item_id": map[string]any{"type": "string"},
					},
					"required": []string{"plan_id", "item_id"},
				},
			},
			provider.ToolDefinition{
				Name:        planItemStartToolName,
				Description: "Mark one plan item as in progress",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"plan_id": map[string]any{"type": "string"},
						"item_id": map[string]any{"type": "string"},
					},
					"required": []string{"plan_id", "item_id"},
				},
			},
			provider.ToolDefinition{
				Name:        planItemCompleteToolName,
				Description: "Mark one plan item as completed",
				Parameters: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"plan_id": map[string]any{"type": "string"},
						"item_id": map[string]any{"type": "string"},
					},
					"required": []string{"plan_id", "item_id"},
				},
			},
		)
	}
	if strings.TrimSpace(a.workspaceRoot) != "" {
		out = append(out, provider.ToolDefinition{
			Name:        projectCaptureRecentToolName,
			Description: "Create or update canonical project files from the recent completed work in this session",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"project_path": map[string]any{"type": "string"},
					"title":        map[string]any{"type": "string"},
				},
			},
		})
	}
	return out, nil
}

func (a *Adapter) executeTool(ctx context.Context, chatID int64, call provider.ToolCall) (string, error) {
	toolName := runtimeToolName(call.Name)
	switch toolName {
	case "skills.list":
		return a.executeSkillsListTool()
	case "skills.read":
		return a.executeSkillsReadTool(call)
	case "skills.activate":
		return a.executeSkillsActivateTool(chatID, call)
	case "memory.search":
		return a.executeMemorySearchTool(chatID, call)
	case "memory.read":
		return a.executeMemoryReadTool(call)
	case "job_start":
		return a.executeJobStartTool(ctx, chatID, call)
	case "job_status":
		return a.executeJobStatusTool(call)
	case "job_cancel":
		return a.executeJobCancelTool(call)
	case "agent_spawn":
		return a.executeAgentSpawnTool(ctx, chatID, call)
	case "agent_message":
		return a.executeAgentMessageTool(ctx, call)
	case "agent_wait":
		return a.executeAgentWaitTool(call)
	case planCreateToolName:
		return a.executePlanCreateTool(ctx, chatID, call)
	case planReplaceItemsToolName:
		return a.executePlanReplaceItemsTool(call)
	case planAnnotateToolName:
		return a.executePlanAnnotateTool(call)
	case planItemAddToolName:
		return a.executePlanItemAddTool(call)
	case planItemInsertAfterToolName:
		return a.executePlanItemInsertAfterTool(call)
	case planItemUpdateToolName:
		return a.executePlanItemUpdateTool(call)
	case planItemRemoveToolName:
		return a.executePlanItemRemoveTool(call)
	case planItemStartToolName:
		return a.executePlanItemStartTool(call)
	case planItemCompleteToolName:
		return a.executePlanItemCompleteTool(call)
	case projectCaptureRecentToolName:
		return a.executeProjectCaptureRecentTool(chatID, call)
	}
	if toolName == "shell.exec" {
		call = a.ensureShellCWD(call)
	}
	decision := a.effectivePolicy(chatID).DecideTool(toolName)
	if !decision.Allowed {
		return "tool denied: " + decision.Reason, nil
	}
	if decision.RequiresApproval && a.approvals != nil {
		return a.requestToolApproval(ctx, chatID, toolName, call)
	}
	if a.tools == nil {
		return "", fmt.Errorf("tool runtime is not configured")
	}

	runCtx := ctx
	if toolName == "shell.exec" && decision.Policy.Timeout > 0 {
		var cancel context.CancelFunc
		runCtx, cancel = context.WithTimeout(ctx, decision.Policy.Timeout)
		defer cancel()
	}
	result, err := a.tools.CallTool(runCtx, toolName, call.Arguments)
	if err != nil {
		return "tool error: " + err.Error(), nil
	}
	return limitToolOutput(result.Content, decision.Policy), nil
}

func (a *Adapter) ensureShellCWD(call provider.ToolCall) provider.ToolCall {
	if _, ok := call.Arguments["cwd"]; ok || strings.TrimSpace(a.workspaceRoot) == "" {
		return call
	}
	args := make(map[string]any, len(call.Arguments)+1)
	for k, v := range call.Arguments {
		args[k] = v
	}
	args["cwd"] = a.workspaceRoot
	call.Arguments = args
	return call
}

func (a *Adapter) requestToolApproval(ctx context.Context, chatID int64, toolName string, call provider.ToolCall) (string, error) {
	sessionID := a.meshSessionID(chatID)
	payload := map[string]any{
		"tool":      toolName,
		"session":   sessionID,
		"arguments": call.Arguments,
	}
	body, err := json.Marshal(payload)
	if err != nil {
		return "", err
	}
	record, err := a.approvals.Create(approvals.Request{
		WorkerID:   toolName,
		SessionID:  sessionID,
		Payload:    string(body),
		Reason:     toolName + " requires approval by action policy",
		TargetType: "run",
		TargetID:   activeRunID(a.runs, chatID),
	})
	if err != nil {
		return "", err
	}
	message := strings.Join([]string{
		"Approval required",
		"id: " + record.ID,
		"tool: " + toolName,
		"details: " + summarizeToolCall(call),
	}, "\n")
	if err := a.saveApprovalContinuation(chatID, record.ID, toolName, call); err != nil {
		return "", err
	}
	a.saveApprovalRequestedEvent(chatID, sessionID, record, toolName)
	if _, err := a.sendMessage(ctx, chatID, message, approvalKeyboard(record.ID)); err != nil {
		// Approval delivery is transport best-effort. The request itself is already
		// persisted and must remain operable through API/CLI even without Telegram.
	}
	defer a.deleteApprovalContinuation(record.ID)
	a.markRunWaitingApproval(chatID, record.ID)
	decision, err := a.approvals.Wait(ctx, record.ID)
	if err != nil {
		return "", err
	}
	a.markRunRunning(chatID)
	if decision.Status == approvals.StatusRejected {
		return "approval rejected: " + record.ID, nil
	}
	return a.executeApprovedTool(withChatID(ctx, chatID), toolName, call)
}

func (a *Adapter) executeApprovedTool(ctx context.Context, toolName string, call provider.ToolCall) (string, error) {
	decision := a.toolExecutionDecision(ctx, toolName)
	if !decision.Allowed {
		return "tool denied: " + decision.Reason, nil
	}
	chatID := callChatIDFromContext(ctx)
	switch toolName {
	case "skills.list":
		return a.executeSkillsListTool()
	case "skills.read":
		return a.executeSkillsReadTool(call)
	case "skills.activate":
		return a.executeSkillsActivateTool(chatID, call)
	case "memory.search":
		return a.executeMemorySearchTool(chatID, call)
	case "memory.read":
		return a.executeMemoryReadTool(call)
	case "job_start":
		return a.executeJobStartTool(ctx, chatID, call)
	case "job_status":
		return a.executeJobStatusTool(call)
	case "job_cancel":
		return a.executeJobCancelTool(call)
	case "agent_spawn":
		return a.executeAgentSpawnTool(ctx, chatID, call)
	case "agent_message":
		return a.executeAgentMessageTool(ctx, call)
	case "agent_wait":
		return a.executeAgentWaitTool(call)
	case planCreateToolName:
		return a.executePlanCreateTool(ctx, chatID, call)
	case planReplaceItemsToolName:
		return a.executePlanReplaceItemsTool(call)
	case planAnnotateToolName:
		return a.executePlanAnnotateTool(call)
	case planItemAddToolName:
		return a.executePlanItemAddTool(call)
	case planItemInsertAfterToolName:
		return a.executePlanItemInsertAfterTool(call)
	case planItemUpdateToolName:
		return a.executePlanItemUpdateTool(call)
	case planItemRemoveToolName:
		return a.executePlanItemRemoveTool(call)
	case planItemStartToolName:
		return a.executePlanItemStartTool(call)
	case planItemCompleteToolName:
		return a.executePlanItemCompleteTool(call)
	case projectCaptureRecentToolName:
		return a.executeProjectCaptureRecentTool(chatID, call)
	}
	if a.tools == nil {
		return "", fmt.Errorf("tool runtime is not configured")
	}
	runCtx := ctx
	if toolName == "shell.exec" && decision.Policy.Timeout > 0 {
		var cancel context.CancelFunc
		runCtx, cancel = context.WithTimeout(ctx, decision.Policy.Timeout)
		defer cancel()
	}
	result, err := a.tools.CallTool(runCtx, toolName, call.Arguments)
	if err != nil {
		return "tool error: " + err.Error(), nil
	}
	return limitToolOutput(result.Content, decision.Policy), nil
}

func (a *Adapter) toolExecutionDecision(ctx context.Context, toolName string) runtimex.ToolExecutionDecision {
	if allowed, ok := rawAllowedToolsFromContext(ctx); ok {
		policy := runtimex.NormalizeMCPPolicy(runtimex.MCPPolicy{
			Mode:           runtimex.MCPPolicyAllowlist,
			AllowedTools:   allowed,
			ShellTimeout:   a.effectivePolicy(callChatIDFromContext(ctx)).MCP.ShellTimeout,
			MaxOutputBytes: a.effectivePolicy(callChatIDFromContext(ctx)).MCP.MaxOutputBytes,
			MaxOutputLines: a.effectivePolicy(callChatIDFromContext(ctx)).MCP.MaxOutputLines,
		})
		return runtimex.EffectivePolicy{
			Summary: a.effectivePolicy(callChatIDFromContext(ctx)).Summary,
			MCP:     policy,
		}.DecideTool(toolName)
	}
	return a.effectivePolicy(callChatIDFromContext(ctx)).DecideTool(toolName)
}

type chatIDContextKey struct{}
type rawAllowedToolsContextKey struct{}

func withChatID(ctx context.Context, chatID int64) context.Context {
	return context.WithValue(ctx, chatIDContextKey{}, chatID)
}

func withRawAllowedTools(ctx context.Context, tools []string) context.Context {
	if len(tools) == 0 {
		return ctx
	}
	return context.WithValue(ctx, rawAllowedToolsContextKey{}, normalizeAllowedToolNames(tools))
}

func callChatIDFromContext(ctx context.Context) int64 {
	if ctx == nil {
		return 0
	}
	value, _ := ctx.Value(chatIDContextKey{}).(int64)
	return value
}

func rawAllowedToolsFromContext(ctx context.Context) ([]string, bool) {
	if ctx == nil {
		return nil, false
	}
	value, ok := ctx.Value(rawAllowedToolsContextKey{}).([]string)
	if !ok || len(value) == 0 {
		return nil, false
	}
	return append([]string(nil), value...), true
}

func limitToolOutput(content string, policy runtimex.MCPToolPolicy) string {
	if policy.MaxOutputLines > 0 {
		lines := strings.Split(content, "\n")
		if len(lines) > policy.MaxOutputLines {
			content = strings.Join(lines[:policy.MaxOutputLines], "\n") + "\n... output truncated by policy ..."
		}
	}
	if policy.MaxOutputBytes > 0 && len(content) > policy.MaxOutputBytes {
		content = truncateUTF8(content, policy.MaxOutputBytes) + "\n... output truncated by policy ..."
	}
	return content
}

func truncateUTF8(input string, maxBytes int) string {
	if maxBytes <= 0 || len(input) <= maxBytes {
		return input
	}
	out := make([]byte, 0, maxBytes)
	for _, r := range input {
		size := utf8.RuneLen(r)
		if size < 0 {
			size = 1
		}
		if len(out)+size > maxBytes {
			break
		}
		out = utf8.AppendRune(out, r)
	}
	return string(out)
}

func (a *Adapter) saveApprovalRequestedEvent(chatID int64, sessionID string, record approvals.Record, toolName string) {
	if a.runStore == nil {
		return
	}
	payload, err := json.Marshal(map[string]any{
		"approval_id": record.ID,
		"tool":        toolName,
		"reason":      record.Reason,
	})
	if err != nil {
		return
	}
	_ = a.runStore.SaveEvent(runtimex.RuntimeEvent{
		EntityType: "run",
		EntityID:   record.TargetID,
		ChatID:     chatID,
		SessionID:  sessionID,
		RunID:      record.TargetID,
		Kind:       "approval.requested",
		Payload:    payload,
		CreatedAt:  time.Now().UTC(),
	})
}

func (a *Adapter) saveApprovalContinuation(chatID int64, approvalID, toolName string, call provider.ToolCall) error {
	if a.runStore == nil {
		return nil
	}
	run, ok := a.runs.Active(chatID)
	if !ok {
		return nil
	}
	return a.runStore.SaveApprovalContinuation(runtimex.ApprovalContinuation{
		ApprovalID:    approvalID,
		RunID:         run.ID,
		ChatID:        chatID,
		SessionID:     a.meshSessionID(chatID),
		Query:         run.Query,
		ToolCallID:    call.ID,
		ToolName:      toolName,
		ToolArguments: cloneToolArguments(call.Arguments),
		RequestedAt:   time.Now().UTC(),
	})
}

func (a *Adapter) deleteApprovalContinuation(approvalID string) {
	if a.runStore == nil || strings.TrimSpace(approvalID) == "" {
		return
	}
	_ = a.runStore.DeleteApprovalContinuation(approvalID)
}

func providerToolName(name string) string {
	var b strings.Builder
	b.Grow(len(name))
	for _, r := range name {
		switch {
		case unicode.IsLetter(r), unicode.IsDigit(r), r == '_', r == '-':
			b.WriteRune(r)
		default:
			b.WriteRune('_')
		}
	}
	return b.String()
}

func activeRunID(runs *RunStateStore, chatID int64) string {
	if runs == nil {
		return ""
	}
	run, ok := runs.Active(chatID)
	if !ok || run == nil {
		return ""
	}
	return run.ID
}

func runtimeToolName(name string) string {
	switch name {
	case providerToolName("filesystem.read_file"):
		return "filesystem.read_file"
	case providerToolName("filesystem.write_file"):
		return "filesystem.write_file"
	case providerToolName("filesystem.list_dir"):
		return "filesystem.list_dir"
	case providerToolName("shell.exec"):
		return "shell.exec"
	case skillsToolListName:
		return "skills.list"
	case skillsToolReadName:
		return "skills.read"
	case skillsToolActivateName:
		return "skills.activate"
	case memoryToolSearchName:
		return "memory.search"
	case memoryToolReadName:
		return "memory.read"
	case jobStartToolName:
		return "job_start"
	case jobStatusToolName:
		return "job_status"
	case jobCancelToolName:
		return "job_cancel"
	case agentSpawnToolName:
		return "agent_spawn"
	case agentMessageToolName:
		return "agent_message"
	case agentWaitToolName:
		return "agent_wait"
	case planCreateToolName:
		return "plan_create"
	case planReplaceItemsToolName:
		return "plan_replace_items"
	case planAnnotateToolName:
		return "plan_annotate"
	case planItemAddToolName:
		return "plan_item_add"
	case planItemInsertAfterToolName:
		return "plan_item_insert_after"
	case planItemInsertBeforeToolName:
		return "plan_item_insert_before"
	case planItemUpdateToolName:
		return "plan_item_update"
	case planItemRemoveToolName:
		return "plan_item_remove"
	case planItemStartToolName:
		return "plan_item_start"
	case planItemCompleteToolName:
		return "plan_item_complete"
	default:
		return name
	}
}
