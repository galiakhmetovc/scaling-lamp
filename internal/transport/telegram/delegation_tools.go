package telegram

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"

	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
)

func (a *Adapter) executeJobStartTool(ctx context.Context, chatID int64, call provider.ToolCall) (string, error) {
	if a.jobControl == nil {
		return "job tools unavailable", nil
	}
	command, _ := call.Arguments["command"].(string)
	command = strings.TrimSpace(command)
	if command == "" {
		return "job_start requires command", nil
	}
	args := stringArrayArg(call.Arguments["args"])
	cwd, _ := call.Arguments["cwd"].(string)
	job, err := a.jobControl.StartDetached(context.WithoutCancel(ctx), runtimex.JobStartRequest{
		ChatID:         chatID,
		SessionID:      a.meshSessionID(chatID),
		Command:        command,
		Args:           args,
		Cwd:            strings.TrimSpace(cwd),
		PolicySnapshot: runtimex.PolicySnapshotForSummary(a.runtimeSummary(chatID)),
	})
	if err != nil {
		return "", err
	}
	return formatJSON(map[string]any{
		"job_id":   job.JobID,
		"status":   job.Status,
		"command":  job.Command,
		"args":     job.Args,
		"session":  job.SessionID,
		"chat_id":  job.ChatID,
		"active":   job.Active,
	}), nil
}

func (a *Adapter) executeJobStatusTool(call provider.ToolCall) (string, error) {
	if a.jobControl == nil {
		return "job tools unavailable", nil
	}
	jobID, _ := call.Arguments["job_id"].(string)
	jobID = strings.TrimSpace(jobID)
	if jobID == "" {
		return "job_status requires job_id", nil
	}
	job, ok, err := a.jobControl.Job(jobID)
	if err != nil {
		return "", err
	}
	if !ok {
		return "job not found", nil
	}
	return formatJSON(job), nil
}

func (a *Adapter) executeJobCancelTool(call provider.ToolCall) (string, error) {
	if a.jobControl == nil {
		return "job tools unavailable", nil
	}
	jobID, _ := call.Arguments["job_id"].(string)
	jobID = strings.TrimSpace(jobID)
	if jobID == "" {
		return "job_cancel requires job_id", nil
	}
	ok, err := a.jobControl.Cancel(jobID)
	if err != nil {
		return "", err
	}
	if !ok {
		return "job not found", nil
	}
	return formatJSON(map[string]any{"ok": true, "job_id": jobID}), nil
}

func (a *Adapter) executeAgentSpawnTool(ctx context.Context, chatID int64, call provider.ToolCall) (string, error) {
	if a.workerControl == nil {
		return "agent tools unavailable", nil
	}
	prompt, _ := call.Arguments["prompt"].(string)
	worker, err := a.workerControl.Spawn(context.WithoutCancel(ctx), runtimex.WorkerSpawnRequest{
		ParentChatID:    chatID,
		ParentSessionID: a.meshSessionID(chatID),
		Prompt:          strings.TrimSpace(prompt),
		PolicySnapshot:  runtimex.PolicySnapshotForSummary(a.runtimeSummary(chatID)),
	})
	if err != nil {
		return "", err
	}
	return formatJSON(worker), nil
}

func (a *Adapter) executeAgentMessageTool(ctx context.Context, call provider.ToolCall) (string, error) {
	if a.workerControl == nil {
		return "agent tools unavailable", nil
	}
	workerID, _ := call.Arguments["worker_id"].(string)
	content, _ := call.Arguments["content"].(string)
	if strings.TrimSpace(workerID) == "" || strings.TrimSpace(content) == "" {
		return "agent_message requires worker_id and content", nil
	}
	worker, err := a.workerControl.Message(context.WithoutCancel(ctx), strings.TrimSpace(workerID), runtimex.WorkerMessageRequest{Content: strings.TrimSpace(content)})
	if err != nil {
		return "", err
	}
	return formatJSON(worker), nil
}

func (a *Adapter) executeAgentWaitTool(call provider.ToolCall) (string, error) {
	if a.workerControl == nil {
		return "agent tools unavailable", nil
	}
	workerID, _ := call.Arguments["worker_id"].(string)
	if strings.TrimSpace(workerID) == "" {
		return "agent_wait requires worker_id", nil
	}
	result, ok, err := a.workerControl.Wait(strings.TrimSpace(workerID), intArg(call.Arguments["after_cursor"]), int64Arg(call.Arguments["after_event_id"]), 50)
	if err != nil {
		return "", err
	}
	if !ok {
		return "worker not found", nil
	}
	return formatJSON(result), nil
}

func (a *Adapter) executePlanCreateTool(ctx context.Context, chatID int64, call provider.ToolCall) (string, error) {
	if a.agentCore == nil {
		return "plan tools unavailable", nil
	}
	title, _ := call.Arguments["title"].(string)
	ownerType, _ := call.Arguments["owner_type"].(string)
	ownerID, _ := call.Arguments["owner_id"].(string)
	if strings.TrimSpace(ownerType) == "" {
		ownerType = "run"
	}
	if strings.TrimSpace(ownerID) == "" {
		if runID := activeRunID(a.runs, chatID); strings.TrimSpace(runID) != "" {
			ownerID = runID
		} else {
			ownerType = "session"
			ownerID = a.meshSessionID(chatID)
		}
	}
	plan, err := a.agentCore.CreatePlan(ctx, strings.TrimSpace(ownerType), strings.TrimSpace(ownerID), strings.TrimSpace(title))
	if err != nil {
		return "", err
	}
	return formatJSON(plan), nil
}

func (a *Adapter) executePlanReplaceItemsTool(call provider.ToolCall) (string, error) {
	if a.agentCore == nil {
		return "plan tools unavailable", nil
	}
	planID, _ := call.Arguments["plan_id"].(string)
	items := planItemsArg(call.Arguments["items"])
	plan, err := a.agentCore.ReplacePlanItems(strings.TrimSpace(planID), items)
	if err != nil {
		return "", err
	}
	return formatJSON(plan), nil
}

func (a *Adapter) executePlanAnnotateTool(call provider.ToolCall) (string, error) {
	if a.agentCore == nil {
		return "plan tools unavailable", nil
	}
	planID, _ := call.Arguments["plan_id"].(string)
	note, _ := call.Arguments["note"].(string)
	plan, err := a.agentCore.AppendPlanNote(strings.TrimSpace(planID), strings.TrimSpace(note))
	if err != nil {
		return "", err
	}
	return formatJSON(plan), nil
}

func (a *Adapter) executePlanItemAddTool(call provider.ToolCall) (string, error) {
	if a.agentCore == nil {
		return "plan tools unavailable", nil
	}
	planID, _ := call.Arguments["plan_id"].(string)
	plan, err := a.agentCore.AddPlanItem(strings.TrimSpace(planID), planItemArg(call.Arguments))
	if err != nil {
		return "", err
	}
	return formatJSON(plan), nil
}

func (a *Adapter) executePlanItemInsertAfterTool(call provider.ToolCall) (string, error) {
	if a.agentCore == nil {
		return "plan tools unavailable", nil
	}
	planID, _ := call.Arguments["plan_id"].(string)
	afterItemID, _ := call.Arguments["after_item_id"].(string)
	plan, err := a.agentCore.InsertPlanItemAfter(strings.TrimSpace(planID), strings.TrimSpace(afterItemID), planItemArg(call.Arguments))
	if err != nil {
		return "", err
	}
	return formatJSON(plan), nil
}

func (a *Adapter) executePlanItemUpdateTool(call provider.ToolCall) (string, error) {
	if a.agentCore == nil {
		return "plan tools unavailable", nil
	}
	planID, _ := call.Arguments["plan_id"].(string)
	itemID, _ := call.Arguments["item_id"].(string)
	plan, err := a.agentCore.UpdatePlanItem(strings.TrimSpace(planID), strings.TrimSpace(itemID), planItemMutationArg(call.Arguments))
	if err != nil {
		return "", err
	}
	return formatJSON(plan), nil
}

func (a *Adapter) executePlanItemRemoveTool(call provider.ToolCall) (string, error) {
	if a.agentCore == nil {
		return "plan tools unavailable", nil
	}
	planID, _ := call.Arguments["plan_id"].(string)
	itemID, _ := call.Arguments["item_id"].(string)
	plan, err := a.agentCore.RemovePlanItem(strings.TrimSpace(planID), strings.TrimSpace(itemID))
	if err != nil {
		return "", err
	}
	return formatJSON(plan), nil
}

func (a *Adapter) executePlanItemStartTool(call provider.ToolCall) (string, error) {
	if a.agentCore == nil {
		return "plan tools unavailable", nil
	}
	planID, _ := call.Arguments["plan_id"].(string)
	itemID, _ := call.Arguments["item_id"].(string)
	plan, err := a.agentCore.StartPlanItem(strings.TrimSpace(planID), strings.TrimSpace(itemID))
	if err != nil {
		return "", err
	}
	return formatJSON(plan), nil
}

func (a *Adapter) executePlanItemCompleteTool(call provider.ToolCall) (string, error) {
	if a.agentCore == nil {
		return "plan tools unavailable", nil
	}
	planID, _ := call.Arguments["plan_id"].(string)
	itemID, _ := call.Arguments["item_id"].(string)
	plan, err := a.agentCore.CompletePlanItem(strings.TrimSpace(planID), strings.TrimSpace(itemID))
	if err != nil {
		return "", err
	}
	return formatJSON(plan), nil
}

func stringArrayArg(v any) []string {
	switch typed := v.(type) {
	case []string:
		return append([]string(nil), typed...)
	case []any:
		out := make([]string, 0, len(typed))
		for _, item := range typed {
			if s, ok := item.(string); ok && strings.TrimSpace(s) != "" {
				out = append(out, s)
			}
		}
		return out
	default:
		return nil
	}
}

func intArg(v any) int {
	switch typed := v.(type) {
	case int:
		return typed
	case float64:
		return int(typed)
	default:
		return 0
	}
}

func int64Arg(v any) int64 {
	switch typed := v.(type) {
	case int64:
		return typed
	case int:
		return int64(typed)
	case float64:
		return int64(typed)
	default:
		return 0
	}
}

func planItemArg(args map[string]any) runtimex.PlanItem {
	item := runtimex.PlanItem{}
	if text, ok := args["item_id"].(string); ok {
		item.ItemID = strings.TrimSpace(text)
	}
	if text, ok := args["content"].(string); ok {
		item.Content = strings.TrimSpace(text)
	}
	if text, ok := args["status"].(string); ok {
		item.Status = runtimex.PlanItemStatus(strings.TrimSpace(text))
	}
	return item
}

func planItemMutationArg(args map[string]any) runtimex.PlanItemMutation {
	patch := runtimex.PlanItemMutation{}
	if text, ok := args["item_id"].(string); ok {
		patch.ItemID = strings.TrimSpace(text)
	}
	if text, ok := args["content"].(string); ok {
		patch.Content = strings.TrimSpace(text)
	}
	if text, ok := args["status"].(string); ok {
		patch.Status = runtimex.PlanItemStatus(strings.TrimSpace(text))
	}
	return patch
}

func planItemsArg(v any) []runtimex.PlanItem {
	switch typed := v.(type) {
	case []runtimex.PlanItem:
		return append([]runtimex.PlanItem(nil), typed...)
	case []any:
		out := make([]runtimex.PlanItem, 0, len(typed))
		for _, item := range typed {
			switch value := item.(type) {
			case string:
				if strings.TrimSpace(value) != "" {
					out = append(out, runtimex.PlanItem{Content: value})
				}
			case map[string]any:
				out = append(out, planItemFromMap(value))
			}
		}
		return out
	default:
		return nil
	}
}

func planItemFromMap(v map[string]any) runtimex.PlanItem {
	item := runtimex.PlanItem{}
	if s, _ := v["item_id"].(string); strings.TrimSpace(s) != "" {
		item.ItemID = strings.TrimSpace(s)
	}
	if s, _ := v["content"].(string); strings.TrimSpace(s) != "" {
		item.Content = strings.TrimSpace(s)
	}
	if s, _ := v["status"].(string); strings.TrimSpace(s) != "" {
		item.Status = runtimex.PlanItemStatus(strings.TrimSpace(s))
	}
	return item
}

func formatJSON(v any) string {
	body, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		return fmt.Sprintf("%v", v)
	}
	return string(body)
}
