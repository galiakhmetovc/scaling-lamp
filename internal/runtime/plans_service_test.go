package runtime

import (
	"context"
	"slices"
	"testing"
)

func TestPlansServiceLifecycleWithSQLiteStore(t *testing.T) {
	store, err := NewSQLiteStore(localRuntimeDBPath(t))
	if err != nil {
		t.Fatalf("new sqlite store: %v", err)
	}
	service := NewPlansService(store)

	plan, err := service.Create(context.Background(), "run", "run-1", "Investigate rollout")
	if err != nil {
		t.Fatalf("create plan: %v", err)
	}
	if plan.PlanID == "" || plan.OwnerType != "run" || plan.OwnerID != "run-1" {
		t.Fatalf("unexpected plan: %+v", plan)
	}

	plan, err = service.ReplaceItems(plan.PlanID, []PlanItem{
		{Content: "Inspect runtime events"},
		{Content: "Verify CLI output"},
	})
	if err != nil {
		t.Fatalf("replace items: %v", err)
	}
	if len(plan.Items) != 2 || plan.Items[0].Status != PlanItemPending {
		t.Fatalf("unexpected plan items: %+v", plan.Items)
	}

	head, ok, err := store.SessionHead(0, "run:run-1")
	if err != nil {
		t.Fatalf("load session head after replace: %v", err)
	}
	if !ok || head.CurrentPlanID != plan.PlanID || head.CurrentPlanTitle != "Investigate rollout" || len(head.CurrentPlanItems) != 2 {
		t.Fatalf("expected plan in session head after replace, got %#v", head)
	}

	plan, err = service.AppendNote(plan.PlanID, "Focus on runtime-owned state.")
	if err != nil {
		t.Fatalf("append note: %v", err)
	}
	if len(plan.Notes) != 1 {
		t.Fatalf("unexpected notes: %+v", plan.Notes)
	}

	plan, err = service.StartItem(plan.PlanID, plan.Items[0].ItemID)
	if err != nil {
		t.Fatalf("start item: %v", err)
	}
	if plan.Items[0].Status != PlanItemInProgress {
		t.Fatalf("expected in_progress item, got %+v", plan.Items[0])
	}

	plan, err = service.CompleteItem(plan.PlanID, plan.Items[0].ItemID)
	if err != nil {
		t.Fatalf("complete item: %v", err)
	}
	if plan.Items[0].Status != PlanItemCompleted {
		t.Fatalf("expected completed item, got %+v", plan.Items[0])
	}

	head, ok, err = store.SessionHead(0, "run:run-1")
	if err != nil {
		t.Fatalf("load session head after complete: %v", err)
	}
	if !ok || !slices.Contains(head.CurrentPlanItems, "[completed] Inspect runtime events") {
		t.Fatalf("expected completed item reflected in session head, got %#v", head)
	}

	got, ok, err := service.Plan(plan.PlanID)
	if err != nil || !ok {
		t.Fatalf("load plan: ok=%v err=%v", ok, err)
	}
	if len(got.Items) != 2 || len(got.Notes) != 1 {
		t.Fatalf("unexpected persisted plan: %+v", got)
	}

	items, err := service.List(PlanQuery{OwnerType: "run", OwnerID: "run-1", Limit: 10})
	if err != nil {
		t.Fatalf("list plans: %v", err)
	}
	if len(items) != 1 || items[0].PlanID != plan.PlanID {
		t.Fatalf("unexpected listed plans: %+v", items)
	}

	events, err := store.ListEvents(EventQuery{EntityType: "plan", EntityID: plan.PlanID, Limit: 20})
	if err != nil {
		t.Fatalf("list events: %v", err)
	}
	kinds := make([]string, 0, len(events))
	for _, event := range events {
		kinds = append(kinds, event.Kind)
	}
	for _, want := range []string{"plan.created", "plan.updated", "plan.item_started", "plan.item_completed"} {
		if !slices.Contains(kinds, want) {
			t.Fatalf("missing event %s in %+v", want, kinds)
		}
	}
}

func TestPlansServiceSupportsIncrementalItemEditing(t *testing.T) {
	store, err := NewSQLiteStore(localRuntimeDBPath(t))
	if err != nil {
		t.Fatalf("new sqlite store: %v", err)
	}
	service := NewPlansService(store)

	plan, err := service.Create(context.Background(), "run", "run-2", "Investigate prompt assembly")
	if err != nil {
		t.Fatalf("create plan: %v", err)
	}
	plan, err = service.ReplaceItems(plan.PlanID, []PlanItem{
		{Content: "Inspect transcript"},
		{Content: "Inspect session head"},
	})
	if err != nil {
		t.Fatalf("replace items: %v", err)
	}

	plan, err = service.InsertItemAfter(plan.PlanID, plan.Items[0].ItemID, PlanItem{Content: "Inspect memory recall"})
	if err != nil {
		t.Fatalf("insert after: %v", err)
	}
	if got, want := plan.Items[1].Content, "Inspect memory recall"; got != want {
		t.Fatalf("unexpected inserted item position: got=%q want=%q", got, want)
	}

	plan, err = service.UpdateItem(plan.PlanID, plan.Items[1].ItemID, PlanItemMutation{Content: "Inspect memory recall deeply"})
	if err != nil {
		t.Fatalf("update item: %v", err)
	}
	if got, want := plan.Items[1].Content, "Inspect memory recall deeply"; got != want {
		t.Fatalf("unexpected updated item content: got=%q want=%q", got, want)
	}

	plan, err = service.RemoveItem(plan.PlanID, plan.Items[0].ItemID)
	if err != nil {
		t.Fatalf("remove item: %v", err)
	}
	if len(plan.Items) != 2 || plan.Items[0].Content != "Inspect memory recall deeply" {
		t.Fatalf("unexpected items after remove: %+v", plan.Items)
	}
}

func TestPlansServiceProjectsSessionOwnedPlanIntoSessionHead(t *testing.T) {
	store, err := NewSQLiteStore(localRuntimeDBPath(t))
	if err != nil {
		t.Fatalf("new sqlite store: %v", err)
	}
	service := NewPlansService(store)

	plan, err := service.Create(context.Background(), "session", "1001:6565", "Raw conversation plan")
	if err != nil {
		t.Fatalf("create plan: %v", err)
	}
	plan, err = service.AddItem(plan.PlanID, PlanItem{Content: "Record current state"})
	if err != nil {
		t.Fatalf("add item: %v", err)
	}

	head, ok, err := store.SessionHead(1001, "1001:6565")
	if err != nil {
		t.Fatalf("load session head: %v", err)
	}
	if !ok {
		t.Fatalf("expected session head to exist")
	}
	if head.CurrentPlanID != plan.PlanID || head.CurrentPlanTitle != "Raw conversation plan" {
		t.Fatalf("unexpected plan snapshot in session head: %#v", head)
	}
	if len(head.CurrentPlanItems) != 1 || head.CurrentPlanItems[0] != "[pending] Record current state" {
		t.Fatalf("unexpected plan items in session head: %#v", head.CurrentPlanItems)
	}
}
