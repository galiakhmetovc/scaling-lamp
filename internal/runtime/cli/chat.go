package cli

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"sort"
	"strings"

	"teamd/internal/provider"
	"teamd/internal/runtime"
	"teamd/internal/runtime/projections"
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
			ToolObserver: func(activity runtime.ToolActivity) {
				switch activity.Phase {
				case runtime.ToolActivityPhaseStarted:
					if agent.Contracts.Chat.Status.Params.ShowToolCalls {
						_, _ = fmt.Fprintf(stdout, "\n[tool] %s\n%s\n", activity.Name, summarizeToolArgs(activity))
					}
				case runtime.ToolActivityPhaseCompleted:
					if agent.Contracts.Chat.Status.Params.ShowToolResults {
						_, _ = fmt.Fprintf(stdout, "%s\n", summarizeToolResult(activity))
					}
					if agent.Contracts.Chat.Status.Params.ShowPlanAfterPlanTools && isPlanTool(activity.Name) {
						if head, ok := agent.CurrentPlanHead(); ok {
							if rendered := renderPlan(head); rendered != "" {
								_, _ = fmt.Fprintln(stdout, rendered)
							}
						}
					}
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

func isPlanTool(name string) bool {
	switch name {
	case "init_plan", "add_task", "set_task_status", "add_task_note", "edit_task":
		return true
	default:
		return false
	}
}

func summarizeToolArgs(activity runtime.ToolActivity) string {
	switch activity.Name {
	case "init_plan":
		return "goal: " + stringArg(activity.Arguments, "goal")
	case "add_task":
		return "description: " + stringArg(activity.Arguments, "description")
	case "set_task_status":
		return fmt.Sprintf("task: %s | status: %s", stringArg(activity.Arguments, "task_id"), stringArg(activity.Arguments, "new_status"))
	case "add_task_note":
		return fmt.Sprintf("task: %s | note: %s", stringArg(activity.Arguments, "task_id"), stringArg(activity.Arguments, "note_text"))
	case "edit_task":
		return fmt.Sprintf("task: %s | description: %s", stringArg(activity.Arguments, "task_id"), stringArg(activity.Arguments, "new_description"))
	case "fs_list", "fs_read_text", "fs_write_text", "fs_patch_text", "fs_mkdir", "fs_trash":
		return "path: " + stringArg(activity.Arguments, "path")
	case "fs_move":
		return fmt.Sprintf("src: %s | dest: %s", stringArg(activity.Arguments, "src"), stringArg(activity.Arguments, "dest"))
	case "shell_exec":
		command := stringArg(activity.Arguments, "command")
		args := stringSliceArg(activity.Arguments, "args")
		if len(args) == 0 {
			return "command: " + command
		}
		return fmt.Sprintf("command: %s | args: %s", command, strings.Join(args, " "))
	default:
		return "args: (see events.jsonl)"
	}
}

func summarizeToolResult(activity runtime.ToolActivity) string {
	if activity.ErrorText != "" {
		return "status: error | " + activity.ErrorText
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(activity.ResultText), &payload); err != nil {
		return "status: ok"
	}
	switch activity.Name {
	case "init_plan":
		return fmt.Sprintf("status: ok | plan_id: %v", payload["plan_id"])
	case "add_task":
		return fmt.Sprintf("status: ok | task_id: %v", payload["task_id"])
	case "set_task_status", "add_task_note", "edit_task", "fs_write_text", "fs_patch_text", "fs_mkdir", "fs_move", "fs_trash":
		return "status: ok"
	case "fs_list":
		if entries, ok := payload["entries"].([]any); ok {
			return fmt.Sprintf("status: ok | entries: %d", len(entries))
		}
		return "status: ok"
	case "fs_read_text":
		if size, ok := payload["bytes"]; ok {
			return fmt.Sprintf("status: ok | bytes: %v", size)
		}
		return "status: ok"
	case "shell_exec":
		return fmt.Sprintf("status: %v | exit: %v | duration: %vms", payload["status"], payload["exit_code"], payload["duration_ms"])
	default:
		return "status: ok"
	}
}

func stringArg(args map[string]any, key string) string {
	if value, ok := args[key].(string); ok {
		return value
	}
	return ""
}

func stringSliceArg(args map[string]any, key string) []string {
	value, ok := args[key]
	if !ok || value == nil {
		return nil
	}
	switch typed := value.(type) {
	case []string:
		return typed
	case []any:
		out := make([]string, 0, len(typed))
		for _, item := range typed {
			if text, ok := item.(string); ok {
				out = append(out, text)
			}
		}
		return out
	default:
		return nil
	}
}

func renderPlan(head projections.PlanHeadSnapshot) string {
	if head.Plan.ID == "" {
		return ""
	}
	lines := []string{"[plan]", "goal: " + head.Plan.Goal}
	tasks := orderedTasks(head.Tasks)
	for _, task := range tasks {
		if task.ParentTaskID != "" {
			continue
		}
		renderTask(&lines, head, task, tasks, 0)
	}
	return strings.Join(lines, "\n")
}

func orderedTasks(tasks map[string]projections.PlanTaskView) []projections.PlanTaskView {
	out := make([]projections.PlanTaskView, 0, len(tasks))
	for _, task := range tasks {
		out = append(out, task)
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].Order == out[j].Order {
			return out[i].ID < out[j].ID
		}
		return out[i].Order < out[j].Order
	})
	return out
}

func renderTask(lines *[]string, head projections.PlanHeadSnapshot, task projections.PlanTaskView, all []projections.PlanTaskView, depth int) {
	prefix := strings.Repeat("  ", depth)
	status := "[todo]"
	switch task.Status {
	case "done":
		status = "[done]"
	case "in_progress":
		status = "[doing]"
	case "blocked":
		status = "[blocked]"
	case "cancelled":
		status = "[cancelled]"
	default:
		if head.WaitingOnDependencies[task.ID] {
			status = "[waiting]"
		} else if head.Ready[task.ID] {
			status = "[ready]"
		}
	}
	*lines = append(*lines, fmt.Sprintf("%s%s %s", prefix, status, task.Description))
	for _, child := range all {
		if child.ParentTaskID == task.ID {
			renderTask(lines, head, child, all, depth+1)
		}
	}
}
