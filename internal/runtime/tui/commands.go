package tui

import (
	"context"
	"net/url"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"teamd/internal/runtime"
	"teamd/internal/runtime/daemon"
)

func waitForDaemonEnvelope(ch <-chan daemon.WebsocketEnvelope) tea.Cmd {
	return func() tea.Msg {
		envelope, ok := <-ch
		if !ok {
			return nil
		}
		return daemonEnvelopeMsg(envelope)
	}
}

func runChatTurnClientCmd(ctx context.Context, client OperatorClient, sessionID, prompt string, overrides sessionOverrides) tea.Cmd {
	return func() tea.Msg {
		result, err := client.SendChat(ctx, sessionID, prompt)
		return chatTurnFinishedMsg{
			SessionID: sessionID,
			Result:    result.Result,
			Queued:    result.Queued,
			Draft:     result.Draft,
			Session:   result.Session,
			Err:       err,
		}
	}
}

func runCancelApprovalAndSendCmd(ctx context.Context, client OperatorClient, sessionID, approvalID, prompt string) tea.Cmd {
	return func() tea.Msg {
		result, err := client.CancelApprovalAndSend(ctx, sessionID, approvalID, prompt)
		return chatTurnFinishedMsg{
			SessionID: sessionID,
			Result:    result.Result,
			Queued:    result.Queued,
			Draft:     result.Draft,
			Session:   result.Session,
			Err:       err,
		}
	}
}

func runBtwTurnClientCmd(client OperatorClient, sessionID, prompt, runID string) tea.Cmd {
	return func() tea.Msg {
		result, err := client.SendBtw(context.Background(), sessionID, prompt)
		return btwTurnFinishedMsg{
			SessionID: sessionID,
			RunID:     runID,
			Prompt:    prompt,
			Result:    result.Result,
			Err:       err,
		}
	}
}

func runShellActionCmd(ctx context.Context, client OperatorClient, sessionID, approvalID, action string) tea.Cmd {
	return func() tea.Msg {
		var (
			result ShellActionResult
			err    error
			status string
		)
		switch action {
		case "approve":
			result, err = client.ApproveShell(ctx, approvalID)
			status = "shell approval granted"
		case "deny":
			result, err = client.DenyShell(ctx, approvalID)
			status = "shell approval denied"
		case "allow_forever":
			result, err = client.ApproveShellAlways(ctx, approvalID)
			status = "shell approval granted and saved"
		case "deny_forever":
			result, err = client.DenyShellAlways(ctx, approvalID)
			status = "shell approval denied and saved"
		default:
			err = nil
			status = ""
		}
		return shellActionFinishedMsg{
			SessionID: sessionID,
			Result:    result,
			Status:    status,
			Err:       err,
		}
	}
}

func runKillShellCmd(ctx context.Context, client OperatorClient, sessionID, commandID string) tea.Cmd {
	return func() tea.Msg {
		result, err := client.KillShell(ctx, commandID)
		return shellActionFinishedMsg{
			SessionID: sessionID,
			Result:    result,
			Status:    "shell command kill requested",
			Err:       err,
		}
	}
}

func reloadSessionSnapshotCmd(ctx context.Context, client OperatorClient, sessionID string) tea.Cmd {
	return func() tea.Msg {
		session, err := client.GetSession(ctx, sessionID)
		return sessionSnapshotReloadedMsg{
			SessionID: sessionID,
			Session:   session,
			Err:       err,
		}
	}
}

func reloadSessionSnapshotAfterDelayCmd(ctx context.Context, client OperatorClient, sessionID string, delay time.Duration) tea.Cmd {
	return func() tea.Msg {
		if delay > 0 {
			timer := time.NewTimer(delay)
			defer timer.Stop()
			select {
			case <-ctx.Done():
				return sessionSnapshotReloadedMsg{SessionID: sessionID, Err: ctx.Err()}
			case <-timer.C:
			}
		}
		session, err := client.GetSession(ctx, sessionID)
		return sessionSnapshotReloadedMsg{
			SessionID: sessionID,
			Session:   session,
			Err:       err,
		}
	}
}

func tickClockCmd() tea.Cmd {
	return tea.Tick(time.Second, func(t time.Time) tea.Msg { return clockTickMsg(t) })
}

func providerLabel(agent *runtime.Agent) string {
	if agent == nil {
		return "unknown"
	}
	baseURL := agent.Contracts.ProviderRequest.Transport.Endpoint.Params.BaseURL
	if parsed, err := url.Parse(baseURL); err == nil && parsed.Host != "" {
		return parsed.Host
	}
	if agent.Contracts.ProviderRequest.Transport.ID != "" {
		return agent.Contracts.ProviderRequest.Transport.ID
	}
	return "provider"
}
