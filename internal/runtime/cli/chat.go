package cli

import (
	"bufio"
	"context"
	"fmt"
	"io"
	"strings"

	"teamd/internal/provider"
	"teamd/internal/runtime"
)

func RunChat(ctx context.Context, agent *runtime.Agent, resumeID string, stdin io.Reader, stdout io.Writer) error {
	if agent.Contracts.Chat.Input.Strategy == "" {
		return fmt.Errorf("chat mode requires chat contract configuration")
	}
	var (
		session *runtime.ChatSession
		err     error
		mode    = "new"
	)
	if strings.TrimSpace(resumeID) != "" {
		session, err = agent.ResumeChatSession(ctx, resumeID)
		mode = "resumed"
	} else {
		session, err = agent.NewChatSession()
	}
	if err != nil {
		return err
	}

	if agent.Contracts.Chat.Status.Params.ShowHeader {
		if _, err := fmt.Fprintf(stdout, "agent: %s\nsession: %s\nmode: %s\nenter twice to send, %s to quit\n", agent.Config.ID, session.SessionID, mode, agent.Contracts.Chat.Command.Params.ExitCommand); err != nil {
			return err
		}
	}

	scanner := bufio.NewScanner(stdin)
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	buffer := make([]string, 0, 8)
	emptyLineThreshold := agent.Contracts.Chat.Submit.Params.EmptyLineThreshold
	if emptyLineThreshold <= 0 {
		emptyLineThreshold = 1
	}
	emptyLines := 0

	printPrompt := func(continuation bool) error {
		prompt := agent.Contracts.Chat.Input.Params.PrimaryPrompt
		if continuation {
			prompt = agent.Contracts.Chat.Input.Params.ContinuationPrompt
		}
		if prompt == "" {
			if continuation {
				prompt = ". "
			} else {
				prompt = "> "
			}
		}
		_, err := fmt.Fprint(stdout, prompt)
		return err
	}

	sendBuffer := func() error {
		prompt := strings.Join(buffer, "\n")
		buffer = buffer[:0]
		emptyLines = 0
		if strings.TrimSpace(prompt) == "" {
			return nil
		}
		if _, err := fmt.Fprintln(stdout, "\nstatus: sending"); err != nil {
			return err
		}
		result, err := agent.ChatTurn(ctx, session, runtime.ChatTurnInput{
			Prompt: prompt,
			StreamObserver: func(event provider.StreamEvent) {
				if event.Kind == provider.StreamEventText {
					_, _ = io.WriteString(stdout, event.Text)
				}
			},
		})
		if err != nil {
			return err
		}
		if result.Provider.Message.Content == "" {
			return fmt.Errorf("chat turn returned empty assistant content")
		}
		if agent.Contracts.Chat.Output.Params.ShowFinalNewline {
			if _, err := fmt.Fprintln(stdout); err != nil {
				return err
			}
		}
		if agent.Contracts.Chat.Status.Params.ShowUsage {
			if _, err := fmt.Fprintf(stdout, "status: done | input %d | output %d | total %d\n", result.Provider.Usage.InputTokens, result.Provider.Usage.OutputTokens, result.Provider.Usage.TotalTokens); err != nil {
				return err
			}
		}
		return nil
	}

	if err := printPrompt(false); err != nil {
		return err
	}

	for scanner.Scan() {
		line := scanner.Text()
		if len(buffer) == 0 && strings.HasPrefix(line, "/") {
			switch strings.TrimSpace(line) {
			case agent.Contracts.Chat.Command.Params.ExitCommand:
				_, err := fmt.Fprintln(stdout)
				return err
			case agent.Contracts.Chat.Command.Params.HelpCommand:
				if _, err := fmt.Fprintf(stdout, "\ncommands: %s %s %s\n", agent.Contracts.Chat.Command.Params.HelpCommand, agent.Contracts.Chat.Command.Params.SessionCommand, agent.Contracts.Chat.Command.Params.ExitCommand); err != nil {
					return err
				}
			case agent.Contracts.Chat.Command.Params.SessionCommand:
				if _, err := fmt.Fprintf(stdout, "\nsession: %s\n", session.SessionID); err != nil {
					return err
				}
			default:
				if _, err := fmt.Fprintf(stdout, "\nunknown command: %s\n", strings.TrimSpace(line)); err != nil {
					return err
				}
			}
			if err := printPrompt(false); err != nil {
				return err
			}
			continue
		}
		if line == "" {
			if len(buffer) == 0 {
				if err := printPrompt(false); err != nil {
					return err
				}
				continue
			}
			emptyLines++
			if emptyLines >= emptyLineThreshold {
				if err := sendBuffer(); err != nil {
					return err
				}
				if err := printPrompt(false); err != nil {
					return err
				}
				continue
			}
			buffer = append(buffer, "")
			if err := printPrompt(true); err != nil {
				return err
			}
			continue
		}
		emptyLines = 0
		buffer = append(buffer, line)
		if err := printPrompt(true); err != nil {
			return err
		}
	}
	if err := scanner.Err(); err != nil {
		return err
	}
	if len(buffer) > 0 {
		if err := sendBuffer(); err != nil {
			return err
		}
	}
	return nil
}
