package daemon

import (
	"context"
	"fmt"

	"teamd/internal/runtime"
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
	switch req.Command {
	case "session.create":
		session, err := s.agent.CreateChatSession(ctx)
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
	case "chat.send":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		prompt, err := requiredString(req.Payload, "prompt")
		if err != nil {
			return nil, err
		}
		session, err := s.agent.ResumeChatSession(ctx, sessionID)
		if err != nil {
			return nil, err
		}
		result, err := s.agent.ChatTurn(ctx, session, runtime.ChatTurnInput{Prompt: prompt})
		if err != nil {
			return nil, err
		}
		snapshot, err := s.buildSessionSnapshot(sessionID)
		if err != nil {
			return nil, err
		}
		return map[string]any{
			"session": snapshot,
			"result": providerResultPayload{
				Provider:     s.providerLabel(),
				Model:        result.Provider.Model,
				InputTokens:  result.Provider.Usage.InputTokens,
				OutputTokens: result.Provider.Usage.OutputTokens,
				TotalTokens:  result.Provider.Usage.TotalTokens,
				Content:      result.Provider.Message.Content,
			},
		}, nil
	case "chat.btw":
		sessionID, err := requiredString(req.Payload, "session_id")
		if err != nil {
			return nil, err
		}
		prompt, err := requiredString(req.Payload, "prompt")
		if err != nil {
			return nil, err
		}
		session, err := s.agent.ResumeChatSession(ctx, sessionID)
		if err != nil {
			return nil, err
		}
		result, err := s.agent.BtwTurn(ctx, session, runtime.BtwTurnInput{Prompt: prompt})
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
		if err := s.agent.CreatePlan(ctx, sessionID, goal); err != nil {
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
		if err := s.agent.AddPlanTask(ctx, sessionID, description, parentTaskID, dependsOn); err != nil {
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
		if err := s.agent.EditPlanTask(ctx, sessionID, taskID, description, dependsOn); err != nil {
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
		if err := s.agent.SetPlanTaskStatus(ctx, sessionID, taskID, status, blockedReason); err != nil {
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
		if err := s.agent.AddPlanTaskNote(ctx, sessionID, taskID, note); err != nil {
			return nil, err
		}
		return s.sessionPayload(sessionID)
	case "shell.approve":
		approvalID, err := requiredString(req.Payload, "approval_id")
		if err != nil {
			return nil, err
		}
		view, ok := s.agent.PendingShellApproval(approvalID)
		if !ok {
			return nil, fmt.Errorf("shell approval %q not found", approvalID)
		}
		commandID, err := s.agent.ApproveShellCommand(ctx, approvalID)
		if err != nil {
			return nil, err
		}
		payload, err := s.sessionPayload(view.SessionID)
		if err != nil {
			return nil, err
		}
		payload["command_id"] = commandID
		return payload, nil
	case "shell.deny":
		approvalID, err := requiredString(req.Payload, "approval_id")
		if err != nil {
			return nil, err
		}
		view, ok := s.agent.PendingShellApproval(approvalID)
		if !ok {
			return nil, fmt.Errorf("shell approval %q not found", approvalID)
		}
		if err := s.agent.DenyShellCommand(ctx, approvalID); err != nil {
			return nil, err
		}
		return s.sessionPayload(view.SessionID)
	case "shell.kill":
		commandID, err := requiredString(req.Payload, "command_id")
		if err != nil {
			return nil, err
		}
		view, ok := s.agent.CurrentShellCommand(commandID)
		if !ok {
			return nil, fmt.Errorf("shell command %q not found", commandID)
		}
		if _, err := s.agent.KillShellCommand(ctx, commandID); err != nil {
			return nil, err
		}
		return s.sessionPayload(view.SessionID)
	default:
		return nil, fmt.Errorf("unsupported daemon command %q", req.Command)
	}
}

func (s *Server) sessionPayload(sessionID string) (map[string]any, error) {
	snapshot, err := s.buildSessionSnapshot(sessionID)
	if err != nil {
		return nil, err
	}
	return map[string]any{"session": snapshot}, nil
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
