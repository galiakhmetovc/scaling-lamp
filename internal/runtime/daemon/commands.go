package daemon

import (
	"context"
	"fmt"
	"log/slog"
	"runtime/debug"
	"strings"
	"sync"
	"time"

	"teamd/internal/runtime"
	"teamd/internal/runtime/eventing"
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
		return s.executeChatSend(ctx, agent, sessionID, prompt)
	case "chat.cancel_approval_and_send":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		approvalID, err := requiredString(req.Payload, "approval_id")
		if err != nil {
			return nil, err
		}
		prompt, err := requiredString(req.Payload, "prompt")
		if err != nil {
			return nil, err
		}
		if err := agent.CancelShellApproval(ctx, approvalID); err != nil {
			return nil, err
		}
		s.finishMainRun(sessionID, nil)
		return s.executeChatSend(ctx, agent, sessionID, prompt)
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
	case "workspace.files.snapshot":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		mgr, err := s.workspaceFilesManager()
		if err != nil {
			return nil, err
		}
		snapshot, err := mgr.Snapshot(sessionID)
		if err != nil {
			return nil, err
		}
		return map[string]any{"files": snapshot}, nil
	case "workspace.files.expand":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		relPath, err := requiredString(req.Payload, "rel_path")
		if err != nil {
			return nil, err
		}
		mgr, err := s.workspaceFilesManager()
		if err != nil {
			return nil, err
		}
		snapshot, err := mgr.Expand(sessionID, relPath)
		if err != nil {
			return nil, err
		}
		return map[string]any{"files": snapshot}, nil
	case "workspace.editor.open":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		relPath, err := requiredString(req.Payload, "rel_path")
		if err != nil {
			return nil, err
		}
		mgr, err := s.workspaceEditorManager()
		if err != nil {
			return nil, err
		}
		buffer, err := mgr.Open(sessionID, relPath)
		if err != nil {
			return nil, err
		}
		return map[string]any{"buffer": buffer}, nil
	case "workspace.editor.update":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		relPath, err := requiredString(req.Payload, "rel_path")
		if err != nil {
			return nil, err
		}
		content, err := requiredString(req.Payload, "content")
		if err != nil {
			return nil, err
		}
		mgr, err := s.workspaceEditorManager()
		if err != nil {
			return nil, err
		}
		buffer, err := mgr.Update(sessionID, relPath, content)
		if err != nil {
			return nil, err
		}
		return map[string]any{"buffer": buffer}, nil
	case "workspace.editor.save":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		relPath, err := requiredString(req.Payload, "rel_path")
		if err != nil {
			return nil, err
		}
		mgr, err := s.workspaceEditorManager()
		if err != nil {
			return nil, err
		}
		buffer, err := mgr.Save(sessionID, relPath)
		if err != nil {
			return nil, err
		}
		return map[string]any{"buffer": buffer}, nil
	case "workspace.artifacts.snapshot":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		mgr, err := s.workspaceArtifactsManager()
		if err != nil {
			return nil, err
		}
		if mgr == nil {
			return map[string]any{"artifacts": workspace.ArtifactSnapshot{SessionID: sessionID}}, nil
		}
		snapshot, err := mgr.Snapshot(sessionID)
		if err != nil {
			return nil, err
		}
		return map[string]any{"artifacts": snapshot}, nil
	case "workspace.artifacts.open":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		artifactRef, err := requiredString(req.Payload, "artifact_ref")
		if err != nil {
			return nil, err
		}
		mgr, err := s.workspaceArtifactsManager()
		if err != nil {
			return nil, err
		}
		if mgr == nil {
			return map[string]any{"artifacts": workspace.ArtifactSnapshot{SessionID: sessionID, SelectedRef: artifactRef}}, nil
		}
		snapshot, err := mgr.Open(sessionID, artifactRef)
		if err != nil {
			return nil, err
		}
		return map[string]any{"artifacts": snapshot}, nil
	case "shell.approve":
		approvalID, err := requiredString(req.Payload, "approval_id")
		if err != nil {
			return nil, err
		}
		view, ok := agent.PendingShellApproval(approvalID)
		if !ok {
			if existing, ok := agent.ShellCommandByApprovalID(approvalID); ok {
				return s.sessionPayload(existing.SessionID)
			}
			return nil, fmt.Errorf("shell approval %q not found", approvalID)
		}
		payload, err := s.optimisticShellApprovalPayload(view.SessionID, approvalID)
		if err != nil {
			return nil, err
		}
		payload["command_id"] = view.CommandID
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_updated", Payload: map[string]any{"session_id": view.SessionID}})
		s.runGuardedShellApproval(view.SessionID, func() {
			s.approveShellAsync(s.currentAgent(), approvalID, view.SessionID)
		})
		return payload, nil
	case "shell.approve_always":
		approvalID, err := requiredString(req.Payload, "approval_id")
		if err != nil {
			return nil, err
		}
		view, ok := agent.PendingShellApproval(approvalID)
		if !ok {
			if existing, ok := agent.ShellCommandByApprovalID(approvalID); ok {
				return s.sessionPayload(existing.SessionID)
			}
			return nil, fmt.Errorf("shell approval %q not found", approvalID)
		}
		payload, err := s.optimisticShellApprovalPayload(view.SessionID, approvalID)
		if err != nil {
			return nil, err
		}
		payload["command_id"] = view.CommandID
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_updated", Payload: map[string]any{"session_id": view.SessionID}})
		s.runGuardedShellApproval(view.SessionID, func() {
			s.approveShellAlwaysAsync(agent, approvalID, view)
		})
		return payload, nil
	case "shell.deny":
		approvalID, err := requiredString(req.Payload, "approval_id")
		if err != nil {
			return nil, err
		}
		view, ok := agent.PendingShellApproval(approvalID)
		if !ok {
			if existing, ok := agent.ShellCommandByApprovalID(approvalID); ok {
				return s.sessionPayload(existing.SessionID)
			}
			return nil, fmt.Errorf("shell approval %q not found", approvalID)
		}
		payload, err := s.optimisticShellApprovalPayload(view.SessionID, approvalID)
		if err != nil {
			return nil, err
		}
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_updated", Payload: map[string]any{"session_id": view.SessionID}})
		s.runGuardedShellApproval(view.SessionID, func() {
			s.denyShellAsync(s.currentAgent(), approvalID, view.SessionID)
		})
		return payload, nil
	case "shell.deny_always":
		approvalID, err := requiredString(req.Payload, "approval_id")
		if err != nil {
			return nil, err
		}
		view, ok := agent.PendingShellApproval(approvalID)
		if !ok {
			if existing, ok := agent.ShellCommandByApprovalID(approvalID); ok {
				return s.sessionPayload(existing.SessionID)
			}
			return nil, fmt.Errorf("shell approval %q not found", approvalID)
		}
		payload, err := s.optimisticShellApprovalPayload(view.SessionID, approvalID)
		if err != nil {
			return nil, err
		}
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_updated", Payload: map[string]any{"session_id": view.SessionID}})
		s.runGuardedShellApproval(view.SessionID, func() {
			s.denyShellAlwaysAsync(agent, approvalID, view)
		})
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
	case "debug.trace":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		traceName, err := requiredString(req.Payload, "trace")
		if err != nil {
			return nil, err
		}
		fields, _ := req.Payload["fields"].(map[string]any)
		runID, _ := optionalString(req.Payload, "run_id")
		if err := s.recordTraceEvent(ctx, "operator.tui", sessionID, runID, traceName, fields); err != nil {
			return nil, err
		}
		return map[string]any{"ok": true}, nil
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

func (s *Server) executeChatSend(ctx context.Context, agent *runtime.Agent, sessionID, prompt string) (map[string]any, error) {
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
		s.failMainRun(sessionID, nil)
		return nil, err
	}
	result, err := agent.ChatTurn(ctx, session, runtime.ChatTurnInput{Prompt: prompt})
	if err != nil {
		s.failMainRun(sessionID, nil)
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
	stillRunning := s.settleMainRunAfterChatTurn(sessionID, resultPayload, result.Provider.FinishReason)
	snapshot, err := s.buildSessionSnapshot(sessionID)
	if err != nil {
		return nil, err
	}
	if !stillRunning {
		s.maybeDispatchQueuedDrafts(sessionID)
	}
	return map[string]any{
		"session": snapshot,
		"queued":  false,
		"result":  resultPayload,
	}, nil
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
	s.traceApprovalContinuation(sessionID, "", approvalID, "daemon.shell.approve.started", nil)
	if _, err := agent.ApproveShellCommand(context.Background(), approvalID); err != nil {
		s.traceApprovalContinuation(sessionID, "", approvalID, "daemon.shell.approve.failed", map[string]any{"error": err.Error()})
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_failed", Payload: map[string]any{"session_id": sessionID}, Error: err.Error()})
		s.failMainRun(sessionID, nil)
		return
	}
	s.traceApprovalContinuation(sessionID, "", approvalID, "daemon.shell.approve.completed", nil)
	s.syncMainRunAfterShellContinuation(agent, sessionID)
}

func (s *Server) approveShellAlwaysAsync(agent *runtime.Agent, approvalID string, view shell.PendingApprovalView) {
	s.traceApprovalContinuation(view.SessionID, view.RunID, approvalID, "daemon.shell.approve_always.started", map[string]any{"command": view.Command})
	s.traceApprovalContinuation(view.SessionID, view.RunID, approvalID, "daemon.shell.reload.started", nil)
	reloaded, err := PersistShellApprovalRuleAndReload(agent.ConfigPath, "allow", shellApprovalPrefix(view.Command, view.Args))
	if err != nil {
		s.traceApprovalContinuation(view.SessionID, view.RunID, approvalID, "daemon.shell.reload.failed", map[string]any{"error": err.Error()})
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_failed", Payload: map[string]any{"session_id": view.SessionID}, Error: err.Error()})
		return
	}
	s.traceApprovalContinuation(view.SessionID, view.RunID, approvalID, "daemon.shell.reload.completed", nil)
	reloaded.UIBus = agent.UIBus
	agent.CopySuspendedToolLoopTo(approvalID, reloaded)
	s.swapAgent(reloaded)
	s.traceApprovalContinuation(view.SessionID, view.RunID, approvalID, "daemon.shell.resume.started", nil)
	if _, err := reloaded.ApproveShellCommand(context.Background(), approvalID); err != nil {
		s.traceApprovalContinuation(view.SessionID, view.RunID, approvalID, "daemon.shell.resume.failed", map[string]any{"error": err.Error()})
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_failed", Payload: map[string]any{"session_id": view.SessionID}, Error: err.Error()})
		s.failMainRun(view.SessionID, nil)
		return
	}
	s.traceApprovalContinuation(view.SessionID, view.RunID, approvalID, "daemon.shell.resume.completed", nil)
	s.syncMainRunAfterShellContinuation(reloaded, view.SessionID)
}

func (s *Server) denyShellAsync(agent *runtime.Agent, approvalID, sessionID string) {
	s.traceApprovalContinuation(sessionID, "", approvalID, "daemon.shell.deny.started", nil)
	if err := agent.DenyShellCommand(context.Background(), approvalID); err != nil {
		s.traceApprovalContinuation(sessionID, "", approvalID, "daemon.shell.deny.failed", map[string]any{"error": err.Error()})
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_failed", Payload: map[string]any{"session_id": sessionID}, Error: err.Error()})
		s.failMainRun(sessionID, nil)
		return
	}
	s.traceApprovalContinuation(sessionID, "", approvalID, "daemon.shell.deny.completed", nil)
	s.syncMainRunAfterShellContinuation(agent, sessionID)
}

func (s *Server) denyShellAlwaysAsync(agent *runtime.Agent, approvalID string, view shell.PendingApprovalView) {
	s.traceApprovalContinuation(view.SessionID, view.RunID, approvalID, "daemon.shell.deny_always.started", map[string]any{"command": view.Command})
	s.traceApprovalContinuation(view.SessionID, view.RunID, approvalID, "daemon.shell.reload.started", nil)
	reloaded, err := PersistShellApprovalRuleAndReload(agent.ConfigPath, "deny", shellApprovalPrefix(view.Command, view.Args))
	if err != nil {
		s.traceApprovalContinuation(view.SessionID, view.RunID, approvalID, "daemon.shell.reload.failed", map[string]any{"error": err.Error()})
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_failed", Payload: map[string]any{"session_id": view.SessionID}, Error: err.Error()})
		return
	}
	s.traceApprovalContinuation(view.SessionID, view.RunID, approvalID, "daemon.shell.reload.completed", nil)
	reloaded.UIBus = agent.UIBus
	agent.CopySuspendedToolLoopTo(approvalID, reloaded)
	s.swapAgent(reloaded)
	s.traceApprovalContinuation(view.SessionID, view.RunID, approvalID, "daemon.shell.resume.started", nil)
	if err := reloaded.DenyShellCommand(context.Background(), approvalID); err != nil {
		s.traceApprovalContinuation(view.SessionID, view.RunID, approvalID, "daemon.shell.resume.failed", map[string]any{"error": err.Error()})
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_failed", Payload: map[string]any{"session_id": view.SessionID}, Error: err.Error()})
		s.failMainRun(view.SessionID, nil)
		return
	}
	s.traceApprovalContinuation(view.SessionID, view.RunID, approvalID, "daemon.shell.resume.completed", nil)
	s.syncMainRunAfterShellContinuation(reloaded, view.SessionID)
}

func (s *Server) runGuardedShellApproval(sessionID string, fn func()) {
	go func() {
		lock := s.sessionApprovalLock(sessionID)
		lock.Lock()
		defer lock.Unlock()
		s.traceApprovalContinuation(sessionID, "", "", "daemon.shell.approval_continuation.started", nil)
		defer func() {
			if recovered := recover(); recovered != nil {
				err := fmt.Errorf("panic in shell approval continuation: %v", recovered)
				s.traceApprovalContinuation(sessionID, "", "", "daemon.shell.approval_continuation.panicked", map[string]any{"error": err.Error()})
				s.logError("daemon.shell_approval.panic", err,
					slog.String("session_id", sessionID),
					slog.String("stack", string(debug.Stack())),
				)
				s.publishDaemon(WebsocketEnvelope{
					Type:    "shell_approval_failed",
					Payload: map[string]any{"session_id": sessionID},
					Error:   err.Error(),
				})
				s.failMainRun(sessionID, nil)
			}
		}()
		fn()
		s.traceApprovalContinuation(sessionID, "", "", "daemon.shell.approval_continuation.completed", nil)
	}()
}

func (s *Server) sessionApprovalLock(sessionID string) *sync.Mutex {
	s.approvalMu.Lock()
	defer s.approvalMu.Unlock()
	if s.approvalLocks == nil {
		s.approvalLocks = map[string]*sync.Mutex{}
	}
	lock, ok := s.approvalLocks[sessionID]
	if !ok {
		lock = &sync.Mutex{}
		s.approvalLocks[sessionID] = lock
	}
	return lock
}

func (s *Server) traceApprovalContinuation(sessionID, runID, approvalID, traceName string, fields map[string]any) {
	if strings.TrimSpace(sessionID) == "" || strings.TrimSpace(traceName) == "" {
		return
	}
	payload := cloneTracePayload(fields)
	if approvalID != "" {
		payload["approval_id"] = approvalID
	}
	if runID != "" {
		payload["run_id"] = runID
	}
	if err := s.recordTraceEvent(context.Background(), "daemon.approval", sessionID, runID, traceName, payload); err != nil {
		s.logError("daemon.trace.record.failed", err,
			slog.String("session_id", sessionID),
			slog.String("run_id", runID),
			slog.String("approval_id", approvalID),
			slog.String("trace", traceName),
		)
	}
}

func (s *Server) recordTraceEvent(ctx context.Context, source, sessionID, runID, traceName string, fields map[string]any) error {
	if strings.TrimSpace(sessionID) == "" || strings.TrimSpace(traceName) == "" {
		return nil
	}
	agent := s.currentAgent()
	if agent == nil {
		return fmt.Errorf("agent is nil")
	}
	if agent.EventLog == nil {
		return nil
	}
	eventID := fmt.Sprintf("evt-trace-%d", time.Now().UTC().UnixNano())
	if agent.NewID != nil {
		eventID = agent.NewID("evt-trace")
	}
	occurredAt := time.Now().UTC()
	if agent.Now != nil {
		occurredAt = agent.Now().UTC()
	}
	payload := map[string]any{
		"session_id": sessionID,
		"trace":      traceName,
		"fields":     cloneTracePayload(fields),
	}
	if runID != "" {
		payload["run_id"] = runID
	}
	if err := agent.RecordEvent(ctx, eventing.Event{
		ID:               eventID,
		Kind:             eventing.EventTraceRecorded,
		OccurredAt:       occurredAt,
		AggregateID:      sessionID,
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
		CorrelationID:    runID,
		Source:           source,
		ActorID:          agent.Config.ID,
		ActorType:        "operator",
		TraceSummary:     traceName,
		Payload:          payload,
	}); err != nil {
		return err
	}
	s.logInfo("trace.recorded",
		slog.String("source", source),
		slog.String("session_id", sessionID),
		slog.String("run_id", runID),
		slog.String("trace", traceName),
		slog.Any("fields", payload["fields"]),
	)
	return nil
}

func cloneTracePayload(fields map[string]any) map[string]any {
	if len(fields) == 0 {
		return map[string]any{}
	}
	out := make(map[string]any, len(fields))
	for k, v := range fields {
		out[k] = v
	}
	return out
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
