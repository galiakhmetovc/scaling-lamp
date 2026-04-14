package projections

import (
	"encoding/json"
	"fmt"
	"slices"
	"time"

	"teamd/internal/runtime/eventing"
)

type PlanView struct {
	ID         string    `json:"id"`
	Goal       string    `json:"goal"`
	Status     string    `json:"status"`
	CreatedAt  time.Time `json:"created_at"`
	ArchivedAt time.Time `json:"archived_at,omitempty"`
}

type PlanTaskNote struct {
	Text      string    `json:"text"`
	CreatedAt time.Time `json:"created_at"`
}

type PlanTaskView struct {
	ID            string         `json:"id"`
	PlanID        string         `json:"plan_id"`
	ParentTaskID  string         `json:"parent_task_id,omitempty"`
	DependsOn     []string       `json:"depends_on,omitempty"`
	Description   string         `json:"description"`
	Status        string         `json:"status"`
	Order         int            `json:"order"`
	Notes         []PlanTaskNote `json:"notes,omitempty"`
	BlockedReason string         `json:"blocked_reason,omitempty"`
}

type ActivePlanSnapshot struct {
	Plan  PlanView                `json:"plan"`
	Tasks map[string]PlanTaskView `json:"tasks"`
}

type ActivePlanProjection struct {
	snapshot ActivePlanSnapshot
}

func NewActivePlanProjection() *ActivePlanProjection {
	return &ActivePlanProjection{
		snapshot: ActivePlanSnapshot{Tasks: map[string]PlanTaskView{}},
	}
}

func (p *ActivePlanProjection) ID() string { return "active_plan" }

func (p *ActivePlanProjection) Apply(event eventing.Event) error {
	if p.snapshot.Tasks == nil {
		p.snapshot.Tasks = map[string]PlanTaskView{}
	}
	switch event.Kind {
	case eventing.EventPlanCreated:
		planID, _ := event.Payload["plan_id"].(string)
		goal, _ := event.Payload["goal"].(string)
		p.snapshot = ActivePlanSnapshot{
			Plan: PlanView{
				ID:        planID,
				Goal:      goal,
				Status:    "active",
				CreatedAt: event.OccurredAt,
			},
			Tasks: map[string]PlanTaskView{},
		}
	case eventing.EventPlanArchived:
		planID, _ := event.Payload["plan_id"].(string)
		if p.snapshot.Plan.ID != planID {
			return nil
		}
		p.snapshot = ActivePlanSnapshot{Tasks: map[string]PlanTaskView{}}
	case eventing.EventTaskAdded:
		task := decodeTaskView(event)
		if p.snapshot.Plan.ID == "" || task.PlanID != p.snapshot.Plan.ID {
			return nil
		}
		p.snapshot.Tasks[task.ID] = task
	case eventing.EventTaskStatusChanged:
		taskID, _ := event.Payload["task_id"].(string)
		task, ok := p.snapshot.Tasks[taskID]
		if !ok {
			return nil
		}
		if status, _ := event.Payload["new_status"].(string); status != "" {
			task.Status = status
		}
		if blockedReason, ok := event.Payload["blocked_reason"].(string); ok {
			task.BlockedReason = blockedReason
		} else if task.Status != "blocked" {
			task.BlockedReason = ""
		}
		p.snapshot.Tasks[taskID] = task
	case eventing.EventTaskNoteAdded:
		taskID, _ := event.Payload["task_id"].(string)
		task, ok := p.snapshot.Tasks[taskID]
		if !ok {
			return nil
		}
		noteText, _ := event.Payload["note_text"].(string)
		if noteText == "" {
			return nil
		}
		task.Notes = append(task.Notes, PlanTaskNote{Text: noteText, CreatedAt: event.OccurredAt})
		p.snapshot.Tasks[taskID] = task
	case eventing.EventTaskEdited:
		taskID, _ := event.Payload["task_id"].(string)
		task, ok := p.snapshot.Tasks[taskID]
		if !ok {
			return nil
		}
		if description, _ := event.Payload["description"].(string); description != "" {
			task.Description = description
		}
		if parentID, ok := event.Payload["parent_task_id"].(string); ok {
			task.ParentTaskID = parentID
		}
		if dependsOn, ok := payloadStringSlice(event.Payload["depends_on"]); ok {
			task.DependsOn = dependsOn
		}
		p.snapshot.Tasks[taskID] = task
	}
	return nil
}

func (p *ActivePlanProjection) Snapshot() ActivePlanSnapshot { return p.snapshot }
func (p *ActivePlanProjection) SnapshotValue() any           { return p.snapshot }

func (p *ActivePlanProjection) RestoreSnapshot(raw []byte) error {
	var snapshot ActivePlanSnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore active plan snapshot: %w", err)
	}
	if snapshot.Tasks == nil {
		snapshot.Tasks = map[string]PlanTaskView{}
	}
	for id, task := range snapshot.Tasks {
		if task.DependsOn == nil {
			task.DependsOn = []string{}
			snapshot.Tasks[id] = task
		}
	}
	p.snapshot = snapshot
	return nil
}

func decodeTaskView(event eventing.Event) PlanTaskView {
	taskID, _ := event.Payload["task_id"].(string)
	planID, _ := event.Payload["plan_id"].(string)
	parentTaskID, _ := event.Payload["parent_task_id"].(string)
	description, _ := event.Payload["description"].(string)
	status, _ := event.Payload["status"].(string)
	order, _ := event.Payload["order"].(int)
	if order == 0 {
		if numeric, ok := event.Payload["order"].(float64); ok {
			order = int(numeric)
		}
	}
	blockedReason, _ := event.Payload["blocked_reason"].(string)
	dependsOn, _ := payloadStringSlice(event.Payload["depends_on"])
	if dependsOn == nil {
		dependsOn = []string{}
	}
	return PlanTaskView{
		ID:            taskID,
		PlanID:        planID,
		ParentTaskID:  parentTaskID,
		DependsOn:     slices.Clone(dependsOn),
		Description:   description,
		Status:        status,
		Order:         order,
		BlockedReason: blockedReason,
	}
}

func payloadStringSlice(v any) ([]string, bool) {
	switch typed := v.(type) {
	case []string:
		return slices.Clone(typed), true
	case []any:
		out := make([]string, 0, len(typed))
		for _, item := range typed {
			text, ok := item.(string)
			if !ok {
				return nil, false
			}
			out = append(out, text)
		}
		return out, true
	default:
		return nil, false
	}
}
