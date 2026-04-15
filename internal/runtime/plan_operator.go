package runtime

import (
	"context"
	"fmt"

	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/plans"
	"teamd/internal/runtime/projections"
)

func (a *Agent) ActivePlan(sessionID string) (projections.ActivePlanSnapshot, bool) {
	projection := a.activePlanProjection()
	if projection == nil {
		return projections.ActivePlanSnapshot{}, false
	}
	return projection.SnapshotForSession(sessionID), true
}

func (a *Agent) CreatePlan(ctx context.Context, sessionID, goal string) error {
	active, _ := a.ActivePlan(sessionID)
	service := plans.NewService(a.now, a.newID)
	events, err := service.InitPlan(active, plans.InitPlanInput{
		SessionID: sessionID,
		Goal:      goal,
		Source:    "tui.plan",
		ActorID:   a.Config.ID,
	})
	if err != nil {
		return err
	}
	return a.recordPlanOperatorEvents(ctx, events)
}

func (a *Agent) AddPlanTask(ctx context.Context, sessionID, description string, parentTaskID string, dependsOn []string) error {
	active, ok := a.ActivePlan(sessionID)
	if !ok {
		return fmt.Errorf("active plan projection is not registered")
	}
	service := plans.NewService(a.now, a.newID)
	events, err := service.AddTask(active, plans.AddTaskInput{
		SessionID:    sessionID,
		PlanID:       active.Plan.ID,
		Description:  description,
		ParentTaskID: parentTaskID,
		DependsOn:    dependsOn,
		Source:       "tui.plan",
		ActorID:      a.Config.ID,
	})
	if err != nil {
		return err
	}
	return a.recordPlanOperatorEvents(ctx, events)
}

func (a *Agent) EditPlanTask(ctx context.Context, sessionID, taskID, description string, dependsOn []string) error {
	active, ok := a.ActivePlan(sessionID)
	if !ok {
		return fmt.Errorf("active plan projection is not registered")
	}
	service := plans.NewService(a.now, a.newID)
	events, err := service.EditTask(active, plans.EditTaskInput{
		SessionID:      sessionID,
		TaskID:         taskID,
		NewDescription: description,
		NewDependsOn:   dependsOn,
		Source:         "tui.plan",
		ActorID:        a.Config.ID,
	})
	if err != nil {
		return err
	}
	return a.recordPlanOperatorEvents(ctx, events)
}

func (a *Agent) SetPlanTaskStatus(ctx context.Context, sessionID, taskID, status, blockedReason string) error {
	active, ok := a.ActivePlan(sessionID)
	if !ok {
		return fmt.Errorf("active plan projection is not registered")
	}
	service := plans.NewService(a.now, a.newID)
	events, err := service.SetTaskStatus(active, plans.SetTaskStatusInput{
		SessionID:     sessionID,
		TaskID:        taskID,
		NewStatus:     status,
		BlockedReason: blockedReason,
		Source:        "tui.plan",
		ActorID:       a.Config.ID,
	})
	if err != nil {
		return err
	}
	return a.recordPlanOperatorEvents(ctx, events)
}

func (a *Agent) AddPlanTaskNote(ctx context.Context, sessionID, taskID, note string) error {
	active, ok := a.ActivePlan(sessionID)
	if !ok {
		return fmt.Errorf("active plan projection is not registered")
	}
	service := plans.NewService(a.now, a.newID)
	events, err := service.AddTaskNote(active, plans.AddTaskNoteInput{
		SessionID: sessionID,
		TaskID:   taskID,
		NoteText: note,
		Source:   "tui.plan",
		ActorID:  a.Config.ID,
	})
	if err != nil {
		return err
	}
	return a.recordPlanOperatorEvents(ctx, events)
}

func (a *Agent) recordPlanOperatorEvents(ctx context.Context, events []eventing.Event) error {
	for _, event := range events {
		if event.Source == "" {
			event.Source = "tui.plan"
		}
		if event.ActorID == "" {
			event.ActorID = a.Config.ID
		}
		if event.ActorType == "" {
			event.ActorType = "agent"
		}
		if err := a.RecordEvent(ctx, event); err != nil {
			return err
		}
	}
	return nil
}
