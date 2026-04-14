package telegram

import (
	"context"
	"encoding/json"
	"strings"
	"testing"
	"time"

	runtimex "teamd/internal/runtime"
	"teamd/internal/provider"
)

type jobControlStub struct{}

func (jobControlStub) StartDetached(ctx context.Context, req runtimex.JobStartRequest) (runtimex.JobView, error) {
	return runtimex.JobView{
		JobID:     "job-1",
		ChatID:    req.ChatID,
		SessionID: req.SessionID,
		Command:   req.Command,
		Args:      req.Args,
		Status:    runtimex.JobQueued,
		StartedAt: time.Now().UTC(),
		Active:    true,
	}, nil
}

func (jobControlStub) Job(jobID string) (runtimex.JobView, bool, error) {
	return runtimex.JobView{JobID: jobID, Status: runtimex.JobCompleted}, true, nil
}

func (jobControlStub) Cancel(jobID string) (bool, error) { return true, nil }

type workerControlStub struct{}

func (workerControlStub) Spawn(ctx context.Context, req runtimex.WorkerSpawnRequest) (runtimex.WorkerView, error) {
	return runtimex.WorkerView{
		WorkerID:        "worker-1",
		ParentChatID:    req.ParentChatID,
		ParentSessionID: req.ParentSessionID,
		WorkerChatID:    -1,
		WorkerSessionID: "worker-1",
		Status:          runtimex.WorkerIdle,
		CreatedAt:       time.Now().UTC(),
		UpdatedAt:       time.Now().UTC(),
	}, nil
}

func (workerControlStub) Message(ctx context.Context, workerID string, req runtimex.WorkerMessageRequest) (runtimex.WorkerView, error) {
	return runtimex.WorkerView{WorkerID: workerID, Status: runtimex.WorkerRunning, UpdatedAt: time.Now().UTC()}, nil
}

func (workerControlStub) Wait(workerID string, afterCursor int, afterEventID int64, eventLimit int) (runtimex.WorkerWaitResult, bool, error) {
	return runtimex.WorkerWaitResult{
		Worker:         runtimex.WorkerView{WorkerID: workerID, Status: runtimex.WorkerIdle, UpdatedAt: time.Now().UTC()},
		Messages:       []runtimex.WorkerMessage{{Cursor: 1, Role: "assistant", Content: "done"}},
		NextCursor:     1,
		NextEventAfter: 0,
	}, true, nil
}

type planToolTestStore struct {
	plans map[string]runtimex.PlanRecord
}

func (s *planToolTestStore) SaveRun(runtimex.RunRecord) error { return nil }
func (s *planToolTestStore) MarkCancelRequested(string) error { return nil }
func (s *planToolTestStore) Run(string) (runtimex.RunRecord, bool, error) {
	return runtimex.RunRecord{}, false, nil
}
func (s *planToolTestStore) ListRuns(runtimex.RunQuery) ([]runtimex.RunRecord, error) {
	return nil, nil
}
func (s *planToolTestStore) ListSessions(runtimex.SessionQuery) ([]runtimex.SessionRecord, error) {
	return nil, nil
}
func (s *planToolTestStore) SaveEvent(runtimex.RuntimeEvent) error { return nil }
func (s *planToolTestStore) ListEvents(runtimex.EventQuery) ([]runtimex.RuntimeEvent, error) {
	return nil, nil
}
func (s *planToolTestStore) RecoverInterruptedRuns(string) (int, error) { return 0, nil }
func (s *planToolTestStore) SavePlan(plan runtimex.PlanRecord) error {
	if s.plans == nil {
		s.plans = map[string]runtimex.PlanRecord{}
	}
	s.plans[plan.PlanID] = plan
	return nil
}
func (s *planToolTestStore) Plan(planID string) (runtimex.PlanRecord, bool, error) {
	plan, ok := s.plans[planID]
	return plan, ok, nil
}
func (s *planToolTestStore) ListPlans(query runtimex.PlanQuery) ([]runtimex.PlanRecord, error) {
	out := make([]runtimex.PlanRecord, 0, len(s.plans))
	for _, plan := range s.plans {
		if query.OwnerType != "" && plan.OwnerType != query.OwnerType {
			continue
		}
		if query.OwnerID != "" && plan.OwnerID != query.OwnerID {
			continue
		}
		out = append(out, plan)
	}
	return out, nil
}

func TestProviderToolsIncludeDelegationToolsWhenServicesConfigured(t *testing.T) {
	adapter := New(TestDeps())
	adapter.SetDelegationServices(jobControlStub{}, workerControlStub{})

	tools, err := adapter.providerTools("telegram")
	if err != nil {
		t.Fatalf("providerTools: %v", err)
	}
	names := map[string]bool{}
	for _, tool := range tools {
		names[tool.Name] = true
	}
	for _, want := range []string{jobStartToolName, jobStatusToolName, jobCancelToolName, agentSpawnToolName, agentMessageToolName, agentWaitToolName} {
		if !names[want] {
			t.Fatalf("missing delegation tool %s", want)
		}
	}
}

func TestExecuteDelegationTools(t *testing.T) {
	adapter := New(TestDeps())
	adapter.SetDelegationServices(jobControlStub{}, workerControlStub{})

	jobOut, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: jobStartToolName,
		Arguments: map[string]any{
			"command": "echo",
			"args":    []any{"hello"},
		},
	})
	if err != nil || !strings.Contains(jobOut, "\"job_id\": \"job-1\"") {
		t.Fatalf("job_start output=%q err=%v", jobOut, err)
	}

	workerOut, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: agentSpawnToolName,
		Arguments: map[string]any{
			"prompt": "hello",
		},
	})
	if err != nil || !strings.Contains(workerOut, "\"WorkerID\": \"worker-1\"") {
		t.Fatalf("agent_spawn output=%q err=%v", workerOut, err)
	}

	waitOut, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: agentWaitToolName,
		Arguments: map[string]any{
			"worker_id": "worker-1",
		},
	})
	if err != nil || !strings.Contains(waitOut, "\"content\": \"done\"") {
		t.Fatalf("agent_wait output=%q err=%v", waitOut, err)
	}
}

func TestProviderToolsIncludePlanToolsWhenAgentCoreConfigured(t *testing.T) {
	store := &planToolTestStore{}
	api := runtimex.NewAPI(store, runtimex.NewActiveRegistry(), nil)
	core := runtimex.NewRuntimeCore(api, nil, nil, nil, nil, provider.RequestConfig{}, runtimex.MemoryPolicy{}, runtimex.ActionPolicy{})
	adapter := New(TestDeps())
	adapter.SetAgentCore(core)

	tools, err := adapter.providerTools("telegram")
	if err != nil {
		t.Fatalf("providerTools: %v", err)
	}
	names := map[string]bool{}
	for _, tool := range tools {
		names[tool.Name] = true
	}
	for _, want := range []string{"plan_create", "plan_replace_items", "plan_annotate", "plan_item_start", "plan_item_complete", "plan_item_add", "plan_item_insert_after", "plan_item_update", "plan_item_remove"} {
		if !names[want] {
			t.Fatalf("missing plan tool %s", want)
		}
	}
}

func TestExecutePlanToolsThroughAgentCore(t *testing.T) {
	store := &planToolTestStore{}
	api := runtimex.NewAPI(store, runtimex.NewActiveRegistry(), nil)
	core := runtimex.NewRuntimeCore(api, nil, nil, nil, nil, provider.RequestConfig{}, runtimex.MemoryPolicy{}, runtimex.ActionPolicy{})
	adapter := New(TestDeps())
	adapter.SetAgentCore(core)
	adapter.runs.CreateWithID(1001, "run-77", "Investigate rollout", time.Now().UTC())

	createOut, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: "plan_create",
		Arguments: map[string]any{
			"title": "Investigate rollout",
		},
	})
	if err != nil {
		t.Fatalf("plan_create: %v", err)
	}
	var created struct {
		PlanID string
	}
	if err := json.Unmarshal([]byte(createOut), &created); err != nil {
		t.Fatalf("decode create output: %v", err)
	}
	if created.PlanID == "" {
		t.Fatalf("expected plan id in output: %q", createOut)
	}

	replaceOut, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: "plan_replace_items",
		Arguments: map[string]any{
			"plan_id": created.PlanID,
			"items": []any{
				map[string]any{"content": "Inspect runtime"},
				map[string]any{"content": "Verify tool loop"},
			},
		},
	})
	if err != nil {
		t.Fatalf("plan_replace_items: %v", err)
	}
	if !strings.Contains(replaceOut, "\"Items\":") {
		t.Fatalf("unexpected replace output: %q", replaceOut)
	}
	var replaced struct {
		Items []struct {
			ItemID string
			Status string
		}
	}
	if err := json.Unmarshal([]byte(replaceOut), &replaced); err != nil {
		t.Fatalf("decode replace output: %v", err)
	}
	if len(replaced.Items) == 0 || replaced.Items[0].ItemID == "" {
		t.Fatalf("expected plan items in replace output: %q", replaceOut)
	}

	noteOut, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: "plan_annotate",
		Arguments: map[string]any{
			"plan_id": created.PlanID,
			"note":    "Focus on runtime-owned state.",
		},
	})
	if err != nil {
		t.Fatalf("plan_annotate: %v", err)
	}
	if !strings.Contains(noteOut, "\"Notes\":") {
		t.Fatalf("unexpected annotate output: %q", noteOut)
	}

	startOut, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: "plan_item_start",
		Arguments: map[string]any{
			"plan_id": created.PlanID,
			"item_id": replaced.Items[0].ItemID,
		},
	})
	if err != nil {
		t.Fatalf("plan_item_start: %v", err)
	}
	if !strings.Contains(startOut, "\"Status\": \"in_progress\"") {
		t.Fatalf("unexpected start output: %q", startOut)
	}

	completeOut, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: "plan_item_complete",
		Arguments: map[string]any{
			"plan_id": created.PlanID,
			"item_id": replaced.Items[0].ItemID,
		},
	})
	if err != nil {
		t.Fatalf("plan_item_complete: %v", err)
	}
	if !strings.Contains(completeOut, "\"Status\": \"completed\"") {
		t.Fatalf("unexpected complete output: %q", completeOut)
	}

	addOut, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: "plan_item_add",
		Arguments: map[string]any{
			"plan_id": created.PlanID,
			"content": "Inspect prompt budget",
		},
	})
	if err != nil {
		t.Fatalf("plan_item_add: %v", err)
	}
	if !strings.Contains(addOut, "Inspect prompt budget") {
		t.Fatalf("unexpected add output: %q", addOut)
	}

	insertOut, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: "plan_item_insert_after",
		Arguments: map[string]any{
			"plan_id":       created.PlanID,
			"after_item_id": replaced.Items[0].ItemID,
			"content":       "Inspect prompt layers",
		},
	})
	if err != nil {
		t.Fatalf("plan_item_insert_after: %v", err)
	}
	if !strings.Contains(insertOut, "Inspect prompt layers") {
		t.Fatalf("unexpected insert output: %q", insertOut)
	}

	updateOut, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: "plan_item_update",
		Arguments: map[string]any{
			"plan_id": created.PlanID,
			"item_id": replaced.Items[0].ItemID,
			"content": "Inspect runtime events deeply",
		},
	})
	if err != nil {
		t.Fatalf("plan_item_update: %v", err)
	}
	if !strings.Contains(updateOut, "Inspect runtime events deeply") {
		t.Fatalf("unexpected update output: %q", updateOut)
	}

	removeOut, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name: "plan_item_remove",
		Arguments: map[string]any{
			"plan_id": created.PlanID,
			"item_id": replaced.Items[0].ItemID,
		},
	})
	if err != nil {
		t.Fatalf("plan_item_remove: %v", err)
	}
	if strings.Contains(removeOut, "Inspect runtime events deeply") {
		t.Fatalf("expected removed item to disappear from output: %q", removeOut)
	}
}
