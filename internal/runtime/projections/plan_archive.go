package projections

import (
	"encoding/json"
	"fmt"

	"teamd/internal/runtime/eventing"
)

type ArchivedPlanView struct {
	Plan  PlanView                `json:"plan"`
	Tasks map[string]PlanTaskView `json:"tasks"`
}

type PlanArchiveSnapshot struct {
	Sessions map[string]map[string]ArchivedPlanView `json:"sessions"`
}

type PlanArchiveProjection struct {
	active   map[string]ActivePlanSnapshot
	snapshot PlanArchiveSnapshot
}

func NewPlanArchiveProjection() *PlanArchiveProjection {
	return &PlanArchiveProjection{
		active:   map[string]ActivePlanSnapshot{},
		snapshot: PlanArchiveSnapshot{Sessions: map[string]map[string]ArchivedPlanView{}},
	}
}

func (p *PlanArchiveProjection) ID() string { return "plan_archive" }

func (p *PlanArchiveProjection) Apply(event eventing.Event) error {
	sessionID, _ := event.Payload["session_id"].(string)
	if sessionID == "" {
		return nil
	}
	if p.active == nil {
		p.active = map[string]ActivePlanSnapshot{}
	}
	if p.snapshot.Sessions == nil {
		p.snapshot.Sessions = map[string]map[string]ArchivedPlanView{}
	}
	active := p.active[sessionID]
	if active.Tasks == nil {
		active.Tasks = map[string]PlanTaskView{}
	}
	if p.snapshot.Sessions[sessionID] == nil {
		p.snapshot.Sessions[sessionID] = map[string]ArchivedPlanView{}
	}
	switch event.Kind {
	case eventing.EventPlanCreated:
		planID, _ := event.Payload["plan_id"].(string)
		goal, _ := event.Payload["goal"].(string)
		active = ActivePlanSnapshot{
			Plan:  PlanView{ID: planID, Goal: goal, Status: "active", CreatedAt: event.OccurredAt},
			Tasks: map[string]PlanTaskView{},
		}
	case eventing.EventTaskAdded:
		task := decodeTaskView(event)
		if task.PlanID == active.Plan.ID {
			active.Tasks[task.ID] = task
		}
	case eventing.EventTaskStatusChanged:
		taskID, _ := event.Payload["task_id"].(string)
		task, ok := active.Tasks[taskID]
		if !ok {
			return nil
		}
		if status, _ := event.Payload["new_status"].(string); status != "" {
			task.Status = status
		}
		if blockedReason, ok := event.Payload["blocked_reason"].(string); ok {
			task.BlockedReason = blockedReason
		}
		active.Tasks[taskID] = task
	case eventing.EventTaskNoteAdded:
		taskID, _ := event.Payload["task_id"].(string)
		task, ok := active.Tasks[taskID]
		if !ok {
			return nil
		}
		noteText, _ := event.Payload["note_text"].(string)
		if noteText == "" {
			return nil
		}
		task.Notes = append(task.Notes, PlanTaskNote{Text: noteText, CreatedAt: event.OccurredAt})
		active.Tasks[taskID] = task
	case eventing.EventTaskEdited:
		taskID, _ := event.Payload["task_id"].(string)
		task, ok := active.Tasks[taskID]
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
		active.Tasks[taskID] = task
	case eventing.EventPlanArchived:
		planID, _ := event.Payload["plan_id"].(string)
		if planID != active.Plan.ID || planID == "" {
			return nil
		}
		plan := active.Plan
		plan.Status = "archived"
		plan.ArchivedAt = event.OccurredAt
		tasks := make(map[string]PlanTaskView, len(active.Tasks))
		for id, task := range active.Tasks {
			tasks[id] = task
		}
		p.snapshot.Sessions[sessionID][planID] = ArchivedPlanView{Plan: plan, Tasks: tasks}
		active = ActivePlanSnapshot{Tasks: map[string]PlanTaskView{}}
	}
	p.active[sessionID] = active
	return nil
}

func (p *PlanArchiveProjection) Snapshot() PlanArchiveSnapshot { return p.snapshot }
func (p *PlanArchiveProjection) SnapshotValue() any            { return p.snapshot }

func (p *PlanArchiveProjection) SnapshotForSession(sessionID string) map[string]ArchivedPlanView {
	if p.snapshot.Sessions == nil {
		return map[string]ArchivedPlanView{}
	}
	plans := p.snapshot.Sessions[sessionID]
	if plans == nil {
		return map[string]ArchivedPlanView{}
	}
	return plans
}

func (p *PlanArchiveProjection) RestoreSnapshot(raw []byte) error {
	var snapshot PlanArchiveSnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore plan archive snapshot: %w", err)
	}
	if snapshot.Sessions == nil {
		snapshot.Sessions = map[string]map[string]ArchivedPlanView{}
	}
	p.snapshot = snapshot
	p.active = map[string]ActivePlanSnapshot{}
	return nil
}
