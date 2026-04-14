package runtime

import (
	"encoding/json"
	"fmt"
	"strings"
	"time"
)

func (a *API) RunReplay(runID string) (RunReplay, bool, error) {
	view, ok, err := a.RunView(runID)
	if err != nil || !ok {
		return RunReplay{}, ok, err
	}
	steps := []ReplayStep{{
		Index:     1,
		Kind:      "run.started",
		Message:   fmt.Sprintf("run started for session %s", view.SessionID),
		CreatedAt: view.StartedAt,
	}}
	events, err := a.ListEvents(EventQuery{
		RunID:   runID,
		Limit:   1000,
		AfterID: 0,
	})
	if err != nil {
		return RunReplay{}, false, err
	}
	for _, event := range events {
		steps = append(steps, ReplayStep{
			Index:     len(steps) + 1,
			Kind:      event.Kind,
			Message:   replayMessageForEvent(event),
			EventID:   event.ID,
			CreatedAt: event.CreatedAt,
		})
	}
	if view.FinalResponse != "" {
		steps = append(steps, ReplayStep{
			Index:     len(steps) + 1,
			Kind:      "assistant.final",
			Message:   view.FinalResponse,
			CreatedAt: replayStepTime(view.EndedAt, view.StartedAt),
		})
	}
	return RunReplay{Run: view, Steps: steps}, true, nil
}

func replayStepTime(endedAt *time.Time, fallback time.Time) time.Time {
	if endedAt != nil {
		return *endedAt
	}
	return fallback
}

func replayMessageForEvent(event RuntimeEvent) string {
	if len(event.Payload) > 0 {
		if summary := replayPayloadSummary(event.Payload); summary != "" {
			return fmt.Sprintf("%s %s", event.Kind, summary)
		}
		return fmt.Sprintf("%s %s", event.Kind, string(event.Payload))
	}
	return event.Kind
}

func replayPayloadSummary(payload []byte) string {
	var body map[string]any
	if err := json.Unmarshal(payload, &body); err != nil {
		return ""
	}
	var parts []string
	if refs := replayStringSlice(body["archive_refs"]); len(refs) > 0 {
		parts = append(parts, "archive_refs="+strings.Join(refs, ","))
	}
	if refs := replayStringSlice(body["artifact_refs"]); len(refs) > 0 {
		parts = append(parts, "artifact_refs="+strings.Join(refs, ","))
	}
	if ref, ok := body["artifact_ref"].(string); ok && strings.TrimSpace(ref) != "" {
		parts = append(parts, "artifact_ref="+ref)
	}
	if len(parts) == 0 {
		return ""
	}
	return strings.Join(parts, " ")
}

func replayStringSlice(value any) []string {
	raw, ok := value.([]any)
	if !ok {
		return nil
	}
	out := make([]string, 0, len(raw))
	for _, item := range raw {
		if s, ok := item.(string); ok && strings.TrimSpace(s) != "" {
			out = append(out, s)
		}
	}
	return out
}
