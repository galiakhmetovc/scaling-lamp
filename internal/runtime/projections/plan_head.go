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

type PlanHeadCatalogSnapshot struct {
	Sessions map[string]PlanHeadSnapshot `json:"sessions"`
}

type PlanHeadProjection struct {
	active   map[string]ActivePlanSnapshot
	snapshot PlanHeadCatalogSnapshot
}

func NewPlanHeadProjection() *PlanHeadProjection {
	return &PlanHeadProjection{
		active: map[string]ActivePlanSnapshot{},
		snapshot: PlanHeadCatalogSnapshot{
			Sessions: map[string]PlanHeadSnapshot{},
		},
	}
}

func (p *PlanHeadProjection) ID() string { return "plan_head" }

func (p *PlanHeadProjection) Apply(event eventing.Event) error {
	sessionID, _ := event.Payload["session_id"].(string)
	if sessionID == "" {
		return nil
	}
	if p.active == nil {
		p.active = map[string]ActivePlanSnapshot{}
	}
	active := p.active[sessionID]
	if active.Tasks == nil {
		active.Tasks = map[string]PlanTaskView{}
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
		if ok {
			if status, _ := event.Payload["new_status"].(string); status != "" {
				task.Status = status
			}
			if blockedReason, ok := event.Payload["blocked_reason"].(string); ok {
				task.BlockedReason = blockedReason
			}
			active.Tasks[taskID] = task
		}
	case eventing.EventTaskNoteAdded:
		taskID, _ := event.Payload["task_id"].(string)
		task, ok := active.Tasks[taskID]
		if ok {
			noteText, _ := event.Payload["note_text"].(string)
			if noteText != "" {
				task.Notes = append(task.Notes, PlanTaskNote{Text: noteText, CreatedAt: event.OccurredAt})
				active.Tasks[taskID] = task
			}
		}
	case eventing.EventTaskEdited:
		taskID, _ := event.Payload["task_id"].(string)
		task, ok := active.Tasks[taskID]
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
			active.Tasks[taskID] = task
		}
	case eventing.EventPlanArchived:
		planID, _ := event.Payload["plan_id"].(string)
		if planID == active.Plan.ID {
			active = ActivePlanSnapshot{Tasks: map[string]PlanTaskView{}}
		}
	case eventing.EventSessionDeleted:
		delete(p.active, sessionID)
		if p.snapshot.Sessions != nil {
			delete(p.snapshot.Sessions, sessionID)
		}
		return nil
	}
	p.active[sessionID] = active
	p.rebuildSession(sessionID)
	return nil
}

func (p *PlanHeadProjection) SnapshotForSession(sessionID string) PlanHeadSnapshot {
	if p.snapshot.Sessions == nil {
		return PlanHeadSnapshot{
			Tasks:                 map[string]PlanTaskView{},
			Ready:                 map[string]bool{},
			WaitingOnDependencies: map[string]bool{},
			Blocked:               map[string]string{},
			Notes:                 map[string][]string{},
		}
	}
	snapshot := p.snapshot.Sessions[sessionID]
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
	return snapshot
}

func (p *PlanHeadProjection) Snapshot() PlanHeadCatalogSnapshot { return p.snapshot }
func (p *PlanHeadProjection) SnapshotValue() any                { return p.snapshot }

func (p *PlanHeadProjection) RestoreSnapshot(raw []byte) error {
	var snapshot PlanHeadCatalogSnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore plan head snapshot: %w", err)
	}
	if snapshot.Sessions == nil {
		snapshot.Sessions = map[string]PlanHeadSnapshot{}
	}
	p.snapshot = snapshot
	p.active = map[string]ActivePlanSnapshot{}
	for sessionID, session := range snapshot.Sessions {
		if session.Tasks == nil {
			session.Tasks = map[string]PlanTaskView{}
		}
		if session.Ready == nil {
			session.Ready = map[string]bool{}
		}
		if session.WaitingOnDependencies == nil {
			session.WaitingOnDependencies = map[string]bool{}
		}
		if session.Blocked == nil {
			session.Blocked = map[string]string{}
		}
		if session.Notes == nil {
			session.Notes = map[string][]string{}
		}
		snapshot.Sessions[sessionID] = session
		p.active[sessionID] = ActivePlanSnapshot{Plan: session.Plan, Tasks: session.Tasks}
	}
	return nil
}

func (p *PlanHeadProjection) rebuildSession(sessionID string) {
	active := p.active[sessionID]
	snapshot := PlanHeadSnapshot{
		Plan:                  active.Plan,
		Tasks:                 map[string]PlanTaskView{},
		Ready:                 map[string]bool{},
		WaitingOnDependencies: map[string]bool{},
		Blocked:               map[string]string{},
		Notes:                 map[string][]string{},
	}
	for id, task := range active.Tasks {
		snapshot.Tasks[id] = task
		if len(task.Notes) > 0 {
			values := make([]string, 0, len(task.Notes))
			for _, note := range task.Notes {
				values = append(values, note.Text)
			}
			snapshot.Notes[id] = values
		}
		if task.Status == "blocked" {
			snapshot.Blocked[id] = task.BlockedReason
			continue
		}
		if task.Status != "todo" {
			continue
		}
		waiting := false
		for _, dep := range task.DependsOn {
			dependency, ok := active.Tasks[dep]
			if !ok || dependency.Status != "done" {
				waiting = true
				break
			}
		}
		if waiting {
			snapshot.WaitingOnDependencies[id] = true
		} else {
			snapshot.Ready[id] = true
		}
	}
	if p.snapshot.Sessions == nil {
		p.snapshot.Sessions = map[string]PlanHeadSnapshot{}
	}
	p.snapshot.Sessions[sessionID] = snapshot
}
