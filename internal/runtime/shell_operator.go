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
			ToolName:   view.ToolName,
			Command:    view.Command,
			Args:       append([]string{}, view.Args...),
			Cwd:        view.Cwd,
			Message:    view.Message,
		})
	}
	return out
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
	return a.ShellRuntime.Approve(ctx, approvalID)
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
	return a.ShellRuntime.Deny(ctx, approvalID)
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
