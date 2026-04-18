package daemon

import (
	"context"
	"fmt"

	"teamd/internal/runtime"
	"teamd/internal/runtime/workspace"
	"teamd/internal/shell"
)

type providerResultPayload struct {
	Provider     string `json:"provider"`
	Model        string `json:"model"`
	InputTokens  int    `json:"input_tokens"`
	OutputTokens int    `json:"output_tokens"`
	TotalTokens  int    `json:"total_tokens"`
	Content      string `json:"content"`
}

func (s *Server) executeCommand(ctx context.Context, req CommandRequest) (any, error) {
	agent := s.currentAgent()
	switch req.Command {
	case "session.create":
		session, err := agent.CreateChatSession(ctx)
		if err != nil {
			return nil, err
		}
		snapshot, err := s.buildSessionSnapshot(session.SessionID)
		if err != nil {
			return nil, err
		}
		return map[string]any{"session": snapshot}, nil
	case "session.get":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		snapshot, err := s.buildSessionSnapshot(sessionID)
		if err != nil {
			return nil, err
		}
		return map[string]any{"session": snapshot}, nil
	case "session.rename":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		title, err := requiredString(req.Payload, "title")
		if err != nil {
			return nil, err
		}
		if err := agent.RenameSession(ctx, sessionID, title); err != nil {
			return nil, err
		}
		return s.sessionPayload(sessionID)
	case "session.delete":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		if err := agent.DeleteSession(ctx, sessionID); err != nil {
			return nil, err
		}
		return map[string]any{"session_id": sessionID, "deleted": true}, nil
	case "session.prompt.set":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		content, err := requiredString(req.Payload, "content")
		if err != nil {
			return nil, err
		}
		if err := agent.SetSessionPromptOverride(ctx, sessionID, content); err != nil {
			return nil, err
		}
		return s.sessionPayload(sessionID)
	case "session.prompt.clear":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		if err := agent.ClearSessionPromptOverride(ctx, sessionID); err != nil {
			return nil, err
		}
		return s.sessionPayload(sessionID)
	case "session.history":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		loadedCount, err := requiredInt(req.Payload, "loaded_count")
		if err != nil {
			return nil, err
		}
		historyLimit, _ := optionalInt(req.Payload, "history_limit")
		return s.buildSessionHistoryChunk(sessionID, loadedCount, historyLimit)
	case "chat.send":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		prompt, err := requiredString(req.Payload, "prompt")
		if err != nil {
			return nil, err
		}
		if !s.startMainRun(sessionID) {
			draft := s.enqueueDraft(sessionID, prompt)
			payload, err := s.sessionPayload(sessionID)
			if err != nil {
				return nil, err
			}
			payload["queued"] = true
			payload["draft"] = draft
			s.publishDaemon(WebsocketEnvelope{Type: "draft_queued", Payload: map[string]any{"session_id": sessionID, "draft": draft}})
			return payload, nil
		}
		session, err := agent.ResumeChatSession(ctx, sessionID)
		if err != nil {
			s.finishMainRun(sessionID, nil)
			return nil, err
		}
		result, err := agent.ChatTurn(ctx, session, runtime.ChatTurnInput{Prompt: prompt})
		if err != nil {
			s.finishMainRun(sessionID, nil)
			return nil, err
		}
		resultPayload := providerResultPayload{
			Provider:     s.providerLabel(),
			Model:        result.Provider.Model,
			InputTokens:  result.Provider.Usage.InputTokens,
			OutputTokens: result.Provider.Usage.OutputTokens,
			TotalTokens:  result.Provider.Usage.TotalTokens,
			Content:      result.Provider.Message.Content,
		}
		s.finishMainRun(sessionID, &resultPayload)
		snapshot, err := s.buildSessionSnapshot(sessionID)
		if err != nil {
			return nil, err
		}
		s.maybeDispatchQueuedDrafts(sessionID)
		return map[string]any{
			"session": snapshot,
			"queued":  false,
			"result":  resultPayload,
		}, nil
	case "draft.enqueue":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		text, err := requiredString(req.Payload, "text")
		if err != nil {
			return nil, err
		}
		draft := s.enqueueDraft(sessionID, text)
		payload, err := s.sessionPayload(sessionID)
		if err != nil {
			return nil, err
		}
		payload["draft"] = draft
		s.publishDaemon(WebsocketEnvelope{Type: "draft_queued", Payload: map[string]any{"session_id": sessionID, "draft": draft}})
		return payload, nil
	case "draft.list":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		return s.sessionPayload(sessionID)
	case "draft.recall":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		draftID, err := requiredString(req.Payload, "draft_id")
		if err != nil {
			return nil, err
		}
		draft, ok := s.recallDraft(sessionID, draftID)
		if !ok {
			return nil, fmt.Errorf("draft %q not found", draftID)
		}
		payload, err := s.sessionPayload(sessionID)
		if err != nil {
			return nil, err
		}
		payload["draft"] = draft
		s.publishDaemon(WebsocketEnvelope{Type: "draft_recalled", Payload: map[string]any{"session_id": sessionID, "draft": draft}})
		return payload, nil
	case "chat.btw":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		prompt, err := requiredString(req.Payload, "prompt")
		if err != nil {
			return nil, err
		}
		session, err := agent.ResumeChatSession(ctx, sessionID)
		if err != nil {
			return nil, err
		}
		result, err := agent.BtwTurn(ctx, session, runtime.BtwTurnInput{Prompt: prompt})
		if err != nil {
			return nil, err
		}
		return map[string]any{
			"session_id": sessionID,
			"prompt":     prompt,
			"result": providerResultPayload{
				Provider:     s.providerLabel(),
				Model:        result.Provider.Model,
				InputTokens:  result.Provider.Usage.InputTokens,
				OutputTokens: result.Provider.Usage.OutputTokens,
				TotalTokens:  result.Provider.Usage.TotalTokens,
				Content:      result.Provider.Message.Content,
			},
		}, nil
	case "plan.create":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		goal, err := requiredString(req.Payload, "goal")
		if err != nil {
			return nil, err
		}
		if err := agent.CreatePlan(ctx, sessionID, goal); err != nil {
			return nil, err
		}
		return s.sessionPayload(sessionID)
	case "plan.add_task":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		description, err := requiredString(req.Payload, "description")
		if err != nil {
			return nil, err
		}
		parentTaskID, _ := optionalString(req.Payload, "parent_task_id")
		dependsOn, err := optionalStringSlice(req.Payload, "depends_on")
		if err != nil {
			return nil, err
		}
		if err := agent.AddPlanTask(ctx, sessionID, description, parentTaskID, dependsOn); err != nil {
			return nil, err
		}
		return s.sessionPayload(sessionID)
	case "plan.edit_task":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		taskID, err := requiredString(req.Payload, "task_id")
		if err != nil {
			return nil, err
		}
		description, err := requiredString(req.Payload, "description")
		if err != nil {
			return nil, err
		}
		dependsOn, err := optionalStringSlice(req.Payload, "depends_on")
		if err != nil {
			return nil, err
		}
		if err := agent.EditPlanTask(ctx, sessionID, taskID, description, dependsOn); err != nil {
			return nil, err
		}
		return s.sessionPayload(sessionID)
	case "plan.set_task_status":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		taskID, err := requiredString(req.Payload, "task_id")
		if err != nil {
			return nil, err
		}
		status, err := requiredString(req.Payload, "status")
		if err != nil {
			return nil, err
		}
		blockedReason, _ := optionalString(req.Payload, "blocked_reason")
		if err := agent.SetPlanTaskStatus(ctx, sessionID, taskID, status, blockedReason); err != nil {
			return nil, err
		}
		return s.sessionPayload(sessionID)
	case "plan.add_task_note":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		taskID, err := requiredString(req.Payload, "task_id")
		if err != nil {
			return nil, err
		}
		note, err := requiredString(req.Payload, "note")
		if err != nil {
			return nil, err
		}
		if err := agent.AddPlanTaskNote(ctx, sessionID, taskID, note); err != nil {
			return nil, err
		}
		return s.sessionPayload(sessionID)
	case "workspace.pty.open":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		cols, err := requiredInt(req.Payload, "cols")
		if err != nil {
			return nil, err
		}
		rows, err := requiredInt(req.Payload, "rows")
		if err != nil {
			return nil, err
		}
		pty, err := s.workspacePTY.Open(sessionID, cols, rows)
		if err != nil {
			return nil, err
		}
		return map[string]any{"pty": pty}, nil
	case "workspace.pty.input":
		ptyID, err := requiredString(req.Payload, "pty_id")
		if err != nil {
			return nil, err
		}
		data, err := requiredString(req.Payload, "data")
		if err != nil {
			return nil, err
		}
		if err := s.workspacePTY.Input(ptyID, []byte(data)); err != nil {
			return nil, err
		}
		return map[string]any{"pty_id": ptyID, "ok": true}, nil
	case "workspace.pty.resize":
		ptyID, err := requiredString(req.Payload, "pty_id")
		if err != nil {
			return nil, err
		}
		cols, err := requiredInt(req.Payload, "cols")
		if err != nil {
			return nil, err
		}
		rows, err := requiredInt(req.Payload, "rows")
		if err != nil {
			return nil, err
		}
		if err := s.workspacePTY.Resize(ptyID, cols, rows); err != nil {
			return nil, err
		}
		pty, err := s.findWorkspacePTYSnapshotByID(ptyID)
		if err != nil {
			return nil, err
		}
		return map[string]any{"pty": pty}, nil
	case "workspace.pty.snapshot":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		pty, ok := s.workspacePTY.Snapshot(sessionID)
		if !ok {
			return nil, fmt.Errorf("workspace pty for session %q not found", sessionID)
		}
		return map[string]any{"pty": pty}, nil
	case "workspace.pty.reset":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		if err := s.workspacePTY.Reset(sessionID); err != nil {
			return nil, err
		}
		pty, ok := s.workspacePTY.Snapshot(sessionID)
		if !ok {
			return nil, fmt.Errorf("workspace pty for session %q not found after reset", sessionID)
		}
		return map[string]any{"pty": pty}, nil
	case "shell.approve":
		approvalID, err := requiredString(req.Payload, "approval_id")
		if err != nil {
			return nil, err
		}
		view, ok := agent.PendingShellApproval(approvalID)
		if !ok {
			return nil, fmt.Errorf("shell approval %q not found", approvalID)
		}
		payload, err := s.optimisticShellApprovalPayload(view.SessionID, approvalID)
		if err != nil {
			return nil, err
		}
		payload["command_id"] = view.CommandID
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_updated", Payload: map[string]any{"session_id": view.SessionID}})
		go s.approveShellAsync(s.currentAgent(), approvalID, view.SessionID)
		return payload, nil
	case "shell.approve_always":
		approvalID, err := requiredString(req.Payload, "approval_id")
		if err != nil {
			return nil, err
		}
		view, ok := agent.PendingShellApproval(approvalID)
		if !ok {
			return nil, fmt.Errorf("shell approval %q not found", approvalID)
		}
		payload, err := s.optimisticShellApprovalPayload(view.SessionID, approvalID)
		if err != nil {
			return nil, err
		}
		payload["command_id"] = view.CommandID
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_updated", Payload: map[string]any{"session_id": view.SessionID}})
		go s.approveShellAlwaysAsync(agent, approvalID, view)
		return payload, nil
	case "shell.deny":
		approvalID, err := requiredString(req.Payload, "approval_id")
		if err != nil {
			return nil, err
		}
		view, ok := agent.PendingShellApproval(approvalID)
		if !ok {
			return nil, fmt.Errorf("shell approval %q not found", approvalID)
		}
		payload, err := s.optimisticShellApprovalPayload(view.SessionID, approvalID)
		if err != nil {
			return nil, err
		}
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_updated", Payload: map[string]any{"session_id": view.SessionID}})
		go s.denyShellAsync(s.currentAgent(), approvalID, view.SessionID)
		return payload, nil
	case "shell.deny_always":
		approvalID, err := requiredString(req.Payload, "approval_id")
		if err != nil {
			return nil, err
		}
		view, ok := agent.PendingShellApproval(approvalID)
		if !ok {
			return nil, fmt.Errorf("shell approval %q not found", approvalID)
		}
		payload, err := s.optimisticShellApprovalPayload(view.SessionID, approvalID)
		if err != nil {
			return nil, err
		}
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_updated", Payload: map[string]any{"session_id": view.SessionID}})
		go s.denyShellAlwaysAsync(agent, approvalID, view)
		return payload, nil
	case "shell.kill":
		commandID, err := requiredString(req.Payload, "command_id")
		if err != nil {
			return nil, err
		}
		view, ok := agent.CurrentShellCommand(commandID)
		if !ok {
			return nil, fmt.Errorf("shell command %q not found", commandID)
		}
		if _, err := agent.KillShellCommand(ctx, commandID); err != nil {
			return nil, err
		}
		return s.sessionPayload(view.SessionID)
	case "settings.get":
		settings, err := s.settingsSnapshot()
		if err != nil {
			return nil, err
		}
		return map[string]any{"settings": settings}, nil
	case "settings.form.apply":
		baseRevision, err := requiredString(req.Payload, "base_revision")
		if err != nil {
			return nil, err
		}
		values, ok := req.Payload["values"].(map[string]any)
		if !ok {
			return nil, fmt.Errorf("settings.form.apply requires values object")
		}
		settings, err := s.applyFormSettings(ctx, baseRevision, values)
		if err != nil {
			return nil, err
		}
		return map[string]any{"settings": settings}, nil
	case "settings.quick.apply":
		baseRevision, err := requiredString(req.Payload, "base_revision")
		if err != nil {
			return nil, err
		}
		values, ok := req.Payload["values"].(map[string]any)
		if !ok {
			return nil, fmt.Errorf("settings.quick.apply requires values object")
		}
		settings, err := s.applyQuickControlSettings(ctx, baseRevision, values)
		if err != nil {
			return nil, err
		}
		return map[string]any{"settings": settings}, nil
	case "settings.raw.get":
		path, err := requiredString(req.Payload, "path")
		if err != nil {
			return nil, err
		}
		file, err := s.settingsRawFile(path)
		if err != nil {
			return nil, err
		}
		return map[string]any{"file": file}, nil
	case "settings.raw.apply":
		path, err := requiredString(req.Payload, "path")
		if err != nil {
			return nil, err
		}
		baseRevision, err := requiredString(req.Payload, "base_revision")
		if err != nil {
			return nil, err
		}
		content, err := requiredString(req.Payload, "content")
		if err != nil {
			return nil, err
		}
		settings, err := s.applyRawSettings(ctx, path, baseRevision, content)
		if err != nil {
			return nil, err
		}
		return map[string]any{"settings": settings}, nil
	default:
		return nil, fmt.Errorf("unsupported daemon command %q", req.Command)
	}
}

func (s *Server) findWorkspacePTYSnapshotByID(ptyID string) (workspace.PTYSnapshot, error) {
	for _, sessionID := range s.workspacePTY.SessionIDs() {
		snap, ok := s.workspacePTY.Snapshot(sessionID)
		if ok && snap.PTYID == ptyID {
			return snap, nil
		}
	}
	return workspace.PTYSnapshot{}, fmt.Errorf("workspace pty %q not found", ptyID)
}

func (s *Server) sessionPayload(sessionID string) (map[string]any, error) {
	snapshot, err := s.buildSessionSnapshot(sessionID)
	if err != nil {
		return nil, err
	}
	return map[string]any{"session": snapshot}, nil
}

func (s *Server) optimisticShellApprovalPayload(sessionID, approvalID string) (map[string]any, error) {
	snapshot, err := s.buildSessionSnapshot(sessionID)
	if err != nil {
		return nil, err
	}
	filtered := snapshot.PendingApprovals[:0]
	for _, approval := range snapshot.PendingApprovals {
		if approval.ApprovalID != approvalID {
			filtered = append(filtered, approval)
		}
	}
	snapshot.PendingApprovals = filtered
	return map[string]any{"session": snapshot}, nil
}

func (s *Server) approveShellAsync(agent *runtime.Agent, approvalID, sessionID string) {
	if _, err := agent.ApproveShellCommand(context.Background(), approvalID); err != nil {
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_failed", Payload: map[string]any{"session_id": sessionID}, Error: err.Error()})
	}
}

func (s *Server) approveShellAlwaysAsync(agent *runtime.Agent, approvalID string, view shell.PendingApprovalView) {
	reloaded, err := PersistShellApprovalRuleAndReload(agent.ConfigPath, "allow", shellApprovalPrefix(view.Command, view.Args))
	if err != nil {
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_failed", Payload: map[string]any{"session_id": view.SessionID}, Error: err.Error()})
		return
	}
	reloaded.UIBus = agent.UIBus
	agent.CopySuspendedToolLoopTo(approvalID, reloaded)
	s.swapAgent(reloaded)
	if _, err := reloaded.ApproveShellCommand(context.Background(), approvalID); err != nil {
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_failed", Payload: map[string]any{"session_id": view.SessionID}, Error: err.Error()})
	}
}

func (s *Server) denyShellAsync(agent *runtime.Agent, approvalID, sessionID string) {
	if err := agent.DenyShellCommand(context.Background(), approvalID); err != nil {
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_failed", Payload: map[string]any{"session_id": sessionID}, Error: err.Error()})
	}
}

func (s *Server) denyShellAlwaysAsync(agent *runtime.Agent, approvalID string, view shell.PendingApprovalView) {
	reloaded, err := PersistShellApprovalRuleAndReload(agent.ConfigPath, "deny", shellApprovalPrefix(view.Command, view.Args))
	if err != nil {
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_failed", Payload: map[string]any{"session_id": view.SessionID}, Error: err.Error()})
		return
	}
	reloaded.UIBus = agent.UIBus
	agent.CopySuspendedToolLoopTo(approvalID, reloaded)
	s.swapAgent(reloaded)
	if err := reloaded.DenyShellCommand(context.Background(), approvalID); err != nil {
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_failed", Payload: map[string]any{"session_id": view.SessionID}, Error: err.Error()})
	}
}

func requiredString(payload map[string]any, key string) (string, error) {
	value, ok := payload[key].(string)
	if !ok || value == "" {
		return "", fmt.Errorf("missing required string payload field %q", key)
	}
	return value, nil
}

func optionalString(payload map[string]any, key string) (string, bool) {
	value, ok := payload[key].(string)
	if !ok {
		return "", false
	}
	return value, value != ""
}

func requiredInt(payload map[string]any, key string) (int, error) {
	value, ok := optionalInt(payload, key)
	if !ok {
		return 0, fmt.Errorf("missing required int payload field %q", key)
	}
	return value, nil
}

func optionalInt(payload map[string]any, key string) (int, bool) {
	raw, ok := payload[key]
	if !ok || raw == nil {
		return 0, false
	}
	switch typed := raw.(type) {
	case int:
		return typed, true
	case int32:
		return int(typed), true
	case int64:
		return int(typed), true
	case float64:
		return int(typed), true
	default:
		return 0, false
	}
}

func optionalStringSlice(payload map[string]any, key string) ([]string, error) {
	raw, ok := payload[key]
	if !ok || raw == nil {
		return nil, nil
	}
	switch typed := raw.(type) {
	case []string:
		return append([]string{}, typed...), nil
	case []any:
		out := make([]string, 0, len(typed))
		for _, item := range typed {
			text, ok := item.(string)
			if !ok {
				return nil, fmt.Errorf("payload field %q must contain only strings", key)
			}
			out = append(out, text)
		}
		return out, nil
	default:
		return nil, fmt.Errorf("payload field %q must be []string", key)
	}
}
