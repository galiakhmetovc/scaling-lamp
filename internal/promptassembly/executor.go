package promptassembly

import (
	"fmt"
	"os"
	"slices"
	"strings"

	"teamd/internal/contracts"
	"teamd/internal/runtime/projections"
)

type Input struct {
	SessionID   string
	Transcript  projections.TranscriptSnapshot
	PlanHead    projections.PlanHeadSnapshot
	RawMessages []contracts.Message
}

type Executor struct{}

func NewExecutor() *Executor {
	return &Executor{}
}

func (e *Executor) Build(contract contracts.PromptAssemblyContract, input Input) ([]contracts.Message, error) {
	if e == nil {
		return nil, fmt.Errorf("prompt-assembly executor is nil")
	}
	transcript := append([]contracts.Message{}, input.Transcript.Sessions[input.SessionID]...)
	rawMessages := append([]contracts.Message{}, input.RawMessages...)

	systemPrompt, err := e.buildSystemPrompt(contract.SystemPrompt)
	if err != nil {
		return nil, err
	}
	sessionHead, err := e.buildSessionHead(contract.SessionHead, input.SessionID, transcript, input.PlanHead)
	if err != nil {
		return nil, err
	}

	out := make([]contracts.Message, 0, len(rawMessages)+2)
	switch {
	case sessionHead.Role != "" && contract.SessionHead.Params.Placement == "message0":
		out = append(out, sessionHead)
		if systemPrompt.Role != "" {
			out = append(out, systemPrompt)
		}
	case systemPrompt.Role != "":
		out = append(out, systemPrompt)
		if sessionHead.Role != "" {
			out = append(out, sessionHead)
		}
	case sessionHead.Role != "":
		out = append(out, sessionHead)
	}
	out = append(out, rawMessages...)
	return out, nil
}

func (e *Executor) buildSystemPrompt(policy contracts.SystemPromptPolicy) (contracts.Message, error) {
	if !policy.Enabled {
		return contracts.Message{}, nil
	}
	if policy.Strategy != "file_static" {
		return contracts.Message{}, fmt.Errorf("unsupported system prompt strategy %q", policy.Strategy)
	}
	if policy.Params.Path == "" {
		if policy.Params.Required {
			return contracts.Message{}, fmt.Errorf("system prompt path is empty")
		}
		return contracts.Message{}, nil
	}
	body, err := os.ReadFile(policy.Params.Path)
	if err != nil {
		if policy.Params.Required {
			return contracts.Message{}, fmt.Errorf("read system prompt file: %w", err)
		}
		return contracts.Message{}, nil
	}
	content := string(body)
	if policy.Params.TrimTrailingWhitespace {
		content = strings.TrimRight(content, " \t\r\n")
	}
	if strings.TrimSpace(content) == "" {
		return contracts.Message{}, nil
	}
	role := policy.Params.Role
	if role == "" {
		role = "system"
	}
	return contracts.Message{Role: role, Content: content}, nil
}

func (e *Executor) buildSessionHead(policy contracts.SessionHeadPolicy, sessionID string, transcript []contracts.Message, planHead projections.PlanHeadSnapshot) (contracts.Message, error) {
	if !policy.Enabled {
		return contracts.Message{}, nil
	}
	if policy.Strategy == "off" {
		return contracts.Message{}, nil
	}
	if policy.Strategy != "projection_summary" {
		return contracts.Message{}, fmt.Errorf("unsupported session head strategy %q", policy.Strategy)
	}
	lines := make([]string, 0, 6)
	if strings.TrimSpace(policy.Params.Title) != "" {
		lines = append(lines, strings.TrimSpace(policy.Params.Title))
	}
	if policy.Params.IncludeSessionID {
		lines = append(lines, "session_id: "+sessionID)
	}
	if policy.Params.IncludeLastUserMessage {
		if msg, ok := lastMessageByRole(transcript, "user"); ok {
			lines = append(lines, "last_user: "+trimForSessionHead(msg.Content, policy.Params.MaxUserChars))
		}
	}
	if policy.Params.IncludeLastAssistantMessage {
		if msg, ok := lastMessageByRole(transcript, "assistant"); ok {
			lines = append(lines, "last_assistant: "+trimForSessionHead(msg.Content, policy.Params.MaxAssistantChars))
		}
	}
	lines = append(lines, buildPlanHeadLines(planHead, policy.Params.CompactPlan)...)
	if policy.Params.MaxItems > 0 && len(lines) > policy.Params.MaxItems {
		lines = lines[:policy.Params.MaxItems]
	}
	if len(lines) == 0 {
		return contracts.Message{}, nil
	}
	return contracts.Message{
		Role:    "system",
		Content: strings.Join(lines, "\n"),
	}, nil
}

func buildPlanHeadLines(snapshot projections.PlanHeadSnapshot, compact bool) []string {
	if snapshot.Plan.ID == "" || snapshot.Plan.Goal == "" {
		return nil
	}
	if compact {
		return buildCompactPlanHeadLines(snapshot)
	}
	lines := []string{"🎯 Цель: " + snapshot.Plan.Goal}
	ordered := make([]projections.PlanTaskView, 0, len(snapshot.Tasks))
	for _, task := range snapshot.Tasks {
		ordered = append(ordered, task)
	}
	slices.SortFunc(ordered, func(a, b projections.PlanTaskView) int {
		if a.Order != b.Order {
			return a.Order - b.Order
		}
		if a.ID < b.ID {
			return -1
		}
		if a.ID > b.ID {
			return 1
		}
		return 0
	})
	for _, task := range ordered {
		switch {
		case snapshot.Ready[task.ID]:
			lines = append(lines, "⬜ ["+task.ID+"] "+task.Description)
		case snapshot.Blocked[task.ID] != "":
			lines = append(lines, "🚫 ["+task.ID+"] "+task.Description+" (Blocked: "+snapshot.Blocked[task.ID]+")")
		case task.Status == "done":
			lines = append(lines, "✅ ["+task.ID+"] "+task.Description)
		case task.Status == "in_progress":
			lines = append(lines, "🏃 ["+task.ID+"] "+task.Description)
		}
		for _, note := range snapshot.Notes[task.ID] {
			lines = append(lines, "📝 "+note)
		}
	}
	return lines
}

func buildCompactPlanHeadLines(snapshot projections.PlanHeadSnapshot) []string {
	lines := []string{"🎯 Цель: " + snapshot.Plan.Goal}
	var todo, inProgress, done, blocked int
	var current *projections.PlanTaskView
	blockedItems := make([]projections.PlanTaskView, 0)

	for _, task := range snapshot.Tasks {
		switch {
		case snapshot.Blocked[task.ID] != "":
			blocked++
			blockedItems = append(blockedItems, task)
		case task.Status == "done":
			done++
		case task.Status == "in_progress":
			inProgress++
			candidate := task
			if current == nil || candidate.Order < current.Order {
				current = &candidate
			}
		default:
			todo++
		}
	}

	lines = append(lines, fmt.Sprintf("📊 Прогресс: %d todo | %d in_progress | %d done | %d blocked", todo, inProgress, done, blocked))
	if current != nil {
		lines = append(lines, fmt.Sprintf("🏃 Текущая: [%s] %s", current.ID, current.Description))
	}
	if len(blockedItems) > 0 {
		slices.SortFunc(blockedItems, func(a, b projections.PlanTaskView) int {
			if a.Order != b.Order {
				return a.Order - b.Order
			}
			return strings.Compare(a.ID, b.ID)
		})
		blockedTask := blockedItems[0]
		lines = append(lines, fmt.Sprintf("⚠️ Blocked: [%s] %s", blockedTask.ID, snapshot.Blocked[blockedTask.ID]))
	}
	return lines
}

func trimForSessionHead(content string, limit int) string {
	content = strings.TrimSpace(content)
	if limit <= 0 || len(content) <= limit {
		return content
	}
	return strings.TrimSpace(content[:limit]) + "…"
}

func lastMessageByRole(messages []contracts.Message, role string) (contracts.Message, bool) {
	for i := len(messages) - 1; i >= 0; i-- {
		if messages[i].Role == role {
			return messages[i], true
		}
	}
	return contracts.Message{}, false
}
