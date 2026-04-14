package runtime

import (
	"context"
	"fmt"
	"strconv"
	"strings"
	"sync/atomic"
	"time"
)

type PlansService struct {
	store   PlanStore
	nextID  atomic.Int64
	resolve func(ownerType, ownerID string) (int64, string, bool)
}

func NewPlansService(store PlanStore) *PlansService {
	service := &PlansService{store: store}
	service.resolve = service.defaultSessionOwner
	return service
}

func (s *PlansService) Create(_ context.Context, ownerType, ownerID, title string) (PlanRecord, error) {
	if s.store == nil {
		return PlanRecord{}, NewControlError(ErrRuntimeUnavailable, "plan store is not configured")
	}
	ownerType = strings.TrimSpace(ownerType)
	ownerID = strings.TrimSpace(ownerID)
	title = strings.TrimSpace(title)
	if ownerType == "" || ownerID == "" || title == "" {
		return PlanRecord{}, NewControlError(ErrValidation, "owner_type, owner_id, and title are required")
	}
	now := time.Now().UTC()
	plan := PlanRecord{
		PlanID:    fmt.Sprintf("plan-%d", s.nextID.Add(1)),
		OwnerType: ownerType,
		OwnerID:   ownerID,
		Title:     title,
		Notes:     nil,
		Items:     nil,
		CreatedAt: now,
		UpdatedAt: now,
	}
	if err := s.store.SavePlan(plan); err != nil {
		return PlanRecord{}, err
	}
	s.refreshSessionHead(plan)
	_ = s.store.SaveEvent(planEvent(plan, "plan.created", map[string]any{
		"title":      title,
		"owner_type": ownerType,
		"owner_id":   ownerID,
	}))
	return plan, nil
}

func (s *PlansService) Plan(planID string) (PlanRecord, bool, error) {
	if s.store == nil {
		return PlanRecord{}, false, NewControlError(ErrRuntimeUnavailable, "plan store is not configured")
	}
	return s.store.Plan(strings.TrimSpace(planID))
}

func (s *PlansService) List(query PlanQuery) ([]PlanRecord, error) {
	if s.store == nil {
		return nil, NewControlError(ErrRuntimeUnavailable, "plan store is not configured")
	}
	return s.store.ListPlans(query)
}

func (s *PlansService) ReplaceItems(planID string, items []PlanItem) (PlanRecord, error) {
	plan, ok, err := s.Plan(planID)
	if err != nil {
		return PlanRecord{}, err
	}
	if !ok {
		return PlanRecord{}, NewControlError(ErrNotFound, "plan not found")
	}
	now := time.Now().UTC()
	next := make([]PlanItem, 0, len(items))
	for i, item := range items {
		content := strings.TrimSpace(item.Content)
		if content == "" {
			continue
		}
		itemID := strings.TrimSpace(item.ItemID)
		if itemID == "" {
			itemID = fmt.Sprintf("%s-item-%d", plan.PlanID, i+1)
		}
		status := item.Status
		if status == "" {
			status = PlanItemPending
		}
		next = append(next, PlanItem{
			ItemID:    itemID,
			Content:   content,
			Status:    status,
			Position:  i + 1,
			CreatedAt: now,
			UpdatedAt: now,
		})
	}
	plan.Items = next
	plan.UpdatedAt = now
	if err := s.store.SavePlan(plan); err != nil {
		return PlanRecord{}, err
	}
	s.refreshSessionHead(plan)
	_ = s.store.SaveEvent(planEvent(plan, "plan.updated", map[string]any{"item_count": len(plan.Items)}))
	return plan, nil
}

func (s *PlansService) AppendNote(planID, note string) (PlanRecord, error) {
	plan, ok, err := s.Plan(planID)
	if err != nil {
		return PlanRecord{}, err
	}
	if !ok {
		return PlanRecord{}, NewControlError(ErrNotFound, "plan not found")
	}
	note = strings.TrimSpace(note)
	if note == "" {
		return PlanRecord{}, NewControlError(ErrValidation, "note is required")
	}
	plan.Notes = append(plan.Notes, note)
	plan.UpdatedAt = time.Now().UTC()
	if err := s.store.SavePlan(plan); err != nil {
		return PlanRecord{}, err
	}
	s.refreshSessionHead(plan)
	_ = s.store.SaveEvent(planEvent(plan, "plan.updated", map[string]any{"note_count": len(plan.Notes)}))
	return plan, nil
}

func (s *PlansService) StartItem(planID, itemID string) (PlanRecord, error) {
	return s.updateItemStatus(planID, itemID, PlanItemInProgress, "plan.item_started")
}

func (s *PlansService) CompleteItem(planID, itemID string) (PlanRecord, error) {
	return s.updateItemStatus(planID, itemID, PlanItemCompleted, "plan.item_completed")
}

func (s *PlansService) updateItemStatus(planID, itemID string, status PlanItemStatus, eventKind string) (PlanRecord, error) {
	plan, ok, err := s.Plan(planID)
	if err != nil {
		return PlanRecord{}, err
	}
	if !ok {
		return PlanRecord{}, NewControlError(ErrNotFound, "plan not found")
	}
	itemID = strings.TrimSpace(itemID)
	if itemID == "" {
		return PlanRecord{}, NewControlError(ErrValidation, "item id is required")
	}
	now := time.Now().UTC()
	found := false
	for i := range plan.Items {
		if plan.Items[i].ItemID != itemID {
			continue
		}
		plan.Items[i].Status = status
		plan.Items[i].UpdatedAt = now
		found = true
		break
	}
	if !found {
		return PlanRecord{}, NewControlError(ErrNotFound, "plan item not found")
	}
	plan.UpdatedAt = now
	if err := s.store.SavePlan(plan); err != nil {
		return PlanRecord{}, err
	}
	s.refreshSessionHead(plan)
	_ = s.store.SaveEvent(planEvent(plan, eventKind, map[string]any{"item_id": itemID, "status": status}))
	return plan, nil
}

func (s *PlansService) AddItem(planID string, item PlanItem) (PlanRecord, error) {
	plan, ok, err := s.Plan(planID)
	if err != nil {
		return PlanRecord{}, err
	}
	if !ok {
		return PlanRecord{}, NewControlError(ErrNotFound, "plan not found")
	}
	now := time.Now().UTC()
	plan.Items = append(plan.Items, buildPlanItem(plan, item, len(plan.Items)+1, now))
	return s.savePlanMutation(plan, "plan.updated", map[string]any{"item_count": len(plan.Items)})
}

func (s *PlansService) InsertItemAfter(planID, afterItemID string, item PlanItem) (PlanRecord, error) {
	plan, ok, err := s.Plan(planID)
	if err != nil {
		return PlanRecord{}, err
	}
	if !ok {
		return PlanRecord{}, NewControlError(ErrNotFound, "plan not found")
	}
	index, err := findPlanItemIndex(plan, afterItemID)
	if err != nil {
		return PlanRecord{}, err
	}
	now := time.Now().UTC()
	next := make([]PlanItem, 0, len(plan.Items)+1)
	next = append(next, plan.Items[:index+1]...)
	next = append(next, buildPlanItem(plan, item, 0, now))
	next = append(next, plan.Items[index+1:]...)
	plan.Items = reindexPlanItems(next, now)
	return s.savePlanMutation(plan, "plan.updated", map[string]any{"item_count": len(plan.Items)})
}

func (s *PlansService) InsertItemBefore(planID, beforeItemID string, item PlanItem) (PlanRecord, error) {
	plan, ok, err := s.Plan(planID)
	if err != nil {
		return PlanRecord{}, err
	}
	if !ok {
		return PlanRecord{}, NewControlError(ErrNotFound, "plan not found")
	}
	index, err := findPlanItemIndex(plan, beforeItemID)
	if err != nil {
		return PlanRecord{}, err
	}
	now := time.Now().UTC()
	next := make([]PlanItem, 0, len(plan.Items)+1)
	next = append(next, plan.Items[:index]...)
	next = append(next, buildPlanItem(plan, item, 0, now))
	next = append(next, plan.Items[index:]...)
	plan.Items = reindexPlanItems(next, now)
	return s.savePlanMutation(plan, "plan.updated", map[string]any{"item_count": len(plan.Items)})
}

func (s *PlansService) UpdateItem(planID, itemID string, patch PlanItemMutation) (PlanRecord, error) {
	plan, ok, err := s.Plan(planID)
	if err != nil {
		return PlanRecord{}, err
	}
	if !ok {
		return PlanRecord{}, NewControlError(ErrNotFound, "plan not found")
	}
	now := time.Now().UTC()
	index, err := findPlanItemIndex(plan, itemID)
	if err != nil {
		return PlanRecord{}, err
	}
	if text := strings.TrimSpace(patch.Content); text != "" {
		plan.Items[index].Content = text
	}
	if patch.Status != "" {
		plan.Items[index].Status = patch.Status
	}
	plan.Items[index].UpdatedAt = now
	return s.savePlanMutation(plan, "plan.updated", map[string]any{"item_id": itemID})
}

func (s *PlansService) RemoveItem(planID, itemID string) (PlanRecord, error) {
	plan, ok, err := s.Plan(planID)
	if err != nil {
		return PlanRecord{}, err
	}
	if !ok {
		return PlanRecord{}, NewControlError(ErrNotFound, "plan not found")
	}
	index, err := findPlanItemIndex(plan, itemID)
	if err != nil {
		return PlanRecord{}, err
	}
	now := time.Now().UTC()
	next := append([]PlanItem{}, plan.Items[:index]...)
	next = append(next, plan.Items[index+1:]...)
	plan.Items = reindexPlanItems(next, now)
	return s.savePlanMutation(plan, "plan.updated", map[string]any{"item_count": len(plan.Items), "removed_item_id": itemID})
}

func (s *PlansService) savePlanMutation(plan PlanRecord, eventKind string, payload map[string]any) (PlanRecord, error) {
	plan.UpdatedAt = time.Now().UTC()
	if err := s.store.SavePlan(plan); err != nil {
		return PlanRecord{}, err
	}
	s.refreshSessionHead(plan)
	_ = s.store.SaveEvent(planEvent(plan, eventKind, payload))
	return plan, nil
}

func buildPlanItem(plan PlanRecord, item PlanItem, position int, now time.Time) PlanItem {
	content := strings.TrimSpace(item.Content)
	itemID := strings.TrimSpace(item.ItemID)
	if itemID == "" {
		itemID = fmt.Sprintf("%s-item-%d", plan.PlanID, now.UnixNano())
	}
	status := item.Status
	if status == "" {
		status = PlanItemPending
	}
	createdAt := item.CreatedAt
	if createdAt.IsZero() {
		createdAt = now
	}
	if position <= 0 {
		position = 1
	}
	return PlanItem{
		ItemID:    itemID,
		Content:   content,
		Status:    status,
		Position:  position,
		CreatedAt: createdAt,
		UpdatedAt: now,
	}
}

func reindexPlanItems(items []PlanItem, now time.Time) []PlanItem {
	for i := range items {
		items[i].Position = i + 1
		items[i].UpdatedAt = now
		if items[i].CreatedAt.IsZero() {
			items[i].CreatedAt = now
		}
	}
	return items
}

func findPlanItemIndex(plan PlanRecord, itemID string) (int, error) {
	itemID = strings.TrimSpace(itemID)
	if itemID == "" {
		return -1, NewControlError(ErrValidation, "item id is required")
	}
	for i := range plan.Items {
		if plan.Items[i].ItemID == itemID {
			return i, nil
		}
	}
	return -1, NewControlError(ErrNotFound, "plan item not found")
}

func (s *PlansService) refreshSessionHead(plan PlanRecord) {
	sessionStore, ok := s.store.(SessionStateStore)
	if !ok {
		return
	}
	chatID, sessionID, resolved := s.resolve(plan.OwnerType, plan.OwnerID)
	if !resolved {
		return
	}
	head, _, err := sessionStore.SessionHead(chatID, sessionID)
	if err != nil {
		return
	}
	head.ChatID = chatID
	head.SessionID = sessionID
	head.CurrentPlanID = plan.PlanID
	head.CurrentPlanTitle = plan.Title
	head.CurrentPlanItems = summarizePlanItems(plan.Items)
	head.UpdatedAt = time.Now().UTC()
	_ = sessionStore.SaveSessionHead(head)
}

func summarizePlanItems(items []PlanItem) []string {
	out := make([]string, 0, len(items))
	for _, item := range items {
		content := strings.TrimSpace(item.Content)
		if content == "" {
			continue
		}
		out = append(out, fmt.Sprintf("[%s] %s", item.Status, content))
	}
	return out
}

func (s *PlansService) defaultSessionOwner(ownerType, ownerID string) (int64, string, bool) {
	ownerType = strings.TrimSpace(ownerType)
	ownerID = strings.TrimSpace(ownerID)
	if ownerType == "" || ownerID == "" {
		return 0, "", false
	}
	if chatID, sessionID, ok := resolveSessionOwner(ownerType, ownerID); ok {
		return chatID, sessionID, true
	}
	if runs, ok := s.store.(RunLifecycleStore); ok && ownerType == "run" {
		record, found, err := runs.Run(ownerID)
		if err == nil && found {
			return record.ChatID, record.SessionID, true
		}
	}
	if workers, ok := s.store.(WorkerStore); ok && ownerType == "worker" {
		record, found, err := workers.Worker(ownerID)
		if err == nil && found {
			return record.ParentChatID, record.ParentSessionID, true
		}
	}
	return 0, ownerType + ":" + ownerID, true
}

func resolveSessionOwner(ownerType, ownerID string) (int64, string, bool) {
	switch ownerType {
	case "session":
		parts := strings.SplitN(ownerID, ":", 2)
		if len(parts) != 2 {
			return 0, "", false
		}
		chatID, err := strconv.ParseInt(parts[0], 10, 64)
		if err != nil {
			return 0, "", false
		}
		return chatID, ownerID, true
	case "run":
		parts := strings.SplitN(ownerID, ":", 2)
		if len(parts) != 2 {
			return 0, "", false
		}
		chatID, err := strconv.ParseInt(parts[0], 10, 64)
		if err != nil {
			return 0, "", false
		}
		return chatID, ownerID, true
	default:
		return 0, "", false
	}
}

func planEvent(plan PlanRecord, kind string, payload map[string]any) RuntimeEvent {
	return RuntimeEvent{
		EntityType: "plan",
		EntityID:   plan.PlanID,
		Kind:       kind,
		Payload:    mustJSONPayload(payload),
		CreatedAt:  time.Now().UTC(),
	}
}
