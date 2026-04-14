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
	Plans map[string]ArchivedPlanView `json:"plans"`
}

type PlanArchiveProjection struct {
	active   ActivePlanSnapshot
	snapshot PlanArchiveSnapshot
}

func NewPlanArchiveProjection() *PlanArchiveProjection {
	return &PlanArchiveProjection{
		active:   ActivePlanSnapshot{Tasks: map[string]PlanTaskView{}},
		snapshot: PlanArchiveSnapshot{Plans: map[string]ArchivedPlanView{}},
	}
}

func (p *PlanArchiveProjection) ID() string { return "plan_archive" }

func (p *PlanArchiveProjection) Apply(event eventing.Event) error {
	if p.active.Tasks == nil {
		p.active.Tasks = map[string]PlanTaskView{}
	}
	if p.snapshot.Plans == nil {
		p.snapshot.Plans = map[string]ArchivedPlanView{}
	}
	switch event.Kind {
	case eventing.EventPlanCreated:
		planID, _ := event.Payload["plan_id"].(string)
		goal, _ := event.Payload["goal"].(string)
		p.active = ActivePlanSnapshot{
			Plan:  PlanView{ID: planID, Goal: goal, Status: "active", CreatedAt: event.OccurredAt},
			Tasks: map[string]PlanTaskView{},
		}
	case eventing.EventTaskAdded:
		task := decodeTaskView(event)
		if task.PlanID == p.active.Plan.ID {
			p.active.Tasks[task.ID] = task
		}
	case eventing.EventTaskStatusChanged:
		taskID, _ := event.Payload["task_id"].(string)
		task, ok := p.active.Tasks[taskID]
		if !ok {
			return nil
		}
		if status, _ := event.Payload["new_status"].(string); status != "" {
			task.Status = status
		}
		if blockedReason, ok := event.Payload["blocked_reason"].(string); ok {
			task.BlockedReason = blockedReason
		}
		p.active.Tasks[taskID] = task
	case eventing.EventTaskNoteAdded:
		taskID, _ := event.Payload["task_id"].(string)
		task, ok := p.active.Tasks[taskID]
		if !ok {
			return nil
		}
		noteText, _ := event.Payload["note_text"].(string)
		if noteText == "" {
			return nil
		}
		task.Notes = append(task.Notes, PlanTaskNote{Text: noteText, CreatedAt: event.OccurredAt})
		p.active.Tasks[taskID] = task
	case eventing.EventTaskEdited:
		taskID, _ := event.Payload["task_id"].(string)
		task, ok := p.active.Tasks[taskID]
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
		p.active.Tasks[taskID] = task
	case eventing.EventPlanArchived:
		planID, _ := event.Payload["plan_id"].(string)
		if planID != p.active.Plan.ID || planID == "" {
			return nil
		}
		plan := p.active.Plan
		plan.Status = "archived"
		plan.ArchivedAt = event.OccurredAt
		tasks := make(map[string]PlanTaskView, len(p.active.Tasks))
		for id, task := range p.active.Tasks {
			tasks[id] = task
		}
		p.snapshot.Plans[planID] = ArchivedPlanView{Plan: plan, Tasks: tasks}
		p.active = ActivePlanSnapshot{Tasks: map[string]PlanTaskView{}}
	}
	return nil
}

func (p *PlanArchiveProjection) Snapshot() PlanArchiveSnapshot { return p.snapshot }
func (p *PlanArchiveProjection) SnapshotValue() any            { return p.snapshot }

func (p *PlanArchiveProjection) RestoreSnapshot(raw []byte) error {
	var snapshot PlanArchiveSnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore plan archive snapshot: %w", err)
	}
	if snapshot.Plans == nil {
		snapshot.Plans = map[string]ArchivedPlanView{}
	}
	p.snapshot = snapshot
	p.active = ActivePlanSnapshot{Tasks: map[string]PlanTaskView{}}
	return nil
}
