package main

import (
	"bufio"
	"context"
	"flag"
	"fmt"
	"io"
	"os"
	"strconv"
	"strings"

	"golang.org/x/term"
	"teamd/internal/runtime"
	runtimecli "teamd/internal/runtime/cli"
	runtimedaemon "teamd/internal/runtime/daemon"
	"teamd/internal/runtime/eventing"
	runtimetui "teamd/internal/runtime/tui"
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
	daemonMode := fs.Bool("daemon", false, "start daemon control-plane server")
	resumeID := fs.String("resume", "", "resume an existing chat session by id")
	inspectSessionID := fs.String("inspect-session", "", "inspect recorded events for a session")
	inspectRunID := fs.String("inspect-run", "", "inspect recorded events for a run")
	inspectKind := fs.String("inspect-kind", "", "filter inspection output by event kind")
	inspectLimit := fs.Int("inspect-limit", 0, "tail inspection output to the last N matching events")

	if err := fs.Parse(args); err != nil {
		return err
	}
	if *configPath == "" {
		return fmt.Errorf("missing required --config")
	}
	if err := loadDotEnv(".env"); err != nil {
		return fmt.Errorf("autoload .env: %w", err)
	}
	modeCount := 0
	for _, enabled := range []bool{*chatMode, *daemonMode, *smokePrompt != "", *inspectSessionID != "", *inspectRunID != ""} {
		if enabled {
			modeCount++
		}
	}
	if modeCount > 1 {
		return fmt.Errorf("--chat, --daemon, --smoke, and inspect flags are mutually exclusive")
	}
	if *inspectSessionID != "" && *inspectRunID != "" {
		return fmt.Errorf("--inspect-session and --inspect-run are mutually exclusive")
	}
	if *inspectLimit < 0 {
		return fmt.Errorf("--inspect-limit must be >= 0")
	}

	agent, err := runtime.BuildAgent(*configPath)
	if err != nil {
		return fmt.Errorf("build agent: %w", err)
	}
	if *inspectSessionID != "" || *inspectRunID != "" {
		opts := runtime.InspectOptions{
			Kind:  eventing.EventKind(*inspectKind),
			Limit: *inspectLimit,
		}
		var report runtime.InspectionReport
		if *inspectSessionID != "" {
			report, err = agent.InspectSession(context.Background(), *inspectSessionID, opts)
		} else {
			report, err = agent.InspectRun(context.Background(), *inspectRunID, opts)
		}
		if err != nil {
			return fmt.Errorf("inspect events: %w", err)
		}
		if err := renderInspection(stdout, report); err != nil {
			return fmt.Errorf("write inspection output: %w", err)
		}
		return nil
	}
	if *daemonMode {
		server, err := runtimedaemon.New(agent)
		if err != nil {
			return fmt.Errorf("build daemon server: %w", err)
		}
		if _, err := fmt.Fprintf(stdout, "daemon listening on %s\n", server.ListenAddr()); err != nil {
			return fmt.Errorf("write daemon startup line: %w", err)
		}
		return server.ListenAndServe(context.Background())
	}
	if *chatMode {
		if file, ok := stdin.(*os.File); ok && term.IsTerminal(int(file.Fd())) {
			return runtimetui.Run(context.Background(), agent, *resumeID, stdin, stdout)
		}
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

func renderInspection(stdout io.Writer, report runtime.InspectionReport) error {
	if _, err := fmt.Fprintf(stdout, "Inspection: %s %s\n", report.Scope, report.ScopeID); err != nil {
		return err
	}
	if _, err := fmt.Fprintf(stdout, "Matching events: %d\n", report.Matching); err != nil {
		return err
	}
	if report.Failure != nil {
		if _, err := fmt.Fprintln(stdout, ""); err != nil {
			return err
		}
		if _, err := fmt.Fprintln(stdout, "Failure Summary"); err != nil {
			return err
		}
		if _, err := fmt.Fprintf(stdout, "- run: %s\n", report.Failure.RunID); err != nil {
			return err
		}
		if report.Failure.Error != "" {
			if _, err := fmt.Fprintf(stdout, "- error: %s\n", report.Failure.Error); err != nil {
				return err
			}
		}
		for _, toolFailure := range report.Failure.ToolErrors {
			label := toolFailure.Name
			if label == "" {
				label = "tool"
			}
			if _, err := fmt.Fprintf(stdout, "- tool error: %s: %s\n", label, toolFailure.Error); err != nil {
				return err
			}
		}
	}
	if report.Diagnostics != nil {
		if _, err := fmt.Fprintln(stdout, ""); err != nil {
			return err
		}
		if _, err := fmt.Fprintln(stdout, "Diagnostics"); err != nil {
			return err
		}
		for _, runID := range report.Diagnostics.StuckRuns {
			if _, err := fmt.Fprintf(stdout, "- stuck run: %s\n", runID); err != nil {
				return err
			}
		}
		for _, command := range report.Diagnostics.ShellCommands {
			line := fmt.Sprintf("- shell command: %s status=%s", command.CommandID, command.Status)
			if command.Command != "" {
				line += " command=" + strconv.Quote(command.Command)
			}
			if command.LastChunk != "" {
				line += " last_chunk=" + strconv.Quote(command.LastChunk)
			}
			if command.Error != "" {
				line += " error=" + strconv.Quote(command.Error)
			}
			if _, err := fmt.Fprintln(stdout, line); err != nil {
				return err
			}
		}
		if len(report.Diagnostics.RecoveryHints) > 0 {
			if _, err := fmt.Fprintln(stdout, "Recovery Hints"); err != nil {
				return err
			}
			for _, hint := range report.Diagnostics.RecoveryHints {
				if _, err := fmt.Fprintf(stdout, "- %s\n", hint); err != nil {
					return err
				}
			}
		}
	}
	if _, err := fmt.Fprintln(stdout, ""); err != nil {
		return err
	}
	if _, err := fmt.Fprintln(stdout, "Events"); err != nil {
		return err
	}
	for _, event := range report.Events {
		if _, err := fmt.Fprintf(stdout, "- #%d %s %s", event.Sequence, event.OccurredAt.Format("2006-01-02 15:04:05Z07:00"), event.Kind); err != nil {
			return err
		}
		extra := inspectionEventSummary(event)
		if extra != "" {
			if _, err := fmt.Fprintf(stdout, " | %s", extra); err != nil {
				return err
			}
		}
		if _, err := fmt.Fprintln(stdout); err != nil {
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

func inspectionEventSummary(event eventing.Event) string {
	var parts []string
	if event.AggregateType == eventing.AggregateRun && event.AggregateID != "" {
		parts = append(parts, "run="+event.AggregateID)
	}
	if sessionID := payloadValue(event.Payload, "session_id"); sessionID != "" {
		parts = append(parts, "session="+sessionID)
	}
	if toolName := payloadValue(event.Payload, "tool_name"); toolName != "" {
		parts = append(parts, "tool="+toolName)
	}
	if commandID := payloadValue(event.Payload, "command_id"); commandID != "" {
		parts = append(parts, "command_id="+commandID)
	}
	if command := payloadValue(event.Payload, "command"); command != "" {
		parts = append(parts, "command="+strconv.Quote(command))
	}
	if errText := payloadValue(event.Payload, "error"); errText != "" {
		parts = append(parts, "error="+strconv.Quote(errText))
	}
	if prompt := payloadValue(event.Payload, "prompt"); prompt != "" {
		parts = append(parts, "prompt="+strconv.Quote(prompt))
	}
	if resultText := payloadValue(event.Payload, "result_text"); resultText != "" {
		parts = append(parts, "result="+strconv.Quote(resultText))
	}
	if len(event.TraceRefs) > 0 {
		parts = append(parts, "trace="+event.TraceRefs[0])
	}
	if len(event.ArtifactRefs) > 0 {
		parts = append(parts, "artifact="+event.ArtifactRefs[0])
	}
	return strings.Join(parts, " ")
}

func payloadValue(payload map[string]any, key string) string {
	if payload == nil {
		return ""
	}
	value, _ := payload[key].(string)
	return value
}
