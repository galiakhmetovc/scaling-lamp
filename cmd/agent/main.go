package main

import (
	"bufio"
	"context"
	"flag"
	"fmt"
	"io"
	"os"
	"strings"

	"teamd/internal/runtime"
)

func main() {
	if err := runWithIO(os.Args[1:], os.Stdin, os.Stdout, os.Stderr); err != nil {
		fmt.Fprintf(os.Stderr, "%v\n", err)
		os.Exit(1)
	}
}

func run(args []string, stdout, stderr io.Writer) error {
	return runWithIO(args, strings.NewReader(""), stdout, stderr)
}

func runWithIO(args []string, stdin io.Reader, stdout, stderr io.Writer) error {
	fs := flag.NewFlagSet("agent", flag.ContinueOnError)
	fs.SetOutput(stderr)

	configPath := fs.String("config", "", "path to root agent config")
	smokePrompt := fs.String("smoke", "", "send one smoke prompt through the configured provider pipeline")
	chatMode := fs.Bool("chat", false, "start interactive chat mode")
	resumeID := fs.String("resume", "", "resume an existing chat session by id")

	if err := fs.Parse(args); err != nil {
		return err
	}
	if *configPath == "" {
		return fmt.Errorf("missing required --config")
	}
	if err := loadDotEnv(".env"); err != nil {
		return fmt.Errorf("autoload .env: %w", err)
	}
	if *chatMode && *smokePrompt != "" {
		return fmt.Errorf("--chat and --smoke are mutually exclusive")
	}

	agent, err := runtime.BuildAgent(*configPath)
	if err != nil {
		return fmt.Errorf("build agent: %w", err)
	}
	if *chatMode {
		return runChat(context.Background(), agent, *resumeID, stdin, stdout)
	}
	if *smokePrompt == "" {
		return nil
	}

	result, err := agent.Smoke(context.Background(), runtime.SmokeInput{Prompt: *smokePrompt})
	if err != nil {
		return fmt.Errorf("smoke request: %w", err)
	}
	if _, err := fmt.Fprintln(stdout, result.Provider.Message.Content); err != nil {
		return fmt.Errorf("write smoke response: %w", err)
	}
	return nil
}

func runChat(ctx context.Context, agent *runtime.Agent, resumeID string, stdin io.Reader, stdout io.Writer) error {
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

	if _, err := fmt.Fprintf(stdout, "agent: %s\nsession: %s\nmode: %s\nenter twice to send, /exit to quit\n", agent.Config.ID, session.SessionID, mode); err != nil {
		return err
	}

	scanner := bufio.NewScanner(stdin)
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	buffer := make([]string, 0, 8)
	printPrompt := func(continuation bool) error {
		prefix := "> "
		if continuation {
			prefix = ". "
		}
		_, err := fmt.Fprint(stdout, prefix)
		return err
	}
	if err := printPrompt(false); err != nil {
		return err
	}
	sendBuffer := func() error {
		prompt := strings.Join(buffer, "\n")
		buffer = buffer[:0]
		if strings.TrimSpace(prompt) == "" {
			return nil
		}
		if _, err := fmt.Fprintln(stdout, "\nstatus: sending"); err != nil {
			return err
		}
		result, err := agent.ChatTurn(ctx, session, runtime.ChatTurnInput{
			Prompt: prompt,
			StreamObserver: func(delta string) {
				_, _ = io.WriteString(stdout, delta)
			},
		})
		if err != nil {
			return err
		}
		if result.Provider.Message.Content == "" {
			return fmt.Errorf("chat turn returned empty assistant content")
		}
		if _, err := fmt.Fprintf(stdout, "\nstatus: done | input %d | output %d | total %d\n", result.Provider.Usage.InputTokens, result.Provider.Usage.OutputTokens, result.Provider.Usage.TotalTokens); err != nil {
			return err
		}
		return nil
	}

	for scanner.Scan() {
		line := scanner.Text()
		if len(buffer) == 0 && strings.HasPrefix(line, "/") {
			switch strings.TrimSpace(line) {
			case "/exit":
				_, err := fmt.Fprintln(stdout)
				return err
			case "/help":
				if _, err := fmt.Fprintln(stdout, "\ncommands: /help /session /exit"); err != nil {
					return err
				}
			case "/session":
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
			if err := sendBuffer(); err != nil {
				return err
			}
			if err := printPrompt(false); err != nil {
				return err
			}
			continue
		}
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

func loadDotEnv(path string) error {
	file, err := os.Open(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	defer file.Close()

	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		name, value, ok := strings.Cut(line, "=")
		if !ok {
			continue
		}
		name = strings.TrimSpace(name)
		if name == "" {
			continue
		}
		if _, exists := os.LookupEnv(name); exists {
			continue
		}
		if err := os.Setenv(name, strings.TrimSpace(value)); err != nil {
			return err
		}
	}
	if err := scanner.Err(); err != nil {
		return err
	}
	return nil
}
