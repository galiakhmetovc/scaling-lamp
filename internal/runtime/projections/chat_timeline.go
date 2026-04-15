package projections

import (
	"encoding/json"
	"fmt"
	"strings"

	"teamd/internal/runtime/eventing"
)

type ChatTimelineItemKind string

const (
	ChatTimelineItemMessage ChatTimelineItemKind = "message"
	ChatTimelineItemTool    ChatTimelineItemKind = "tool"
	ChatTimelineItemPlan    ChatTimelineItemKind = "plan"
)

type ChatTimelineItem struct {
	Kind    ChatTimelineItemKind `json:"kind"`
	Role    string               `json:"role,omitempty"`
	Content string               `json:"content"`
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
func (p *ChatTimelineProjection) SnapshotValue() any            { return p.snapshot }

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
		return ChatTimelineItem{Kind: ChatTimelineItemMessage, Role: role, Content: content}, true
	case eventing.EventToolCallStarted:
		name, _ := event.Payload["tool_name"].(string)
		if strings.TrimSpace(name) == "" {
			return ChatTimelineItem{}, false
		}
		return ChatTimelineItem{Kind: ChatTimelineItemTool, Content: fmt.Sprintf("**Tool** `%s`", name)}, true
	case eventing.EventToolCallCompleted:
		name, _ := event.Payload["tool_name"].(string)
		if strings.TrimSpace(name) == "" {
			return ChatTimelineItem{}, false
		}
		if errText, _ := event.Payload["error"].(string); strings.TrimSpace(errText) != "" {
			return ChatTimelineItem{Kind: ChatTimelineItemTool, Content: fmt.Sprintf("**Tool error** `%s`\n\n%s", name, errText)}, true
		}
		if resultText, _ := event.Payload["result_text"].(string); strings.TrimSpace(resultText) != "" {
			return ChatTimelineItem{Kind: ChatTimelineItemTool, Content: fmt.Sprintf("**Tool result** `%s`\n\n`%s`", name, summarizeTimelineText(resultText))}, true
		}
		return ChatTimelineItem{Kind: ChatTimelineItemTool, Content: fmt.Sprintf("**Tool done** `%s`", name)}, true
	case eventing.EventPlanCreated:
		goal, _ := event.Payload["goal"].(string)
		if strings.TrimSpace(goal) == "" {
			goal = "plan"
		}
		return ChatTimelineItem{Kind: ChatTimelineItemPlan, Content: fmt.Sprintf("**Plan created**\n\n`%s`", goal)}, true
	case eventing.EventPlanArchived:
		planID, _ := event.Payload["plan_id"].(string)
		if strings.TrimSpace(planID) == "" {
			planID = "plan"
		}
		return ChatTimelineItem{Kind: ChatTimelineItemPlan, Content: fmt.Sprintf("**Plan archived** `%s`", planID)}, true
	case eventing.EventTaskAdded:
		description, _ := event.Payload["description"].(string)
		if strings.TrimSpace(description) == "" {
			description = "task"
		}
		return ChatTimelineItem{Kind: ChatTimelineItemPlan, Content: fmt.Sprintf("**Task added**\n\n`%s`", description)}, true
	case eventing.EventTaskEdited:
		description, _ := event.Payload["description"].(string)
		if strings.TrimSpace(description) == "" {
			description = "task"
		}
		return ChatTimelineItem{Kind: ChatTimelineItemPlan, Content: fmt.Sprintf("**Task edited**\n\n`%s`", description)}, true
	case eventing.EventTaskStatusChanged:
		taskID, _ := event.Payload["task_id"].(string)
		newStatus, _ := event.Payload["new_status"].(string)
		if strings.TrimSpace(taskID) == "" {
			taskID = "task"
		}
		if strings.TrimSpace(newStatus) == "" {
			newStatus = "updated"
		}
		return ChatTimelineItem{Kind: ChatTimelineItemPlan, Content: fmt.Sprintf("**Task status** `%s` → `%s`", taskID, newStatus)}, true
	case eventing.EventTaskNoteAdded:
		taskID, _ := event.Payload["task_id"].(string)
		noteText, _ := event.Payload["note_text"].(string)
		if strings.TrimSpace(taskID) == "" {
			taskID = "task"
		}
		if strings.TrimSpace(noteText) == "" {
			noteText = "note"
		}
		return ChatTimelineItem{Kind: ChatTimelineItemPlan, Content: fmt.Sprintf("**Task note** `%s`\n\n%s", taskID, summarizeTimelineText(noteText))}, true
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
