package projections

import (
	"encoding/json"
	"fmt"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"teamd/internal/runtime/eventing"
)

type ChatTimelineItemKind string

const (
	ChatTimelineItemMessage ChatTimelineItemKind = "message"
	ChatTimelineItemTool    ChatTimelineItemKind = "tool"
	ChatTimelineItemPlan    ChatTimelineItemKind = "plan"
)

type ChatTimelineItem struct {
	OccurredAt time.Time            `json:"occurred_at"`
	Kind       ChatTimelineItemKind `json:"kind"`
	Role       string               `json:"role,omitempty"`
	Content    string               `json:"content"`
}

type ChatTimelineSnapshot struct {
	Sessions map[string][]ChatTimelineItem `json:"sessions"`
}

type ChatTimelineProjection struct {
	snapshot ChatTimelineSnapshot
}

func NewChatTimelineProjection() *ChatTimelineProjection {
	return &ChatTimelineProjection{
		snapshot: ChatTimelineSnapshot{Sessions: map[string][]ChatTimelineItem{}},
	}
}

func (p *ChatTimelineProjection) ID() string { return "chat_timeline" }

func (p *ChatTimelineProjection) Apply(event eventing.Event) error {
	if event.Kind == eventing.EventSessionDeleted {
		if p.snapshot.Sessions != nil {
			delete(p.snapshot.Sessions, event.AggregateID)
		}
		return nil
	}
	sessionID, ok := sessionIDForTimelineEvent(event)
	if !ok {
		return nil
	}
	item, ok := buildTimelineItem(event)
	if !ok {
		return nil
	}
	if p.snapshot.Sessions == nil {
		p.snapshot.Sessions = map[string][]ChatTimelineItem{}
	}
	p.snapshot.Sessions[sessionID] = append(p.snapshot.Sessions[sessionID], item)
	return nil
}

func (p *ChatTimelineProjection) Snapshot() ChatTimelineSnapshot { return p.snapshot }
func (p *ChatTimelineProjection) SnapshotValue() any             { return p.snapshot }

func (p *ChatTimelineProjection) SnapshotForSession(sessionID string) []ChatTimelineItem {
	if p.snapshot.Sessions == nil {
		return nil
	}
	items := p.snapshot.Sessions[sessionID]
	return append([]ChatTimelineItem{}, items...)
}

func (p *ChatTimelineProjection) RestoreSnapshot(raw []byte) error {
	var snapshot ChatTimelineSnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore chat timeline snapshot: %w", err)
	}
	if snapshot.Sessions == nil {
		snapshot.Sessions = map[string][]ChatTimelineItem{}
	}
	p.snapshot = snapshot
	return nil
}

func sessionIDForTimelineEvent(event eventing.Event) (string, bool) {
	switch event.Kind {
	case eventing.EventMessageRecorded,
		eventing.EventToolCallStarted,
		eventing.EventToolCallCompleted,
		eventing.EventPlanCreated,
		eventing.EventPlanArchived,
		eventing.EventTaskAdded,
		eventing.EventTaskEdited,
		eventing.EventTaskStatusChanged,
		eventing.EventTaskNoteAdded:
		sessionID, _ := event.Payload["session_id"].(string)
		return sessionID, strings.TrimSpace(sessionID) != ""
	default:
		return "", false
	}
}

func buildTimelineItem(event eventing.Event) (ChatTimelineItem, bool) {
	switch event.Kind {
	case eventing.EventMessageRecorded:
		role, _ := event.Payload["role"].(string)
		content, _ := event.Payload["content"].(string)
		if strings.TrimSpace(role) == "" || content == "" {
			return ChatTimelineItem{}, false
		}
		return ChatTimelineItem{OccurredAt: event.OccurredAt, Kind: ChatTimelineItemMessage, Role: role, Content: content}, true
	case eventing.EventToolCallStarted:
		name, _ := event.Payload["tool_name"].(string)
		if strings.TrimSpace(name) == "" {
			return ChatTimelineItem{}, false
		}
		return ChatTimelineItem{
			Kind:       ChatTimelineItemTool,
			OccurredAt: event.OccurredAt,
			Content:    compactTimelineToolLine(name, payloadMap(event.Payload["arguments"]), "", "", true),
		}, true
	case eventing.EventToolCallCompleted:
		name, _ := event.Payload["tool_name"].(string)
		if strings.TrimSpace(name) == "" {
			return ChatTimelineItem{}, false
		}
		if errText, _ := event.Payload["error"].(string); strings.TrimSpace(errText) != "" {
			return ChatTimelineItem{
				Kind:       ChatTimelineItemTool,
				OccurredAt: event.OccurredAt,
				Content:    compactTimelineToolLine(name, payloadMap(event.Payload["arguments"]), "", errText, false),
			}, true
		}
		if resultText, _ := event.Payload["result_text"].(string); strings.TrimSpace(resultText) != "" {
			return ChatTimelineItem{
				Kind:       ChatTimelineItemTool,
				OccurredAt: event.OccurredAt,
				Content:    compactTimelineToolLine(name, payloadMap(event.Payload["arguments"]), resultText, "", false),
			}, true
		}
		return ChatTimelineItem{OccurredAt: event.OccurredAt, Kind: ChatTimelineItemTool, Content: compactTimelineToolLine(name, payloadMap(event.Payload["arguments"]), "", "", false)}, true
	case eventing.EventPlanCreated:
		goal, _ := event.Payload["goal"].(string)
		if strings.TrimSpace(goal) == "" {
			goal = "plan"
		}
		return ChatTimelineItem{OccurredAt: event.OccurredAt, Kind: ChatTimelineItemPlan, Content: fmt.Sprintf("**Plan created**\n\n`%s`", goal)}, true
	case eventing.EventPlanArchived:
		planID, _ := event.Payload["plan_id"].(string)
		if strings.TrimSpace(planID) == "" {
			planID = "plan"
		}
		return ChatTimelineItem{OccurredAt: event.OccurredAt, Kind: ChatTimelineItemPlan, Content: fmt.Sprintf("**Plan archived** `%s`", planID)}, true
	case eventing.EventTaskAdded:
		description, _ := event.Payload["description"].(string)
		if strings.TrimSpace(description) == "" {
			description = "task"
		}
		return ChatTimelineItem{OccurredAt: event.OccurredAt, Kind: ChatTimelineItemPlan, Content: fmt.Sprintf("**Task added**\n\n`%s`", description)}, true
	case eventing.EventTaskEdited:
		description, _ := event.Payload["description"].(string)
		if strings.TrimSpace(description) == "" {
			description = "task"
		}
		return ChatTimelineItem{OccurredAt: event.OccurredAt, Kind: ChatTimelineItemPlan, Content: fmt.Sprintf("**Task edited**\n\n`%s`", description)}, true
	case eventing.EventTaskStatusChanged:
		taskID, _ := event.Payload["task_id"].(string)
		newStatus, _ := event.Payload["new_status"].(string)
		if strings.TrimSpace(taskID) == "" {
			taskID = "task"
		}
		if strings.TrimSpace(newStatus) == "" {
			newStatus = "updated"
		}
		return ChatTimelineItem{OccurredAt: event.OccurredAt, Kind: ChatTimelineItemPlan, Content: fmt.Sprintf("**Task status** `%s` → `%s`", taskID, newStatus)}, true
	case eventing.EventTaskNoteAdded:
		taskID, _ := event.Payload["task_id"].(string)
		noteText, _ := event.Payload["note_text"].(string)
		if strings.TrimSpace(taskID) == "" {
			taskID = "task"
		}
		if strings.TrimSpace(noteText) == "" {
			noteText = "note"
		}
		return ChatTimelineItem{OccurredAt: event.OccurredAt, Kind: ChatTimelineItemPlan, Content: fmt.Sprintf("**Task note** `%s`\n\n%s", taskID, summarizeTimelineText(noteText))}, true
	default:
		return ChatTimelineItem{}, false
	}
}

func summarizeTimelineText(input string) string {
	text := strings.TrimSpace(input)
	if text == "" {
		return ""
	}
	text = strings.ReplaceAll(text, "\n", " ")
	if len(text) > 80 {
		return text[:77] + "..."
	}
	return text
}

func compactTimelineToolLine(toolName string, arguments map[string]any, resultText, errorText string, started bool) string {
	base := compactTimelineToolInvocation(toolName, arguments)
	switch {
	case started:
		return base + " started"
	case strings.Contains(errorText, "requires approval"):
		return base + " approval required"
	case strings.TrimSpace(errorText) != "":
		return base + " error: " + sanitizeTimelineToolError(toolName, errorText)
	default:
		if summary := compactTimelineToolResult(toolName, resultText); summary != "" {
			return base + " " + summary
		}
		return base + " ok"
	}
}

func compactTimelineToolInvocation(toolName string, arguments map[string]any) string {
	switch toolName {
	case "plan_snapshot":
		return "plan_snapshot current plan"
	case "fs_read_lines":
		path := compactTimelinePath(arguments["path"])
		if path == "" {
			return "fs_read_lines"
		}
		return fmt.Sprintf("fs_read_lines %s:%s-%s", path, compactTimelineInt(arguments["start_line"]), compactTimelineInt(arguments["end_line"]))
	case "fs_read_text", "fs_write_text", "fs_patch_text", "fs_replace_lines", "fs_replace_in_line", "fs_insert_text", "fs_mkdir", "fs_trash":
		if path := compactTimelinePath(arguments["path"]); path != "" {
			return toolName + " " + path
		}
		return toolName
	case "fs_list":
		if path := compactTimelinePath(arguments["path"]); path != "" {
			return "fs_list " + path
		}
		return "fs_list"
	case "fs_search_text":
		path := compactTimelinePath(arguments["path"])
		query := compactTimelineString(arguments["query"])
		switch {
		case path != "" && query != "":
			return fmt.Sprintf("fs_search_text %s %q", path, query)
		case path != "":
			return "fs_search_text " + path
		case query != "":
			return fmt.Sprintf("fs_search_text %q", query)
		default:
			return "fs_search_text"
		}
	case "fs_find_in_files", "fs_replace_in_files":
		if query := compactTimelineString(arguments["query"]); query != "" {
			return fmt.Sprintf("%s %q", toolName, query)
		}
		return toolName
	case "fs_move":
		src := compactTimelinePath(arguments["src"])
		dest := compactTimelinePath(arguments["dest"])
		if src != "" && dest != "" {
			return fmt.Sprintf("fs_move %s -> %s", src, dest)
		}
		return "fs_move"
	case "shell_exec", "shell_start":
		command := compactTimelineString(arguments["command"])
		args := compactTimelineStringSlice(arguments["args"])
		if len(args) == 0 {
			return toolName + " " + command
		}
		return toolName + " " + command + " " + strings.Join(args, " ")
	case "shell_poll", "shell_kill":
		return toolName + " " + compactTimelineString(arguments["command_id"])
	default:
		if summary := summarizeTimelineArguments(arguments); summary != "" {
			return toolName + " " + summary
		}
		return toolName
	}
}

func compactTimelineToolResult(toolName, resultText string) string {
	if strings.TrimSpace(resultText) == "" {
		return ""
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(resultText), &payload); err != nil {
		return summarizeTimelineText(resultText)
	}
	switch toolName {
	case "fs_read_lines":
		if lines, ok := payload["lines"].([]any); ok {
			return fmt.Sprintf("ok %d lines", len(lines))
		}
	case "fs_list":
		if count, ok := compactTimelineIntFromAny(payload["entry_count"]); ok {
			return fmt.Sprintf("ok %d entries", count)
		}
		if entries, ok := payload["entries"].([]any); ok {
			return fmt.Sprintf("ok %d entries", len(entries))
		}
	case "shell_exec":
		status := compactTimelineString(payload["status"])
		exitCode, ok := compactTimelineIntFromAny(payload["exit_code"])
		if status == "" {
			status = "ok"
		}
		if ok {
			return fmt.Sprintf("%s exit=%d", status, exitCode)
		}
		return status
	case "shell_start":
		if id := compactTimelineString(payload["command_id"]); id != "" {
			return "running " + id
		}
	}
	return "ok"
}

func payloadMap(v any) map[string]any {
	out, _ := v.(map[string]any)
	return out
}

func summarizeTimelineArguments(arguments map[string]any) string {
	if len(arguments) == 0 {
		return ""
	}
	parts := make([]string, 0, len(arguments))
	if command := compactTimelineString(arguments["command"]); command != "" {
		parts = append(parts, "command="+command)
	}
	if path := compactTimelineString(arguments["path"]); path != "" {
		parts = append(parts, "path="+path)
	}
	if description := compactTimelineString(arguments["description"]); description != "" {
		parts = append(parts, "description="+description)
	}
	if goal := compactTimelineString(arguments["goal"]); goal != "" {
		parts = append(parts, "goal="+goal)
	}
	if len(parts) == 0 {
		return fmt.Sprintf("%d fields", len(arguments))
	}
	return strings.Join(parts, " | ")
}

func sanitizeTimelineToolError(toolName, text string) string {
	out := strings.TrimSpace(strings.ReplaceAll(text, "\n", " "))
	out = strings.TrimPrefix(out, fmt.Sprintf("tool call %q: ", toolName))
	out = strings.TrimPrefix(out, fmt.Sprintf("tool call %q ", toolName))
	out = strings.TrimPrefix(out, "tool call ")
	if strings.HasPrefix(out, "\""+toolName+"\"") {
		out = strings.TrimSpace(strings.TrimPrefix(out, "\""+toolName+"\""))
	}
	return strings.TrimLeft(out, ": ")
}

func compactTimelinePath(v any) string {
	path := compactTimelineString(v)
	if path == "" {
		return ""
	}
	dir, file := filepath.Split(path)
	if dir == "" || dir == "/" {
		return path
	}
	base := filepath.Base(strings.TrimRight(dir, "/"))
	if base == "." || base == "/" || base == "" {
		return path
	}
	return filepath.Join(base, file)
}

func compactTimelineString(v any) string {
	s, _ := v.(string)
	return strings.TrimSpace(s)
}

func compactTimelineInt(v any) string {
	if n, ok := compactTimelineIntFromAny(v); ok {
		return strconv.Itoa(n)
	}
	return "?"
}

func compactTimelineIntFromAny(v any) (int, bool) {
	switch value := v.(type) {
	case int:
		return value, true
	case int32:
		return int(value), true
	case int64:
		return int(value), true
	case float64:
		return int(value), true
	case json.Number:
		n, err := value.Int64()
		if err == nil {
			return int(n), true
		}
	case string:
		n, err := strconv.Atoi(strings.TrimSpace(value))
		if err == nil {
			return n, true
		}
	}
	return 0, false
}

func compactTimelineStringSlice(v any) []string {
	switch typed := v.(type) {
	case []string:
		return append([]string(nil), typed...)
	case []any:
		out := make([]string, 0, len(typed))
		for _, item := range typed {
			if value, ok := item.(string); ok {
				out = append(out, value)
			}
		}
		return out
	default:
		return nil
	}
}
