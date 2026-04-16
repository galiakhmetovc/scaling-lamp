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
