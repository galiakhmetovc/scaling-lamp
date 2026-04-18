package runtime

import (
	"context"
	"fmt"

	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
)

func (a *Agent) PendingShellApprovals(sessionID string) []shell.PendingApprovalView {
	projection := a.shellCommandProjection()
	if projection == nil {
		return nil
	}
	views := projection.PendingForSession(sessionID)
	out := make([]shell.PendingApprovalView, 0, len(views))
	for _, view := range views {
		out = append(out, shell.PendingApprovalView{
			ApprovalID: view.ApprovalID,
			CommandID:  view.CommandID,
			SessionID:  view.SessionID,
			RunID:      view.RunID,
			OccurredAt: view.OccurredAt,
			ToolName:   view.ToolName,
			Command:    view.Command,
			Args:       append([]string{}, view.Args...),
			Cwd:        view.Cwd,
			Message:    view.Message,
		})
	}
	return out
}

func (a *Agent) PendingShellApproval(approvalID string) (shell.PendingApprovalView, bool) {
	view, err := a.pendingShellApproval(approvalID)
	if err != nil {
		return shell.PendingApprovalView{}, false
	}
	return shell.PendingApprovalView{
		ApprovalID: view.ApprovalID,
		CommandID:  view.CommandID,
		SessionID:  view.SessionID,
		RunID:      view.RunID,
		OccurredAt: view.OccurredAt,
		ToolName:   view.ToolName,
		Command:    view.Command,
		Args:       append([]string{}, view.Args...),
		Cwd:        view.Cwd,
		Message:    view.Message,
	}, true
}

func (a *Agent) ApproveShellCommand(ctx context.Context, approvalID string) (string, error) {
	view, err := a.pendingShellApproval(approvalID)
	if err != nil {
		return "", err
	}
	if a.ShellRuntime == nil {
		return "", fmt.Errorf("shell runtime is nil")
	}
	if err := a.ShellRuntime.RecoverApproval(a.Contracts.ShellExecution, shell.PendingApprovalView{
		ApprovalID: view.ApprovalID,
		CommandID:  view.CommandID,
		SessionID:  view.SessionID,
		RunID:      view.RunID,
		OccurredAt: view.OccurredAt,
		ToolName:   view.ToolName,
		Command:    view.Command,
		Args:       append([]string{}, view.Args...),
		Cwd:        view.Cwd,
		Message:    view.Message,
	}, shell.ExecutionMeta{
		SessionID:   view.SessionID,
		RunID:       view.RunID,
		Source:      "runtime.shell_operator",
		ActorID:     a.Config.ID,
		ActorType:   "operator",
		RecordEvent: a.RecordEvent,
		Now:         a.Now,
		NewID:       a.NewID,
	}); err != nil {
		return "", err
	}
	out, err := a.ShellRuntime.Approve(ctx, approvalID)
	if err != nil {
		return "", err
	}
	if err := a.resumeSuspendedToolLoopAfterApproval(ctx, approvalID, out); err != nil {
		return "", err
	}
	return out, nil
}

func (a *Agent) KillShellCommand(ctx context.Context, commandID string) (string, error) {
	if a.ShellRuntime == nil {
		return "", fmt.Errorf("shell runtime is nil")
	}
	return a.ShellRuntime.ExecuteWithMeta(ctx, a.Contracts.ShellExecution, "shell_kill", map[string]any{
		"command_id": commandID,
	}, shell.ExecutionMeta{
		Source:      "runtime.shell_operator",
		ActorID:     a.Config.ID,
		ActorType:   "operator",
		RecordEvent: a.RecordEvent,
		Now:         a.Now,
		NewID:       a.NewID,
	})
}

func (a *Agent) DenyShellCommand(ctx context.Context, approvalID string) error {
	view, err := a.pendingShellApproval(approvalID)
	if err != nil {
		return err
	}
	if a.ShellRuntime == nil {
		return fmt.Errorf("shell runtime is nil")
	}
	if err := a.ShellRuntime.RecoverApproval(a.Contracts.ShellExecution, shell.PendingApprovalView{
		ApprovalID: view.ApprovalID,
		CommandID:  view.CommandID,
		SessionID:  view.SessionID,
		RunID:      view.RunID,
		OccurredAt: view.OccurredAt,
		ToolName:   view.ToolName,
		Command:    view.Command,
		Args:       append([]string{}, view.Args...),
		Cwd:        view.Cwd,
		Message:    view.Message,
	}, shell.ExecutionMeta{
		SessionID:   view.SessionID,
		RunID:       view.RunID,
		Source:      "runtime.shell_operator",
		ActorID:     a.Config.ID,
		ActorType:   "operator",
		RecordEvent: a.RecordEvent,
		Now:         a.Now,
		NewID:       a.NewID,
	}); err != nil {
		return err
	}
	if err := a.ShellRuntime.Deny(ctx, approvalID); err != nil {
		return err
	}
	return a.resumeSuspendedToolLoopAfterDenial(ctx, approvalID, "shell command denied by operator")
}

func (a *Agent) CancelShellApproval(ctx context.Context, approvalID string) error {
	view, err := a.pendingShellApproval(approvalID)
	if err != nil {
		return err
	}
	if a.ShellRuntime == nil {
		return fmt.Errorf("shell runtime is nil")
	}
	if err := a.ShellRuntime.RecoverApproval(a.Contracts.ShellExecution, shell.PendingApprovalView{
		ApprovalID: view.ApprovalID,
		CommandID:  view.CommandID,
		SessionID:  view.SessionID,
		RunID:      view.RunID,
		OccurredAt: view.OccurredAt,
		ToolName:   view.ToolName,
		Command:    view.Command,
		Args:       append([]string{}, view.Args...),
		Cwd:        view.Cwd,
		Message:    view.Message,
	}, shell.ExecutionMeta{
		SessionID:   view.SessionID,
		RunID:       view.RunID,
		Source:      "runtime.shell_operator",
		ActorID:     a.Config.ID,
		ActorType:   "operator",
		RecordEvent: a.RecordEvent,
		Now:         a.Now,
		NewID:       a.NewID,
	}); err != nil {
		return err
	}
	if err := a.ShellRuntime.Deny(ctx, approvalID); err != nil {
		return err
	}
	a.discardSuspendedToolLoop(approvalID)
	return nil
}

func (a *Agent) pendingShellApproval(approvalID string) (projections.ShellCommandView, error) {
	projection := a.shellCommandProjection()
	if projection == nil {
		return projections.ShellCommandView{}, fmt.Errorf("shell command projection is not configured")
	}
	for _, view := range projection.SnapshotForSession("") {
		if view.Status == "approval_pending" && view.ApprovalID == approvalID {
			return view, nil
		}
	}
	return projections.ShellCommandView{}, fmt.Errorf("shell approval %q not found", approvalID)
}
