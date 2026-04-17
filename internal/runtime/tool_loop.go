package runtime

import (
	"context"
	"encoding/json"
	"fmt"
	"time"

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
	Phase      ToolActivityPhase `json:"phase"`
	Name       string            `json:"name"`
	Arguments  map[string]any    `json:"arguments,omitempty"`
	ResultText string            `json:"result_text,omitempty"`
	ErrorText  string            `json:"error_text,omitempty"`
}

func (a *Agent) executeProviderLoop(ctx context.Context, contractSet contracts.ResolvedContracts, sessionID, runID, correlationID, source string, input provider.ClientInput, observer func(ToolActivity), maxRoundsOverride int) (provider.ClientResult, error) {
	currentMessages := append([]contracts.Message{}, input.Messages...)
	streamObserver := input.StreamObserver
	input.StreamObserver = func(event provider.StreamEvent) {
		if streamObserver != nil {
			streamObserver(event)
		}
		if a.UIBus != nil && event.Kind == provider.StreamEventText {
			a.UIBus.Publish(UIEvent{Kind: UIEventStreamText, SessionID: sessionID, RunID: runID, Text: event.Text})
		}
	}
	maxRounds := a.MaxToolRounds
	if maxRoundsOverride > 0 {
		maxRounds = maxRoundsOverride
	}
	if maxRounds <= 0 {
		maxRounds = 4
	}
	for round := 0; round < maxRounds; round++ {
		assembledMessages, err := a.preparePromptMessages(ctx, contractSet, sessionID, append([]contracts.Message{}, currentMessages...), true)
		if err != nil {
			return provider.ClientResult{}, err
		}

		result, err := a.ProviderClient.Execute(ctx, contractSet, provider.ClientInput{
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

		toolMessages, err := a.executeToolCalls(ctx, contractSet, runID, sessionID, correlationID, source, result.Provider.ToolCalls, result.ToolDecisions, observer)
		if err != nil {
			return result, err
		}
		currentMessages = append(currentMessages, assistantToolCallMessage(result.Provider.ToolCalls))
		currentMessages = append(currentMessages, toolMessages...)
	}

	return provider.ClientResult{}, fmt.Errorf("provider tool loop exceeded %d rounds", maxRounds)
}

func (a *Agent) executeToolCalls(ctx context.Context, contractSet contracts.ResolvedContracts, runID, sessionID, correlationID, source string, calls []provider.ToolCall, decisions []provider.ToolDecision, observer func(ToolActivity)) ([]contracts.Message, error) {
	activeProjection := a.activePlanProjection()
	service := plans.NewService(a.now, a.newID)
	filesystemExecutor := filesystem.NewExecutor()
	shellExecutor := a.ShellRuntime
	if shellExecutor == nil {
		shellExecutor = shell.NewExecutor()
		a.ShellRuntime = shellExecutor
	}
	delegateRuntime := a.DelegateRuntime
	if delegateRuntime == nil {
		delegateRuntime = NewLocalDelegateRuntime(a)
		a.DelegateRuntime = delegateRuntime
	}

	decisionByTool := make(map[string]provider.ToolDecision, len(decisions))
	for _, decision := range decisions {
		decisionByTool[decision.ToolID] = decision
	}

	out := make([]contracts.Message, 0, len(calls))
	for _, call := range calls {
		if observer != nil {
			observer(ToolActivity{Phase: ToolActivityPhaseStarted, Name: call.Name, Arguments: call.Arguments})
		}
		if a.UIBus != nil {
			a.UIBus.Publish(UIEvent{Kind: UIEventToolStarted, SessionID: sessionID, RunID: runID, Tool: ToolActivity{Phase: ToolActivityPhaseStarted, Name: call.Name, Arguments: call.Arguments}})
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
			if err := a.recordToolCallCompleted(ctx, runID, sessionID, correlationID, source, call.Name, call.Arguments, resultText, "tool call has no execution decision", nil); err != nil {
				return nil, fmt.Errorf("record tool call completed: %w", err)
			}
			if observer != nil {
				observer(ToolActivity{Phase: ToolActivityPhaseCompleted, Name: call.Name, Arguments: call.Arguments, ErrorText: "tool call has no execution decision", ResultText: resultText})
			}
			if a.UIBus != nil {
				a.UIBus.Publish(UIEvent{Kind: UIEventToolCompleted, SessionID: sessionID, RunID: runID, Tool: ToolActivity{Phase: ToolActivityPhaseCompleted, Name: call.Name, Arguments: call.Arguments, ErrorText: "tool call has no execution decision", ResultText: resultText}})
			}
			out = append(out, contracts.Message{Role: "tool", Name: call.Name, ToolCallID: call.ID, Content: resultText})
			continue
		}
		if !decision.Decision.Allowed {
			reason := fmt.Sprintf("tool call %q denied: %s", call.Name, decision.Decision.Reason)
			resultText := toolErrorResult(call.Name, fmt.Errorf("%s", reason))
			if err := a.recordToolCallCompleted(ctx, runID, sessionID, correlationID, source, call.Name, call.Arguments, resultText, reason, nil); err != nil {
				return nil, fmt.Errorf("record tool call completed: %w", err)
			}
			if observer != nil {
				observer(ToolActivity{Phase: ToolActivityPhaseCompleted, Name: call.Name, Arguments: call.Arguments, ErrorText: reason, ResultText: resultText})
			}
			if a.UIBus != nil {
				a.UIBus.Publish(UIEvent{Kind: UIEventToolCompleted, SessionID: sessionID, RunID: runID, Tool: ToolActivity{Phase: ToolActivityPhaseCompleted, Name: call.Name, Arguments: call.Arguments, ErrorText: reason, ResultText: resultText}})
			}
			out = append(out, contracts.Message{Role: "tool", Name: call.Name, ToolCallID: call.ID, Content: resultText})
			continue
		}
		if decision.Decision.ApprovalRequired {
			reason := fmt.Sprintf("tool call %q requires approval", call.Name)
			resultText := toolErrorResult(call.Name, fmt.Errorf("%s", reason))
			if err := a.recordToolCallCompleted(ctx, runID, sessionID, correlationID, source, call.Name, call.Arguments, resultText, reason, nil); err != nil {
				return nil, fmt.Errorf("record tool call completed: %w", err)
			}
			if observer != nil {
				observer(ToolActivity{Phase: ToolActivityPhaseCompleted, Name: call.Name, Arguments: call.Arguments, ErrorText: reason, ResultText: resultText})
			}
			if a.UIBus != nil {
				a.UIBus.Publish(UIEvent{Kind: UIEventToolCompleted, SessionID: sessionID, RunID: runID, Tool: ToolActivity{Phase: ToolActivityPhaseCompleted, Name: call.Name, Arguments: call.Arguments, ErrorText: reason, ResultText: resultText}})
			}
			out = append(out, contracts.Message{Role: "tool", Name: call.Name, ToolCallID: call.ID, Content: resultText})
			continue
		}

		events, resultText, err := a.executeToolCommand(ctx, contractSet, runID, sessionID, activeProjection, service, filesystemExecutor, shellExecutor, delegateRuntime, source, call)
		if err != nil {
			resultText := toolErrorResult(call.Name, err)
			if recordErr := a.recordToolCallCompleted(ctx, runID, sessionID, correlationID, source, call.Name, call.Arguments, resultText, err.Error(), nil); recordErr != nil {
				return nil, fmt.Errorf("record tool call completed: %w", recordErr)
			}
			if observer != nil {
				observer(ToolActivity{Phase: ToolActivityPhaseCompleted, Name: call.Name, Arguments: call.Arguments, ErrorText: err.Error(), ResultText: resultText})
			}
			if a.UIBus != nil {
				a.UIBus.Publish(UIEvent{Kind: UIEventToolCompleted, SessionID: sessionID, RunID: runID, Tool: ToolActivity{Phase: ToolActivityPhaseCompleted, Name: call.Name, Arguments: call.Arguments, ErrorText: err.Error(), ResultText: resultText}})
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
		displayText, artifactRefs, err := a.maybeOffloadToolResult(ctx, contractSet, call.Name, resultText)
		if err != nil {
			return nil, fmt.Errorf("offload tool result: %w", err)
		}
		if err := a.recordToolCallCompleted(ctx, runID, sessionID, correlationID, source, call.Name, call.Arguments, displayText, "", artifactRefs); err != nil {
			return nil, fmt.Errorf("record tool call completed: %w", err)
		}
		if observer != nil {
			observer(ToolActivity{Phase: ToolActivityPhaseCompleted, Name: call.Name, Arguments: call.Arguments, ResultText: displayText})
		}
		if a.UIBus != nil {
			a.UIBus.Publish(UIEvent{Kind: UIEventToolCompleted, SessionID: sessionID, RunID: runID, Tool: ToolActivity{Phase: ToolActivityPhaseCompleted, Name: call.Name, Arguments: call.Arguments, ResultText: displayText}})
		}
		out = append(out, contracts.Message{
			Role:       "tool",
			Name:       call.Name,
			ToolCallID: call.ID,
			Content:    displayText,
		})
	}
	return out, nil
}

func (a *Agent) recordToolCallCompleted(ctx context.Context, runID, sessionID, correlationID, source, toolName string, arguments map[string]any, resultText, errorText string, artifactRefs []string) error {
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
		ArtifactRefs:  append([]string(nil), artifactRefs...),
		Payload:       payload,
	})
}

func (a *Agent) executeToolCommand(ctx context.Context, contractSet contracts.ResolvedContracts, runID, sessionID string, activeProjection *projections.ActivePlanProjection, service *plans.Service, filesystemExecutor *filesystem.Executor, shellExecutor *shell.Executor, delegateRuntime DelegateRuntime, source string, call provider.ToolCall) ([]eventing.Event, string, error) {
	switch call.Name {
	case "init_plan", "add_task", "set_task_status", "add_task_note", "edit_task", "plan_snapshot", "plan_lint":
		if activeProjection == nil {
			return nil, "", fmt.Errorf("active plan projection is not registered")
		}
		return a.executePlanCommand(sessionID, activeProjection.SnapshotForSession(sessionID), service, source, call)
	case "fs_list", "fs_read_text", "fs_read_lines", "fs_search_text", "fs_find_in_files", "fs_write_text", "fs_patch_text", "fs_replace_lines", "fs_replace_in_line", "fs_insert_text", "fs_replace_in_files", "fs_mkdir", "fs_move", "fs_trash":
		resultText, err := filesystemExecutor.Execute(contractSet.FilesystemExecution, call.Name, call.Arguments)
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		return nil, resultText, nil
	case "shell_exec", "shell_start", "shell_poll", "shell_kill":
		resultText, err := shellExecutor.ExecuteWithMeta(ctx, contractSet.ShellExecution, call.Name, call.Arguments, shell.ExecutionMeta{
			SessionID:   sessionID,
			RunID:       runID,
			Source:      source,
			ActorID:     a.Config.ID,
			ActorType:   "agent",
			RecordEvent: a.RecordEvent,
			Now:         a.now,
			NewID:       a.newID,
		})
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		return nil, resultText, nil
	case "delegate_spawn", "delegate_message", "delegate_wait", "delegate_close", "delegate_handoff":
		resultText, err := a.executeDelegationCommand(ctx, contractSet, runID, sessionID, delegateRuntime, call)
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		return nil, resultText, nil
	case "artifact_read", "artifact_search":
		resultText, err := a.executeArtifactCommand(ctx, contractSet, call.Name, call.Arguments)
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		return nil, resultText, nil
	default:
		return nil, "", fmt.Errorf("tool call %q is not implemented", call.Name)
	}
}

func (a *Agent) executeDelegationCommand(ctx context.Context, contractSet contracts.ResolvedContracts, runID, sessionID string, delegateRuntime DelegateRuntime, call provider.ToolCall) (string, error) {
	if delegateRuntime == nil {
		return "", fmt.Errorf("delegate runtime is not configured")
	}
	backendPolicy := contractSet.DelegationExecution.Backend
	resultPolicy := contractSet.DelegationExecution.Result

	switch call.Name {
	case "delegate_spawn":
		prompt, err := stringArg(call.Arguments, "prompt")
		if err != nil {
			return "", err
		}
		delegateID, _ := optionalStringArg(call.Arguments, "delegate_id")
		backendValue, _ := optionalStringArg(call.Arguments, "backend")
		if backendValue == "" {
			backendValue = backendPolicy.Params.DefaultBackend
		}
		if backendValue == "" {
			backendValue = string(DelegateBackendLocalWorker)
		}
		if !containsString(backendPolicy.Params.AllowedBackends, backendValue) {
			return "", fmt.Errorf("delegate backend %q is not allowed", backendValue)
		}
		view, err := delegateRuntime.Spawn(ctx, DelegateSpawnRequest{
			DelegateID:     delegateID,
			Backend:        DelegateBackend(backendValue),
			OwnerSessionID: sessionID,
			Prompt:         prompt,
			PolicySnapshot: mustDelegatePolicySnapshotMap(a, contractSet),
			Metadata: map[string]any{
				"owner_run_id":     runID,
				"owner_session_id": sessionID,
				"backend":          backendValue,
			},
		})
		if err != nil {
			return "", err
		}
		return jsonString(map[string]any{
			"status":      "ok",
			"tool":        call.Name,
			"delegate_id": view.DelegateID,
			"backend":     string(view.Backend),
			"state":       string(view.Status),
		}), nil
	case "delegate_message":
		delegateID, err := stringArg(call.Arguments, "delegate_id")
		if err != nil {
			return "", err
		}
		content, err := stringArg(call.Arguments, "content")
		if err != nil {
			return "", err
		}
		view, err := delegateRuntime.Message(ctx, delegateID, DelegateMessageRequest{Content: content})
		if err != nil {
			return "", err
		}
		return jsonString(map[string]any{
			"status":      "ok",
			"tool":        call.Name,
			"delegate_id": view.DelegateID,
			"state":       string(view.Status),
		}), nil
	case "delegate_wait":
		delegateID, err := stringArg(call.Arguments, "delegate_id")
		if err != nil {
			return "", err
		}
		afterCursor, err := optionalIntArg(call.Arguments, "after_cursor")
		if err != nil {
			return "", err
		}
		afterEventID, err := optionalInt64Arg(call.Arguments, "after_event_id")
		if err != nil {
			return "", err
		}
		eventLimit, err := optionalIntArg(call.Arguments, "event_limit")
		if err != nil {
			return "", err
		}
		if eventLimit <= 0 {
			eventLimit = resultPolicy.Params.DefaultEventLimit
		}
		if max := resultPolicy.Params.MaxEventLimit; max > 0 && eventLimit > max {
			eventLimit = max
		}
		result, ok, err := delegateRuntime.Wait(ctx, DelegateWaitRequest{
			DelegateID:   delegateID,
			AfterCursor:  afterCursor,
			AfterEventID: afterEventID,
			EventLimit:   eventLimit,
		})
		if err != nil {
			return "", err
		}
		if !ok {
			return "", fmt.Errorf("delegate %q not found", delegateID)
		}
		payload := map[string]any{
			"status":           "ok",
			"tool":             call.Name,
			"delegate_id":      result.Delegate.DelegateID,
			"backend":          string(result.Delegate.Backend),
			"state":            string(result.Delegate.Status),
			"messages":         delegateMessagesPayload(result.Messages),
			"next_cursor":      result.NextCursor,
			"next_event_after": result.NextEventAfter,
		}
		if resultPolicy.Params.IncludeEvents {
			payload["events"] = delegateEventsPayload(result.Events)
		}
		if result.Handoff != nil {
			payload["handoff"] = delegateHandoffPayload(*result.Handoff, resultPolicy.Params.IncludeArtifacts)
		}
		if resultPolicy.Params.IncludePolicySnapshot && result.Delegate.PolicySnapshot != nil {
			payload["policy_snapshot"] = result.Delegate.PolicySnapshot
		}
		if resultPolicy.Params.IncludeArtifacts {
			payload["artifacts"] = delegateArtifactsPayload(result.Delegate.ArtifactRefs)
		}
		return jsonString(payload), nil
	case "delegate_close":
		delegateID, err := stringArg(call.Arguments, "delegate_id")
		if err != nil {
			return "", err
		}
		view, ok, err := delegateRuntime.Close(ctx, delegateID)
		if err != nil {
			return "", err
		}
		if !ok {
			return "", fmt.Errorf("delegate %q not found", delegateID)
		}
		return jsonString(map[string]any{
			"status":      "ok",
			"tool":        call.Name,
			"delegate_id": view.DelegateID,
			"state":       string(view.Status),
		}), nil
	case "delegate_handoff":
		delegateID, err := stringArg(call.Arguments, "delegate_id")
		if err != nil {
			return "", err
		}
		handoff, ok, err := delegateRuntime.Handoff(ctx, delegateID)
		if err != nil {
			return "", err
		}
		if !ok {
			return "", fmt.Errorf("delegate %q has no handoff", delegateID)
		}
		payload := map[string]any{
			"status":      "ok",
			"tool":        call.Name,
			"delegate_id": handoff.DelegateID,
			"backend":     string(handoff.Backend),
			"summary":     handoff.Summary,
		}
		payload["handoff"] = delegateHandoffPayload(handoff, resultPolicy.Params.IncludeArtifacts)
		return jsonString(payload), nil
	default:
		return "", fmt.Errorf("tool call %q is not implemented", call.Name)
	}
}

func (a *Agent) executePlanCommand(sessionID string, active projections.ActivePlanSnapshot, service *plans.Service, source string, call provider.ToolCall) ([]eventing.Event, string, error) {
	head, _ := a.CurrentPlanHead(sessionID)
	switch call.Name {
	case "init_plan":
		if active.Plan.ID != "" {
			return nil, "", fmt.Errorf("tool call %q: active plan already exists; continue the active plan instead of reinitializing it", call.Name)
		}
		goal, err := stringArg(call.Arguments, "goal")
		if err != nil {
			return nil, "", fmt.Errorf("tool call %q: %w", call.Name, err)
		}
		events, err := service.InitPlan(active, plans.InitPlanInput{SessionID: sessionID, Goal: goal, Source: source, ActorID: a.Config.ID})
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
			SessionID:    sessionID,
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
			SessionID:     sessionID,
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
			SessionID: sessionID,
			TaskID:    taskID,
			NoteText:  noteText,
			Source:    source,
			ActorID:   a.Config.ID,
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
			SessionID:      sessionID,
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
	case "plan_snapshot":
		return nil, jsonString(buildPlanSnapshotPayload(active, head)), nil
	case "plan_lint":
		return nil, jsonString(buildPlanLintPayload(active, head)), nil
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

func optionalIntArg(args map[string]any, key string) (int, error) {
	value, ok := args[key]
	if !ok || value == nil {
		return 0, nil
	}
	switch typed := value.(type) {
	case int:
		return typed, nil
	case int64:
		return int(typed), nil
	case float64:
		return int(typed), nil
	default:
		return 0, fmt.Errorf("argument %q must be an integer", key)
	}
}

func optionalInt64Arg(args map[string]any, key string) (int64, error) {
	value, ok := args[key]
	if !ok || value == nil {
		return 0, nil
	}
	switch typed := value.(type) {
	case int:
		return int64(typed), nil
	case int64:
		return typed, nil
	case float64:
		return int64(typed), nil
	default:
		return 0, fmt.Errorf("argument %q must be an integer", key)
	}
}

func containsString(items []string, target string) bool {
	for _, item := range items {
		if item == target {
			return true
		}
	}
	return false
}

func delegateMessagesPayload(messages []DelegateMessage) []map[string]any {
	if len(messages) == 0 {
		return nil
	}
	out := make([]map[string]any, 0, len(messages))
	for _, message := range messages {
		out = append(out, map[string]any{
			"cursor":       message.Cursor,
			"role":         message.Role,
			"content":      message.Content,
			"name":         message.Name,
			"tool_call_id": message.ToolCallID,
		})
	}
	return out
}

func delegateEventsPayload(events []DelegateEventRef) []map[string]any {
	if len(events) == 0 {
		return nil
	}
	out := make([]map[string]any, 0, len(events))
	for _, event := range events {
		out = append(out, map[string]any{
			"event_id": event.EventID,
			"kind":     event.Kind,
		})
	}
	return out
}

func delegateArtifactsPayload(artifacts []DelegateArtifactRef) []map[string]any {
	if len(artifacts) == 0 {
		return nil
	}
	out := make([]map[string]any, 0, len(artifacts))
	for _, artifact := range artifacts {
		out = append(out, map[string]any{
			"ref":          artifact.Ref,
			"kind":         artifact.Kind,
			"label":        artifact.Label,
			"content_type": artifact.ContentType,
		})
	}
	return out
}

func delegateHandoffPayload(handoff DelegateHandoff, includeArtifacts bool) map[string]any {
	payload := map[string]any{
		"delegate_id":           handoff.DelegateID,
		"backend":               string(handoff.Backend),
		"last_run_id":           handoff.LastRunID,
		"summary":               handoff.Summary,
		"promoted_facts":        append([]string(nil), handoff.PromotedFacts...),
		"open_questions":        append([]string(nil), handoff.OpenQuestions...),
		"recommended_next_step": handoff.RecommendedNextStep,
		"created_at":            handoff.CreatedAt.Format(time.RFC3339Nano),
		"updated_at":            handoff.UpdatedAt.Format(time.RFC3339Nano),
	}
	if includeArtifacts {
		payload["artifacts"] = delegateArtifactsPayload(handoff.Artifacts)
	}
	return payload
}

func mustDelegatePolicySnapshotMap(a *Agent, contractSet contracts.ResolvedContracts) map[string]any {
	snapshot, err := encodeDelegatePolicySnapshot(DelegatePolicySnapshot{
		Tools:               contractSet.Tools,
		FilesystemTools:     contractSet.FilesystemTools,
		FilesystemExecution: contractSet.FilesystemExecution,
		ShellTools:          contractSet.ShellTools,
		ShellExecution:      contractSet.ShellExecution,
		DelegationTools:     contractSet.DelegationTools,
		DelegationExecution: contractSet.DelegationExecution,
		PlanTools:           contractSet.PlanTools,
		ToolExecution:       contractSet.ToolExecution,
	})
	if err != nil {
		if a != nil {
			return map[string]any{"encode_error": err.Error()}
		}
		return nil
	}
	return snapshot
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
