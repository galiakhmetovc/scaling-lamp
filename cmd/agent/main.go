package main

import (
	"context"
	"flag"
	"fmt"
	"io"
	"os"

	"teamd/internal/runtime"
)

func main() {
	if err := run(os.Args[1:], os.Stdout, os.Stderr); err != nil {
		fmt.Fprintf(os.Stderr, "%v\n", err)
		os.Exit(1)
	}
}

func run(args []string, stdout, stderr io.Writer) error {
	fs := flag.NewFlagSet("agent", flag.ContinueOnError)
	fs.SetOutput(stderr)

	configPath := fs.String("config", "", "path to root agent config")
	smokePrompt := fs.String("smoke", "", "send one smoke prompt through the configured provider pipeline")

	if err := fs.Parse(args); err != nil {
		return err
	}
	if *configPath == "" {
		return fmt.Errorf("missing required --config")
	}

	agent, err := runtime.BuildAgent(*configPath)
	if err != nil {
		return fmt.Errorf("build agent: %w", err)
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
