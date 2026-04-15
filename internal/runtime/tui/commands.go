package tui

import (
	"context"
	"net/url"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"teamd/internal/contracts"
	"teamd/internal/runtime"
)

func waitForUIEvent(ch <-chan runtime.UIEvent) tea.Cmd {
	return func() tea.Msg {
		event, ok := <-ch
		if !ok {
			return nil
		}
		return uiEventMsg(event)
	}
}

func runChatTurnCmd(agent *runtime.Agent, session *runtime.ChatSession, prompt string, overrides sessionOverrides) tea.Cmd {
	return func() tea.Msg {
		working := &runtime.ChatSession{
			SessionID: session.SessionID,
			Messages:  append([]contracts.Message{}, session.Messages...),
		}
		result, err := agent.ChatTurn(context.Background(), working, runtime.ChatTurnInput{
			Prompt:                prompt,
			MaxToolRoundsOverride: overrides.MaxToolRounds,
		})
		return chatTurnFinishedMsg{
			SessionID: session.SessionID,
			Result: runtimeResultMeta{
				Provider:     providerLabel(agent),
				Model:        result.Provider.Model,
				InputTokens:  result.Provider.Usage.InputTokens,
				OutputTokens: result.Provider.Usage.OutputTokens,
				TotalTokens:  result.Provider.Usage.TotalTokens,
				Content:      result.Provider.Message.Content,
			},
			Err: err,
		}
	}
}

func runBtwTurnCmd(agent *runtime.Agent, session *runtime.ChatSession, prompt, runID string) tea.Cmd {
	return func() tea.Msg {
		working := &runtime.ChatSession{
			SessionID: session.SessionID,
			Messages:  append([]contracts.Message{}, session.Messages...),
		}
		result, err := agent.BtwTurn(context.Background(), working, runtime.BtwTurnInput{Prompt: prompt})
		return btwTurnFinishedMsg{
			SessionID: session.SessionID,
			RunID:     runID,
			Prompt:    prompt,
			Result: runtimeResultMeta{
				Provider:     providerLabel(agent),
				Model:        result.Provider.Model,
				InputTokens:  result.Provider.Usage.InputTokens,
				OutputTokens: result.Provider.Usage.OutputTokens,
				TotalTokens:  result.Provider.Usage.TotalTokens,
				Content:      result.Provider.Message.Content,
			},
			Err: err,
		}
	}
}

func rebuildAgentCmd(configPath string) tea.Cmd {
	return func() tea.Msg {
		agent, err := runtime.BuildAgent(configPath)
		return rebuildFinishedMsg{Agent: agent, Err: err}
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
