package tui

import (
	"context"

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
		_, err := agent.ChatTurn(context.Background(), working, runtime.ChatTurnInput{
			Prompt:                prompt,
			MaxToolRoundsOverride: overrides.MaxToolRounds,
		})
		return chatTurnFinishedMsg{SessionID: session.SessionID, Err: err}
	}
}

func rebuildAgentCmd(configPath string) tea.Cmd {
	return func() tea.Msg {
		agent, err := runtime.BuildAgent(configPath)
		return rebuildFinishedMsg{Agent: agent, Err: err}
	}
}
