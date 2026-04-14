package runtime

import (
	"context"
	"encoding/json"
	"fmt"

	"teamd/internal/contracts"
	"teamd/internal/filesystem"
	"teamd/internal/provider"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/plans"
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
)

type ToolActivityPhase string

const (
	ToolActivityPhaseStarted   ToolActivityPhase = "started"
	ToolActivityPhaseCompleted ToolActivityPhase = "completed"
)

type ToolActivity struct {
	Phase      ToolActivityPhase
	Name       string
	Arguments  map[string]any
	ResultText string
	ErrorText  string
}

func (a *Agent) executeProviderLoop(ctx context.Context, sessionID, runID, correlationID, source string, input provider.ClientInput, observer func(ToolActivity)) (provider.ClientResult, error) {
	currentMessages := append([]contracts.Message{}, input.Messages...)
	maxRounds := a.MaxToolRounds
	if maxRounds <= 0 {
		maxRounds = 4
	}
	for round := 0; round < maxRounds; round++ {
		assembledMessages, err := a.assemblePromptMessages(sessionID, append([]contracts.Message{}, currentMessages...))
		if err != nil {
			return provider.ClientResult{}, err
		}

		result, err := a.ProviderClient.Execute(ctx, a.Contracts, provider.ClientInput{
			PromptAssetSelection: input.PromptAssetSelection,
			Messages:             assembledMessages,
			Tools:                input.Tools,
			AttemptObserver:      input.AttemptObserver,
			StreamObserver:       input.StreamObserver,
		})
		if recordErr := a.recordProviderRequestEvent(ctx, runID, sessionID, correlationID, source, result.RequestBody); recordErr != nil {
			if err != nil {
				return provider.ClientResult{}, fmt.Errorf("%v; record provider request: %w", err, recordErr)
			}
			return provider.ClientResult{}, fmt.Errorf("record provider request: %w", recordErr)
		}
		if recordErr := a.recordTransportAttemptEvents(ctx, runID, sessionID, correlationID, result.TransportAttempts); recordErr != nil {
			if err != nil {
				return provider.ClientResult{}, fmt.Errorf("%v; record transport attempts: %w", err, recordErr)
			}
			return provider.ClientResult{}, fmt.Errorf("record transport attempts: %w", recordErr)
		}
		if err != nil {
			return result, err
		}
		if len(result.Provider.ToolCalls) == 0 {
			return result, nil
		}

		toolMessages, err := a.executeToolCalls(ctx, runID, sessionID, correlationID, source, result.Provider.ToolCalls, result.ToolDecisions, observer)
		if err != nil {
			return result, err
		}
		currentMessages = append(currentMessages, assistantToolCallMessage(result.Provider.ToolCalls))
		currentMessages = append(currentMessages, toolMessages...)
	}

	return provider.ClientResult{}, fmt.Errorf("provider tool loop exceeded %d rounds", maxRounds)
}

func (a *Agent) executeToolCalls(ctx context.Context, runID, sessionID, correlationID, source string, calls []provider.ToolCall, decisions []provider.ToolDecision, observer func(ToolActivity)) ([]contracts.Message, error) {
	activeProjection := a.activePlanProjection()
	service := plans.NewService(a.now, a.newID)
	filesystemExecutor := filesystem.NewExecutor()
	shellExecutor := shell.NewExecutor()

	decisionByTool := make(map[string]provider.ToolDecision, len(decisions))
	for _, decision := range decisions {
		decisionByTool[decision.ToolID] = decision
	}

	out := make([]contracts.Message, 0, len(calls))
	for _, call := range calls {
		if observer != nil {
			observer(ToolActivity{Phase: ToolActivityPhaseStarted, Name: call.Name, Arguments: call.Arguments})
		}
		if err := a.RecordEvent(ctx, eventing.Event{
			ID:            a.newID("evt-tool-call-started"),
			Kind:          eventing.EventToolCallStarted,
			OccurredAt:    a.now(),
			AggregateID:   runID,
			AggregateType: eventing.AggregateRun,
			CorrelationID: correlationID,
			CausationID:   runID,
			Source:        source,
			ActorID:       a.Config.ID,
			ActorType:     "agent",
			TraceSummary:  "tool call started",
			Payload: map[string]any{
				"session_id": sessionID,
				"tool_name":  call.Name,
				"arguments":  call.Arguments,
			},
		}); err != nil {
			return nil, fmt.Errorf("record tool call started: %w", err)
		}
		decision, ok := decisionByTool[call.Name]
		if !ok {
			resultText := toolErrorResult(call.Name, fmt.Errorf("tool call %q has no execution decision", call.Name))
			if err := a.recordToolCallCompleted(ctx, runID, sessionID, correlationID, source, call.Name, call.Arguments, resultText, "tool call has no execution decision"); err != nil {
				return nil, fmt.Errorf("record tool call completed: %w", err)
			}
			if observer != nil {
				observer(ToolActivity{Phase: ToolActivityPhaseCompleted, Name: call.Name, Arguments: call.Arguments, ErrorText: "tool call has no execution decision", ResultText: resultText})
			}
			out = append(out, contracts.Message{Role: "tool", Name: call.Name, ToolCallID: call.ID, Content: resultText})
			continue
		}
		if !decision.Decision.Allowed {
			reason := fmt.Sprintf("tool call %q denied: %s", call.Name, decision.Decision.Reason)
			resultText := toolErrorResult(call.Name, fmt.Errorf("%s", reason))
			if err := a.recordToolCallCompleted(ctx, runID, sessionID, correlationID, source, call.Name, call.Arguments, resultText, reason); err != nil {
				return nil, fmt.Errorf("record tool call completed: %w", err)
			}
			if observer != nil {
				observer(ToolActivity{Phase: ToolActivityPhaseCompleted, Name: call.Name, Arguments: call.Arguments, ErrorText: reason, ResultText: resultText})
			}
			out = append(out, contracts.Message{Role: "tool", Name: call.Name, ToolCallID: call.ID, Content: resultText})
			continue
		}
		if decision.Decision.ApprovalRequired {
			reason := fmt.Sprintf("tool call %q requires approval", call.Name)
			resultText := toolErrorResult(call.Name, fmt.Errorf("%s", reason))
			if err := a.recordToolCallCompleted(ctx, runID, sessionID, correlationID, source, call.Name, call.Arguments, resultText, reason); err != nil {
				return nil, fmt.Errorf("record tool call completed: %w", err)
			}
			if observer != nil {
				observer(ToolActivity{Phase: ToolActivityPhaseCompleted, Name: call.Name, Arguments: call.Arguments, ErrorText: reason, ResultText: resultText})
			}
			out = append(out, contracts.Message{Role: "tool", Name: call.Name, ToolCallID: call.ID, Content: resultText})
			continue
		}

		events, resultText, err := a.executeToolCommand(activeProjection, service, filesystemExecutor, shellExecutor, source, call)
		if err != nil {
			resultText := toolErrorResult(call.Name, err)
			if recordErr := a.recordToolCallCompleted(ctx, runID, sessionID, correlationID, source, call.Name, call.Arguments, resultText, err.Error()); recordErr != nil {
				return nil, fmt.Errorf("record tool call completed: %w", recordErr)
			}
			if observer != nil {
				observer(ToolActivity{Phase: ToolActivityPhaseCompleted, Name: call.Name, Arguments: call.Arguments, ErrorText: err.Error(), ResultText: resultText})
			}
			out = append(out, contracts.Message{
				Role:       "tool",
				Name:       call.Name,
				ToolCallID: call.ID,
				Content:    resultText,
			})
			continue
		}
		for _, event := range events {
			if event.CorrelationID == "" {
				event.CorrelationID = correlationID
			}
			if event.Source == "" {
				event.Source = source
			}
			if event.ActorID == "" {
				event.ActorID = a.Config.ID
			}
			if event.ActorType == "" {
				event.ActorType = "agent"
			}
			if err := a.RecordEvent(ctx, event); err != nil {
				return nil, fmt.Errorf("record plan event %q: %w", event.Kind, err)
			}
		}
		if err := a.recordToolCallCompleted(ctx, runID, sessionID, correlationID, source, call.Name, call.Arguments, resultText, ""); err != nil {
			return nil, fmt.Errorf("record tool call completed: %w", err)
		}
		if observer != nil {
			observer(ToolActivity{Phase: ToolActivityPhaseCompleted, Name: call.Name, Arguments: call.Arguments, ResultText: resultText})
		}
		out = append(out, contracts.Message{
			Role:       "tool",
			Name:       call.Name,
			ToolCallID: call.ID,
			Content:    resultText,
		})
	}
	return out, nil
}

func (a *Agent) recordToolCallCompleted(ctx context.Context, runID, sessionID, correlationID, source, toolName string, arguments map[string]any, resultText, errorText string) error {
	payload := map[string]any{
		"session_id": sessionID,
		"tool_name":  toolName,
		"arguments":  arguments,
	}
	if resultText != "" {
		payload["result_text"] = resultText
	}
	if errorText != "" {
		payload["error"] = errorText
	}
	return a.RecordEvent(ctx, eventing.Event{
		ID:            a.newID("evt-tool-call-completed"),
		Kind:          eventing.EventToolCallCompleted,
		OccurredAt:    a.now(),
		AggregateID:   runID,
		AggregateType: eventing.AggregateRun,
		CorrelationID: correlationID,
		CausationID:   runID,
		Source:        source,
		ActorID:       a.Config.ID,
		ActorType:     "agent",
		TraceSummary:  "tool call completed",
		Payload:       payload,
	})
}

func (a *Agent) executeToolCommand(activeProjection *projections.ActivePlanProjection, service *plans.Service, filesystemExecutor *filesystem.Executor, shellExecutor *shell.Executor, source string, call provider.ToolCall) ([]eventing.Event, string, error) {
	switch call.Name {
	case "init_plan", "add_task", "set_task_status", "add_task_note", "edit_task":
		if activeProjection == nil {
			return nil, "", fmt.Errorf("active plan projection is not registered")
		}
		return a.executePlanCommand(activeProjection.Snapshot(), service, source, call)
	case "fs_list", "fs_read_text", "fs_write_text", "fs_patch_text", "fs_mkdir", "fs_move", "fs_trash":
		resultText, err := filesystemExecutor.Execute(a.Contracts.FilesystemExecution, call.Name, call.Arguments)
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		return nil, resultText, nil
	case "shell_exec":
		resultText, err := shellExecutor.Execute(a.Contracts.ShellExecution, call.Name, call.Arguments)
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		return nil, resultText, nil
	default:
		return nil, "", fmt.Errorf("tool call %q is not implemented", call.Name)
	}
}

func (a *Agent) executePlanCommand(active projections.ActivePlanSnapshot, service *plans.Service, source string, call provider.ToolCall) ([]eventing.Event, string, error) {
	switch call.Name {
	case "init_plan":
		goal, err := stringArg(call.Arguments, "goal")
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		events, err := service.InitPlan(active, plans.InitPlanInput{Goal: goal, Source: source, ActorID: a.Config.ID})
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		planID, _ := events[len(events)-1].Payload["plan_id"].(string)
		return events, jsonString(map[string]any{"status": "ok", "tool": call.Name, "plan_id": planID, "goal": goal}), nil
	case "add_task":
		planID, _ := optionalStringArg(call.Arguments, "plan_id")
		description, err := stringArg(call.Arguments, "description")
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		parentTaskID, _ := optionalStringArg(call.Arguments, "parent_task_id")
		dependsOn, err := optionalStringSliceArg(call.Arguments, "depends_on")
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		events, err := service.AddTask(active, plans.AddTaskInput{
			PlanID:       planID,
			Description:  description,
			ParentTaskID: parentTaskID,
			DependsOn:    dependsOn,
			Source:       source,
			ActorID:      a.Config.ID,
		})
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		taskID, _ := events[0].Payload["task_id"].(string)
		return events, jsonString(map[string]any{"status": "ok", "tool": call.Name, "task_id": taskID, "description": description}), nil
	case "set_task_status":
		taskID, err := stringArg(call.Arguments, "task_id")
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		newStatus, err := stringArg(call.Arguments, "new_status")
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		blockedReason, _ := optionalStringArg(call.Arguments, "blocked_reason")
		events, err := service.SetTaskStatus(active, plans.SetTaskStatusInput{
			TaskID:        taskID,
			NewStatus:     newStatus,
			BlockedReason: blockedReason,
			Source:        source,
			ActorID:       a.Config.ID,
		})
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		return events, jsonString(map[string]any{"status": "ok", "tool": call.Name, "task_id": taskID, "new_status": newStatus}), nil
	case "add_task_note":
		taskID, err := stringArg(call.Arguments, "task_id")
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		noteText, err := stringArg(call.Arguments, "note_text")
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		events, err := service.AddTaskNote(active, plans.AddTaskNoteInput{
			TaskID:   taskID,
			NoteText: noteText,
			Source:   source,
			ActorID:  a.Config.ID,
		})
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		return events, jsonString(map[string]any{"status": "ok", "tool": call.Name, "task_id": taskID}), nil
	case "edit_task":
		taskID, err := stringArg(call.Arguments, "task_id")
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		newDescription, err := stringArg(call.Arguments, "new_description")
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		newDependsOn, err := optionalStringSliceArg(call.Arguments, "new_depends_on")
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		events, err := service.EditTask(active, plans.EditTaskInput{
			TaskID:         taskID,
			NewDescription: newDescription,
			NewDependsOn:   newDependsOn,
			Source:         source,
			ActorID:        a.Config.ID,
		})
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		return events, jsonString(map[string]any{"status": "ok", "tool": call.Name, "task_id": taskID}), nil
	}
	return nil, "", fmt.Errorf("tool call %q is not implemented", call.Name)
}

func assistantToolCallMessage(calls []provider.ToolCall) contracts.Message {
	toolCalls := make([]contracts.MessageToolCall, 0, len(calls))
	for _, call := range calls {
		args, _ := json.Marshal(call.Arguments)
		toolCalls = append(toolCalls, contracts.MessageToolCall{
			ID:   call.ID,
			Type: "function",
			Function: contracts.MessageToolFunction{
				Name:      call.Name,
				Arguments: string(args),
			},
		})
	}
	return contracts.Message{Role: "assistant", Content: "", ToolCalls: toolCalls}
}

func (a *Agent) activePlanProjection() *projections.ActivePlanProjection {
	for _, projection := range a.Projections {
		activePlan, ok := projection.(*projections.ActivePlanProjection)
		if ok {
			return activePlan
		}
	}
	return nil
}

func stringArg(args map[string]any, key string) (string, error) {
	value, ok := args[key]
	if !ok {
		return "", fmt.Errorf("missing required argument %q", key)
	}
	text, ok := value.(string)
	if !ok || text == "" {
		return "", fmt.Errorf("argument %q must be a non-empty string", key)
	}
	return text, nil
}

func optionalStringArg(args map[string]any, key string) (string, bool) {
	value, ok := args[key]
	if !ok || value == nil {
		return "", false
	}
	text, ok := value.(string)
	if !ok || text == "" {
		return "", false
	}
	return text, true
}

func optionalStringSliceArg(args map[string]any, key string) ([]string, error) {
	value, ok := args[key]
	if !ok || value == nil {
		return nil, nil
	}
	switch typed := value.(type) {
	case []string:
		return append([]string{}, typed...), nil
	case []any:
		out := make([]string, 0, len(typed))
		for _, item := range typed {
			text, ok := item.(string)
			if !ok {
				return nil, fmt.Errorf("argument %q must contain only strings", key)
			}
			out = append(out, text)
		}
		return out, nil
	default:
		return nil, fmt.Errorf("argument %q must be a string array", key)
	}
}

func jsonString(value map[string]any) string {
	body, err := json.Marshal(value)
	if err != nil {
		return `{"status":"error","error":"marshal"}`
	}
	return string(body)
}

func toolErrorResult(toolName string, err error) string {
	return jsonString(map[string]any{
		"status": "error",
		"tool":   toolName,
		"error":  err.Error(),
	})
}
