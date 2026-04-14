package plans

import (
	"fmt"
	"slices"
	"time"

	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

type Service struct {
	now   func() time.Time
	newID func(string) string
}

func NewService(now func() time.Time, newID func(string) string) *Service {
	if now == nil {
		now = func() time.Time { return time.Now().UTC() }
	}
	if newID == nil {
		newID = func(prefix string) string { return fmt.Sprintf("%s-%d", prefix, now().UnixNano()) }
	}
	return &Service{now: now, newID: newID}
}

type InitPlanInput struct {
	Goal    string
	Source  string
	ActorID string
}

type AddTaskInput struct {
	PlanID       string
	TaskID       string
	Description  string
	ParentTaskID string
	DependsOn    []string
	Source       string
	ActorID      string
}

type EditTaskInput struct {
	TaskID         string
	NewDescription string
	NewDependsOn   []string
	Source         string
	ActorID        string
}

type SetTaskStatusInput struct {
	TaskID        string
	NewStatus     string
	BlockedReason string
	Source        string
	ActorID       string
}

type AddTaskNoteInput struct {
	TaskID   string
	NoteText string
	Source   string
	ActorID  string
}

func (s *Service) InitPlan(active projections.ActivePlanSnapshot, input InitPlanInput) ([]eventing.Event, error) {
	if input.Goal == "" {
		return nil, fmt.Errorf("plan goal is empty")
	}
	now := s.now().UTC()
	var events []eventing.Event
	if active.Plan.ID != "" {
		events = append(events, eventing.Event{
			ID:            s.newID("evt-plan-archive"),
			Kind:          eventing.EventPlanArchived,
			OccurredAt:    now,
			AggregateID:   active.Plan.ID,
			AggregateType: eventing.AggregatePlan,
			Payload: map[string]any{
				"plan_id": active.Plan.ID,
			},
			Source:  input.Source,
			ActorID: input.ActorID,
		})
	}
	planID := s.newID("plan")
	events = append(events, eventing.Event{
		ID:            s.newID("evt-plan-create"),
		Kind:          eventing.EventPlanCreated,
		OccurredAt:    now,
		AggregateID:   planID,
		AggregateType: eventing.AggregatePlan,
		Payload: map[string]any{
			"plan_id": planID,
			"goal":    input.Goal,
		},
		Source:  input.Source,
		ActorID: input.ActorID,
	})
	return events, nil
}

func (s *Service) AddTask(active projections.ActivePlanSnapshot, input AddTaskInput) ([]eventing.Event, error) {
	if active.Plan.ID == "" {
		return nil, fmt.Errorf("no active plan")
	}
	if input.PlanID == "" {
		input.PlanID = active.Plan.ID
	}
	if input.PlanID != active.Plan.ID {
		return nil, fmt.Errorf("task plan_id %q does not match active plan %q", input.PlanID, active.Plan.ID)
	}
	if input.Description == "" {
		return nil, fmt.Errorf("task description is empty")
	}
	taskID := input.TaskID
	if taskID == "" {
		taskID = s.newID("task")
	}
	if slices.Contains(input.DependsOn, taskID) {
		return nil, fmt.Errorf("task %q cannot depend on itself", taskID)
	}
	if err := validateDependencyGraph(active.Tasks, taskID, input.DependsOn); err != nil {
		return nil, err
	}
	order := len(active.Tasks) + 1
	return []eventing.Event{{
		ID:            s.newID("evt-task-add"),
		Kind:          eventing.EventTaskAdded,
		OccurredAt:    s.now().UTC(),
		AggregateID:   taskID,
		AggregateType: eventing.AggregatePlanTask,
		Payload: map[string]any{
			"plan_id":        input.PlanID,
			"task_id":        taskID,
			"parent_task_id": input.ParentTaskID,
			"description":    input.Description,
			"status":         string(TaskStatusTodo),
			"order":          order,
			"depends_on":     slices.Clone(input.DependsOn),
		},
		Source:  input.Source,
		ActorID: input.ActorID,
	}}, nil
}

func (s *Service) EditTask(active projections.ActivePlanSnapshot, input EditTaskInput) ([]eventing.Event, error) {
	task, ok := active.Tasks[input.TaskID]
	if !ok {
		return nil, fmt.Errorf("task %q not found", input.TaskID)
	}
	dependsOn := task.DependsOn
	if input.NewDependsOn != nil {
		dependsOn = slices.Clone(input.NewDependsOn)
		if slices.Contains(dependsOn, input.TaskID) {
			return nil, fmt.Errorf("task %q cannot depend on itself", input.TaskID)
		}
		if err := validateDependencyGraph(active.Tasks, input.TaskID, dependsOn); err != nil {
			return nil, err
		}
	}
	description := task.Description
	if input.NewDescription != "" {
		description = input.NewDescription
	}
	return []eventing.Event{{
		ID:            s.newID("evt-task-edit"),
		Kind:          eventing.EventTaskEdited,
		OccurredAt:    s.now().UTC(),
		AggregateID:   input.TaskID,
		AggregateType: eventing.AggregatePlanTask,
		Payload: map[string]any{
			"plan_id":        task.PlanID,
			"task_id":        input.TaskID,
			"parent_task_id": task.ParentTaskID,
			"description":    description,
			"depends_on":     dependsOn,
		},
		Source:  input.Source,
		ActorID: input.ActorID,
	}}, nil
}

func (s *Service) SetTaskStatus(active projections.ActivePlanSnapshot, input SetTaskStatusInput) ([]eventing.Event, error) {
	task, ok := active.Tasks[input.TaskID]
	if !ok {
		return nil, fmt.Errorf("task %q not found", input.TaskID)
	}
	if err := validateStatusTransition(active.Tasks, task, input.NewStatus); err != nil {
		return nil, err
	}
	return []eventing.Event{{
		ID:            s.newID("evt-task-status"),
		Kind:          eventing.EventTaskStatusChanged,
		OccurredAt:    s.now().UTC(),
		AggregateID:   input.TaskID,
		AggregateType: eventing.AggregatePlanTask,
		Payload: map[string]any{
			"plan_id":        task.PlanID,
			"task_id":        input.TaskID,
			"new_status":     input.NewStatus,
			"blocked_reason": input.BlockedReason,
		},
		Source:  input.Source,
		ActorID: input.ActorID,
	}}, nil
}

func (s *Service) AddTaskNote(active projections.ActivePlanSnapshot, input AddTaskNoteInput) ([]eventing.Event, error) {
	task, ok := active.Tasks[input.TaskID]
	if !ok {
		return nil, fmt.Errorf("task %q not found", input.TaskID)
	}
	if input.NoteText == "" {
		return nil, fmt.Errorf("task note is empty")
	}
	return []eventing.Event{{
		ID:            s.newID("evt-task-note"),
		Kind:          eventing.EventTaskNoteAdded,
		OccurredAt:    s.now().UTC(),
		AggregateID:   input.TaskID,
		AggregateType: eventing.AggregatePlanTask,
		Payload: map[string]any{
			"plan_id":   task.PlanID,
			"task_id":   input.TaskID,
			"note_text": input.NoteText,
		},
		Source:  input.Source,
		ActorID: input.ActorID,
	}}, nil
}

func validateDependencyGraph(tasks map[string]projections.PlanTaskView, taskID string, dependsOn []string) error {
	graph := map[string][]string{}
	for id, task := range tasks {
		graph[id] = slices.Clone(task.DependsOn)
	}
	graph[taskID] = slices.Clone(dependsOn)
	seen := map[string]bool{}
	stack := map[string]bool{}
	var visit func(string) error
	visit = func(node string) error {
		if stack[node] {
			return fmt.Errorf("dependency graph must remain acyclic")
		}
		if seen[node] {
			return nil
		}
		seen[node] = true
		stack[node] = true
		for _, dep := range graph[node] {
			if dep == node {
				return fmt.Errorf("task %q cannot depend on itself", node)
			}
			if err := visit(dep); err != nil {
				return err
			}
		}
		delete(stack, node)
		return nil
	}
	return visit(taskID)
}

func validateStatusTransition(tasks map[string]projections.PlanTaskView, task projections.PlanTaskView, next string) error {
	switch next {
	case string(TaskStatusInProgress), string(TaskStatusBlocked), string(TaskStatusCancelled):
	case string(TaskStatusDone):
		for _, child := range tasks {
			if child.ParentTaskID != task.ID {
				continue
			}
			if child.Status != string(TaskStatusDone) && child.Status != string(TaskStatusCancelled) {
				return fmt.Errorf("task %q cannot be done while child %q is %q", task.ID, child.ID, child.Status)
			}
		}
	default:
		return fmt.Errorf("unsupported task status %q", next)
	}
	return nil
}
