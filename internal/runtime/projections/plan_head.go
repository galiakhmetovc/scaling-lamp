package projections

import (
	"encoding/json"
	"fmt"

	"teamd/internal/runtime/eventing"
)

type PlanHeadSnapshot struct {
	Plan                  PlanView                `json:"plan"`
	Tasks                 map[string]PlanTaskView `json:"tasks"`
	Ready                 map[string]bool         `json:"ready"`
	WaitingOnDependencies map[string]bool         `json:"waiting_on_dependencies"`
	Blocked               map[string]string       `json:"blocked"`
	Notes                 map[string][]string     `json:"notes"`
}

type PlanHeadProjection struct {
	active   ActivePlanSnapshot
	snapshot PlanHeadSnapshot
}

func NewPlanHeadProjection() *PlanHeadProjection {
	return &PlanHeadProjection{
		active: ActivePlanSnapshot{Tasks: map[string]PlanTaskView{}},
		snapshot: PlanHeadSnapshot{
			Tasks:                 map[string]PlanTaskView{},
			Ready:                 map[string]bool{},
			WaitingOnDependencies: map[string]bool{},
			Blocked:               map[string]string{},
			Notes:                 map[string][]string{},
		},
	}
}

func (p *PlanHeadProjection) ID() string { return "plan_head" }

func (p *PlanHeadProjection) Apply(event eventing.Event) error {
	if p.active.Tasks == nil {
		p.active.Tasks = map[string]PlanTaskView{}
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
		if ok {
			if status, _ := event.Payload["new_status"].(string); status != "" {
				task.Status = status
			}
			if blockedReason, ok := event.Payload["blocked_reason"].(string); ok {
				task.BlockedReason = blockedReason
			}
			p.active.Tasks[taskID] = task
		}
	case eventing.EventTaskNoteAdded:
		taskID, _ := event.Payload["task_id"].(string)
		task, ok := p.active.Tasks[taskID]
		if ok {
			noteText, _ := event.Payload["note_text"].(string)
			if noteText != "" {
				task.Notes = append(task.Notes, PlanTaskNote{Text: noteText, CreatedAt: event.OccurredAt})
				p.active.Tasks[taskID] = task
			}
		}
	case eventing.EventTaskEdited:
		taskID, _ := event.Payload["task_id"].(string)
		task, ok := p.active.Tasks[taskID]
		if ok {
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
		}
	case eventing.EventPlanArchived:
		planID, _ := event.Payload["plan_id"].(string)
		if planID == p.active.Plan.ID {
			p.active = ActivePlanSnapshot{Tasks: map[string]PlanTaskView{}}
		}
	}
	p.rebuild()
	return nil
}

func (p *PlanHeadProjection) Snapshot() PlanHeadSnapshot { return p.snapshot }
func (p *PlanHeadProjection) SnapshotValue() any         { return p.snapshot }

func (p *PlanHeadProjection) RestoreSnapshot(raw []byte) error {
	var snapshot PlanHeadSnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore plan head snapshot: %w", err)
	}
	if snapshot.Tasks == nil {
		snapshot.Tasks = map[string]PlanTaskView{}
	}
	if snapshot.Ready == nil {
		snapshot.Ready = map[string]bool{}
	}
	if snapshot.WaitingOnDependencies == nil {
		snapshot.WaitingOnDependencies = map[string]bool{}
	}
	if snapshot.Blocked == nil {
		snapshot.Blocked = map[string]string{}
	}
	if snapshot.Notes == nil {
		snapshot.Notes = map[string][]string{}
	}
	p.snapshot = snapshot
	p.active = ActivePlanSnapshot{Plan: snapshot.Plan, Tasks: snapshot.Tasks}
	return nil
}

func (p *PlanHeadProjection) rebuild() {
	p.snapshot = PlanHeadSnapshot{
		Plan:                  p.active.Plan,
		Tasks:                 map[string]PlanTaskView{},
		Ready:                 map[string]bool{},
		WaitingOnDependencies: map[string]bool{},
		Blocked:               map[string]string{},
		Notes:                 map[string][]string{},
	}
	for id, task := range p.active.Tasks {
		p.snapshot.Tasks[id] = task
		if len(task.Notes) > 0 {
			values := make([]string, 0, len(task.Notes))
			for _, note := range task.Notes {
				values = append(values, note.Text)
			}
			p.snapshot.Notes[id] = values
		}
		if task.Status == "blocked" {
			p.snapshot.Blocked[id] = task.BlockedReason
			continue
		}
		if task.Status != "todo" {
			continue
		}
		waiting := false
		for _, dep := range task.DependsOn {
			dependency, ok := p.active.Tasks[dep]
			if !ok || dependency.Status != "done" {
				waiting = true
				break
			}
		}
		if waiting {
			p.snapshot.WaitingOnDependencies[id] = true
		} else {
			p.snapshot.Ready[id] = true
		}
	}
}
