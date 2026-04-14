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
	runtimecli "teamd/internal/runtime/cli"
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
		return runtimecli.RunChat(context.Background(), agent, *resumeID, stdin, stdout)
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
